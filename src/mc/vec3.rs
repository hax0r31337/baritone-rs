// Ported from Minecraft 26.3 net/minecraft/world/phys/Vec3.java (client jar bytecode)

use serde::{Deserialize, Serialize};

/// `net.minecraft.world.phys.Vec3`, an immutable double precision vector.
///
/// Serialized as `[x, y, z]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f64; 3]", into = "[f64; 3]")]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// `add(double, double, double)`
    pub fn add(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(self.x + x, self.y + y, self.z + z)
    }

    /// `add(Vec3)`
    pub fn add_vec(self, other: Vec3) -> Self {
        self.add(other.x, other.y, other.z)
    }

    /// `subtract(double, double, double)`
    pub fn subtract(self, x: f64, y: f64, z: f64) -> Self {
        self.add(-x, -y, -z)
    }

    /// `subtract(Vec3)`
    pub fn subtract_vec(self, other: Vec3) -> Self {
        self.subtract(other.x, other.y, other.z)
    }

    pub fn scale(self, scale: f64) -> Self {
        Self::new(self.x * scale, self.y * scale, self.z * scale)
    }

    pub fn dot(self, other: Vec3) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn length_sqr(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn length(self) -> f64 {
        self.length_sqr().sqrt()
    }

    pub fn distance_to_sqr(self, other: Vec3) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        let dz = other.z - self.z;
        dx * dx + dy * dy + dz * dz
    }

    pub fn distance_to(self, other: Vec3) -> f64 {
        self.distance_to_sqr(other).sqrt()
    }

    /// `multiply(double, double, double)`
    pub fn multiply(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(self.x * x, self.y * y, self.z * z)
    }

    /// `equals(Object)`: `Double.compare` per component, so `-0.0 != 0.0` and `NaN == NaN`.
    /// (`==` is Rust's float comparison.)
    pub fn equals(self, other: Vec3) -> bool {
        fn same(a: f64, b: f64) -> bool {
            a.total_cmp(&b).is_eq() || (a.is_nan() && b.is_nan())
        }
        same(self.x, other.x) && same(self.y, other.y) && same(self.z, other.z)
    }
}

impl From<[f64; 3]> for Vec3 {
    fn from(v: [f64; 3]) -> Self {
        Self::new(v[0], v[1], v[2])
    }
}

impl From<Vec3> for [f64; 3] {
    fn from(v: Vec3) -> Self {
        [v.x, v.y, v.z]
    }
}
