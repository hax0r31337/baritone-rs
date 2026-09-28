// Ported from Minecraft 26.3 net/minecraft/world/phys/Vec3.java (client jar bytecode)

/// `net.minecraft.world.phys.Vec3`, an immutable double precision vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
}
