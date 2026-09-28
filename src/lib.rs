//! A Rust port of the [Baritone](https://github.com/cabaletta/baritone) pathfinding core.
//!
//! The module tree mirrors upstream one file per file: `api` is `src/api/java/baritone/api`,
//! `behavior`, `cache`, `event`, `pathing`, `process` and `utils` are
//! `src/main/java/baritone/{behavior,cache,event,pathing,process,utils}`, and [`Baritone`] is
//! `baritone.Baritone`. The upstream commit is recorded in `UPSTREAM`; porting conventions are
//! in `docs/porting.md`. `host` is not from upstream: it is what the host sends in place of the
//! Minecraft client (block state table, world, player).

pub mod api;
mod baritone;
pub mod behavior;
pub mod cache;
pub mod event;
pub mod host;
pub mod java;
pub mod mc;
pub mod pathing;
pub mod process;
pub mod settings;
pub mod utils;

pub use baritone::Baritone;
