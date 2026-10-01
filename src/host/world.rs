//! The world as the host last described it: loaded chunks, world border, dimension bounds.
//!
//! A `World` is a value. The tick thread keeps the current one in an `Arc<World>` and hands
//! clones of that `Arc` to path calculations as snapshots. Updates go through
//! `Arc::make_mut(&mut world)`: while a snapshot is alive this clones the chunk map (the
//! chunks are `Arc`s too), and each update clones only the chunk and section it touches, so a
//! running calculation keeps its snapshot. This replaces upstream's `createThreadSafeCopy` of
//! the client chunk cache.
//!
//! Chunk lifetime is the host's: it loads and unloads chunks explicitly, like the client.

use std::fmt;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::block_state::{BlockState, BlockStateTable, FluidState};
use super::paletted::{self, PalettedStorage};
use crate::api::utils::BetterBlockPos;
use crate::mc::{BlockHitResult, Direction, Vec3, VoxelShape, block_getter};

/// `ChunkPos.pack(x, z)`: the chunk map key.
#[inline]
pub fn chunk_key(chunk_x: i32, chunk_z: i32) -> i64 {
    (chunk_x as u32 as i64) | ((chunk_z as u32 as i64) << 32)
}

/// Why a world update was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldError {
    /// `min_y` and `height` must be multiples of 16, `height` positive, and the top within `i32`.
    Dimension { min_y: i32, height: i32 },
    /// A chunk with the wrong number of sections for the dimension.
    SectionCount { expected: usize, actual: usize },
    /// A state id that is not in the block state table.
    UnknownState(u32),
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorldError::Dimension { min_y, height } => {
                write!(f, "invalid dimension: min_y {min_y}, height {height}")
            }
            WorldError::SectionCount { expected, actual } => {
                write!(f, "chunk has {actual} sections, expected {expected}")
            }
            WorldError::UnknownState(id) => write!(f, "unknown block state {id}"),
        }
    }
}

impl std::error::Error for WorldError {}

/// The parts of Minecraft's `DimensionType` the port reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DimensionType {
    /// `minY()`: lowest block Y.
    pub min_y: i32,
    /// `height()`: number of block layers.
    pub height: i32,
    /// Placed water evaporates. Upstream checks the dimension key (`dimension() != Level.NETHER`),
    /// not the dimension type's `ultrawarm`: set this exactly in `minecraft:the_nether`.
    pub water_evaporates: bool,
}

impl DimensionType {
    /// Sections per chunk.
    pub fn section_count(&self) -> usize {
        (self.height >> 4) as usize
    }

    fn validate(&self) -> Result<(), WorldError> {
        if self.min_y & 15 != 0
            || self.height & 15 != 0
            || self.height <= 0
            || self.min_y.checked_add(self.height).is_none()
        {
            return Err(WorldError::Dimension {
                min_y: self.min_y,
                height: self.height,
            });
        }
        Ok(())
    }
}

/// World border bounds (`WorldBorder.getMinX()` ...).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldBorder {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}

impl Default for WorldBorder {
    /// The vanilla default border: 59999968 wide, centered on 0, 0.
    fn default() -> Self {
        Self {
            min_x: -29999984.0,
            max_x: 29999984.0,
            min_z: -29999984.0,
            max_z: 29999984.0,
        }
    }
}

/// One 16×16×16 section of host state ids.
///
/// The block layer holds one state per block. With the `bedrock` feature a section also has
/// Bedrock's liquid layer (layer 1), which holds the water of waterlogged blocks; on Java
/// waterlogging is part of the block's state. Ask the section for the fluid at a position
/// ([`Self::fluid`]) rather than the block's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubChunk {
    blocks: PalettedStorage,
    /// `None` when the layer is all air.
    #[cfg(feature = "bedrock")]
    liquid: Option<PalettedStorage>,
}

/// Index of local coordinates (only the low 4 bits of each are used).
#[inline]
fn local_index(x: i32, y: i32, z: i32) -> usize {
    paletted::index((x & 15) as usize, (y & 15) as usize, (z & 15) as usize)
}

