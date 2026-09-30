// Ported from baritone src/main/java/baritone/process/GetToBlockProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Also `IGetToBlockProcess`. `getToBlock` and `blacklistClosest` reach the `Baritone`, so they
// are associated functions taking it.
//
// Upstream's `synchronized` methods lock the process's fields (`State`), which the rescan on
// the executor shares. A rescan started after the process lost control finds no block to get
// to and does nothing (upstream's throws a `NullPointerException` on the executor).
// `GetToBlockCalculationContext` is a `CalculationContext` of kind `GetToBlock`. The open
// container upstream prints is not ported; only whether one is open is known.

use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::Baritone;
use crate::api::pathing::goals::{
    Goal, GoalBlock, GoalComposite, GoalGetToBlock, GoalRunAway, GoalTwoBlocks, goal_equals,
};
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::helper::{log_debug, log_direct, println};
use crate::api::utils::input::Input;
use crate::api::utils::{
    BetterBlockPos, BlockOptionalMeta, BlockOptionalMetaLookup, IPlayerContext, rotation_utils,
};
use crate::behavior::PathingBehavior;
use crate::java;
use crate::pathing::movement::calculation_context::CalculationContextKind;
use crate::pathing::movement::{CalculationContext, movement_helper};
use crate::settings::settings;

#[derive(Debug, Default)]
pub struct GetToBlockProcess {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    getting_to: Option<BlockOptionalMeta>,
    known_locations: Option<Vec<BetterBlockPos>>,
    blacklist: Option<Vec<BetterBlockPos>>, // locations we failed to calc to
    start: Option<BetterBlockPos>,

    tick_count: i32,
    arrival_tick_count: i32,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl GetToBlockProcess {
    pub fn new() -> Self {
        Self::default()
    }

    /// `getToBlock(BlockOptionalMeta)`
    pub fn get_to_block(baritone: &mut Baritone, block: BlockOptionalMeta) {
        baritone.with_process_of(|this: &mut GetToBlockProcess, baritone| {
            this.on_lost_control(baritone);
            {
                let mut s = lock(&this.state);
                s.getting_to = Some(block);
                s.start = Some(baritone.player_context.player_feet());
                s.blacklist = Some(Vec::new());
                s.arrival_tick_count = 0;
            }
            let context = get_to_block_calculation_context(baritone, false);
            rescan(&mut lock(&this.state), Vec::new(), &context);
        });
    }

    /// blacklist the closest block and its adjacent blocks
    pub fn blacklist_closest(baritone: &mut Baritone) -> bool {
        baritone.with_process_of(|this: &mut GetToBlockProcess, baritone| {
            lock(&this.state).blacklist_closest(baritone)
        })
    }
}

/// `new GetToBlockCalculationContext(boolean)`
///
/// this is to signal to MineProcess that we don't care about the allowBreak setting
/// it is NOT to be used to actually calculate a path
fn get_to_block_calculation_context(
    baritone: &Baritone,
    for_use_on_another_thread: bool,
) -> CalculationContext {
    let mut context = CalculationContext::from_baritone_thread(baritone, for_use_on_another_thread);
    context.kind = CalculationContextKind::GetToBlock;
    context
}

/// `rescan(List<BlockPos>, CalculationContext)`, with the process's fields locked.
fn rescan(s: &mut State, known: Vec<BetterBlockPos>, context: &CalculationContext) {
    let (Some(getting_to), Some(blacklist)) = (&s.getting_to, &s.blacklist) else {
        // upstream throws on the executor
        return;
    };
    let mut positions = crate::process::MineProcess::search_world(
        context,
        &BlockOptionalMetaLookup::new(vec![getting_to.clone()]),
        64,
        &known,
        blacklist,
        &mut Vec::new(),
    );
    positions.retain(|pos| !blacklist.contains(pos));
    s.known_locations = Some(positions);
}

/// safer than direct double comparison from distanceSq
fn are_adjacent(pos_a: BetterBlockPos, pos_b: BetterBlockPos) -> bool {
    let diff_x = pos_a.x.wrapping_sub(pos_b.x).wrapping_abs();
    let diff_y = pos_a.y.wrapping_sub(pos_b.y).wrapping_abs();
    let diff_z = pos_a.z.wrapping_sub(pos_b.z).wrapping_abs();
    diff_x.wrapping_add(diff_y).wrapping_add(diff_z) == 1
}

fn walk_into_instead_of_adjacent(block: &str) -> bool {
    if !settings().enter_portal {
        return false;
    }
    block == "minecraft:nether_portal"
}

fn right_click_on_arrival(block: &str) -> bool {
    if !settings().right_click_container_on_arrival {
        return false;
    }
    matches!(
        block,
        "minecraft:crafting_table"
            | "minecraft:furnace"
            | "minecraft:blast_furnace"
            | "minecraft:ender_chest"
            | "minecraft:chest"
            | "minecraft:trapped_chest"
    )
}

fn block_on_top_must_be_removed(block: &str) -> bool {
    if !right_click_on_arrival(block) {
        // only if we plan to actually open it on arrival
        return false;
    }
    // only these chests; you can open a crafting table or furnace even with a block on top
    matches!(
        block,
        "minecraft:ender_chest" | "minecraft:chest" | "minecraft:trapped_chest"
    )
}

impl State {
    fn on_lost_control(&mut self, baritone: &mut Baritone) {
        self.getting_to = None;
        self.known_locations = None;
        self.start = None;
        self.blacklist = None;
        baritone.input_override_handler.clear_all_keys();
    }

