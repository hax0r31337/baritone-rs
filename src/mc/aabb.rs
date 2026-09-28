// Ported from Minecraft 26.3 net/minecraft/world/phys/AABB.java (client jar bytecode)
//
// The constructor, moving, intersection and clipping (raytracing) parts.

use serde::{Deserialize, Serialize};

use crate::api::utils::BetterBlockPos;
use crate::java::{max_f64, min_f64};
use crate::mc::{BlockHitResult, Direction, Vec3};

/// `net.minecraft.world.phys.AABB`, an axis-aligned box. Block shapes in the host table are
/// lists of these, relative to the block's origin.
///
/// Serialized as `[minX, minY, minZ, maxX, maxY, maxZ]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 6]", into = "[f64; 6]")]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    /// `new AABB(double, double, double, double, double, double)`: the corners in any order.
    pub fn new(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> Self {
        Self {
            min_x: min_f64(x1, x2),
            min_y: min_f64(y1, y2),
            min_z: min_f64(z1, z2),
            max_x: max_f64(x1, x2),
            max_y: max_f64(y1, y2),
            max_z: max_f64(z1, z2),
        }
    }

    /// `ofSize(Vec3, double, double, double)`
    pub fn of_size(center: Vec3, size_x: f64, size_y: f64, size_z: f64) -> Self {
        Self::new(
            center.x - size_x / 2.0,
            center.y - size_y / 2.0,
            center.z - size_z / 2.0,
            center.x + size_x / 2.0,
            center.y + size_y / 2.0,
            center.z + size_z / 2.0,
        )
    }

    /// `move(double, double, double)` (`move` is reserved in Rust)
    pub fn move_by(&self, xa: f64, ya: f64, za: f64) -> Self {
        Self::new(
            self.min_x + xa,
            self.min_y + ya,
            self.min_z + za,
            self.max_x + xa,
            self.max_y + ya,
            self.max_z + za,
        )
    }

    /// `move(BlockPos)`
    pub fn move_pos(&self, pos: BetterBlockPos) -> Self {
        self.move_by(pos.x as f64, pos.y as f64, pos.z as f64)
    }

    /// `move(Vec3)`
    pub fn move_vec(&self, pos: Vec3) -> Self {
        self.move_by(pos.x, pos.y, pos.z)
    }

    /// `intersects(double, double, double, double, double, double)`
    pub fn intersects_coords(
        &self,
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
    ) -> bool {
        self.min_x < max_x
            && self.max_x > min_x
            && self.min_y < max_y
            && self.max_y > min_y
            && self.min_z < max_z
            && self.max_z > min_z
    }

    /// `intersects(AABB)`
    pub fn intersects(&self, aabb: &Aabb) -> bool {
        self.intersects_coords(
            aabb.min_x, aabb.min_y, aabb.min_z, aabb.max_x, aabb.max_y, aabb.max_z,
        )
    }

    /// `intersects(Vec3, Vec3)`: the box spanned by two corners in any order.
    pub fn intersects_vec(&self, min: Vec3, max: Vec3) -> bool {
        self.intersects_coords(
            min_f64(min.x, max.x),
            min_f64(min.y, max.y),
            min_f64(min.z, max.z),
            max_f64(min.x, max.x),
            max_f64(min.y, max.y),
            max_f64(min.z, max.z),
        )
    }

    /// `contains(double, double, double)`
    pub fn contains(&self, x: f64, y: f64, z: f64) -> bool {
        x >= self.min_x
            && x < self.max_x
            && y >= self.min_y
            && y < self.max_y
            && z >= self.min_z
            && z < self.max_z
    }

    /// `clip(Vec3, Vec3)`
    pub fn clip(&self, from: Vec3, to: Vec3) -> Option<Vec3> {
        Self::clip_coords(
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z, from, to,
        )
    }

    /// `clip(double, double, double, double, double, double, Vec3, Vec3)`
    #[allow(clippy::too_many_arguments)]
    pub fn clip_coords(
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
        from: Vec3,
        to: Vec3,
    ) -> Option<Vec3> {
        let mut scale_reference = 1.0;
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let dz = to.z - from.z;
        let direction = get_direction(
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
            from,
            &mut scale_reference,
            None,
            dx,
            dy,
            dz,
        );
        direction?;
        let scale = scale_reference;
        Some(from.add(scale * dx, scale * dy, scale * dz))
    }

    /// `clip(Iterable<AABB>, Vec3, Vec3, BlockPos)`: `aabbs` are relative to `pos`.
    pub fn clip_all(
        aabbs: impl IntoIterator<Item = Aabb>,
        from: Vec3,
        to: Vec3,
        pos: BetterBlockPos,
    ) -> Option<BlockHitResult> {
        let mut scale_reference = 1.0;
        let mut direction = None;
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let dz = to.z - from.z;
        for aabb in aabbs {
            let aabb = aabb.move_pos(pos);
            direction = get_direction(
                aabb.min_x,
                aabb.min_y,
                aabb.min_z,
                aabb.max_x,
                aabb.max_y,
                aabb.max_z,
                from,
                &mut scale_reference,
                direction,
                dx,
                dy,
                dz,
            );
        }
        let direction = direction?;
        let scale = scale_reference;
        Some(BlockHitResult::new(
            from.add(scale * dx, scale * dy, scale * dz),
            direction,
            pos,
            false,
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn get_direction(
    min_x: f64,
    min_y: f64,
    min_z: f64,
    max_x: f64,
    max_y: f64,
    max_z: f64,
    from: Vec3,
    scale_reference: &mut f64,
    mut direction: Option<Direction>,
    dx: f64,
    dy: f64,
    dz: f64,
) -> Option<Direction> {
    if dx > 1.0E-7 {
        direction = clip_point(
            scale_reference,
            direction,
            dx,
            dy,
            dz,
            min_x,
            min_y,
            max_y,
            min_z,
            max_z,
            Direction::West,
            from.x,
            from.y,
            from.z,
        );
    } else if dx < -1.0E-7 {
        direction = clip_point(
            scale_reference,
            direction,
            dx,
            dy,
            dz,
            max_x,
            min_y,
            max_y,
            min_z,
            max_z,
            Direction::East,
            from.x,
            from.y,
            from.z,
        );
    }

    if dy > 1.0E-7 {
        direction = clip_point(
            scale_reference,
            direction,
            dy,
            dz,
            dx,
            min_y,
            min_z,
            max_z,
            min_x,
            max_x,
            Direction::Down,
            from.y,
            from.z,
            from.x,
        );
    } else if dy < -1.0E-7 {
        direction = clip_point(
            scale_reference,
            direction,
            dy,
            dz,
            dx,
            max_y,
            min_z,
            max_z,
            min_x,
            max_x,
            Direction::Up,
            from.y,
            from.z,
            from.x,
        );
    }

    if dz > 1.0E-7 {
        direction = clip_point(
            scale_reference,
            direction,
            dz,
            dx,
            dy,
            min_z,
            min_x,
            max_x,
            min_y,
            max_y,
            Direction::North,
            from.z,
            from.x,
            from.y,
        );
    } else if dz < -1.0E-7 {
        direction = clip_point(
            scale_reference,
            direction,
            dz,
            dx,
            dy,
            max_z,
            min_x,
            max_x,
            min_y,
            max_y,
            Direction::South,
            from.z,
            from.x,
            from.y,
        );
    }

    direction
}

#[allow(clippy::too_many_arguments)]
fn clip_point(
    scale_reference: &mut f64,
    direction: Option<Direction>,
    da: f64,
    db: f64,
    dc: f64,
    point: f64,
    min_b: f64,
    max_b: f64,
    min_c: f64,
    max_c: f64,
    new_direction: Direction,
    from_a: f64,
    from_b: f64,
    from_c: f64,
) -> Option<Direction> {
    let s = (point - from_a) / da;
    let pb = from_b + s * db;
    let pc = from_c + s * dc;
    if 0.0 < s
        && s < *scale_reference
        && min_b - 1.0E-7 < pb
        && pb < max_b + 1.0E-7
        && min_c - 1.0E-7 < pc
        && pc < max_c + 1.0E-7
    {
        *scale_reference = s;
        Some(new_direction)
    } else {
        direction
    }
}

impl From<[f64; 6]> for Aabb {
    fn from(v: [f64; 6]) -> Self {
        Self::new(v[0], v[1], v[2], v[3], v[4], v[5])
    }
}

impl From<Aabb> for [f64; 6] {
    fn from(b: Aabb) -> Self {
        [b.min_x, b.min_y, b.min_z, b.max_x, b.max_y, b.max_z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_in_any_order() {
        let b = Aabb::new(1.0, 0.5, 1.0, 0.0, 0.0, 0.25);
        assert_eq!(<[f64; 6]>::from(b), [0.0, 0.0, 0.25, 1.0, 0.5, 1.0]);
    }

    #[test]
    fn serde_as_array() {
        let b: Aabb = serde_json::from_str("[0, 0, 0, 1, 0.5, 1]").unwrap();
        assert_eq!(b, Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0));
        assert_eq!(
            serde_json::to_string(&b).unwrap(),
            "[0.0,0.0,0.0,1.0,0.5,1.0]"
        );
    }
}
