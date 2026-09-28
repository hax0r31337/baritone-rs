// Ported from baritone src/api/java/baritone/api/utils/RayTraceUtils.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The entity is always the local player; its level is passed next to it.

use crate::api::utils::Rotation;
use crate::api::utils::rotation_utils;
use crate::host::{Player, World};
use crate::mc::{BlockHitResult, Vec3};

/// Performs a block raytrace with the specified rotations. This should only be used when
/// any entity collisions can be ignored, because this method will not recognize if an
/// entity is in the way or not. The local player's block reach distance will be used.
///
/// `rayTraceTowards(Entity, Rotation, double)`
pub fn ray_trace_towards(
    entity: &Player,
    world: &World,
    rotation: Rotation,
    block_reach_distance: f64,
) -> BlockHitResult {
    ray_trace_towards_sneak(entity, world, rotation, block_reach_distance, false)
}

/// `rayTraceTowards(Entity, Rotation, double, boolean)`
pub fn ray_trace_towards_sneak(
    entity: &Player,
    world: &World,
    rotation: Rotation,
    block_reach_distance: f64,
    would_sneak: bool,
) -> BlockHitResult {
    let start = if would_sneak {
        infer_sneaking_eye_position(entity)
    } else {
        entity.get_eye_position_partial(1.0) // do whatever is correct
    };

    let direction = rotation_utils::calc_look_direction_from_rotation(rotation);
    let end = start.add(
        direction.x * block_reach_distance,
        direction.y * block_reach_distance,
        direction.z * block_reach_distance,
    );
    world.clip(start, end)
}

pub fn infer_sneaking_eye_position(entity: &Player) -> Vec3 {
    Vec3::new(
        entity.get_x(),
        entity.get_y() + entity.crouching_eye_height as f64,
        entity.get_z(),
    )
}
