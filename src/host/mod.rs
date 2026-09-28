//! What the host tells baritone-rs about the game: the block state table, the world and the
//! player.
//!
//! Not ported from upstream. Upstream reads the Minecraft client directly; the port only sees
//! what the host sends. See `plans/port.md` (World model, Block trait table) and
//! `docs/trait-mapping.md`.

pub mod block_state;
pub mod paletted;
pub mod player;
pub mod world;

pub use block_state::{
    BlockState, BlockStateTable, Climbable, Fluid, Half, Openable, SlabType, SpeedKind, Stairs,
    TableError,
};
pub use paletted::PalettedStorage;
pub use player::{BlockSet, Inventory, ItemStack, MobEffectInstance, Player, Tool, ToolRule};
pub use world::{Chunk, DimensionType, SubChunk, World, WorldBorder, WorldError};
