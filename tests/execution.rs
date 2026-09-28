//! Executes paths end to end: Baritone ticks against the client simulation in `common::sim`
//! (simplified movement and game mode) until the player stands in the goal. These check that
//! execution does something sensible; `reference_execution.rs` checks that it does what
//! upstream does.

mod common;

use std::sync::{Arc, Mutex, MutexGuard};

use baritone::api::pathing::goals::{Goal, GoalBlock};
use baritone::api::utils::BetterBlockPos;
use baritone::host::{Chunk, DimensionType, Inventory, ItemStack, Player, World};
use baritone::mc::Vec3;
use baritone::settings::{Settings, set_settings, update_settings};
use common::sim::Sim;
use common::{TABLE, block};

const DIMENSION: DimensionType = DimensionType {
    min_y: -16,
    height: 64,
    water_evaporates: false,
};

/// Chunks -3..=3 loaded, stone up to y = -1 and grass at y = 0.
fn flat() -> World {
    let mut world = World::new(Arc::clone(&TABLE), DIMENSION).unwrap();
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

fn set(world: &mut World, name: &str, positions: impl IntoIterator<Item = (i32, i32, i32)>) {
    let id = block(name);
    for (x, y, z) in positions {
        world.set_block(x, y, z, id).unwrap();
    }
}

fn item(name: &str, count: i32) -> ItemStack {
    ItemStack {
        count,
        ..ItemStack::of(name)
    }
}

/// A stone pickaxe and cobblestone to place.
fn player() -> Player {
    let mut items = vec![ItemStack::empty(); 36];
    items[0] = item("minecraft:stone_pickaxe", 1);
    items[0].tool = Some(stone_pickaxe_tool());
    items[1] = item("minecraft:cobblestone", 64);
    Player {
        position: Vec3::new(0.5, 1.0, 0.5),
        inventory: Inventory { items, selected: 0 },
        ..Player::default()
    }
}

/// The stone pickaxe's tool component, as the fixture's inventories carry it.
fn stone_pickaxe_tool() -> baritone::host::Tool {
    use baritone::host::{BlockSet, Tool, ToolRule};
    Tool {
        rules: vec![
            ToolRule {
                blocks: BlockSet::Tag("minecraft:incorrect_for_stone_tool".into()),
                speed: None,
                correct_for_drops: Some(false),
            },
            ToolRule {
                blocks: BlockSet::Tag("minecraft:mineable/pickaxe".into()),
                speed: Some(4.0),
                correct_for_drops: Some(true),
            },
        ],
        default_mining_speed: 1.0,
    }
}

/// Settings are process-global: every test holds this, and one that changes settings puts the
/// defaults back when it is done.
static SETTINGS: Mutex<()> = Mutex::new(());

fn settings_lock() -> MutexGuard<'static, ()> {
    let guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    set_settings(Settings::default());
    guard
}

/// Ticks until the player stands in the goal and Baritone stopped, at most `max_ticks`.
fn run(world: World, player: Player, goal: BetterBlockPos, max_ticks: u32) -> Sim {
    let _settings = settings_lock();
    run_with_settings(world, player, goal, max_ticks)
}

fn run_with_settings(world: World, player: Player, goal: BetterBlockPos, max_ticks: u32) -> Sim {
    let mut sim = Sim::new(world, player, 0x1234);
    let goal: Arc<dyn Goal> = Arc::new(GoalBlock::from_pos(goal));
    sim.baritone
        .get_custom_goal_process_mut()
        .set_goal_and_path(Some(Arc::clone(&goal)));
    for _ in 0..max_ticks {
        let r = sim.tick();
        if std::env::var("SIM_TRACE").is_ok() {
            eprintln!(
                "{} {:?} feet {} inputs {:?} actions {:?} events {:?} path {:?}",
                sim.ticks,
                sim.player().position,
                sim.feet(),
                r.inputs
                    .iter()
                    .filter(|i| i.1)
                    .map(|i| i.0)
                    .collect::<Vec<_>>(),
                r.actions,
                r.events,
                r.path
            );
        }
        if goal.is_in_goal_pos(sim.feet())
            && !sim.baritone.get_pathing_behavior().is_pathing()
            && !sim.baritone.get_custom_goal_process().is_active_now()
        {
            return sim;
        }
    }
    panic!(
        "not in the goal after {max_ticks} ticks: at {:?} ({})",
        sim.player().position,
        sim.feet()
    );
}

trait IsActiveNow {
    fn is_active_now(&self) -> bool;
}

impl IsActiveNow for baritone::process::CustomGoalProcess {
    fn is_active_now(&self) -> bool {
        self.get_goal().is_some()
    }
}

#[test]
fn walks_across_flat_ground() {
    let sim = run(flat(), player(), BetterBlockPos::new(12, 1, 5), 200);
    assert!(sim.ticks < 100, "took {} ticks", sim.ticks);
}

