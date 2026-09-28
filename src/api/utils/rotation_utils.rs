// Ported from baritone src/api/java/baritone/api/utils/RotationUtils.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The deprecated entity-based overloads and `calcVec3dFromRotation` are not ported. Upstream's
// `reachableOffset` finds the look behavior's aim processor through
// `BaritoneAPI.getProvider().getBaritoneForPlayer(ctx.player())`; the port passes it in.

use crate::api::behavior::look::IAimProcessor;
use crate::api::utils::i_player_context::IPlayerContext;
use crate::api::utils::{BetterBlockPos, Rotation, ray_trace_utils, vec_utils};
use crate::mc::{Axis, HitResultType, Vec3, VoxelShape, mth};
use crate::settings::settings;

/// Offsets from the root block position to the center of each side.
const BLOCK_SIDE_MULTIPLIERS: [Vec3; 6] = [
    Vec3::new(0.5, 0.0, 0.5), // Down
    Vec3::new(0.5, 1.0, 0.5), // Up
    Vec3::new(0.5, 0.5, 0.0), // North
    Vec3::new(0.5, 0.5, 1.0), // South
    Vec3::new(0.0, 0.5, 0.5), // West
    Vec3::new(1.0, 0.5, 0.5), // East
];

/// Constant that a degree value is multiplied by to get the equivalent radian value
pub const DEG_TO_RAD: f64 = std::f64::consts::PI / 180.0;
pub const DEG_TO_RAD_F: f32 = DEG_TO_RAD as f32;

/// Constant that a radian value is multiplied by to get the equivalent degree value
pub const RAD_TO_DEG: f64 = 180.0 / std::f64::consts::PI;
pub const RAD_TO_DEG_F: f32 = RAD_TO_DEG as f32;

/// Calculates the rotation from BlockPos<sub>dest</sub> to BlockPos<sub>orig</sub>
pub fn calc_rotation_from_coords(orig: BetterBlockPos, dest: BetterBlockPos) -> Rotation {
    calc_rotation_from_vec3d_absolute(
        Vec3::new(orig.x as f64, orig.y as f64, orig.z as f64),
        Vec3::new(dest.x as f64, dest.y as f64, dest.z as f64),
    )
}

/// Wraps the target angles to a relative value from the current angles. This is done by
/// subtracting the current from the target, normalizing it, and then adding the current angles
/// back to it.
pub fn wrap_angles_to_relative(current: Rotation, target: Rotation) -> Rotation {
    if current.yaw_is_really_close(&target) {
        return Rotation::new(current.get_yaw(), target.get_pitch());
    }
    target.subtract(&current).normalize().add(&current)
}

/// Calculates the rotation from Vec<sub>dest</sub> to Vec<sub>orig</sub> and makes the return
/// value relative to the specified current rotations.
pub fn calc_rotation_from_vec3d(orig: Vec3, dest: Vec3, current: Rotation) -> Rotation {
    wrap_angles_to_relative(current, calc_rotation_from_vec3d_absolute(orig, dest))
}

