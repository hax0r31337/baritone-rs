//! Processes end to end against the client simulation in `common::sim`, for what
//! `reference_execution.rs` cannot compare with upstream: item matching (upstream matches item
//! stacks through a mixin the reference generator does not have, and its loot roll fails in
//! 26.3, so no block drops anything there), blocks upstream finds in its chunk cache (which
//! the port does not have), and settings the processes change.

mod common;

use std::sync::{Arc, Mutex, MutexGuard};

use baritone::api::utils::{BetterBlockPos, BlockOptionalMeta};
use baritone::host::{
    BlockState, BlockStateTable, Chunk, DimensionType, Inventory, ItemStack, Player, World,
};
use baritone::mc::Vec3;
use baritone::process::{GetToBlockProcess, MineProcess};
use baritone::settings::{Settings, set_settings, settings, update_settings};
use common::sim::{Sim, TickRecord};
use common::{TABLE, block};

const DIMENSION: DimensionType = DimensionType {
    min_y: -16,
    height: 64,
    water_evaporates: false,
};

/// Settings are process-global: every test holds this.
static SETTINGS: Mutex<()> = Mutex::new(());

fn settings_lock() -> MutexGuard<'static, ()> {
    let guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    set_settings(Settings::default());
    guard
}

/// The fixture's table, with oak logs dropping themselves.
fn table_with_drops() -> Arc<BlockStateTable> {
    let mut states: Vec<BlockState> = TABLE.iter().cloned().collect();
    for state in &mut states {
        if state.name == "minecraft:oak_log" {
            state.drops = vec!["minecraft:oak_log".to_owned()];
        }
    }
    Arc::new(BlockStateTable::new(states, TABLE.air().id).unwrap())
}

/// Chunks -3..=3 loaded, stone up to y = -1 and grass at y = 0.
fn flat(table: Arc<BlockStateTable>) -> World {
    let mut world = World::new(table, DIMENSION).unwrap();
    for cx in -3..=3 {
        for cz in -3..=3 {
            world
                .load_chunk(cx, cz, Chunk::new(DIMENSION.section_count()))
                .unwrap();
        }
    }
    let stone = block("minecraft:stone");
    let grass = block("minecraft:grass_block");
    for x in -48..48 {
        for z in -48..48 {
            for y in -16..0 {
                world.set_block(x, y, z, stone).unwrap();
            }
            world.set_block(x, 0, z, grass).unwrap();
        }
    }
    world
}

/// Cobblestone to place, and nothing to mine with.
fn player() -> Player {
    let mut items = vec![ItemStack::empty(); 36];
    items[1] = ItemStack {
        count: 64,
        ..ItemStack::of("minecraft:cobblestone")
    };
    Player {
        position: Vec3::new(0.5, 1.0, 0.5),
        inventory: Inventory { items, selected: 0 },
        ..Player::default()
    }
}

/// Puts `count` of `item` in slot 8, as if picked up.
fn give(sim: &mut Sim, item: &str, count: i32) {
    sim.baritone.player_mut().unwrap().inventory.items[8] = ItemStack {
        count,
        ..ItemStack::of(item)
    };
}

fn trace(sim: &Sim, r: &TickRecord) {
    if std::env::var("SIM_TRACE").is_ok() {
        eprintln!(
            "{} {:?} {:?} {:?} {:?}",
            sim.ticks,
            sim.player().position,
            r.in_control,
            r.command,
            r.actions
        );
    }
}

#[test]
fn mines_until_it_has_enough() {
    let _settings = settings_lock();
    update_settings(|s| {
        s.mine_scan_dropped_items = false;
        s.explore_for_blocks = false;
    });
    let mut world = flat(table_with_drops());
    let logs = [(4, 1, 0), (4, 1, 3), (-4, 1, 2), (0, 1, -6)];
    for (x, y, z) in logs {
        world
            .set_block(x, y, z, block("minecraft:oak_log"))
            .unwrap();
    }
    let mut sim = Sim::new(world, player(), 7);
    MineProcess::mine_by_name(&mut sim.baritone, 2, &["oak_log"]).unwrap();
    let mut broken = 0;
    let mut finished = false;
    for _ in 0..3000 {
        let r = sim.tick();
        trace(&sim, &r);
        // the client picks up what was broken
        let now = logs
            .iter()
            .filter(|&&(x, y, z)| {
                sim.world()
                    .get_block_state(BetterBlockPos::new(x, y, z))
                    .air
            })
            .count();
        if now != broken {
            broken = now;
            give(&mut sim, "minecraft:oak_log", broken as i32);
        }
        if r.in_control.is_none() {
            finished = true;
            break;
        }
    }
    assert!(finished, "still mining after 3000 ticks");
    assert_eq!(broken, 2, "stops once it has two logs");
}

#[test]
fn opens_a_chest_under_a_block() {
    let _settings = settings_lock();
    let mut world = flat(Arc::clone(&TABLE));
    let chest = BetterBlockPos::new(6, 1, 0);
    world
        .set_block(chest.x, chest.y, chest.z, block("minecraft:chest"))
        .unwrap();
    world
        .set_block(chest.x, chest.y + 1, chest.z, block("minecraft:stone"))
        .unwrap();
    let mut player = player();
    // a pickaxe for the stone on top
    player.inventory.items[0] = ItemStack {
        tool: Some(baritone::host::Tool {
            rules: vec![baritone::host::ToolRule {
                blocks: baritone::host::BlockSet::Tag("minecraft:mineable/pickaxe".into()),
                speed: Some(4.0),
                correct_for_drops: Some(true),
            }],
            default_mining_speed: 1.0,
        }),
        ..ItemStack::of("minecraft:stone_pickaxe")
    };
    let mut sim = Sim::new(world, player, 7);
    // upstream looks chests up in its chunk cache; the port scans the loaded chunks
    GetToBlockProcess::get_to_block(
        &mut sim.baritone,
        BlockOptionalMeta::from_selector(&TABLE, "chest").unwrap(),
    );
    let first = sim.tick();
    // the chest cannot open with a block on top: stand where that block is
    assert_eq!(
        first.command.as_deref(),
        Some("REVALIDATE_GOAL_AND_PATH GoalComposite[GoalBlock{x=6,y=2,z=0}]")
    );
    for _ in 0..600 {
        let r = sim.tick();
        trace(&sim, &r);
        if sim.player().container_open {
            assert!(
                sim.world().get_block_state(chest.above()).air,
                "the stone was broken"
            );
            return;
        }
    }
    panic!("the chest never opened");
}

#[test]
fn backfill_turns_itself_off_with_parkour() {
    let _settings = settings_lock();
    update_settings(|s| {
        s.backfill = true;
        s.allow_parkour = true;
    });
    let mut sim = Sim::new(flat(Arc::clone(&TABLE)), player(), 7);
    sim.tick();
    assert!(!settings().backfill);
}
