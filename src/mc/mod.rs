//! Ports of the Minecraft utility classes the Baritone core depends on.
//!
//! Only math and geometry live here (`Mth`, `Vec3`, `Direction`, `AABB`). Block and item
//! identity never crosses the host API; see `docs/trait-mapping.md`.

pub mod aabb;
pub mod direction;
pub mod mth;
mod mth_tables;
pub mod vec3;

pub use aabb::Aabb;
pub use direction::{Axis, AxisDirection, Direction};
pub use vec3::Vec3;
