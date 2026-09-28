// Ported from baritone src/main/java/baritone/cache/CachedWorld.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Only which chunks are cached (`isCached`, `regionLoaded`), which the explore process asks.
// The cache itself (packed chunks, block locations, region files, the packer thread) is not
// ported. A chunk counts as cached once Baritone has seen it loaded, which is when upstream
// packs it (`GameEventHandler.onChunkEvent`); it is cached at once instead of after the
// packer thread gets to it. Nothing is kept on disk, so every region is loaded, and a world
// change forgets everything (upstream reloads the dimension's regions from disk).

use rustc_hash::FxHashSet;

use crate::host::world::chunk_key;

#[derive(Clone, Debug, Default)]
pub struct CachedWorld {
    /// `chunk_key`s of the cached chunks.
    chunks: FxHashSet<i64>,
}

impl CachedWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_cached(&self, block_x: i32, block_z: i32) -> bool {
        self.chunks.contains(&chunk_key(block_x >> 4, block_z >> 4))
    }

    /// Always true: there are no regions to load from disk.
    pub fn region_loaded(&self, _block_x: i32, _block_z: i32) -> bool {
        true
    }

    /// `queueForPacking(LevelChunk)`: the chunk is cached from now on.
    pub fn queue_for_packing(&mut self, chunk_x: i32, chunk_z: i32) {
        self.chunks.insert(chunk_key(chunk_x, chunk_z));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_by_chunk() {
        let mut cache = CachedWorld::new();
        assert!(!cache.is_cached(0, 0));
        cache.queue_for_packing(-1, 2);
        assert!(cache.is_cached(-16, 32));
        assert!(cache.is_cached(-1, 47));
        assert!(!cache.is_cached(0, 47));
        assert!(!cache.is_cached(-1, 48));
        assert!(cache.region_loaded(123_456, -99));
    }
}
