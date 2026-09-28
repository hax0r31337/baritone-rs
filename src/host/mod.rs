//! What the host tells baritone-rs about the game: the block state table, the world, the
//! player, the other entities and the client options.
//!
//! Not ported from upstream. Upstream reads the Minecraft client directly; the port only sees
//! what the host sends. See `plans/port.md` (World model, Block trait table) and
//! `docs/trait-mapping.md`.

pub mod block_state;
pub mod entity;
pub mod options;
pub mod paletted;
pub mod player;
pub mod world;

pub use block_state::{
    BlockOffset, BlockState, BlockStateTable, Climbable, Fluid, Half, OffsetType, Openable,
    SlabType, SpeedKind, Stairs, TableError,
};
pub use entity::Entity;
pub use options::Options;
pub use paletted::PalettedStorage;
pub use player::{
    BlockSet, InteractionHand, Inventory, ItemStack, MobEffectInstance, Player, Tool, ToolRule,
};
pub use world::{Chunk, DimensionType, SubChunk, World, WorldBorder, WorldError};
