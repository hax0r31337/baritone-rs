//! Java numeric semantics that Rust does not provide out of the box.
//!
//! Rust's `f64::min`/`f64::max` drop NaN and leave the sign of zero unspecified, while Java's
//! `Math.min`/`Math.max` propagate NaN and order `-0.0` below `0.0`. Heuristics and costs are
//! compared bit-exactly against upstream, so ported code uses these helpers wherever Java calls
//! `Math.min`/`Math.max` on floating point values.

/// `java.lang.Math.min(double, double)`
#[inline]
pub fn min_f64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.to_bits() == (-0.0f64).to_bits() {
        return b;
    }
    if a <= b { a } else { b }
}

/// `java.lang.Math.max(double, double)`
#[inline]
pub fn max_f64(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.to_bits() == (-0.0f64).to_bits() {
        return b;
    }
    if a >= b { a } else { b }
}

/// `java.lang.Math.min(float, float)`
#[inline]
pub fn min_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && b.to_bits() == (-0.0f32).to_bits() {
        return b;
    }
    if a <= b { a } else { b }
}

/// `java.lang.Math.max(float, float)`
#[inline]
pub fn max_f32(a: f32, b: f32) -> f32 {
    if a.is_nan() {
        return a;
    }
    if a == 0.0 && b == 0.0 && a.to_bits() == (-0.0f32).to_bits() {
        return b;
    }
    if a >= b { a } else { b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nan_propagates() {
        assert!(min_f64(f64::NAN, 1.0).is_nan());
        assert!(min_f64(1.0, f64::NAN).is_nan());
        assert!(max_f64(f64::NAN, 1.0).is_nan());
        assert!(max_f64(1.0, f64::NAN).is_nan());
        assert!(min_f32(f32::NAN, 1.0).is_nan());
        assert!(max_f32(1.0, f32::NAN).is_nan());
    }

    #[test]
    fn signed_zero() {
        assert!(min_f64(0.0, -0.0).is_sign_negative());
        assert!(min_f64(-0.0, 0.0).is_sign_negative());
        assert!(max_f64(0.0, -0.0).is_sign_positive());
        assert!(max_f64(-0.0, 0.0).is_sign_positive());
        assert!(min_f32(0.0, -0.0).is_sign_negative());
        assert!(max_f32(-0.0, 0.0).is_sign_positive());
    }

    #[test]
    fn ordinary() {
        assert_eq!(min_f64(1.0, 2.0), 1.0);
        assert_eq!(max_f64(1.0, 2.0), 2.0);
        assert_eq!(min_f64(f64::NEG_INFINITY, 2.0), f64::NEG_INFINITY);
        assert_eq!(max_f32(-90.0, -100.0), -90.0);
    }
}