    /// `onTick(boolean, boolean)`; `shared` is where this state lives, for the rescan.
    fn on_tick(
        &mut self,
        shared: &Arc<Mutex<State>>,
        baritone: &mut Baritone,
        calc_failed: bool,
        is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        if self.known_locations.is_none() {
            let context = get_to_block_calculation_context(baritone, false);
            rescan(self, Vec::new(), &context);
        }
        let getting_to = self
            .getting_to
            .as_ref()
            .expect("NullPointerException: gettingTo")
            .to_string();
        let settings = settings();
        if self.known_locations.as_ref().is_some_and(Vec::is_empty) {
            if settings.explore_for_blocks && !calc_failed {
                let start = self.start.expect("NullPointerException: start");
                return Some(PathingCommand::new(
                    Some(Arc::new(ExploreRunaway(GoalRunAway::new(
                        1.0,
                        None,
                        vec![start],
                    )))),
                    PathingCommandType::ForceRevalidateGoalAndPath,
                ));
            }
            log_direct(&format!(
                "No known locations of {getting_to}, canceling GetToBlock"
            ));
            if is_safe_to_cancel {
                self.on_lost_control(baritone);
            }
            return Some(PathingCommand::new(
                None,
                PathingCommandType::CancelAndSetGoal,
            ));
        }
        let goal: Arc<dyn Goal> = Arc::new(GoalComposite::new(
            self.known_locations
                .as_ref()
                .expect("NullPointerException: knownLocations")
                .iter()
                .map(|&pos| self.create_goal(baritone, pos))
                .collect(),
        ));
        if calc_failed {
            if settings.blacklist_closest_on_failure {
                log_direct(&format!(
                    "Unable to find any path to {getting_to}, blacklisting presumably unreachable closest instances..."
                ));
                self.blacklist_closest(baritone);
                return self.on_tick(shared, baritone, false, is_safe_to_cancel); // gamer moment
            } else {
                log_direct(&format!(
                    "Unable to find any path to {getting_to}, canceling GetToBlock"
                ));
                if is_safe_to_cancel {
                    self.on_lost_control(baritone);
                }
                return Some(PathingCommand::new(
                    Some(goal),
                    PathingCommandType::CancelAndSetGoal,
                ));
            }
        }
        let mine_goal_update_interval = settings.mine_goal_update_interval;
        if mine_goal_update_interval != 0 && {
            let tick_count = self.tick_count;
            self.tick_count = tick_count.wrapping_add(1);
            tick_count.wrapping_rem(mine_goal_update_interval) == 0
        } {
            // big brain
            let current = self.known_locations.clone().unwrap_or_default();
            let context = get_to_block_calculation_context(baritone, true);
            let state = Arc::clone(shared);
            baritone
                .executor
                .execute(move || rescan(&mut lock(&state), current, &context));
        }
        let ctx = &baritone.player_context;
        if goal.is_in_goal_pos(ctx.player_feet())
            && goal.is_in_goal_pos(PathingBehavior::path_start(ctx))
            && is_safe_to_cancel
        {
            // we're there
            let block = self
                .getting_to
                .as_ref()
                .expect("NullPointerException: gettingTo")
                .get_block()
                .to_owned();
            if right_click_on_arrival(&block) {
                if self.right_click(baritone) {
                    self.on_lost_control(baritone);
                    return Some(PathingCommand::new(
                        None,
                        PathingCommandType::CancelAndSetGoal,
                    ));
                }
            } else {
                self.on_lost_control(baritone);
                return Some(PathingCommand::new(
                    None,
                    PathingCommandType::CancelAndSetGoal,
                ));
            }
        }
        Some(PathingCommand::new(
            Some(goal),
            PathingCommandType::RevalidateGoalAndPath,
        ))
    }

