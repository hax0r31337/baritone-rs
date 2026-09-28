// Ported from baritone src/api/java/baritone/api/utils/RotationUtils.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Only the pure math is ported so far. The `reachable*` family needs the player context,
// raytracing and LookBehavior and is ported with execution (phase 4). The deprecated
// entity-based overloads and `calcVec3dFromRotation` are not ported.

use crate::api::utils::{BetterBlockPos, Rotation};
use crate::mc::{Vec3, mth};

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
