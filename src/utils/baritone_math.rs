// Ported from baritone src/main/java/baritone/utils/BaritoneMath.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

const FLOOR_DOUBLE_D: f64 = 1_073_741_824.0;
const FLOOR_DOUBLE_I: i32 = 1_073_741_824;

pub fn fast_floor(v: f64) -> i32 {
    ((v + FLOOR_DOUBLE_D) as i32).wrapping_sub(FLOOR_DOUBLE_I)
}

pub fn fast_ceil(v: f64) -> i32 {
    FLOOR_DOUBLE_I.wrapping_sub((FLOOR_DOUBLE_D - v) as i32)
}
