// Ported from baritone src/main/java/baritone/cache/FasterWorldScanner.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The world scanner upstream's provider hands out (`BaritoneAPI.getProvider().getWorldScanner()`),
// over the host's loaded chunks. Upstream reads the `IPlayerContext` for the world and the
// player's feet only; they are passed in, so a scan can run on another thread over a world
// snapshot. `repack` is not ported (there is no chunk cache), and neither is the
// `IWorldScanner` interface, which has no other implementation here.
//
// Chunks are scanned in `getChunkRange`'s order, the order upstream's parallel stream keeps.
// A section whose storage holds a single value is Java's `SingleValuePalette`, scanned x, y,
// z; any other is scanned in Java's storage order (y, z, x), whatever the host's storage order.
// A missing section, or a single-valued air one, has only air (`hasOnlyAir()`); a packed
// section of nothing but air is scanned like any other. The registry-wide filter upstream
// builds for a `GlobalPalette` finds the same blocks as the palette filter, so it is not
// ported. A player below the world scans a section with a negative index, which panics like
// upstream's `ArrayIndexOutOfBoundsException`.

use crate::api::utils::{BetterBlockPos, BlockOptionalMetaLookup};
use crate::host::paletted;
use crate::host::{BlockStateTable, Chunk, SubChunk, World};

/// `scanChunkRadius(IPlayerContext, BlockOptionalMetaLookup, int, int, int)`: up to `max`
/// matching blocks (all of them if `max` is negative) in the loaded chunks within
/// `max_search_radius` chunks, nearest chunks first. `y_level_threshold` is not used.
pub fn scan_chunk_radius(
    world: &World,
    player_feet: BetterBlockPos,
    filter: &BlockOptionalMetaLookup,
    max: i32,
    _y_level_threshold: i32,
    max_search_radius: i32,
) -> Vec<BetterBlockPos> {
    if max_search_radius < 0 {
        panic!("chunkRange must be >= 0");
    }
    scan_chunks_internal(
        world,
        player_feet,
        filter,
        &get_chunk_range(player_feet.x >> 4, player_feet.z >> 4, max_search_radius),
        max,
    )
}

/// `scanChunk(IPlayerContext, BlockOptionalMetaLookup, ChunkPos, int, int)`
pub fn scan_chunk(
    world: &World,
    player_feet: BetterBlockPos,
    filter: &BlockOptionalMetaLookup,
    pos: (i32, i32),
    max: i32,
    _y_level_threshold: i32,
) -> Vec<BetterBlockPos> {
    let mut blocks = scan_chunk_internal(world, player_feet, filter, pos);
    if let Ok(max) = usize::try_from(max) {
        blocks.truncate(max);
    }
    blocks
}

/// ordered in a way that the closest blocks are generally first
pub fn get_chunk_range(center_x: i32, center_z: i32, chunk_radius: i32) -> Vec<(i32, i32)> {
    let mut chunks = Vec::new();
    // spiral out
    chunks.push((center_x, center_z));
    for i in 1..chunk_radius {
        for j in 0..=i {
            chunks.push((center_x - j, center_z - i));
            if j != 0 {
                chunks.push((center_x + j, center_z - i));
                chunks.push((center_x - j, center_z + i));
            }
            chunks.push((center_x + j, center_z + i));
            if j != i {
                chunks.push((center_x - i, center_z - j));
                chunks.push((center_x + i, center_z - j));
                if j != 0 {
                    chunks.push((center_x - i, center_z + j));
                    chunks.push((center_x + i, center_z + j));
                }
            }
        }
    }
    chunks
}

fn scan_chunks_internal(
    world: &World,
    player_feet: BetterBlockPos,
    lookup: &BlockOptionalMetaLookup,
    chunk_positions: &[(i32, i32)],
    max_blocks: i32,
) -> Vec<BetterBlockPos> {
    let limit = usize::try_from(max_blocks).ok();
    let mut blocks = Vec::new();
    for &pos in chunk_positions {
        if limit.is_some_and(|limit| blocks.len() >= limit) {
            break;
        }
        blocks.extend(scan_chunk_internal(world, player_feet, lookup, pos));
    }
    if let Some(limit) = limit {
        // WARNING: this can be expensive if maxBlocks is large...
        blocks.truncate(limit);
    }
    blocks
}

