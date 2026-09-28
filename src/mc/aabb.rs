// Ported from Minecraft 26.3 net/minecraft/world/phys/AABB.java (client jar bytecode)
//
// Only the constructor and fields; the rest comes with raytracing (phase 4).

use serde::{Deserialize, Serialize};

use crate::java::{max_f64, min_f64};

/// `net.minecraft.world.phys.AABB`, an axis-aligned box. Block shapes in the host table are
/// lists of these, relative to the block's origin.
///
/// Serialized as `[minX, minY, minZ, maxX, maxY, maxZ]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 6]", into = "[f64; 6]")]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    /// `new AABB(double, double, double, double, double, double)`: the corners in any order.
    pub fn new(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> Self {
        Self {
            min_x: min_f64(x1, x2),
            min_y: min_f64(y1, y2),
            min_z: min_f64(z1, z2),
            max_x: max_f64(x1, x2),
            max_y: max_f64(y1, y2),
            max_z: max_f64(z1, z2),
        }
    }
}

impl From<[f64; 6]> for Aabb {
    fn from(v: [f64; 6]) -> Self {
        Self::new(v[0], v[1], v[2], v[3], v[4], v[5])
    }
}

impl From<Aabb> for [f64; 6] {
    fn from(b: Aabb) -> Self {
        [b.min_x, b.min_y, b.min_z, b.max_x, b.max_y, b.max_z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_in_any_order() {
        let b = Aabb::new(1.0, 0.5, 1.0, 0.0, 0.0, 0.25);
        assert_eq!(<[f64; 6]>::from(b), [0.0, 0.0, 0.25, 1.0, 0.5, 1.0]);
    }

    #[test]
    fn serde_as_array() {
        let b: Aabb = serde_json::from_str("[0, 0, 0, 1, 0.5, 1]").unwrap();
        assert_eq!(b, Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0));
        assert_eq!(
            serde_json::to_string(&b).unwrap(),
            "[0.0,0.0,0.0,1.0,0.5,1.0]"
        );
    }
}
