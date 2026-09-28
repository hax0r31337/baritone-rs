// Ported from Minecraft 26.3 net/minecraft/world/phys/shapes/VoxelShape.java (client jar bytecode)
//
// A shape is the list of boxes the host table holds for a state (`toAabbs()` of the real
// shape), moved by the state's offset at its position (`shape.move(state.getOffset(pos))`).
// Upstream keeps a voxel grid; the parts ported code uses are read off the boxes: `min`/`max`
// are the extremes of the boxes, and `clip`'s "starts inside" test (`isFullWide` of the cell
// holding the test point) is whether the point is in a box, half-open like the grid's cells.

use crate::api::utils::BetterBlockPos;
use crate::mc::{Aabb, Axis, BlockHitResult, Direction, Vec3};

/// `net.minecraft.world.phys.shapes.VoxelShape`, borrowed from a block state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelShape<'a> {
    boxes: &'a [Aabb],
    /// `move(Vec3)` applied to the host's boxes; `None` for shapes that were never moved.
    offset: Option<Vec3>,
}

/// `Shapes.block()`
static BLOCK: [Aabb; 1] = [Aabb {
    min_x: 0.0,
    min_y: 0.0,
    min_z: 0.0,
    max_x: 1.0,
    max_y: 1.0,
    max_z: 1.0,
}];

impl<'a> VoxelShape<'a> {
    pub fn new(boxes: &'a [Aabb], offset: Option<Vec3>) -> Self {
        Self { boxes, offset }
    }

    /// `Shapes.empty()`
    pub fn empty() -> VoxelShape<'static> {
        VoxelShape {
            boxes: &[],
            offset: None,
        }
    }

    /// `Shapes.block()`
    pub fn block() -> VoxelShape<'static> {
        VoxelShape {
            boxes: &BLOCK,
            offset: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// `toAabbs()`
    pub fn to_aabbs(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.boxes.iter().map(move |b| match self.offset {
            Some(offset) => b.move_vec(offset),
            None => *b,
        })
    }

    /// `min(Direction.Axis)`: positive infinity for an empty shape.
    pub fn min(&self, axis: Axis) -> f64 {
        self.to_aabbs()
            .map(|b| match axis {
                Axis::X => b.min_x,
                Axis::Y => b.min_y,
                Axis::Z => b.min_z,
            })
            .fold(f64::INFINITY, |a, b| if b < a { b } else { a })
    }

    /// `max(Direction.Axis)`: negative infinity for an empty shape.
    pub fn max(&self, axis: Axis) -> f64 {
        self.to_aabbs()
            .map(|b| match axis {
                Axis::X => b.max_x,
                Axis::Y => b.max_y,
                Axis::Z => b.max_z,
            })
            .fold(f64::NEG_INFINITY, |a, b| if b > a { b } else { a })
    }

    /// `clip(Vec3, Vec3, BlockPos)`
    pub fn clip(&self, from: Vec3, to: Vec3, pos: BetterBlockPos) -> Option<BlockHitResult> {
        if self.is_empty() {
            return None;
        }
        let diff = to.subtract_vec(from);
        if diff.length_sqr() < 1.0E-7 {
            return None;
        }
        let test_point = from.add_vec(diff.scale(0.001));
        let x = test_point.x - pos.x as f64;
        let y = test_point.y - pos.y as f64;
        let z = test_point.z - pos.z as f64;
        if self.to_aabbs().any(|b| b.contains(x, y, z)) {
            return Some(BlockHitResult::new(
                test_point,
                Direction::get_approximate_nearest(diff.x, diff.y, diff.z).get_opposite(),
                pos,
                true,
            ));
        }
        Aabb::clip_all(self.to_aabbs(), from, to, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extremes() {
        let boxes = [
            Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0),
            Aabb::new(0.0, 0.5, 0.5, 1.0, 1.0, 1.0),
        ];
        let shape = VoxelShape::new(&boxes, None);
        assert_eq!(shape.min(Axis::Z), 0.0);
        assert_eq!(shape.max(Axis::Y), 1.0);
        let moved = VoxelShape::new(&boxes, Some(Vec3::new(0.25, 0.0, -0.125)));
        assert_eq!(moved.min(Axis::X), 0.25);
        assert_eq!(moved.max(Axis::Z), 0.875);
        assert_eq!(VoxelShape::empty().min(Axis::X), f64::INFINITY);
        assert_eq!(VoxelShape::empty().max(Axis::X), f64::NEG_INFINITY);
    }

    #[test]
    fn clip_hits_the_near_face() {
        let pos = BetterBlockPos::new(3, 4, 5);
        let hit = VoxelShape::block()
            .clip(Vec3::new(0.5, 4.5, 5.5), Vec3::new(10.5, 4.5, 5.5), pos)
            .unwrap();
        assert_eq!(hit.get_direction(), Direction::West);
        assert_eq!(hit.get_location(), Vec3::new(3.0, 4.5, 5.5));
        assert!(!hit.is_inside());
        // starting inside
        let hit = VoxelShape::block()
            .clip(Vec3::new(3.5, 4.5, 5.5), Vec3::new(10.5, 4.5, 5.5), pos)
            .unwrap();
        assert!(hit.is_inside());
        assert_eq!(hit.get_direction(), Direction::West);
        // missing
        assert!(
            VoxelShape::block()
                .clip(Vec3::new(0.5, 6.5, 5.5), Vec3::new(10.5, 6.5, 5.5), pos)
                .is_none()
        );
    }
}
