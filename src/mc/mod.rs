//! Ports of the Minecraft utility classes the Baritone core depends on.
//!
//! Only math and geometry live here (`Mth`, `Vec3`, `Direction`, `AABB`, `VoxelShape`,
//! raytracing). Block and item identity never crosses the host API; see
//! `docs/trait-mapping.md`.

pub mod aabb;
pub mod block_getter;
pub mod block_hit_result;
pub mod direction;
pub mod mth;
mod mth_tables;
pub mod vec3;
pub mod voxel_shape;

pub use aabb::Aabb;
pub use block_hit_result::{BlockHitResult, HitResultType};
pub use direction::{Axis, AxisDirection, Direction};
pub use vec3::Vec3;
pub use voxel_shape::VoxelShape;
