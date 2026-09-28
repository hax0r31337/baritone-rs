//! A Rust port of the [Baritone](https://github.com/cabaletta/baritone) pathfinding core.
//!
//! The module tree mirrors upstream one file per file: `api` is `src/api/java/baritone/api`,
//! `pathing` and `utils` are `src/main/java/baritone/{pathing,utils}`. The upstream commit is
//! recorded in `UPSTREAM`; porting conventions are in `docs/porting.md`.

pub mod api;
pub mod java;
pub mod mc;
pub mod pathing;
pub mod settings;
pub mod utils;
