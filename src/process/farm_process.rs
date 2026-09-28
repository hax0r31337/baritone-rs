// Ported from baritone src/main/java/baritone/process/FarmProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Also `IFarmProcess`: `farm` reaches the `Baritone` (for the player's feet), so it is an
// associated function taking it; `pos` is the nullable center.
//
// Blocks are the host's: `Blocks.X` and `instanceof XxxBlock` are block names, crop ages are
// the `age` property (`CropBlock.isMaxAge` against the crop's maximum age), and
// `BonemealableBlock`'s checks are the `bonemealable` trait, except for bamboo, whose check
// (the stalk's height and its top's `stage` property) is ported here. Items are item ids.
//
// `locations` is shared with the scan that runs on the executor. `onTick` holds it
// throughout, so a scan started during a tick lands after it; upstream races the two. The
// scan reads the world and the player as they were when it was started. The selection
// manager is not ported, so `farmUsingSelection` never finds a selection.

use std::any::Any;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::Baritone;
use crate::api::pathing::goals::{Goal, GoalBlock, GoalComposite, GoalGetToBlock};
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::helper::{log_direct, log_notification};
use crate::api::utils::input::Input;
use crate::api::utils::{
    BetterBlockPos, BlockOptionalMetaLookup, IPlayerContext, ray_trace_utils, rotation_utils,
};
use crate::behavior::InventoryBehavior;
use crate::cache::faster_world_scanner;
use crate::host::{BlockState, ItemStack, World};
use crate::mc::{Direction, Vec3};
use crate::pathing::movement::movement_helper;
use crate::process::builder_process::GoalBreak;
use crate::settings::settings;

const FARMLAND: &str = "minecraft:farmland";
const SOUL_SAND: &str = "minecraft:soul_sand";
const JUNGLE_LOG: &str = "minecraft:jungle_log";
const BAMBOO: &str = "minecraft:bamboo";
/// `BambooStalkBlock.MAX_HEIGHT`
const BAMBOO_MAX_HEIGHT: i32 = 16;
/// `BambooStalkBlock.STAGE_DONE_GROWING`
const BAMBOO_STAGE_DONE_GROWING: i32 = 1;

const FARMLAND_PLANTABLE: [&str; 6] = [
    "minecraft:beetroot_seeds",
    "minecraft:melon_seeds",
    "minecraft:wheat_seeds",
    "minecraft:pumpkin_seeds",
    "minecraft:potato",
    "minecraft:carrot",
];

const PICKUP_DROPPED: [&str; 16] = [
    "minecraft:beetroot_seeds",
    "minecraft:beetroot",
    "minecraft:melon_seeds",
    "minecraft:melon_slice",
    "minecraft:melon",
    "minecraft:wheat_seeds",
    "minecraft:wheat",
    "minecraft:pumpkin_seeds",
    "minecraft:pumpkin",
    "minecraft:potato",
    "minecraft:carrot",
    "minecraft:nether_wart",
    "minecraft:cocoa_beans",
    "minecraft:sugar_cane",
    "minecraft:bamboo",
    "minecraft:cactus",
];

#[derive(Debug, Default)]
pub struct FarmProcess {
    active: bool,

    locations: Arc<Mutex<Option<Vec<BetterBlockPos>>>>,
    tick_count: i32,

    range: i32,
    center: Option<BetterBlockPos>,
}

fn lock(
    locations: &Mutex<Option<Vec<BetterBlockPos>>>,
) -> MutexGuard<'_, Option<Vec<BetterBlockPos>>> {
    locations.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Harvest {
    Wheat,
    Carrots,
    Potatoes,
    Beetroot,
    Pumpkin,
    Melon,
    Netherwart,
    Cocoa,
    Sugarcane,
    Bamboo,
    Cactus,
}

impl Harvest {
    const VALUES: [Harvest; 11] = [
        Harvest::Wheat,
        Harvest::Carrots,
        Harvest::Potatoes,
        Harvest::Beetroot,
        Harvest::Pumpkin,
        Harvest::Melon,
        Harvest::Netherwart,
        Harvest::Cocoa,
        Harvest::Sugarcane,
        Harvest::Bamboo,
        Harvest::Cactus,
    ];

