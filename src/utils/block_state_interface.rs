// Ported from baritone src/main/java/baritone/utils/BlockStateInterface.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The world is an immutable snapshot (`Arc<World>`), which replaces the `copyLoadedChunks`
// constructor (`createThreadSafeCopy`). The chunk cache (`CachedRegion`, `WorldData`) is not
// ported: where upstream falls back to it, the port behaves like upstream without world data
// (air, not loaded). Not needed: `access` and `isPassableBlockPos`, which exist to call
// Minecraft APIs; block shapes come from the host table.
//
// The interface also holds the settings active when it was created, which the movement
// helpers that take it read instead of `Baritone.settings()`: a calculation sees one set of
// settings throughout, where upstream would see a change made while it runs.

use std::cell::Cell;
use std::ptr::{self, NonNull};
use std::sync::Arc;

use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::host::{BlockState, BlockStateTable, Chunk, SubChunk, World};
use crate::settings::{Settings, settings, settings_snapshot};
use crate::utils::pathing::BetterWorldBorder;

/// Wraps get for chuck caching capability
///
/// Owned by one thread at a time (it is `Send`, not `Sync`), like upstream's.
pub struct BlockStateInterface {
    world: Arc<World>,
    pub world_border: BetterWorldBorder,
    /// Chunk X, chunk Z and the chunk of the last lookup. The chunk is owned by `world`.
    prev: Cell<Option<(i32, i32, NonNull<Chunk>)>>,
    /// The section of the last lookup, in a loaded chunk and inside the world.
    prev_section: Cell<PrevSection>,
    use_the_real_world: bool,
    settings: Arc<Settings>,
}

/// Section coordinates (Y counted from the bottom of the world) and the section, owned by the
/// interface's `world`; null for an empty section.
#[derive(Clone, Copy)]
struct PrevSection {
    x: i32,
    y: i32,
    z: i32,
    section: *const SubChunk,
}

/// No section yet: `i32::MIN` is below any block coordinate `>> 4`.
const NO_SECTION: PrevSection = PrevSection {
    x: i32::MIN,
    y: i32::MIN,
    z: i32::MIN,
    section: ptr::null(),
};

// SAFETY: `prev` and `prev_section` only point into chunks owned by `self.world`, an immutable
// snapshot that lives as long as `self` (a `World` behind a shared `Arc` is never mutated:
// `Arc::make_mut` clones it). `Chunk` and `SubChunk` are `Send + Sync`, so moving the pointers
// with `self` is sound; `Cell` keeps the type `!Sync`.
unsafe impl Send for BlockStateInterface {}

impl Clone for BlockStateInterface {
    /// Another interface over the same snapshot and settings, with its own lookup cache.
    fn clone(&self) -> Self {
        Self {
            world: Arc::clone(&self.world),
            world_border: self.world_border,
            prev: Cell::new(None),
            prev_section: Cell::new(NO_SECTION),
            use_the_real_world: self.use_the_real_world,
            settings: Arc::clone(&self.settings),
        }
    }
}

impl BlockStateInterface {
    /// `new BlockStateInterface(ctx, true)`: reads from a snapshot of the world.
    pub fn new(world: Arc<World>) -> Self {
        let settings = settings_snapshot();
        Self {
            world_border: BetterWorldBorder::new(&world.border()),
            world,
            prev: Cell::new(None),
            prev_section: Cell::new(NO_SECTION),
            use_the_real_world: !settings.path_through_cached_only,
            settings,
        }
    }

    /// The settings active when this interface was created.
    #[inline]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// [`Self::settings`], shared.
    pub fn settings_arc(&self) -> &Arc<Settings> {
        &self.settings
    }

    /// `new BlockStateInterface(IPlayerContext)`: reads the context's current world.
    pub fn from_ctx(ctx: &dyn IPlayerContext) -> Self {
        Self::new(Arc::clone(ctx.world()))
    }