impl SubChunk {
    /// Every block is `id`.
    pub fn filled(id: u32) -> Self {
        Self::from_storage(PalettedStorage::single(id))
    }

    pub fn from_storage(blocks: PalettedStorage) -> Self {
        Self {
            blocks,
            #[cfg(feature = "bedrock")]
            liquid: None,
        }
    }

    /// A Bedrock section from its block layer (layer 0) and liquid layer (layer 1), `None`
    /// when the section has no liquid layer.
    #[cfg(feature = "bedrock")]
    pub fn from_layers(blocks: PalettedStorage, liquid: Option<PalettedStorage>) -> Self {
        Self { blocks, liquid }
    }

    /// The block state at local coordinates (only the low 4 bits of each are used).
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> u32 {
        self.blocks.get(local_index(x, y, z))
    }

    /// Sets the block state at local coordinates (only the low 4 bits of each are used).
    pub fn set(&mut self, x: i32, y: i32, z: i32, id: u32) {
        self.blocks.set(local_index(x, y, z), id);
    }

    /// The fluid at local coordinates (only the low 4 bits of each are used), where `block` is
    /// the block state there ([`Self::get`]'s): the liquid layer's fluid if it has one there,
    /// else the block's own.
    #[inline]
    #[cfg_attr(not(feature = "bedrock"), allow(unused_variables))]
    pub fn fluid_in(
        &self,
        table: &BlockStateTable,
        x: i32,
        y: i32,
        z: i32,
        block: &BlockState,
    ) -> FluidState {
        #[cfg(feature = "bedrock")]
        if let Some(liquid) = &self.liquid {
            let fluid = table.get(liquid.get(local_index(x, y, z))).own_fluid();
            if !fluid.is_empty() {
                return fluid;
            }
        }
        block.own_fluid()
    }

    /// The fluid at local coordinates (only the low 4 bits of each are used).
    pub fn fluid(&self, table: &BlockStateTable, x: i32, y: i32, z: i32) -> FluidState {
        self.fluid_in(table, x, y, z, table.get(self.get(x, y, z)))
    }

    /// The liquid layer's state at local coordinates (only the low 4 bits of each are used),
    /// `None` if the section has no liquid layer.
    #[cfg(feature = "bedrock")]
    #[inline]
    pub fn get_liquid(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        Some(self.liquid.as_ref()?.get(local_index(x, y, z)))
    }

    /// Sets the liquid layer's state at local coordinates (only the low 4 bits of each are
    /// used). `air` is the table's air, which the rest of a new layer is filled with.
    #[cfg(feature = "bedrock")]
    pub fn set_liquid(&mut self, x: i32, y: i32, z: i32, id: u32, air: u32) {
        let liquid = match &mut self.liquid {
            Some(liquid) => liquid,
            None if id == air => return,
            None => self.liquid.insert(PalettedStorage::single(air)),
        };
        liquid.set(local_index(x, y, z), id);
    }

    /// The block layer.
    pub fn storage(&self) -> &PalettedStorage {
        &self.blocks
    }

    /// The liquid layer, `None` when it is all air.
    #[cfg(feature = "bedrock")]
    pub fn liquid_storage(&self) -> Option<&PalettedStorage> {
        self.liquid.as_ref()
    }

    /// Every layer's storage.
    fn layers(&self) -> impl Iterator<Item = &PalettedStorage> {
        let layers = std::iter::once(&self.blocks);
        #[cfg(feature = "bedrock")]
        let layers = layers.chain(&self.liquid);
        layers
    }
}

/// A 16-wide column of sections, bottom first. A missing section is all air.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    sections: Box<[Option<Arc<SubChunk>>]>,
}

impl Chunk {
    /// An empty (all air) chunk; use [`DimensionType::section_count`].
    pub fn new(section_count: usize) -> Self {
        Self {
            sections: vec![None; section_count].into_boxed_slice(),
        }
    }

