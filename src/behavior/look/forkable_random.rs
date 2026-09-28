// Ported from baritone src/main/java/baritone/behavior/look/ForkableRandom.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::time::{SystemTime, UNIX_EPOCH};

use crate::java::current_time_millis;

const DOUBLE_UNIT: f64 = 1.0 / (1u64 << 53) as f64; // 0x1.0p-53

/// Implementation of Xoroshiro256++
///
/// Extended to produce random double-precision floating point numbers, and allow copies to be spawned via
/// [`ForkableRandom::fork`], which share the same internal state as the source object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkableRandom {
    s: [i64; 4],
}

impl Default for ForkableRandom {
    fn default() -> Self {
        Self::new()
    }
}

impl ForkableRandom {
    /// `ForkableRandom()`: seeded from the clocks (upstream: `System.nanoTime() ^
    /// System.currentTimeMillis()`).
    pub fn new() -> Self {
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as i64);
        Self::with_seed(wall ^ current_time_millis())
    }

    /// `ForkableRandom(long)`
    pub fn with_seed(seed_in: i64) -> Self {
        let mut seed = seed_in;
        let mut splitmix64 = || {
            seed = seed.wrapping_add(0x9e3779b97f4a7c15u64 as i64);
            let mut z = seed;
            z = (z ^ ((z as u64) >> 30) as i64).wrapping_mul(0xbf58476d1ce4e5b9u64 as i64);
            z = (z ^ ((z as u64) >> 27) as i64).wrapping_mul(0x94d049bb133111ebu64 as i64);
            z ^ ((z as u64) >> 31) as i64
        };
        let s = [splitmix64(), splitmix64(), splitmix64(), splitmix64()];
        Self { s }
    }

    pub fn next_double(&mut self) -> f64 {
        ((self.next() as u64) >> 11) as f64 * DOUBLE_UNIT
    }

    #[allow(clippy::should_implement_trait)] // upstream's name
    pub fn next(&mut self) -> i64 {
        let s = &mut self.s;
        let result = rotl(s[0].wrapping_add(s[3]), 23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = rotl(s[3], 45);
        result
    }

    pub fn fork(&self) -> ForkableRandom {
        self.clone()
    }
}

fn rotl(x: i64, k: u32) -> i64 {
    x.rotate_left(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_continues_the_same_sequence() {
        let mut a = ForkableRandom::with_seed(42);
        a.next();
        let mut b = a.fork();
        for _ in 0..10 {
            assert_eq!(a.next(), b.next());
        }
        let d = a.next_double();
        assert!((0.0..1.0).contains(&d));
    }
}