fn scan_chunk_internal(
    world: &World,
    player_feet: BetterBlockPos,
    lookup: &BlockOptionalMetaLookup,
    (x, z): (i32, i32),
) -> Vec<BetterBlockPos> {
    // if chunk is not loaded, return empty stream
    let Some(chunk) = world.get_chunk(x, z) else {
        return Vec::new();
    };

    let chunk_x = x.wrapping_shl(4);
    let chunk_z = z.wrapping_shl(4);

    let min_y = world.dimension().min_y;
    let player_section_y = player_feet.y.wrapping_sub(min_y) >> 4;

    collect_chunk_sections(
        world.table(),
        lookup,
        chunk,
        chunk_x,
        chunk_z,
        min_y,
        player_section_y,
    )
}

fn collect_chunk_sections(
    table: &BlockStateTable,
    lookup: &BlockOptionalMetaLookup,
    chunk: &Chunk,
    chunk_x: i32,
    chunk_z: i32,
    chunk_y: i32,
    player_section: i32,
) -> Vec<BetterBlockPos> {
    // iterate over sections relative to player
    let mut blocks = Vec::new();
    let l = chunk.section_count() as i32;
    let mut i = player_section - 1;
    let mut j = player_section;
    while i >= 0 || j < l {
        if j < l {
            visit_section(
                table,
                lookup,
                chunk.section(j as usize),
                &mut blocks,
                chunk_x,
                chunk_y + j * 16,
                chunk_z,
            );
        }
        if i >= 0 {
            visit_section(
                table,
                lookup,
                chunk.section(i as usize),
                &mut blocks,
                chunk_x,
                chunk_y + i * 16,
                chunk_z,
            );
        }
        j += 1;
        i -= 1;
    }
    blocks
}

fn visit_section(
    table: &BlockStateTable,
    lookup: &BlockOptionalMetaLookup,
    section: Option<&SubChunk>,
    blocks: &mut Vec<BetterBlockPos>,
    chunk_x: i32,
    section_y: i32,
    chunk_z: i32,
) {
    let Some(section) = section else {
        return;
    };
    let storage = section.storage();

    if storage.is_single_value() {
        let state = table.get(storage.palette()[0]);
        if state.air {
            // hasOnlyAir()
            return;
        }
        // single value palette doesn't have any data
        if lookup.has(state) {
            // TODO this is 4k hits, maybe don't return all of them?
            for x in 0..16 {
                for y in 0..16 {
                    for z in 0..16 {
                        blocks.push(BetterBlockPos::new(chunk_x + x, section_y + y, chunk_z + z));
                    }
                }
            }
        }
        return;
    }

    let is_in_filter = get_included_filter_indices(table, lookup, storage.palette());
    if is_in_filter.is_empty() {
        return;
    }

    for idx in 0..paletted::SECTION_VOLUME {
        let x = idx & 15;
        let y = idx >> 8;
        let z = (idx & 255) >> 4;
        let value = storage.palette_index(paletted::index(x, y, z));
        if is_in_filter[value as usize] {
            blocks.push(BetterBlockPos::new(
                chunk_x + x as i32,
                section_y + y as i32,
                chunk_z + z as i32,
            ));
        }
    }
}

