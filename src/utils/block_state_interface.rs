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
use std::ptr::NonNull;
use std::sync::Arc;

use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::host::{BlockState, BlockStateTable, Chunk, World};
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
    use_the_real_world: bool,
    settings: Arc<Settings>,
}

// SAFETY: `prev` only points into chunks owned by `self.world`, an immutable snapshot that
// lives as long as `self` (a `World` behind a shared `Arc` is never mutated: `Arc::make_mut`
// clones it). `Chunk` is `Send + Sync`, so moving the pointer with `self` is sound; `Cell`
// keeps the type `!Sync`.
unsafe impl Send for BlockStateInterface {}

impl Clone for BlockStateInterface {
    /// Another interface over the same snapshot and settings, with its own lookup cache.
    fn clone(&self) -> Self {
        Self {
            world: Arc::clone(&self.world),
            world_border: self.world_border,
            prev: Cell::new(None),
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

    pub fn get0(&self, x: i32, y: i32, z: i32) -> &BlockState {
        // Mickey resigned
        let dimension = self.world.dimension();
        let y = y.wrapping_sub(dimension.min_y);
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
            if let Some((chunk_x, chunk_z, cached)) = self.prev.get()
                && chunk_x == x >> 4
                && chunk_z == z >> 4
            {
                // SAFETY: see `unsafe impl Send`; the chunk lives as long as `self.world`.
                return self.get_from_chunk(unsafe { cached.as_ref() }, x, y, z);
            }
            if let Some(chunk) = self.world.get_chunk(x >> 4, z >> 4) {
                self.prev
                    .set(Some((x >> 4, z >> 4, NonNull::from(&**chunk))));
                return self.get_from_chunk(chunk, x, y, z);
            }
        }
        // upstream falls back to the chunk cache (CachedRegion) here, which is not ported
        self.table().air()
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
