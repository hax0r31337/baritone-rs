//! Ports of `baritone.cache`: the scanner over the loaded chunks, and which chunks count as
//! cached. The chunk cache itself (packed chunks, region files, waypoints) is not ported; see
//! `plans/port.md`.

pub mod cached_world;
pub mod faster_world_scanner;

pub use cached_world::CachedWorld;