#[test]
fn climbs_stairs() {
    let mut world = flat();
    // a staircase along +x: step n is n blocks high
    for n in 1..=4 {
        set(
            &mut world,
            "minecraft:stone",
            (1..=n).map(|y| (2 + n, y, 0)),
        );
    }
    let sim = run(world, player(), BetterBlockPos::new(6, 5, 0), 300);
    assert_eq!(sim.feet(), BetterBlockPos::new(6, 5, 0));
}

#[test]
fn walks_down_stairs_and_falls() {
    let mut world = flat();
    // a tower to start on, with steps down on one side and a 3 block drop on the other
    set(&mut world, "minecraft:stone", (1..=4).map(|y| (0, y, 0)));
    for n in 1..=3 {
        set(
            &mut world,
            "minecraft:stone",
            (1..=4 - n).map(|y| (n, y, 0)),
        );
    }
    let mut player = player();
    player.position = Vec3::new(0.5, 5.0, 0.5);
    let sim = run(
        world.clone(),
        player.clone(),
        BetterBlockPos::new(5, 1, 0),
        300,
    );
    assert_eq!(sim.feet(), BetterBlockPos::new(5, 1, 0));
    // the other way is a drop: ground at y = 1 (a slab of stone) so the fall is 3 blocks
    set(&mut world, "minecraft:stone", (-3..=-1).map(|x| (x, 1, 0)));
    let sim = run(world, player, BetterBlockPos::new(-3, 2, 0), 300);
    assert_eq!(sim.feet(), BetterBlockPos::new(-3, 2, 0));
}

#[test]
fn breaks_through_a_wall() {
    let mut world = flat();
    for z in -6..=6 {
        set(&mut world, "minecraft:stone", (1..=4).map(|y| (5, y, z)));
    }
    let sim = run(world, player(), BetterBlockPos::new(9, 1, 0), 600);
    assert!(
        sim.world()
            .get_block_state(BetterBlockPos::new(5, 1, 0))
            .air
    );
}

#[test]
fn bridges_a_gap() {
    let mut world = flat();
    // a trench two wide and deep across the path
    for z in -20..=20 {
        for x in 4..=5 {
            set(&mut world, "minecraft:air", (-10..=0).map(|y| (x, y, z)));
        }
    }
    let sim = run(world, player(), BetterBlockPos::new(8, 1, 0), 800);
    let placed = (4..=5)
        .filter(|&x| {
            !sim.world()
                .get_block_state(BetterBlockPos::new(x, 0, 0))
                .air
        })
        .count();
    assert!(placed > 0, "crossed without placing");
}

#[test]
fn pillars_up() {
    let mut world = flat();
    // a ledge five blocks up, reachable only by pillaring
    set(
        &mut world,
        "minecraft:stone",
        (-2..=2).map(|z| (0, 5, z + 10)),
    );
    let sim = run(world, player(), BetterBlockPos::new(0, 6, 10), 800);
    assert_eq!(sim.feet(), BetterBlockPos::new(0, 6, 10));
}

#[test]
fn runs_are_deterministic() {
    // what the reference replay relies on: a run depends on nothing but its inputs
    let mut world = flat();
    for z in -48..48 {
        set(&mut world, "minecraft:stone", (1..=4).map(|y| (5, y, z)));
    }
    let _settings = settings_lock();
    let records = |world: World| {
        let mut sim = Sim::new(world, player(), 99);
        sim.baritone
            .get_custom_goal_process_mut()
            .set_goal_and_path(Some(Arc::new(GoalBlock::new(12, 1, 3))));
        (0..150).map(|_| sim.tick()).collect::<Vec<_>>()
    };
    let first = records(world.clone());
    assert!(
        first
            .iter()
            .any(|r| r.actions.iter().any(|a| a.starts_with("damage")))
    );
    assert_eq!(first, records(world));
}

#[test]
fn walks_far_in_several_segments() {
    let _settings = settings_lock();
    // stop each search at its first timeout check that has a path: short segments, planned
    // ahead and spliced while walking
    update_settings(|s| {
        s.primary_timeout_ms = 0;
        s.plan_ahead_primary_timeout_ms = 0;
    });
    // walls with a gap on alternating sides, so the search needs more than 64 nodes
    let mut world = flat();
    for (i, x) in [8, 16, 24, 32].into_iter().enumerate() {
        let gap = if i % 2 == 0 { 30 } else { -30 };
        for z in (-48..48).filter(|&z| z != gap) {
            set(&mut world, "minecraft:bedrock", (1..=3).map(|y| (x, y, z)));
        }
    }
    let sim = run_with_settings(world, player(), BetterBlockPos::new(40, 1, 0), 2000);
    set_settings(Settings::default());
    assert_eq!(sim.feet(), BetterBlockPos::new(40, 1, 0));
}
