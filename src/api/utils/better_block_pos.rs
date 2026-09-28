// Ported from baritone src/api/java/baritone/api/utils/BetterBlockPos.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::utils::settings_util;
use crate::mc::{Direction, mth};

const NUM_X_BITS: i32 = 26;
const NUM_Z_BITS: i32 = NUM_X_BITS;
const NUM_Y_BITS: i32 = 64 - NUM_X_BITS - NUM_Z_BITS;
const Y_SHIFT: i32 = NUM_Z_BITS;
const X_SHIFT: i32 = Y_SHIFT + NUM_Y_BITS;
const X_MASK: i64 = (1i64 << NUM_X_BITS) - 1;
const Y_MASK: i64 = (1i64 << NUM_Y_BITS) - 1;
const Z_MASK: i64 = (1i64 << NUM_Z_BITS) - 1;

/// A block position. Upstream it subclasses Minecraft's `BlockPos`; the port uses it for every
/// `BlockPos` as well.
///
/// All coordinate arithmetic wraps like Java `int` arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BetterBlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BetterBlockPos {
    pub const ORIGIN: BetterBlockPos = BetterBlockPos::new(0, 0, 0);

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// `new BetterBlockPos(double, double, double)`
    pub fn from_f64(x: f64, y: f64, z: f64) -> Self {
        Self::new(mth::floor(x), mth::floor(y), mth::floor(z))
    }

    /// `hashCode()`
    pub fn hash_code(&self) -> i32 {
        Self::long_hash(self.x, self.y, self.z) as i32
    }

    /// `longHash(BetterBlockPos)`
    pub fn long_hash_pos(pos: BetterBlockPos) -> i64 {
        Self::long_hash(pos.x, pos.y, pos.z)
    }

    pub fn long_hash(x: i32, y: i32, z: i32) -> i64 {
        // TODO use the same thing as BlockPos.fromLong();
        // invertibility would be incredibly useful
        /*
         *   This is the hashcode implementation of Vec3i (the superclass of the class which I shall not name)
         *
         *   public int hashCode() {
         *       return (this.getY() + this.getZ() * 31) * 31 + this.getX();
         *   }
         *
         *   That is terrible and has tons of collisions and makes the HashMap terribly inefficient.
         *
         *   That's why we grab out the X, Y, Z and calculate our own hashcode
         */
        let mut hash: i64 = 3241;
        hash = 3457689i64.wrapping_mul(hash).wrapping_add(x as i64);
        hash = 8734625i64.wrapping_mul(hash).wrapping_add(y as i64);
        hash = 2873465i64.wrapping_mul(hash).wrapping_add(z as i64);
        hash
    }

    pub fn above(self) -> Self {
        Self::new(self.x, self.y.wrapping_add(1), self.z)
    }

    /// `above(int)`
    pub fn above_n(self, amt: i32) -> Self {
        if amt == 0 {
            self
        } else {
            Self::new(self.x, self.y.wrapping_add(amt), self.z)
        }
    }

    pub fn below(self) -> Self {
        Self::new(self.x, self.y.wrapping_sub(1), self.z)
    }

    /// `below(int)`
    pub fn below_n(self, amt: i32) -> Self {
        if amt == 0 {
            self
        } else {
            Self::new(self.x, self.y.wrapping_sub(amt), self.z)
        }
    }

    pub fn relative(self, dir: Direction) -> Self {
        let (dx, dy, dz) = dir.get_unit_vec3i();
        Self::new(
            self.x.wrapping_add(dx),
            self.y.wrapping_add(dy),
            self.z.wrapping_add(dz),
        )
    }

    /// `relative(Direction, int)`
    pub fn relative_n(self, dir: Direction, dist: i32) -> Self {
        if dist == 0 {
            return self;
        }
        let (dx, dy, dz) = dir.get_unit_vec3i();
        Self::new(
            self.x.wrapping_add(dx.wrapping_mul(dist)),
            self.y.wrapping_add(dy.wrapping_mul(dist)),
            self.z.wrapping_add(dz.wrapping_mul(dist)),
        )
    }

    pub fn north(self) -> Self {
        Self::new(self.x, self.y, self.z.wrapping_sub(1))
    }

    /// `north(int)`
    pub fn north_n(self, amt: i32) -> Self {
        if amt == 0 {
            self
        } else {
            Self::new(self.x, self.y, self.z.wrapping_sub(amt))
        }
    }

    pub fn south(self) -> Self {
        Self::new(self.x, self.y, self.z.wrapping_add(1))
    }

    /// `south(int)`
    pub fn south_n(self, amt: i32) -> Self {
        if amt == 0 {
            self
        } else {
            Self::new(self.x, self.y, self.z.wrapping_add(amt))
        }
    }

    pub fn east(self) -> Self {
        Self::new(self.x.wrapping_add(1), self.y, self.z)
    }

    /// `east(int)`
    pub fn east_n(self, amt: i32) -> Self {
        if amt == 0 {
            self
        } else {
            Self::new(self.x.wrapping_add(amt), self.y, self.z)
        }
    }

    pub fn west(self) -> Self {
        Self::new(self.x.wrapping_sub(1), self.y, self.z)
    }

    /// `west(int)`
    pub fn west_n(self, amt: i32) -> Self {
        if amt == 0 {
            self
        } else {
            Self::new(self.x.wrapping_sub(amt), self.y, self.z)
        }
    }

    pub fn distance_sq(&self, to: &BetterBlockPos) -> f64 {
        let dx = self.x as f64 - to.x as f64;
        let dy = self.y as f64 - to.y as f64;
        let dz = self.z as f64 - to.z as f64;
        dx * dx + dy * dy + dz * dz
    }

    pub fn distance_to(&self, to: &BetterBlockPos) -> f64 {
        let dx = self.x as f64 - to.x as f64;
        let dy = self.y as f64 - to.y as f64;
        let dz = self.z as f64 - to.z as f64;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    pub fn serialize_to_long(x: i32, y: i32, z: i32) -> i64 {
        ((x as i64) & X_MASK) << X_SHIFT | ((y as i64) & Y_MASK) << Y_SHIFT | ((z as i64) & Z_MASK)
    }

    pub fn deserialize_from_long(serialized: i64) -> BetterBlockPos {
        let x = (serialized << (64 - X_SHIFT - NUM_X_BITS) >> (64 - NUM_X_BITS)) as i32;
        let y = (serialized << (64 - Y_SHIFT - NUM_Y_BITS) >> (64 - NUM_Y_BITS)) as i32;
        let z = (serialized << (64 - NUM_Z_BITS) >> (64 - NUM_Z_BITS)) as i32;
        BetterBlockPos::new(x, y, z)
    }
}