    fn blacklist_closest(&mut self, baritone: &Baritone) -> bool {
        let known_locations = self
            .known_locations
            .as_mut()
            .expect("NullPointerException: knownLocations");
        let mut new_blacklist = Vec::new();
        let feet = baritone.player_context.player_feet();
        if let Some(&closest) = known_locations
            .iter()
            .min_by(|a, b| java::double_compare(feet.distance_sq(a), feet.distance_sq(b)))
        {
            new_blacklist.push(closest);
        }
        'outer: loop {
            for i in 0..known_locations.len() {
                let known = known_locations[i];
                for &blacklist in &new_blacklist {
                    if are_adjacent(known, blacklist) {
                        // directly adjacent
                        new_blacklist.push(known);
                        known_locations.remove(i);
                        continue 'outer;
                    }
                }
            }
            break;
        }
        log_debug(&format!(
            "Blacklisting unreachable locations [{}]",
            new_blacklist
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let blacklist_empty = new_blacklist.is_empty();
        self.blacklist
            .as_mut()
            .expect("NullPointerException: blacklist")
            .extend(new_blacklist);
        !blacklist_empty
    }

    fn create_goal(&self, baritone: &Baritone, pos: BetterBlockPos) -> Arc<dyn Goal> {
        let block = self
            .getting_to
            .as_ref()
            .expect("NullPointerException: gettingTo")
            .get_block();
        if walk_into_instead_of_adjacent(block) {
            return Arc::new(GoalTwoBlocks::from_pos(pos));
        }
        if block_on_top_must_be_removed(block)
            && movement_helper::is_block_normal_cube(
                baritone
                    .bsi
                    .as_ref()
                    .expect("the block state interface of this tick")
                    .get0_pos(pos.above()),
            )
        {
            // TODO this should be the check for chest openability
            return Arc::new(GoalBlock::from_pos(pos.above()));
        }
        Arc::new(GoalGetToBlock::new(pos))
    }

    fn right_click(&mut self, baritone: &mut Baritone) -> bool {
        let known_locations = self.known_locations.clone().unwrap_or_default();
        for &pos in &known_locations {
            let ctx = &baritone.player_context;
            let reachable = rotation_utils::reachable_distance(
                ctx,
                baritone.look_behavior.get_aim_processor(),
                pos,
                ctx.player_controller_ref().get_block_reach_distance(),
            );
            if let Some(reachable) = reachable {
                baritone.look_behavior.update_target(reachable);
                if ctx
                    .get_selected_block()
                    .is_some_and(|selected| known_locations.contains(&selected))
                {
                    baritone
                        .input_override_handler
                        .set_input_force_state(Input::ClickRight, true); // TODO find some way to right click even if we're in an ESC menu
                    let container_open = baritone.player_context.player().container_open;
                    println(if container_open {
                        "container menu"
                    } else {
                        "inventory menu"
                    });
                    if container_open {
                        return true;
                    }
                }
                let arrival_tick_count = self.arrival_tick_count;
                self.arrival_tick_count = arrival_tick_count.wrapping_add(1);
                if arrival_tick_count > 20 {
                    log_direct("Right click timed out");
                    return true;
                }
                return false; // trying to right click, will do it next tick or so
            }
        }
        log_direct("Arrived but failed to right click open");
        true
    }
}

impl IBaritoneProcess for GetToBlockProcess {
    fn is_active(&mut self, _baritone: &mut Baritone) -> bool {
        lock(&self.state).getting_to.is_some()
    }

    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        calc_failed: bool,
        is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        let state = Arc::clone(&self.state);
        lock(&state).on_tick(&state, baritone, calc_failed, is_safe_to_cancel)
    }

    fn is_temporary(&self) -> bool {
        false
    }

    fn on_lost_control(&mut self, baritone: &mut Baritone) {
        lock(&self.state).on_lost_control(baritone);
    }

    fn display_name0(&mut self, _baritone: &mut Baritone) -> String {
        let s = lock(&self.state);
        let getting_to = s
            .getting_to
            .as_ref()
            .map_or_else(|| "null".to_owned(), ToString::to_string);
        let known = s.known_locations.as_ref().map_or(0, Vec::len);
        if known == 0 {
            return format!("Exploring randomly to find {getting_to}, no known locations");
        }
        format!("Get To {getting_to}, {known} known locations")
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `new GoalRunAway(1, start) { ... }`: never reached, so the path goes as far from the start
/// as it can.
#[derive(Clone, Debug, PartialEq)]
struct ExploreRunaway(GoalRunAway);

impl Goal for ExploreRunaway {
    fn is_in_goal(&self, _x: i32, _y: i32, _z: i32) -> bool {
        false
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        self.0.heuristic(x, y, z)
    }

    fn heuristic_at_goal(&self) -> f64 {
        f64::NEG_INFINITY
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for ExploreRunaway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