    /// `getSections()[index]`; `None` for an empty section.
    #[inline]
    pub fn section(&self, index: usize) -> Option<&SubChunk> {
        self.sections[index].as_deref()
    }

    pub fn set_section(&mut self, index: usize, section: Option<SubChunk>) {
        self.sections[index] = section.map(Arc::new);
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }
}

/// See the module docs.
#[derive(Clone, Debug)]
pub struct World {
    table: Arc<BlockStateTable>,
    dimension: DimensionType,
    border: WorldBorder,
    chunks: FxHashMap<i64, Arc<Chunk>>,
}

impl World {
    /// A world with no chunks loaded and the default border.
    pub fn new(table: Arc<BlockStateTable>, dimension: DimensionType) -> Result<Self, WorldError> {
        dimension.validate()?;
        Ok(Self {
            table,
            dimension,
            border: WorldBorder::default(),
            chunks: FxHashMap::default(),
        })
    }

    /// The block state table the ids in this world refer to.
    #[inline]
    pub fn table(&self) -> &Arc<BlockStateTable> {
        &self.table
    }

    #[inline]
    pub fn dimension(&self) -> DimensionType {
        self.dimension
    }

    pub fn border(&self) -> WorldBorder {
        self.border
    }

    pub fn set_border(&mut self, border: WorldBorder) {
        self.border = border;
    }

    /// `ClientChunkCache.getChunk(x, z, ChunkStatus.FULL, false)`
    #[inline]
    pub fn get_chunk(&self, chunk_x: i32, chunk_z: i32) -> Option<&Arc<Chunk>> {
        self.chunks.get(&chunk_key(chunk_x, chunk_z))
    }

