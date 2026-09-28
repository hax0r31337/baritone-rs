// Ported from baritone src/main/java/baritone/utils/pathing/BetterWorldBorder.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::host::WorldBorder;

/// Essentially, a "rule" for the path finder, prevents proposed movements from attempting to venture
/// into the world border, and prevents actual movements from placing blocks in the world border.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BetterWorldBorder {
    min_x: f64,
    max_x: f64,
    min_z: f64,
    max_z: f64,
}

impl BetterWorldBorder {
    pub fn new(border: &WorldBorder) -> Self {
        Self {
            min_x: border.min_x,
            max_x: border.max_x,
            min_z: border.min_z,
            max_z: border.max_z,
        }
    }

    pub fn entirely_contains(&self, x: i32, z: i32) -> bool {
        x.wrapping_add(1) as f64 > self.min_x
            && (x as f64) < self.max_x
            && z.wrapping_add(1) as f64 > self.min_z
            && (z as f64) < self.max_z
    }

    pub fn can_place_at(&self, x: i32, z: i32) -> bool {
        // move it in 1 block on all sides
        // because we can't place a block at the very edge against a block outside the border
        // it won't let us right click it
        x as f64 > self.min_x
            && (x.wrapping_add(1) as f64) < self.max_x
            && z as f64 > self.min_z
            && (z.wrapping_add(1) as f64) < self.max_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges() {
        let border = BetterWorldBorder::new(&WorldBorder {
            min_x: -2.5,
            max_x: 3.0,
            min_z: 0.0,
            max_z: 10.0,
        });
        assert!(border.entirely_contains(-3, 0));
        assert!(!border.entirely_contains(-4, 0));
        assert!(border.entirely_contains(2, 9));
        assert!(!border.entirely_contains(3, 0));
        assert!(!border.entirely_contains(0, 10));
        assert!(!border.entirely_contains(0, -1));

        assert!(border.can_place_at(-2, 1));
        assert!(!border.can_place_at(-3, 1));
        assert!(border.can_place_at(1, 8));
        assert!(!border.can_place_at(2, 1));
        assert!(!border.can_place_at(0, 0));
        assert!(!border.can_place_at(0, 9));
    }

    #[test]
    fn java_int_overflow() {
        // x + 1 is int arithmetic upstream, so it wraps before the comparison
        let border = BetterWorldBorder::new(&WorldBorder::default());
        let wide = BetterWorldBorder::new(&WorldBorder {
            min_x: f64::NEG_INFINITY,
            max_x: f64::INFINITY,
            min_z: f64::NEG_INFINITY,
            max_z: f64::INFINITY,
        });
        assert!(!border.entirely_contains(i32::MAX, 0));
        assert!(wide.entirely_contains(i32::MAX, 0));
        assert!(wide.can_place_at(i32::MAX, 0));
        let tight = BetterWorldBorder::new(&WorldBorder {
            min_x: -3e9,
            max_x: 0.0,
            min_z: -1.0,
            max_z: 2.0,
        });
        assert!(
            tight.can_place_at(i32::MAX, 0),
            "i32::MAX + 1 wraps to i32::MIN"
        );
    }
}