impl fmt::Display for BetterBlockPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BetterBlockPos{{x={},y={},z={}}}",
            settings_util::maybe_censor(self.x),
            settings_util::maybe_censor(self.y),
            settings_util::maybe_censor(self.z)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_hash_matches_java() {
        // from tests/fixtures/reference/math_goals.json (upstream Java); the fixture replay
        // covers many more
        assert_eq!(
            BetterBlockPos::long_hash(-7, -750184011, -10),
            7129665533046068973
        );
        assert_eq!(
            BetterBlockPos::new(-7, -750184011, -10).hash_code(),
            411997933
        );
    }

    #[test]
    fn serialize_round_trip_in_range() {
        for (x, y, z) in [
            (0, 0, 0),
            (-1, -1, -1),
            (30_000_000, 2047, -30_000_000),
            (-33_554_432, -2048, 33_554_431),
        ] {
            let s = BetterBlockPos::serialize_to_long(x, y, z);
            assert_eq!(
                BetterBlockPos::deserialize_from_long(s),
                BetterBlockPos::new(x, y, z)
            );
        }
    }

    #[test]
    fn zero_offsets_return_self() {
        let p = BetterBlockPos::new(1, 2, 3);
        assert_eq!(p.above_n(0), p);
        assert_eq!(p.relative_n(Direction::East, 0), p);
    }
}