    /// `ClientChunkCache.hasChunk(x, z)`
    #[inline]
    pub fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunks.contains_key(&chunk_key(chunk_x, chunk_z))
    }

    /// Loaded chunk coordinates, in no particular order.
    pub fn loaded_chunks(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.chunks.keys().map(|&k| (k as i32, (k >> 32) as i32))
    }

    /// Loads (or replaces) a chunk.
    pub fn load_chunk(
        &mut self,
        chunk_x: i32,
        chunk_z: i32,
        chunk: Chunk,
    ) -> Result<(), WorldError> {
        let expected = self.dimension.section_count();
        if chunk.section_count() != expected {
            return Err(WorldError::SectionCount {
                expected,
                actual: chunk.section_count(),
            });
        }
        for layer in chunk.sections.iter().flatten().flat_map(|s| s.layers()) {
            self.check_ids(layer.palette())?;
        }
        self.chunks
            .insert(chunk_key(chunk_x, chunk_z), Arc::new(chunk));
        Ok(())
    }

    /// Unloads a chunk. Returns whether it was loaded.
    pub fn unload_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunks.remove(&chunk_key(chunk_x, chunk_z)).is_some()
    }

    /// A block update. Returns `Ok(false)` if the chunk is not loaded or `y` is outside the
    /// dimension; the client ignores those updates too.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: u32) -> Result<bool, WorldError> {
        self.update_section(x, y, z, id, |section| section.set(x, y, z, id))
    }

    /// A block update in the liquid layer (Bedrock's layer 1), like [`Self::set_block`].
    #[cfg(feature = "bedrock")]
    pub fn set_liquid(&mut self, x: i32, y: i32, z: i32, id: u32) -> Result<bool, WorldError> {
        let air = self.table.air().id;
        self.update_section(x, y, z, id, |section| section.set_liquid(x, y, z, id, air))
    }

    /// Runs `update` on the section at a block position, creating it (all air) unless `id` is
    /// air; see [`Self::set_block`].
    fn update_section(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        id: u32,
        update: impl FnOnce(&mut SubChunk),
    ) -> Result<bool, WorldError> {
        self.check_ids(&[id])?;
        let air = self.table.air().id;
        let y = y.wrapping_sub(self.dimension.min_y);
        if y < 0 || y >= self.dimension.height {
            return Ok(false);
        }
        let Some(chunk) = self.chunks.get_mut(&chunk_key(x >> 4, z >> 4)) else {
            return Ok(false);
        };
        let slot = &mut Arc::make_mut(chunk).sections[(y >> 4) as usize];
        let section = match slot {
            Some(section) => section,
            None if id == air => return Ok(true),
            None => slot.insert(Arc::new(SubChunk::filled(air))),
        };
        update(Arc::make_mut(section));
        Ok(true)
    }

    /// The section holding a block position, if it is inside the world, loaded and not empty.
    fn section_at(&self, pos: BetterBlockPos) -> Option<&SubChunk> {
        let y = pos.y.wrapping_sub(self.dimension.min_y);
        if y < 0 || y >= self.dimension.height {
            return None;
        }
        self.get_chunk(pos.x >> 4, pos.z >> 4)?
            .section((y >> 4) as usize)
    }

    /// `Level.getBlockState(BlockPos)`: air above and below the world and in unloaded chunks.
    /// The client returns void air there, which has the same traits.
    pub fn get_block_state(&self, pos: BetterBlockPos) -> &BlockState {
        match self.section_at(pos) {
            Some(section) => self.table.get(section.get(pos.x, pos.y, pos.z)),
            None => self.table.air(),
        }
    }

    /// `Level.getFluidState(BlockPos)`: empty above and below the world and in unloaded chunks.
    pub fn get_fluid_state(&self, pos: BetterBlockPos) -> FluidState {
        match self.section_at(pos) {
            Some(section) => section.fluid(&self.table, pos.x, pos.y, pos.z),
            None => FluidState::EMPTY,
        }
    }

    /// `Level.clip(new ClipContext(from, to, ClipContext.Block.OUTLINE, ClipContext.Fluid.NONE,
    /// entity))`, the only raytrace ported code runs. Outline shapes are the host's, computed
    /// without an entity (see `docs/trait-mapping.md`).
    pub fn clip(&self, from: Vec3, to: Vec3) -> BlockHitResult {
        block_getter::traverse_blocks(
            from,
            to,
            |pos| {
                let block_state = self.get_block_state(pos);
                // Fluid.NONE: the fluid shape is empty and never hits
                self.clip_with_interaction_override(
                    from,
                    to,
                    pos,
                    block_state.get_shape(pos),
                    block_state,
                )
            },
            || {
                let delta = from.subtract_vec(to);
                BlockHitResult::miss(
                    to,
                    Direction::get_approximate_nearest(delta.x, delta.y, delta.z),
                    BetterBlockPos::from_f64(to.x, to.y, to.z),
                )
            },
        )
    }

    /// `BlockGetter.clipWithInteractionOverride`
    fn clip_with_interaction_override(
        &self,
        from: Vec3,
        to: Vec3,
        pos: BetterBlockPos,
        block_shape: VoxelShape<'_>,
        block_state: &BlockState,
    ) -> Option<BlockHitResult> {
        let result = block_shape.clip(from, to, pos)?;
        if let Some(hit_override) = block_state.get_interaction_shape(pos).clip(from, to, pos)
            && hit_override.get_location().subtract_vec(from).length_sqr()
                < result.get_location().subtract_vec(from).length_sqr()
        {
            return Some(result.with_direction(hit_override.get_direction()));
        }
        Some(result)
    }

    fn check_ids(&self, ids: &[u32]) -> Result<(), WorldError> {
        match ids.iter().find(|&&id| self.table.try_get(id).is_none()) {
            Some(&id) => Err(WorldError::UnknownState(id)),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::host::BlockState;

    /// air (0), stone (1), dirt (2)
    pub(crate) fn test_table() -> Arc<BlockStateTable> {
        let state = |name: &str, air: bool| BlockState {
            name: name.to_owned(),
            air,
            ..BlockState::default()
        };
        Arc::new(
            BlockStateTable::new(
                vec![
                    state("minecraft:air", true),
                    state("minecraft:stone", false),
                    state("minecraft:dirt", false),
                ],
                0,
            )
            .unwrap(),
        )
    }

    fn small() -> DimensionType {
        DimensionType {
            min_y: -16,
            height: 32,
            water_evaporates: false,
        }
    }

    fn block(world: &World, x: i32, y: i32, z: i32) -> Option<u32> {
        let chunk = world.get_chunk(x >> 4, z >> 4)?;
        let y = y - world.dimension().min_y;
        Some(
            chunk
                .section((y >> 4) as usize)
                .map_or(0, |s| s.get(x, y, z)),
        )
    }

    #[test]
    fn chunk_key_matches_java() {
        assert_eq!(chunk_key(0, 0), 0);
        assert_eq!(chunk_key(1, 0), 1);
        assert_eq!(chunk_key(0, 1), 1 << 32);
        assert_eq!(chunk_key(-1, 0), 0xffff_ffff);
        assert_eq!(chunk_key(-1, -1), -1);
        assert_eq!(chunk_key(i32::MIN, 5), 0x0000_0005_8000_0000);
    }

    #[test]
    fn dimension_validation() {
        let table = test_table();
        assert!(World::new(table.clone(), small()).is_ok());
        for (min_y, height) in [(-15, 32), (0, 0), (0, 17), (i32::MAX - 15, 32)] {
            let dim = DimensionType {
                min_y,
                height,
                water_evaporates: false,
            };
            assert_eq!(
                World::new(table.clone(), dim).unwrap_err(),
                WorldError::Dimension { min_y, height }
            );
        }
    }

    #[test]
    fn set_block_and_bounds() {
        let mut world = World::new(test_table(), small()).unwrap();
        assert_eq!(world.set_block(3, 0, 3, 1), Ok(false), "not loaded");
        world.load_chunk(0, 0, Chunk::new(2)).unwrap();
        assert_eq!(world.set_block(3, 0, 3, 1), Ok(true));
        assert_eq!(world.set_block(3, -16, 3, 2), Ok(true));
        assert_eq!(world.set_block(3, 15, 3, 2), Ok(true));
        assert_eq!(world.set_block(3, 16, 3, 1), Ok(false), "above the world");
        assert_eq!(world.set_block(3, -17, 3, 1), Ok(false), "below the world");
        assert_eq!(
            world.set_block(3, 0, 3, 3),
            Err(WorldError::UnknownState(3))
        );
        assert_eq!(block(&world, 3, 0, 3), Some(1));
        assert_eq!(block(&world, 3, -16, 3), Some(2));
        assert_eq!(block(&world, 3, 15, 3), Some(2));
        assert_eq!(block(&world, 4, 0, 3), Some(0));
        assert_eq!(block(&world, 16, 0, 3), None);

        // setting air into an empty section does not create it
        world.set_block(0, 0, 0, 0).unwrap();
        world.load_chunk(1, 0, Chunk::new(2)).unwrap();
        world.set_block(16, 0, 0, 0).unwrap();
        assert!(world.get_chunk(1, 0).unwrap().section(1).is_none());
    }

    #[test]
    fn load_validation() {
        let mut world = World::new(test_table(), small()).unwrap();
        assert_eq!(
            world.load_chunk(0, 0, Chunk::new(3)),
            Err(WorldError::SectionCount {
                expected: 2,
                actual: 3
            })
        );
        let mut chunk = Chunk::new(2);
        chunk.set_section(0, Some(SubChunk::filled(9)));
        assert_eq!(
            world.load_chunk(0, 0, chunk),
            Err(WorldError::UnknownState(9))
        );
        assert!(!world.has_chunk(0, 0));

        let mut chunk = Chunk::new(2);
        chunk.set_section(1, Some(SubChunk::filled(2)));
        world.load_chunk(-1, 2, chunk).unwrap();
        assert_eq!(world.loaded_chunks().collect::<Vec<_>>(), [(-1, 2)]);
        assert_eq!(block(&world, -16, 5, 32), Some(2));
        assert_eq!(block(&world, -16, -5, 32), Some(0));
        assert!(world.unload_chunk(-1, 2));
        assert!(!world.unload_chunk(-1, 2));
    }

    #[test]
    fn snapshots_are_isolated() {
        let mut world = Arc::new(World::new(test_table(), small()).unwrap());
        {
            let w = Arc::make_mut(&mut world);
            w.load_chunk(0, 0, Chunk::new(2)).unwrap();
            w.load_chunk(1, 0, Chunk::new(2)).unwrap();
            w.set_block(1, 1, 1, 1).unwrap();
            w.set_block(17, 1, 1, 1).unwrap();
        }
        let snapshot = Arc::clone(&world);

        let w = Arc::make_mut(&mut world);
        w.set_block(1, 1, 1, 2).unwrap();
        w.unload_chunk(1, 0);
        w.set_border(WorldBorder {
            min_x: 0.0,
            max_x: 1.0,
            min_z: 0.0,
            max_z: 1.0,
        });

        assert_eq!(block(&snapshot, 1, 1, 1), Some(1));
        assert_eq!(block(&snapshot, 17, 1, 1), Some(1));
        assert_eq!(snapshot.border(), WorldBorder::default());
        assert_eq!(block(&world, 1, 1, 1), Some(2));
        assert_eq!(block(&world, 17, 1, 1), None);

        // an update copies the section it touches, and shares the others
        Arc::make_mut(&mut world).set_block(1, -10, 1, 2).unwrap();
        let snapshot = Arc::clone(&world);
        Arc::make_mut(&mut world).set_block(1, 1, 1, 1).unwrap();
        let section = |w: &World, i: usize| {
            Arc::clone(w.get_chunk(0, 0).unwrap().sections[i].as_ref().unwrap())
        };
        assert!(Arc::ptr_eq(&section(&snapshot, 0), &section(&world, 0)));
        assert!(!Arc::ptr_eq(&section(&snapshot, 1), &section(&world, 1)));
        assert_eq!(block(&snapshot, 1, 1, 1), Some(2));
        assert_eq!(block(&world, 1, 1, 1), Some(1));
    }

    pub(crate) const WATER: FluidState = FluidState {
        fluid: crate::host::Fluid::Water,
        source: true,
        amount: 8,
    };

    /// air (0), stone (1), an oak fence that can hold water (2), water (3), flowing water (4),
    /// an oak fence holding its own water, as on Java (5)
    pub(crate) fn fluid_table() -> Arc<BlockStateTable> {
        use crate::host::Waterlogged;
        use crate::pathing::precompute::Ternary;

        let water = |amount: u8| BlockState {
            name: "minecraft:water".to_owned(),
            can_walk_on: Ternary::Maybe,
            can_walk_through: if amount == 8 {
                Ternary::Maybe
            } else {
                Ternary::No
            },
            fluid: WATER.fluid,
            fluid_source: amount == 8,
            fluid_amount: amount,
            liquid_block: true,
            liquid: true,
            ..BlockState::default()
        };
        let fence = BlockState {
            name: "minecraft:oak_fence".to_owned(),
            waterlogged: Some(Waterlogged {
                can_walk_on: Ternary::Maybe,
                can_walk_through: Ternary::Maybe,
                fully_passable: Ternary::No,
            }),
            ..BlockState::default()
        };
        let states = vec![
            BlockState {
                name: "minecraft:air".to_owned(),
                air: true,
                can_walk_through: Ternary::Yes,
                fully_passable: Ternary::Yes,
                ..BlockState::default()
            },
            BlockState {
                name: "minecraft:stone".to_owned(),
                can_walk_on: Ternary::Yes,
                ..BlockState::default()
            },
            fence.clone(),
            water(8),
            water(7),
            BlockState {
                can_walk_on: Ternary::Maybe,
                can_walk_through: Ternary::Maybe,
                waterlogged: None,
                fluid: WATER.fluid,
                fluid_source: true,
                fluid_amount: 8,
                ..fence
            },
        ];
        Arc::new(BlockStateTable::new(states, 0).unwrap())
    }

    #[test]
    fn fluid_state_of_a_state() {
        let mut world = World::new(fluid_table(), small()).unwrap();
        world.load_chunk(0, 0, Chunk::new(2)).unwrap();
        world.set_block(0, 0, 0, 3).unwrap();
        world.set_block(1, 0, 0, 5).unwrap();
        world.set_block(2, 0, 0, 2).unwrap();
        world.set_block(3, 0, 0, 4).unwrap();
        let fluid = |x| world.get_fluid_state(BetterBlockPos::new(x, 0, 0));
        assert_eq!(fluid(0), WATER);
        assert_eq!(fluid(1), WATER, "waterlogged in its state");
        assert_eq!(fluid(2), FluidState::EMPTY);
        assert!(fluid(3).possibly_flowing());
        assert_eq!(fluid(4), FluidState::EMPTY);
        assert_eq!(
            world.get_fluid_state(BetterBlockPos::new(16, 0, 0)),
            FluidState::EMPTY,
            "not loaded"
        );
        assert_eq!(
            world.get_fluid_state(BetterBlockPos::new(0, 16, 0)),
            FluidState::EMPTY,
            "above the world"
        );
    }

    #[cfg(feature = "bedrock")]
    #[test]
    fn liquid_layer() {
        let mut world = World::new(fluid_table(), small()).unwrap();
        world.load_chunk(0, 0, Chunk::new(2)).unwrap();
        world.set_block(1, 0, 0, 2).unwrap();
        world.set_block(2, 0, 0, 2).unwrap();
        assert_eq!(world.set_liquid(1, 0, 0, 3), Ok(true));
        assert_eq!(world.set_liquid(16, 0, 0, 3), Ok(false), "not loaded");
        assert_eq!(
            world.set_liquid(1, 0, 0, 9),
            Err(WorldError::UnknownState(9))
        );
        let fluid = |w: &World, x| w.get_fluid_state(BetterBlockPos::new(x, 0, 0));
        assert_eq!(fluid(&world, 1), WATER, "waterlogged by the liquid layer");
        assert_eq!(world.get_block_state(BetterBlockPos::new(1, 0, 0)).id, 2);
        assert_eq!(fluid(&world, 2), FluidState::EMPTY);

        // air in the liquid layer of an empty section creates neither
        world.set_liquid(0, -16, 0, 0).unwrap();
        assert!(world.get_chunk(0, 0).unwrap().section(0).is_none());
        // a liquid in an empty section fills its block layer with air
        world.set_liquid(0, -16, 0, 3).unwrap();
        let section = world.get_chunk(0, 0).unwrap().section(0).unwrap();
        assert_eq!(section.get(0, 0, 0), 0);
        assert_eq!(section.get_liquid(0, 0, 0), Some(3));
        assert_eq!(section.get_liquid(1, 0, 0), Some(0));

        // the layer is copied on write like the blocks
        let snapshot = Arc::new(world.clone());
        world.set_liquid(1, 0, 0, 0).unwrap();
        assert_eq!(fluid(&snapshot, 1), WATER);
        assert_eq!(fluid(&world, 1), FluidState::EMPTY);
    }

    #[cfg(feature = "bedrock")]
    #[test]
    fn load_checks_the_liquid_layer() {
        let mut world = World::new(fluid_table(), small()).unwrap();
        let mut chunk = Chunk::new(2);
        chunk.set_section(
            0,
            Some(SubChunk::from_layers(
                PalettedStorage::single(2),
                Some(PalettedStorage::single(9)),
            )),
        );
        assert_eq!(
            world.load_chunk(0, 0, chunk),
            Err(WorldError::UnknownState(9))
        );
        let mut chunk = Chunk::new(2);
        chunk.set_section(
            0,
            Some(SubChunk::from_layers(
                PalettedStorage::single(2),
                Some(PalettedStorage::single(3)),
            )),
        );
        world.load_chunk(0, 0, chunk).unwrap();
        assert_eq!(world.get_fluid_state(BetterBlockPos::new(5, -10, 5)), WATER);
    }
}
