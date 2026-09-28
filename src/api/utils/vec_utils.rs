// Ported from baritone src/api/java/baritone/api/utils/VecUtils.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The entity overloads take the entity position instead of the entity.

use crate::api::utils::BetterBlockPos;
use crate::host::World;
use crate::mc::{Axis, Vec3};

/// Calculates the center of the block at the specified position's bounding box
///
/// Panics if the shape's center is NaN, like upstream's `IllegalStateException`.
pub fn calculate_block_center(world: &World, pos: BetterBlockPos) -> Vec3 {
    let b = world.get_block_state(pos);
    let shape = b.get_collision_shape(pos);
    if shape.is_empty() {
        return get_block_pos_center(pos);
    }
    let x_diff = (shape.min(Axis::X) + shape.max(Axis::X)) / 2.0;
    let mut y_diff = (shape.min(Axis::Y) + shape.max(Axis::Y)) / 2.0;
    let z_diff = (shape.min(Axis::Z) + shape.max(Axis::Z)) / 2.0;
    if x_diff.is_nan() || y_diff.is_nan() || z_diff.is_nan() {
        panic!("{} {} {:?}", b.name, pos, shape);
    }
    if b.fire {
        //look at bottom of fire when putting it out
        y_diff = 0.0;
    }
    Vec3::new(
        pos.x as f64 + x_diff,
        pos.y as f64 + y_diff,
        pos.z as f64 + z_diff,
    )
}

/// Gets the assumed center position of the given block position. This is done by adding 0.5 to
/// the X, Y, and Z axes.
pub fn get_block_pos_center(pos: BetterBlockPos) -> Vec3 {
    Vec3::new(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5)
}

/// Gets the distance from the specified position to the assumed center of the specified block
/// position.
pub fn distance_to_center(pos: BetterBlockPos, x: f64, y: f64, z: f64) -> f64 {
    let xdiff = pos.x as f64 + 0.5 - x;
    let ydiff = pos.y as f64 + 0.5 - y;
    let zdiff = pos.z as f64 + 0.5 - z;
    (xdiff * xdiff + ydiff * ydiff + zdiff * zdiff).sqrt()
}

/// Gets the distance from the specified entity's position to the assumed center of the
/// specified block position.
pub fn entity_distance_to_center(entity_position: Vec3, pos: BetterBlockPos) -> f64 {
    distance_to_center(pos, entity_position.x, entity_position.y, entity_position.z)
}

/// Gets the distance from the specified entity's position to the assumed center of the
/// specified block position, ignoring the Y axis.
pub fn entity_flat_distance_to_center(entity_position: Vec3, pos: BetterBlockPos) -> f64 {
    distance_to_center(
        pos,
        entity_position.x,
        pos.y as f64 + 0.5,
        entity_position.z,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_distances() {
        let pos = BetterBlockPos::new(1, 2, 3);
        assert_eq!(get_block_pos_center(pos), Vec3::new(1.5, 2.5, 3.5));
        assert_eq!(distance_to_center(pos, 1.5, 2.5, 3.5), 0.0);
        assert_eq!(
            entity_distance_to_center(Vec3::new(4.5, 6.5, 3.5), pos),
            5.0
        );
        assert_eq!(
            entity_flat_distance_to_center(Vec3::new(4.5, 100.0, 7.5), pos),
            5.0
        );
    }
}