/// For each palette entry, whether the lookup has it; empty if it has none.
fn get_included_filter_indices(
    table: &BlockStateTable,
    lookup: &BlockOptionalMetaLookup,
    palette: &[u32],
) -> Vec<bool> {
    let is_in_filter: Vec<bool> = palette
        .iter()
        .map(|&id| lookup.has(table.get(id)))
        .collect();
    if !is_in_filter.contains(&true) {
        return Vec::new();
    }
    is_in_filter
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::host::{BlockState, DimensionType, PalettedStorage};

    fn table() -> Arc<BlockStateTable> {
        let state = |name: &str| BlockState {
            name: name.to_owned(),
            air: name == "minecraft:air",
            ..BlockState::default()
        };
        Arc::new(
            BlockStateTable::new(
                vec![
                    state("minecraft:air"),
                    state("minecraft:stone"),
                    state("minecraft:gold_ore"),
                ],
                0,
            )
            .unwrap(),
        )
    }

    fn world() -> World {
        let dimension = DimensionType {
            min_y: -16,
            height: 64,
            water_evaporates: false,
        };
        let mut world = World::new(table(), dimension).unwrap();
        for cx in -2..=2 {
            for cz in -2..=2 {
                world.load_chunk(cx, cz, Chunk::new(4)).unwrap();
            }
        }
        world
    }

    #[test]
    fn chunk_range_spirals_out() {
        assert_eq!(get_chunk_range(0, 0, 0), [(0, 0)]);
        assert_eq!(get_chunk_range(0, 0, 1), [(0, 0)]);
        assert_eq!(
            get_chunk_range(5, 5, 2),
            [
                (5, 5),
                (5, 4),
                (5, 6),
                (4, 5),
                (6, 5),
                (4, 4),
                (6, 4),
                (4, 6),
                (6, 6)
            ]
        );
        let range = get_chunk_range(0, 0, 4);
        assert_eq!(range.len(), 49);
        let mut sorted = range.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 49);
    }

    #[test]
    fn scan_order() {
        let mut world = world();
        let gold = 2;
        // packed sections: y, z, x order; sections nearest the player's first
        world.set_block(3, 20, 1, gold).unwrap();
        world.set_block(1, 20, 3, gold).unwrap();
        world.set_block(2, 5, 2, gold).unwrap();
        world.set_block(2, -10, 2, gold).unwrap();
        // the next chunk in the spiral
        world.set_block(4, 5, -14, gold).unwrap();
        let lookup = BlockOptionalMetaLookup::from_blocks(world.table(), [world.table().get(2)]);
        let feet = BetterBlockPos::new(8, 6, 8);
        let found = scan_chunk_radius(&world, feet, &lookup, -1, 10, 32);
        // the player's section, the one below, the one above
        assert_eq!(
            found,
            [
                BetterBlockPos::new(2, 5, 2),
                BetterBlockPos::new(2, -10, 2),
                BetterBlockPos::new(3, 20, 1),
                BetterBlockPos::new(1, 20, 3),
                BetterBlockPos::new(4, 5, -14),
            ]
        );
        assert_eq!(
            scan_chunk_radius(&world, feet, &lookup, 2, 10, 32),
            found[..2]
        );
        assert_eq!(
            scan_chunk(&world, feet, &lookup, (0, -1), -1, 10),
            [BetterBlockPos::new(4, 5, -14)]
        );
        // radius 1 is only the player's chunk
        assert_eq!(scan_chunk_radius(&world, feet, &lookup, -1, 10, 1).len(), 4);
        assert!(scan_chunk(&world, feet, &lookup, (9, 9), -1, 10).is_empty());
    }

    #[test]
    fn single_value_sections() {
        let mut world = world();
        let mut section = SubChunk::from_storage(PalettedStorage::single(2));
        assert!(section.storage().is_single_value());
        world.set_section(0, 0, 0, Some(section.clone())).unwrap();
        let lookup = BlockOptionalMetaLookup::from_blocks(world.table(), [world.table().get(2)]);
        let feet = BetterBlockPos::new(8, 0, 8);
        let found = scan_chunk(&world, feet, &lookup, (0, 0), -1, 10);
        assert_eq!(found.len(), 4096);
        // x, y, z order
        assert_eq!(found[1], BetterBlockPos::new(0, 0, 1));
        assert_eq!(found[16], BetterBlockPos::new(0, 1, 0));
        assert_eq!(found[256], BetterBlockPos::new(1, 0, 0));

        // a single-valued air section has only air, even for a lookup that has air
        section = SubChunk::from_storage(PalettedStorage::single(0));
        world.set_section(0, 0, 0, Some(section)).unwrap();
        let air = BlockOptionalMetaLookup::from_blocks(world.table(), [world.table().get(0)]);
        assert!(scan_chunk(&world, feet, &air, (0, 0), -1, 10).is_empty());
    }

    #[test]
    #[should_panic(expected = "chunkRange must be >= 0")]
    fn negative_radius() {
        let world = world();
        let lookup = BlockOptionalMetaLookup::new(Vec::new());
        scan_chunk_radius(&world, BetterBlockPos::ORIGIN, &lookup, 1, 1, -1);
    }
}
