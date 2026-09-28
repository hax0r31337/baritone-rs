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

use super::block_state::BlockStateTable;
use super::paletted::{self, PalettedStorage};

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
    /// A section index outside the dimension.
    Section(i32),
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
            WorldError::Section(y) => write!(f, "section {y} is outside the dimension"),
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
    /// Placed water evaporates (upstream checks for the Nether).
    pub water_evaporates: bool,
}

impl DimensionType {
    /// The Overworld: Y -64 to 319.
    pub const OVERWORLD: DimensionType = DimensionType {
        min_y: -64,
        height: 384,
        water_evaporates: false,
    };

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
/// Holds one resolved state per block. On Bedrock, the host (or the phase 6 ingestion code)
/// resolves layer 0 and the liquid layer into the state that describes both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubChunk {
    blocks: PalettedStorage,
}

impl SubChunk {
    /// Every block is `id`.
    pub fn filled(id: u32) -> Self {
        Self {
            blocks: PalettedStorage::single(id),
        }
    }

    pub fn from_storage(blocks: PalettedStorage) -> Self {
        Self { blocks }
    }

    /// The state at local coordinates (only the low 4 bits of each are used).
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> u32 {
        self.blocks.get(paletted::index(
            (x & 15) as usize,
            (y & 15) as usize,
            (z & 15) as usize,
        ))
    }

    /// Sets the state at local coordinates (only the low 4 bits of each are used).
    pub fn set(&mut self, x: i32, y: i32, z: i32, id: u32) {
        self.blocks.set(
            paletted::index((x & 15) as usize, (y & 15) as usize, (z & 15) as usize),
            id,
        );
    }

    pub fn storage(&self) -> &PalettedStorage {
        &self.blocks
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
    pub fn table(&self) -> &Arc<BlockStateTable> {
        &self.table
    }

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
        for section in chunk.sections.iter().flatten() {
            self.check_ids(section.storage().palette())?;
        }
        self.chunks
            .insert(chunk_key(chunk_x, chunk_z), Arc::new(chunk));
        Ok(())
    }

    /// Unloads a chunk. Returns whether it was loaded.
    pub fn unload_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunks.remove(&chunk_key(chunk_x, chunk_z)).is_some()
    }

    /// Replaces one section of a loaded chunk. `section_y` is a section coordinate (block Y
    /// `>> 4`). Returns `Ok(false)` if the chunk is not loaded.
    pub fn set_section(
        &mut self,
        chunk_x: i32,
        section_y: i32,
        chunk_z: i32,
        section: Option<SubChunk>,
    ) -> Result<bool, WorldError> {
        let index = section_y.wrapping_sub(self.dimension.min_y >> 4);
        if index < 0 || index as usize >= self.dimension.section_count() {
            return Err(WorldError::Section(section_y));
        }
        if let Some(section) = &section {
            self.check_ids(section.storage().palette())?;
        }
        let Some(chunk) = self.chunks.get_mut(&chunk_key(chunk_x, chunk_z)) else {
            return Ok(false);
        };
        Arc::make_mut(chunk).set_section(index as usize, section);
        Ok(true)
    }

    /// A block update. Returns `Ok(false)` if the chunk is not loaded or `y` is outside the
    /// dimension; the client ignores those updates too.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: u32) -> Result<bool, WorldError> {
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
        Arc::make_mut(section).set(x, y, z, id);
        Ok(true)
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
        assert!(World::new(table.clone(), DimensionType::OVERWORLD).is_ok());
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

        world.load_chunk(-1, 2, Chunk::new(2)).unwrap();
        assert_eq!(world.loaded_chunks().collect::<Vec<_>>(), [(-1, 2)]);
        assert_eq!(
            world.set_section(-1, -2, 2, None),
            Err(WorldError::Section(-2))
        );
        assert_eq!(
            world.set_section(-1, 1, 2, None),
            Err(WorldError::Section(1))
        );
        assert_eq!(
            world.set_section(-1, 0, 2, Some(SubChunk::filled(7))),
            Err(WorldError::UnknownState(7))
        );
        assert_eq!(
            world.set_section(-1, 0, 2, Some(SubChunk::filled(2))),
            Ok(true)
        );
        assert_eq!(block(&world, -16, 5, 32), Some(2));
        assert_eq!(block(&world, -16, -5, 32), Some(0));
        assert_eq!(world.set_section(5, 0, 5, None), Ok(false));
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
}