    fn block(self) -> &'static str {
        match self {
            Harvest::Wheat => "minecraft:wheat",
            Harvest::Carrots => "minecraft:carrots",
            Harvest::Potatoes => "minecraft:potatoes",
            Harvest::Beetroot => "minecraft:beetroots",
            Harvest::Pumpkin => "minecraft:pumpkin",
            Harvest::Melon => "minecraft:melon",
            Harvest::Netherwart => "minecraft:nether_wart",
            Harvest::Cocoa => "minecraft:cocoa",
            Harvest::Sugarcane => "minecraft:sugar_cane",
            Harvest::Bamboo => "minecraft:bamboo",
            Harvest::Cactus => "minecraft:cactus",
        }
    }

    fn ready_to_harvest(self, world: &World, pos: BetterBlockPos, state: &BlockState) -> bool {
        // max age is 7 for wheat, carrots, and potatoes, but 3 for beetroot
        let below_is = |block: &str| {
            if settings().replant_crops {
                return world.get_block_state(pos.below()).name == block;
            }
            true
        };
        match self {
            Harvest::Wheat | Harvest::Carrots | Harvest::Potatoes => age(state) >= 7,
            Harvest::Beetroot => age(state) >= 3,
            Harvest::Pumpkin | Harvest::Melon => true,
            Harvest::Netherwart => age(state) >= 3,
            Harvest::Cocoa => age(state) >= 2,
            Harvest::Sugarcane => below_is("minecraft:sugar_cane"),
            Harvest::Bamboo => below_is("minecraft:bamboo"),
            Harvest::Cactus => below_is("minecraft:cactus"),
        }
    }
}

/// The `age` property.
fn age(state: &BlockState) -> i32 {
    int_property(state, "age")
}

