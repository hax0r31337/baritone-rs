// Ported from Minecraft 26.3 net/minecraft/util/Mth.java (client jar bytecode)

//! The subset of `net.minecraft.util.Mth` used by Baritone, bit-exact with the Java client.
//!
//! `SIN` is computed with the platform `sin`, which matches HotSpot's `Math.sin` on every entry
//! with glibc (checked by the reference tests). `ASIN_TAB`/`COS_TAB` are embedded
//! (`mth_tables.rs`) because no Rust libm reproduces Java's values for them.

use std::sync::LazyLock;

use super::mth_tables::{ASIN_TAB, COS_TAB};

/// `65536 / (2 * PI)`, the scale of the `SIN` lookup table.
const SIN_SCALE: f64 = 10430.378350470453;

static SIN: LazyLock<Box<[f32; 65536]>> = LazyLock::new(|| {
    let mut table = Box::new([0.0f32; 65536]);
    for (i, v) in table.iter_mut().enumerate() {
        *v = (i as f64 / SIN_SCALE).sin() as f32;
    }
    table
});

const FRAC_BIAS: f64 = f64::from_bits(4805340802404319232);

/// The `SIN` lookup table (exposed for verification against the Java client).
pub fn sin_table() -> &'static [f32; 65536] {
    &SIN
}

/// The `ASIN_TAB` lookup table used by [`atan2`].
pub fn asin_tab() -> &'static [f64; 257] {
    &ASIN_TAB
}

/// The `COS_TAB` lookup table used by [`atan2`].
pub fn cos_tab() -> &'static [f64; 257] {
    &COS_TAB
}

/// The `FRAC_BIAS` constant used by [`atan2`].
pub fn frac_bias() -> f64 {
    FRAC_BIAS
}

pub fn sin(value: f64) -> f32 {
    SIN[((value * SIN_SCALE) as i64 & 65535) as usize]
}

pub fn cos(value: f64) -> f32 {
    SIN[((value * SIN_SCALE + 16384.0) as i64 & 65535) as usize]
}

pub fn floor(value: f64) -> i32 {
    value.floor() as i32
}

/// `lfloor(double)`
pub fn lfloor(value: f64) -> i64 {
    value.floor() as i64
}

/// `frac(double)`
pub fn frac(num: f64) -> f64 {
    num - lfloor(num) as f64
}

/// `sign(double)`
pub fn sign(number: f64) -> i32 {
    if number == 0.0 {
        0
    } else if number > 0.0 {
        1
    } else {
        -1
    }
}

/// `lerp(double, double, double)`
pub fn lerp(alpha1: f64, p0: f64, p1: f64) -> f64 {
    p0 + alpha1 * (p1 - p0)
}

/// `clamp(double, double, double)`
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else {
        crate::java::min_f64(value, max)
    }
}

/// `clamp(float, float, float)`
pub fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else {
        crate::java::min_f32(value, max)
    }
}

/// `getSeed(int, int, int)`: `x * 3129871` is `int` arithmetic, the rest `long`.
pub fn get_seed(x: i32, y: i32, z: i32) -> i64 {
    let mut seed = (x.wrapping_mul(3129871) as i64) ^ (z as i64).wrapping_mul(116129781) ^ y as i64;
    seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42317861)
        .wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

/// `wrapDegrees(float)`
pub fn wrap_degrees(angle: f32) -> f32 {
    let mut normalized = angle % 360.0;
    if normalized >= 180.0 {
        normalized -= 360.0;
    }
    if normalized < -180.0 {
        normalized += 360.0;
    }
    normalized
}

/// `wrapDegrees(double)`
pub fn wrap_degrees_f64(angle: f64) -> f64 {
    let mut normalized = angle % 360.0;
    if normalized >= 180.0 {
        normalized -= 360.0;
    }
    if normalized < -180.0 {
        normalized += 360.0;
    }
    normalized
}

pub fn atan2(mut y: f64, mut x: f64) -> f64 {
    let d2 = x * x + y * y;
    if d2.is_nan() {
        return f64::NAN;
    }
    let neg_y = y < 0.0;
    if neg_y {
        y = -y;
    }
    let neg_x = x < 0.0;
    if neg_x {
        x = -x;
    }
    let steep = y > x;
    if steep {
        std::mem::swap(&mut x, &mut y);
    }
    let rinv = fast_inv_sqrt(d2);
    x *= rinv;
    y *= rinv;
    let yp = FRAC_BIAS + y;
    // Java: (int) Double.doubleToRawLongBits(yp), i.e. the low 32 bits
    let index = yp.to_bits() as i32;
    let phi = ASIN_TAB[index as usize];
    let c_phi = COS_TAB[index as usize];
    let s_phi = yp - FRAC_BIAS;
    let sd = y * c_phi - x * s_phi;
    let d = (6.0 + sd * sd) * sd * 0.16666666666666666;
    let mut theta = phi + d;
    if steep {
        theta = std::f64::consts::FRAC_PI_2 - theta;
    }
    if neg_x {
        theta = std::f64::consts::PI - theta;
    }
    if neg_y {
        theta = -theta;
    }
    theta
}

pub fn fast_inv_sqrt(mut x: f64) -> f64 {
    let xhalf = 0.5 * x;
    let mut i = x.to_bits() as i64;
    i = 6910469410427058090i64.wrapping_sub(i >> 1);
    x = f64::from_bits(i as u64);
    x * (1.5 - xhalf * x * x)
}
