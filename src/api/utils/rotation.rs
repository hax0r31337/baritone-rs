// Ported from baritone src/api/java/baritone/api/utils/Rotation.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::java;

/// A yaw/pitch pair in degrees. Arithmetic is `f32`, like upstream.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    /// The yaw angle of this Rotation
    yaw: f32,
    /// The pitch angle of this Rotation
    pitch: f32,
}

impl Rotation {
    /// Panics on NaN or infinite angles, like upstream's `IllegalStateException`.
    pub fn new(yaw: f32, pitch: f32) -> Self {
        if yaw.is_infinite() || yaw.is_nan() || pitch.is_infinite() || pitch.is_nan() {
            panic!("{yaw} {pitch}");
        }
        Self { yaw, pitch }
    }

    /// The yaw of this rotation
    pub fn get_yaw(&self) -> f32 {
        self.yaw
    }

    /// The pitch of this rotation
    pub fn get_pitch(&self) -> f32 {
        self.pitch
    }

    /// Adds the yaw/pitch of the specified rotations to this rotation's yaw/pitch, and returns
    /// the result.
    pub fn add(&self, other: &Rotation) -> Rotation {
        Rotation::new(self.yaw + other.yaw, self.pitch + other.pitch)
    }

    /// Subtracts the yaw/pitch of the specified rotations from this rotation's yaw/pitch, and
    /// returns the result.
    pub fn subtract(&self, other: &Rotation) -> Rotation {
        Rotation::new(self.yaw - other.yaw, self.pitch - other.pitch)
    }

    /// A copy of this rotation with the pitch clamped
    pub fn clamp(&self) -> Rotation {
        Rotation::new(self.yaw, Self::clamp_pitch(self.pitch))
    }

    /// A copy of this rotation with the yaw normalized
    pub fn normalize(&self) -> Rotation {
        Rotation::new(Self::normalize_yaw(self.yaw), self.pitch)
    }

    /// A copy of this rotation with the pitch clamped and the yaw normalized
    pub fn normalize_and_clamp(&self) -> Rotation {
        Rotation::new(Self::normalize_yaw(self.yaw), Self::clamp_pitch(self.pitch))
    }

    pub fn with_pitch(&self, pitch: f32) -> Rotation {
        Rotation::new(self.yaw, pitch)
    }

    /// Is really close to
    pub fn is_really_close_to(&self, other: &Rotation) -> bool {
        self.yaw_is_really_close(other) && ((self.pitch - other.pitch).abs() as f64) < 0.01
    }

    pub fn yaw_is_really_close(&self, other: &Rotation) -> bool {
        let yaw_diff =
            (Self::normalize_yaw(self.yaw) - Self::normalize_yaw(other.yaw)).abs() as f64; // you cant fool me
        yaw_diff < 0.01 || yaw_diff > 359.99
    }

    /// Clamps the specified pitch value between -90 and 90.
    pub fn clamp_pitch(pitch: f32) -> f32 {
        java::max_f32(-90.0, java::min_f32(90.0, pitch))
    }

    /// Normalizes the specified yaw value between -180 and 180.
    pub fn normalize_yaw(yaw: f32) -> f32 {
        let mut new_yaw = yaw % 360.0;
        if new_yaw < -180.0 {
            new_yaw += 360.0;
        }
        if new_yaw > 180.0 {
            new_yaw -= 360.0;
        }
        new_yaw
    }

    /// Gets the distance between a starting yaw and an offset yaw. Distance can be negative if
    /// the offset yaw is behind of the starting yaw.
    pub fn yaw_distance_from_offset(yaw: f32, offset_yaw: f32) -> f32 {
        if ((yaw > 0.0) ^ (offset_yaw > 0.0))
            && ((yaw > 90.0 || yaw < -90.0) ^ (offset_yaw > 90.0 || offset_yaw < -90.0))
        {
            if yaw < 0.0 {
                360.0 + (yaw - offset_yaw)
            } else {
                360.0 - (yaw - offset_yaw)
            }
        } else {
            yaw - offset_yaw
        }
    }
}

/// Uses Rust float formatting, not Java's `Float.toString` (log output only).
impl fmt::Display for Rotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Yaw: {:?}, Pitch: {:?}", self.yaw, self.pitch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn rejects_nan() {
        Rotation::new(f32::NAN, 0.0);
    }

    #[test]
    #[should_panic]
    fn rejects_infinite() {
        Rotation::new(0.0, f32::INFINITY);
    }

    #[test]
    fn normalize_and_clamp() {
        let r = Rotation::new(270.0, 100.0).normalize_and_clamp();
        assert_eq!(r.get_yaw(), -90.0);
        assert_eq!(r.get_pitch(), 90.0);
        assert!(Rotation::new(10.0, 0.0).is_really_close_to(&Rotation::new(370.005, 0.005)));
    }
}