    /// `get(IPlayerContext, BlockPos)`: `new BlockStateInterface(ctx).get0(pos)`, the context's
    /// current world (air above and below it and in unloaded chunks).
    pub fn get(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> &BlockState {
        if settings().path_through_cached_only {
            // get0 skips the real world and asks the chunk cache, which is not ported
            return ctx.world().table().air();
        }
        ctx.world().get_block_state(pos)
    }

    /// `getBlock(IPlayerContext, BlockPos)`: the state stands for its block.
    pub fn get_block(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> &BlockState {
        Self::get(ctx, pos)
    }

    /// The snapshot this reads from.
    pub fn world(&self) -> &Arc<World> {
        &self.world
    }

    /// The block state table the snapshot's ids refer to.
    #[inline]
    pub fn table(&self) -> &BlockStateTable {
        self.world.table()
    }

    pub fn world_contains_loaded_chunk(&self, block_x: i32, block_z: i32) -> bool {
        self.world.has_chunk(block_x >> 4, block_z >> 4)
    }

    /// `get0(BlockPos)`
    pub fn get0_pos(&self, pos: BetterBlockPos) -> &BlockState {
        self.get0(pos.x, pos.y, pos.z)
    }

    #[inline]
    pub fn get0(&self, x: i32, y: i32, z: i32) -> &BlockState {
        // Mickey resigned
        let dimension = self.world.dimension();
        let y = y.wrapping_sub(dimension.min_y);
        // Not in upstream: the same section as last time is inside the world and loaded, so
        // it skips the checks and the chunk below
        let prev = self.prev_section.get();
        if prev.x == x >> 4 && prev.y == y >> 4 && prev.z == z >> 4 {
            return self.get_from_section(prev.section, x, y, z);
        }
        self.get0_other_section(x, y, z)
    }

    /// [`Self::get0`] outside the last section; `y` is relative to the bottom of the world.
    #[inline(never)]
    fn get0_other_section(&self, x: i32, y: i32, z: i32) -> &BlockState {
        let dimension = self.world.dimension();
        // Invalid vertical position
        if y < 0 || y >= dimension.height {
            return self.table().air();
        }

        if self.use_the_real_world {
            // there's great cache locality in block state lookups
            // generally it's within each movement
            // if it's the same chunk as last time
            // we can just skip the mc.world.getChunk lookup
            // which is a Long2ObjectOpenHashMap.get
            // see issue #113
            let chunk = if let Some((chunk_x, chunk_z, cached)) = self.prev.get()
                && chunk_x == x >> 4
                && chunk_z == z >> 4
            {
                // SAFETY: see `unsafe impl Send`; the chunk lives as long as `self.world`.
                unsafe { cached.as_ref() }
            } else if let Some(chunk) = self.world.get_chunk(x >> 4, z >> 4) {
                self.prev
                    .set(Some((x >> 4, z >> 4, NonNull::from(&**chunk))));
                chunk
            } else {
                // upstream falls back to the chunk cache (CachedRegion) here, which is not ported
                return self.table().air();
            };
            let section = chunk
                .section((y >> 4) as usize)
                .map_or(ptr::null(), ptr::from_ref);
            self.prev_section.set(PrevSection {
                x: x >> 4,
                y: y >> 4,
                z: z >> 4,
                section,
            });
            return self.get_from_section(section, x, y, z);
        }
        // upstream falls back to the chunk cache (CachedRegion) here, which is not ported
        self.table().air()
    }

    /// [`Self::get_from_chunk`] with the chunk's section at `y`, null for an empty one.
    #[inline]
    fn get_from_section(&self, section: *const SubChunk, x: i32, y: i32, z: i32) -> &BlockState {
        // SAFETY: see `unsafe impl Send`; the section lives as long as `self.world`.
        match unsafe { section.as_ref() } {
            None => self.table().air(),
            Some(section) => self.table().get(section.get(x, y, z)),
        }
    }

    pub fn is_loaded(&self, x: i32, z: i32) -> bool {
        if let Some((chunk_x, chunk_z, _)) = self.prev.get()
            && chunk_x == x >> 4
            && chunk_z == z >> 4
        {
            return true;
        }
        if let Some(chunk) = self.world.get_chunk(x >> 4, z >> 4) {
            self.prev
                .set(Some((x >> 4, z >> 4, NonNull::from(&**chunk))));
            return true;
        }
        // upstream asks the chunk cache (CachedRegion) here, which is not ported
        false
    }

    /// get the block at x,y,z from this chunk WITHOUT creating a single blockpos object
    ///
    /// `y` is relative to the bottom of the world. Upstream this is static; the port needs the
    /// table to turn the id into a state. Upstream also returns `AIR` for a section that holds
    /// only air variants (`hasOnlyAir()`); the port returns the stored variant (`cave_air`,
    /// `void_air`), which has the same traits and differs only to checks by name.
    #[inline]
    pub fn get_from_chunk(&self, chunk: &Chunk, x: i32, y: i32, z: i32) -> &BlockState {
        match chunk.section((y >> 4) as usize) {
            None => self.table().air(),
            Some(section) => self.table().get(section.get(x, y, z)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::world::tests::test_table;
    use crate::host::{DimensionType, WorldBorder};

    const AIR: u32 = 0;
    const STONE: u32 = 1;
    const DIRT: u32 = 2;

    fn world() -> Arc<World> {
        let mut world = World::new(
            test_table(),
            DimensionType {
                min_y: -16,
                height: 32,
                water_evaporates: false,
            },
        )
        .unwrap();
        for (cx, cz) in [(0, 0), (-1, 0), (0, -1)] {
            world.load_chunk(cx, cz, Chunk::new(2)).unwrap();
        }
        world.set_block(0, 0, 0, STONE).unwrap();
        world.set_block(-1, -16, 0, DIRT).unwrap();
        world.set_block(5, 15, -3, DIRT).unwrap();
        world.set_border(WorldBorder {
            min_x: -10.0,
            max_x: 10.0,
            min_z: -10.0,
            max_z: 10.0,
        });
        Arc::new(world)
    }

    #[test]
    fn get0() {
        let bsi = BlockStateInterface::new(world());
        // alternate chunks so both the cached and the uncached path run
        for _ in 0..2 {
            assert_eq!(bsi.get0(0, 0, 0).id, STONE);
            assert_eq!(bsi.get0(-1, -16, 0).id, DIRT);
            assert_eq!(bsi.get0(5, 15, -3).id, DIRT);
            assert_eq!(bsi.get0(0, 1, 0).id, AIR);
            assert_eq!(bsi.get0_pos(BetterBlockPos::new(0, 0, 0)).id, STONE);
        }
        assert_eq!(bsi.get0(0, 16, 0).id, AIR, "above the world");
        assert_eq!(bsi.get0(0, -17, 0).id, AIR, "below the world");
        assert_eq!(bsi.get0(0, i32::MIN, 0).id, AIR);
        assert_eq!(bsi.get0(0, i32::MAX, 0).id, AIR);
        assert_eq!(bsi.get0(-1, 0, -1).id, AIR, "not loaded");
        assert_eq!(bsi.get0(0, 0, 0).id, STONE);
    }

    #[test]
    fn get0_section_cache() {
        let bsi = BlockStateInterface::new(world());
        assert_eq!(bsi.get0(0, 0, 0).id, STONE);
        assert_eq!(bsi.get0(15, 15, 15).id, AIR, "same section");
        assert_eq!(
            bsi.get0(0, 16, 0).id,
            AIR,
            "above the cached section and the world"
        );
        assert_eq!(
            bsi.get0(0, -1, 0).id,
            AIR,
            "empty section of the same chunk"
        );
        assert_eq!(bsi.get0(0, -16, 0).id, AIR, "empty section, cached");
        assert_eq!(
            bsi.get0(0, -17, 0).id,
            AIR,
            "below the cached section and the world"
        );
        assert_eq!(bsi.get0(0, 0, 0).id, STONE);
        assert_eq!(
            bsi.get0(16, 0, 0).id,
            AIR,
            "not loaded, next to the cached section"
        );
        assert_eq!(bsi.get0(15, 0, 0).id, AIR);
        assert_eq!(bsi.get0(-1, -16, 0).id, DIRT, "another chunk");
        assert_eq!(bsi.get0(0, 0, 0).id, STONE);
        assert_eq!(bsi.clone().get0(0, 0, 0).id, STONE);
    }

    #[test]
    fn loaded() {
        let bsi = BlockStateInterface::new(world());
        assert!(bsi.world_contains_loaded_chunk(15, 15));
        assert!(bsi.world_contains_loaded_chunk(-16, 0));
        assert!(!bsi.world_contains_loaded_chunk(-1, -1));
        assert!(!bsi.world_contains_loaded_chunk(16, 0));
        assert!(bsi.is_loaded(0, 0));
        assert!(bsi.is_loaded(3, 3), "same chunk as the last lookup");
        assert!(bsi.is_loaded(3, -3));
        assert!(!bsi.is_loaded(-1, -1));
        assert!(!bsi.is_loaded(16, 16));
    }

    #[test]
    fn border_from_snapshot() {
        let bsi = BlockStateInterface::new(world());
        assert!(bsi.world_border.can_place_at(0, 0));
        assert!(!bsi.world_border.can_place_at(9, 0));
    }

    #[test]
    fn keeps_its_snapshot() {
        let mut world = world();
        let bsi = BlockStateInterface::new(Arc::clone(&world));
        assert_eq!(bsi.get0(0, 0, 0).id, STONE);
        let w = Arc::make_mut(&mut world);
        w.set_block(0, 0, 0, DIRT).unwrap();
        w.unload_chunk(0, 0);
        drop(world);
        assert_eq!(bsi.get0(0, 0, 0).id, STONE);
        assert_eq!(bsi.get0(0, 0, 1).id, AIR);
        assert!(bsi.is_loaded(0, 0));
    }

    #[test]
    fn path_through_cached_only() {
        // what pathThroughCachedOnly sets; not via settings, other tests read them concurrently
        let mut bsi = BlockStateInterface::new(world());
        bsi.use_the_real_world = false;
        // no chunk cache: upstream without world data reads air everywhere
        assert_eq!(bsi.get0(0, 0, 0).id, AIR);
        assert!(bsi.is_loaded(0, 0));
    }

    #[test]
    fn is_send() {
        fn send<T: Send>() {}
        send::<BlockStateInterface>();
    }
}