/// `calcRotationFromVec3d(Vec3, Vec3)` (private upstream): the rotation from
/// Vec<sub>dest</sub> to Vec<sub>orig</sub>, not relative to any current rotation.
pub fn calc_rotation_from_vec3d_absolute(orig: Vec3, dest: Vec3) -> Rotation {
    let delta = [orig.x - dest.x, orig.y - dest.y, orig.z - dest.z];
    let yaw = mth::atan2(delta[0], -delta[2]);
    let dist = (delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
    let pitch = mth::atan2(delta[1], dist);
    Rotation::new((yaw * RAD_TO_DEG) as f32, (pitch * RAD_TO_DEG) as f32)
}

/// Calculates the look vector for the specified yaw/pitch rotations.
pub fn calc_look_direction_from_rotation(rotation: Rotation) -> Vec3 {
    let pi = std::f64::consts::PI as f32;
    let flat_z = mth::cos(((-rotation.get_yaw() * DEG_TO_RAD_F) - pi) as f64);
    let flat_x = mth::sin(((-rotation.get_yaw() * DEG_TO_RAD_F) - pi) as f64);
    let pitch_base = -mth::cos((-rotation.get_pitch() * DEG_TO_RAD_F) as f64);
    let pitch_height = mth::sin((-rotation.get_pitch() * DEG_TO_RAD_F) as f64);
    Vec3::new(
        (flat_x * pitch_base) as f64,
        pitch_height as f64,
        (flat_z * pitch_base) as f64,
    )
}

/// `reachable(IPlayerContext, BlockPos)`
pub fn reachable(
    ctx: &dyn IPlayerContext,
    aim: &dyn IAimProcessor,
    pos: BetterBlockPos,
) -> Option<Rotation> {
    reachable_sneak(ctx, aim, pos, false)
}

/// `reachable(IPlayerContext, BlockPos, boolean)`
pub fn reachable_sneak(
    ctx: &dyn IPlayerContext,
    aim: &dyn IAimProcessor,
    pos: BetterBlockPos,
    would_sneak: bool,
) -> Option<Rotation> {
    reachable_distance_sneak(
        ctx,
        aim,
        pos,
        ctx.player_controller_ref().get_block_reach_distance(),
        would_sneak,
    )
}

/// Determines if the specified entity is able to reach the center of any of the sides
/// of the specified block. It first checks if the block center is reachable, and if so,
/// that rotation will be returned. If not, it will return the first center of a given
/// side that is reachable. The return type will be `None` if the entity is
/// unable to reach any of the sides of the block.
///
/// `reachable(IPlayerContext, BlockPos, double)`
pub fn reachable_distance(
    ctx: &dyn IPlayerContext,
    aim: &dyn IAimProcessor,
    pos: BetterBlockPos,
    block_reach_distance: f64,
) -> Option<Rotation> {
    reachable_distance_sneak(ctx, aim, pos, block_reach_distance, false)
}

/// `reachable(IPlayerContext, BlockPos, double, boolean)`
pub fn reachable_distance_sneak(
    ctx: &dyn IPlayerContext,
    aim: &dyn IAimProcessor,
    pos: BetterBlockPos,
    block_reach_distance: f64,
    would_sneak: bool,
) -> Option<Rotation> {
    if settings().remain_with_existing_look_direction && ctx.is_looking_at(pos) {
        /*
         * why add 0.0001?
         * to indicate that we actually have a desired pitch
         * the way we indicate that the pitch can be whatever and we only care about the yaw
         * is by setting the desired pitch to the current pitch
         * setting the desired pitch to the current pitch + 0.0001 means that we do have a desired pitch, it's
         * just what it currently is
         *
         * or if you're a normal person literally all this does it ensure that we don't nudge the pitch to a normal level
         */
        let hypothetical = ctx.player_rotations().add(&Rotation::new(0.0, 0.0001));
        if would_sneak {
            // the concern here is: what if we're looking at it now, but as soon as we start sneaking we no longer are
            let result = ray_trace_utils::ray_trace_towards_sneak(
                ctx.player(),
                ctx.world(),
                hypothetical,
                block_reach_distance,
                true,
            );
            if result.get_type() == HitResultType::Block && result.get_block_pos() == pos {
                return Some(hypothetical); // yes, if we sneaked we would still be looking at the block
            }
        } else {
            return Some(hypothetical);
        }
    }
    let possible_rotation = reachable_center(ctx, aim, pos, block_reach_distance, would_sneak);
    //System.out.println("center: " + possibleRotation);
    if possible_rotation.is_some() {
        return possible_rotation;
    }

    let state = ctx.world().get_block_state(pos);
    let mut shape = state.get_shape(pos);
    if shape.is_empty() {
        shape = VoxelShape::block();
    }
    for side_offset in BLOCK_SIDE_MULTIPLIERS {
        let x_diff =
            shape.min(Axis::X) * side_offset.x + shape.max(Axis::X) * (1.0 - side_offset.x);
        let y_diff =
            shape.min(Axis::Y) * side_offset.y + shape.max(Axis::Y) * (1.0 - side_offset.y);
        let z_diff =
            shape.min(Axis::Z) * side_offset.z + shape.max(Axis::Z) * (1.0 - side_offset.z);
        let possible_rotation = reachable_offset(
            ctx,
            aim,
            pos,
            Vec3::new(pos.x as f64, pos.y as f64, pos.z as f64).add(x_diff, y_diff, z_diff),
            block_reach_distance,
            would_sneak,
        );
        if possible_rotation.is_some() {
            return possible_rotation;
        }
    }
    None
}

/// Determines if the specified entity is able to reach the specified block with
/// the given offsetted position. The return type will be `None` if
/// the entity is unable to reach the block with the offset applied.
pub fn reachable_offset(
    ctx: &dyn IPlayerContext,
    aim: &dyn IAimProcessor,
    pos: BetterBlockPos,
    offset_pos: Vec3,
    block_reach_distance: f64,
    would_sneak: bool,
) -> Option<Rotation> {
    let eyes = if would_sneak {
        ray_trace_utils::infer_sneaking_eye_position(ctx.player())
    } else {
        ctx.player().get_eye_position_partial(1.0)
    };
    let rotation = calc_rotation_from_vec3d(eyes, offset_pos, ctx.player_rotations());
    let actual_rotation = aim.peek_rotation(ctx, rotation);
    let result = ray_trace_utils::ray_trace_towards_sneak(
        ctx.player(),
        ctx.world(),
        actual_rotation,
        block_reach_distance,
        would_sneak,
    );
    //System.out.println(result);
    if result.get_type() == HitResultType::Block {
        if result.get_block_pos() == pos {
            return Some(rotation);
        }
        if ctx.world().get_block_state(pos).fire && result.get_block_pos() == pos.below() {
            return Some(rotation);
        }
    }
    None
}

/// Determines if the specified entity is able to reach the specified block where it is
/// looking at the direct center of it's hitbox.
pub fn reachable_center(
    ctx: &dyn IPlayerContext,
    aim: &dyn IAimProcessor,
    pos: BetterBlockPos,
    block_reach_distance: f64,
    would_sneak: bool,
) -> Option<Rotation> {
    reachable_offset(
        ctx,
        aim,
        pos,
        vec_utils::calculate_block_center(ctx.world(), pos),
        block_reach_distance,
        would_sneak,
    )
}