fn int_property(state: &BlockState, name: &str) -> i32 {
    state
        .properties
        .get(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// `BonemealableBlock.isValidBonemealTarget && isBonemealSuccess`: the `bonemealable` trait,
/// except for bamboo, whose check asks the stalk (`BambooStalkBlock`, which always succeeds).
fn is_bonemealable(world: &World, pos: BetterBlockPos, state: &BlockState) -> bool {
    if state.name != BAMBOO {
        return state.bonemealable;
    }
    let i = bamboo_height_above_up_to_max(world, pos);
    let j = bamboo_height_below_up_to_max(world, pos);
    i + j + 1 < BAMBOO_MAX_HEIGHT
        && int_property(world.get_block_state(pos.above_n(i)), "stage") != BAMBOO_STAGE_DONE_GROWING
}

/// `BambooStalkBlock.getHeightAboveUpToMax`
fn bamboo_height_above_up_to_max(world: &World, pos: BetterBlockPos) -> i32 {
    let mut i = 0;
    while i < BAMBOO_MAX_HEIGHT && world.get_block_state(pos.above_n(i + 1)).name == BAMBOO {
        i += 1;
    }
    i
}

/// `BambooStalkBlock.getHeightBelowUpToMax`
fn bamboo_height_below_up_to_max(world: &World, pos: BetterBlockPos) -> i32 {
    let mut i = 0;
    while i < BAMBOO_MAX_HEIGHT && world.get_block_state(pos.below_n(i + 1)).name == BAMBOO {
        i += 1;
    }
    i
}

fn ready_for_harvest(world: &World, pos: BetterBlockPos, state: &BlockState) -> bool {
    for harvest in Harvest::VALUES {
        if harvest.block() == state.name {
            return harvest.ready_to_harvest(world, pos, state);
        }
    }
    false
}

fn is_plantable(stack: &ItemStack) -> bool {
    FARMLAND_PLANTABLE.contains(&stack.get_item())
}

fn is_bone_meal(stack: &ItemStack) -> bool {
    !stack.is_empty() && stack.get_item() == "minecraft:bone_meal"
}

fn is_nether_wart(stack: &ItemStack) -> bool {
    !stack.is_empty() && stack.get_item() == "minecraft:nether_wart"
}

fn is_cocoa(stack: &ItemStack) -> bool {
    !stack.is_empty() && stack.get_item() == "minecraft:cocoa_beans"
}

impl FarmProcess {
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin to search for crops to farm with in specified aria from specified location
    /// (`None`: the player's feet).
    pub fn farm(baritone: &mut Baritone, range: i32, pos: Option<BetterBlockPos>) {
        let center = pos.unwrap_or_else(|| baritone.player_context.player_feet());
        baritone.with_process_of(|this: &mut FarmProcess, _| {
            this.center = Some(center);
            this.range = range;
            this.active = true;
            *lock(&this.locations) = None;
        });
    }
}

impl IBaritoneProcess for FarmProcess {
    fn is_active(&mut self, _baritone: &mut Baritone) -> bool {
        self.active
    }

    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        calc_failed: bool,
        is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        let settings = settings();
        let shared = Arc::clone(&self.locations);
        let mut locations = lock(&shared);
        if settings.mine_goal_update_interval != 0 && {
            let tick_count = self.tick_count;
            self.tick_count = tick_count.wrapping_add(1);
            tick_count.wrapping_rem(settings.mine_goal_update_interval) == 0
        } {
            let mut scan: Vec<&'static str> = Vec::new();
            for harvest in Harvest::VALUES {
                scan.push(harvest.block());
            }
            if settings.replant_crops {
                scan.push(FARMLAND);
                scan.push(JUNGLE_LOG);
                if settings.replant_nether_wart {
                    scan.push(SOUL_SAND);
                }
            }

            let ctx = &baritone.player_context;
            let world = Arc::clone(ctx.world());
            let feet = ctx.player_feet();
            let max = settings.farm_max_scan_size;
            let shared = Arc::clone(&self.locations);
            baritone.executor.execute(move || {
                let table = world.table();
                let filter = BlockOptionalMetaLookup::from_blocks(
                    table,
                    scan.iter()
                        .filter_map(|&block| table.get_default_state(block)),
                );
                let found =
                    faster_world_scanner::scan_chunk_radius(&world, feet, &filter, max, 10, 10);
                *lock(&shared) = Some(found);
            });
        }
        let Some(locations) = locations.as_mut() else {
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        };
        if settings.farm_using_selection {
            // the selection manager is not ported, so there is never a last selection
        }
        let mut to_break = Vec::new();
        let mut open_farmland = Vec::new();
        let mut bonemealable = Vec::new();
        let mut open_soulsand = Vec::new();
        let mut open_log = Vec::new();
        let world = Arc::clone(baritone.player_context.world());
        let center = self.center.expect("NullPointerException: center");
        for &pos in locations.iter() {
            //check if the target block is out of range.
            if self.range != 0
                && pos.distance_sq(&center) > self.range.wrapping_mul(self.range) as f64
            {
                continue;
            }

            let state = world.get_block_state(pos);
            let air_above = world.get_block_state(pos.above()).air;
            if state.name == FARMLAND {
                if air_above {
                    open_farmland.push(pos);
                }
                continue;
            }
            if state.name == SOUL_SAND {
                if air_above {
                    open_soulsand.push(pos);
                }
                continue;
            }
            if state.name == JUNGLE_LOG {
                for direction in Direction::HORIZONTAL {
                    if world.get_block_state(pos.relative(direction)).air {
                        open_log.push(pos);
                        break;
                    }
                }
                continue;
            }
            if ready_for_harvest(&world, pos, state) {
                to_break.push(pos);
                continue;
            }
            if is_bonemealable(&world, pos, state) {
                bonemealable.push(pos);
            }
        }

        baritone.input_override_handler.clear_all_keys();
        let player_pos = baritone.player_context.player_feet();
        let block_reach_distance = baritone
            .player_context
            .player_controller_ref()
            .get_block_reach_distance();
        let reach_sq = block_reach_distance * block_reach_distance;
        for &pos in &to_break {
            if player_pos.distance_sq(&pos) > reach_sq {
                continue;
            }
            let ctx = &baritone.player_context;
            let rot =
                rotation_utils::reachable(ctx, baritone.look_behavior.get_aim_processor(), pos);
            if let Some(rot) = rot
                && is_safe_to_cancel
            {
                baritone.look_behavior.update_target(ctx, rot, true);
                movement_helper::switch_to_best_tool_for(
                    &mut baritone.player_context,
                    world.get_block_state(pos),
                );
                if baritone.player_context.is_looking_at(pos) {
                    baritone
                        .input_override_handler
                        .set_input_force_state(Input::ClickLeft, true);
                }
                return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
            }
        }
        let mut both = open_farmland.clone();
        both.extend_from_slice(&open_soulsand);
        for &pos in &both {
            if player_pos.distance_sq(&pos) > reach_sq {
                continue;
            }
            let soulsand = open_soulsand.contains(&pos);
            let rot = rotation_utils::reachable_offset(
                &baritone.player_context,
                baritone.look_behavior.get_aim_processor(),
                pos,
                Vec3::new(
                    pos.x as f64 + 0.5,
                    pos.y.wrapping_add(1) as f64,
                    pos.z as f64 + 0.5,
                ),
                block_reach_distance,
                false,
            );
            if let Some(rot) = rot
                && is_safe_to_cancel
                && InventoryBehavior::throwaway(
                    baritone,
                    true,
                    if soulsand {
                        is_nether_wart
                    } else {
                        is_plantable
                    },
                )
            {
                let ctx = &baritone.player_context;
                let result = ray_trace_utils::ray_trace_towards(
                    ctx.player(),
                    ctx.world(),
                    rot,
                    block_reach_distance,
                );
                if result.get_direction() == Direction::Up {
                    baritone.look_behavior.update_target(ctx, rot, true);
                    if ctx.is_looking_at(pos) {
                        baritone
                            .input_override_handler
                            .set_input_force_state(Input::ClickRight, true);
                    }
                    return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
                }
            }
        }
        for &pos in &open_log {
            if player_pos.distance_sq(&pos) > reach_sq {
                continue;
            }
            for dir in Direction::HORIZONTAL {
                if !world.get_block_state(pos.relative(dir)).air {
                    continue;
                }
                let face_center = Vec3::new(
                    pos.x as f64 + 0.5 + dir.get_step_x() as f64 * 0.5,
                    pos.y as f64 + 0.5 + dir.get_step_y() as f64 * 0.5,
                    pos.z as f64 + 0.5 + dir.get_step_z() as f64 * 0.5,
                );
                let rot = rotation_utils::reachable_offset(
                    &baritone.player_context,
                    baritone.look_behavior.get_aim_processor(),
                    pos,
                    face_center,
                    block_reach_distance,
                    false,
                );
                if let Some(rot) = rot
                    && is_safe_to_cancel
                    && InventoryBehavior::throwaway(baritone, true, is_cocoa)
                {
                    let ctx = &baritone.player_context;
                    let result = ray_trace_utils::ray_trace_towards(
                        ctx.player(),
                        ctx.world(),
                        rot,
                        block_reach_distance,
                    );
                    if result.get_direction() == dir {
                        baritone.look_behavior.update_target(ctx, rot, true);
                        if ctx.is_looking_at(pos) {
                            baritone
                                .input_override_handler
                                .set_input_force_state(Input::ClickRight, true);
                        }
                        return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
                    }
                }
            }
        }
        for &pos in &bonemealable {
            if player_pos.distance_sq(&pos) > reach_sq {
                continue;
            }
            let ctx = &baritone.player_context;
            let rot =
                rotation_utils::reachable(ctx, baritone.look_behavior.get_aim_processor(), pos);
            if let Some(rot) = rot
                && is_safe_to_cancel
                && InventoryBehavior::throwaway(baritone, true, is_bone_meal)
            {
                let ctx = &baritone.player_context;
                baritone.look_behavior.update_target(ctx, rot, true);
                if ctx.is_looking_at(pos) {
                    baritone
                        .input_override_handler
                        .set_input_force_state(Input::ClickRight, true);
                }
                return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
            }
        }

        if calc_failed {
            log_direct("Farm failed");
            if settings.notification_on_farm_fail {
                log_notification("Farm failed", true);
            }
            self.on_lost_control(baritone);
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        }

        let mut goalz: Vec<Arc<dyn Goal>> = Vec::new();
        for &pos in &to_break {
            goalz.push(Arc::new(GoalBreak::new(pos)));
        }
        if InventoryBehavior::throwaway(baritone, false, is_plantable) {
            for &pos in &open_farmland {
                goalz.push(Arc::new(GoalBlock::from_pos(pos.above())));
            }
        }
        if InventoryBehavior::throwaway(baritone, false, is_nether_wart) {
            for &pos in &open_soulsand {
                goalz.push(Arc::new(GoalBlock::from_pos(pos.above())));
            }
        }
        if InventoryBehavior::throwaway(baritone, false, is_cocoa) {
            for &pos in &open_log {
                for direction in Direction::HORIZONTAL {
                    if world.get_block_state(pos.relative(direction)).air {
                        goalz.push(Arc::new(GoalGetToBlock::new(pos.relative(direction))));
                    }
                }
            }
        }
        if InventoryBehavior::throwaway(baritone, false, is_bone_meal) {
            for &pos in &bonemealable {
                goalz.push(Arc::new(GoalBlock::from_pos(pos)));
            }
        }
        for entity in baritone.player_context.entities() {
            if let Some(item) = entity.as_item_entity()
                && entity.on_ground
                && PICKUP_DROPPED.contains(&item.get_item())
            {
                // +0.1 because of farmland's 0.9375 dummy height lol
                goalz.push(Arc::new(GoalBlock::from_pos(BetterBlockPos::from_f64(
                    entity.position.x,
                    entity.position.y + 0.1,
                    entity.position.z,
                ))));
            }
        }
        if goalz.is_empty() {
            log_direct("Farm failed");
            if settings.notification_on_farm_fail {
                log_notification("Farm failed", true);
            }
            self.on_lost_control(baritone);
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        }
        Some(PathingCommand::new(
            Some(Arc::new(GoalComposite::new(goalz))),
            PathingCommandType::SetGoalAndPath,
        ))
    }

    fn is_temporary(&self) -> bool {
        false
    }

    fn on_lost_control(&mut self, _baritone: &mut Baritone) {
        self.active = false;
    }

    fn display_name0(&mut self, _baritone: &mut Baritone) -> String {
        "Farming".to_owned()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{BlockStateTable, Chunk, DimensionType};

    const AIR: u32 = 0;
    const GROWING: u32 = 1;
    const DONE: u32 = 2;
    const WHEAT: u32 = 3;

    fn world() -> World {
        let bamboo = |stage: &str| BlockState {
            name: BAMBOO.to_owned(),
            properties: [("stage".to_owned(), stage.to_owned())].into(),
            // what the host sends: the state alone, in an empty world
            bonemealable: stage == "0",
            ..BlockState::default()
        };
        let table = BlockStateTable::new(
            vec![
                BlockState {
                    name: "minecraft:air".to_owned(),
                    air: true,
                    ..BlockState::default()
                },
                bamboo("0"),
                bamboo("1"),
                BlockState {
                    name: "minecraft:wheat".to_owned(),
                    bonemealable: true,
                    ..BlockState::default()
                },
            ],
            AIR,
        )
        .unwrap();
        let dimension = DimensionType {
            min_y: 0,
            height: 32,
            water_evaporates: false,
        };
        let mut world = World::new(Arc::new(table), dimension).unwrap();
        world
            .load_chunk(0, 0, Chunk::new(dimension.section_count()))
            .unwrap();
        world
    }

    fn bonemealable(world: &World, x: i32, y: i32) -> bool {
        let pos = BetterBlockPos::new(x, y, 0);
        is_bonemealable(world, pos, world.get_block_state(pos))
    }

    #[test]
    fn bamboo_asks_its_stalk() {
        let mut world = world();
        // a grown stalk: only its top is done growing
        world.set_block(0, 0, 0, GROWING).unwrap();
        world.set_block(0, 1, 0, GROWING).unwrap();
        world.set_block(0, 2, 0, DONE).unwrap();
        for y in 0..3 {
            assert!(!bonemealable(&world, 0, y), "grown stalk at y = {y}");
        }
        // still growing
        world.set_block(0, 2, 0, GROWING).unwrap();
        assert!(bonemealable(&world, 0, 0));
        assert!(bonemealable(&world, 0, 2));
        // 15 blocks tall grows, 16 does not, whatever the stage
        for y in 0..15 {
            world.set_block(1, y, 0, GROWING).unwrap();
        }
        assert!(bonemealable(&world, 1, 0));
        world.set_block(1, 15, 0, GROWING).unwrap();
        assert!(!bonemealable(&world, 1, 0));
        assert!(!bonemealable(&world, 1, 15));
        // other blocks are the trait
        world.set_block(2, 0, 0, WHEAT).unwrap();
        assert!(bonemealable(&world, 2, 0));
    }
}
