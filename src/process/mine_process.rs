// Ported from baritone src/main/java/baritone/process/MineProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Also `IMineProcess`: its overloads are `mine` with a lookup and `mine_by_name` with
// selectors, and `cancel`. `mine`, `mine_by_name` and `cancel` reach the `Baritone`, so they
// are associated functions taking it.
//
// The fields a rescan reads and writes live in a mutex shared with it (`State`), since the
// rescan runs on the executor. `onTick` holds it throughout, so a rescan started during a tick
// waits for the tick to end before it reads the filter and the blacklist, and its locations
// land after the tick; upstream races the two. A rescan reads the world and the player as they
// were when it was started (the tick does not change them). The item entities are ignored, so
// the dropped items are only the anticipated drops.
//
// No chunk cache: `searchWorld` treats every block as untracked, so every search scans the
// loaded chunks (upstream looks up the blocks of `CachedChunk.BLOCKS_TO_KEEP_TRACK_OF` in the
// cache, which holds the loaded chunks too, and only scans for the others).
// `anticipatedDrops` iterates in Java `HashMap` order (`java::JavaHashMap`).

use std::any::Any;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::goals::{
    Goal, GoalBlock, GoalComposite, GoalRunAway, GoalTwoBlocks, goal_equals,
};
use crate::api::pathing::movement::COST_INF;
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::helper::{log_direct, log_notification};
use crate::api::utils::i_player_context::player_feet;
use crate::api::utils::input::Input;
use crate::api::utils::interfaces::IGoalRenderPos;
use crate::api::utils::settings_util::maybe_censor;
use crate::api::utils::{BetterBlockPos, BlockOptionalMetaLookup, IPlayerContext, rotation_utils};
use crate::cache::faster_world_scanner;
use crate::host::Inventory;
use crate::java::{self, IllegalArgumentException, JavaHashMap};
use crate::pathing::movement::CalculationContext;
use crate::pathing::movement::movement_helper;
use crate::settings::settings;
use crate::utils::BlockStateInterface;

const BEDROCK: &str = "minecraft:bedrock";

/// Mine blocks of a certain type
#[derive(Debug)]
pub struct MineProcess {
    state: Arc<Mutex<State>>,
    tick_count: i32,
}

#[derive(Debug)]
struct State {
    filter: Option<Arc<BlockOptionalMetaLookup>>,
    known_ore_locations: Vec<BetterBlockPos>,
    blacklist: Vec<BetterBlockPos>, // inaccessible
    anticipated_drops: JavaHashMap<BetterBlockPos, i64>,
    branch_point: Option<BetterBlockPos>,
    branch_point_runaway: Option<Arc<dyn Goal>>,
    desired_quantity: i32,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn filter_string(filter: Option<&Arc<BlockOptionalMetaLookup>>) -> String {
    filter.map_or_else(|| "null".to_owned(), ToString::to_string)
}

impl Default for MineProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl MineProcess {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                filter: None,
                known_ore_locations: Vec::new(),
                blacklist: Vec::new(),
                anticipated_drops: JavaHashMap::new(BetterBlockPos::block_pos_hash_code),
                branch_point: None,
                branch_point_runaway: None,
                desired_quantity: 0,
            })),
            tick_count: 0,
        }
    }

    /// `mineByName(int, String...)`: block selectors, resolved against the world's block
    /// state table.
    pub fn mine_by_name<S: AsRef<str>>(
        baritone: &mut Baritone,
        quantity: i32,
        blocks: &[S],
    ) -> Result<(), IllegalArgumentException> {
        let filter = BlockOptionalMetaLookup::from_selectors(
            baritone.player_context.world().table(),
            blocks,
        )?;
        Self::mine(baritone, quantity, Some(filter));
        Ok(())
    }

    /// `mine(int, BlockOptionalMetaLookup)`: begin to search for and mine the specified
    /// blocks until the number of specified items to get from the blocks that are mined.
    /// `None` stops mining.
    pub fn mine(baritone: &mut Baritone, quantity: i32, filter: Option<BlockOptionalMetaLookup>) {
        baritone.with_process_of(|this: &mut MineProcess, baritone| {
            this.mine0(baritone, quantity, filter.map(Arc::new));
        });
    }

    /// Cancels the current mining task
    pub fn cancel(baritone: &mut Baritone) {
        baritone.with_process_of(|this: &mut MineProcess, baritone| this.on_lost_control(baritone));
    }

    fn mine0(
        &mut self,
        baritone: &mut Baritone,
        quantity: i32,
        filter: Option<Arc<BlockOptionalMetaLookup>>,
    ) {
        let rescan_now = filter.is_some();
        lock(&self.state).mine(quantity, filter);
        if rescan_now {
            let context = CalculationContext::from_baritone(baritone);
            rescan(&self.state, Vec::new(), &context);
        }
    }

    pub fn search_world(
        ctx: &CalculationContext,
        filter: &BlockOptionalMetaLookup,
        max: i32,
        already_known: &[BetterBlockPos],
        blacklist: &[BetterBlockPos],
        dropped: &mut Vec<BetterBlockPos>,
    ) -> Vec<BetterBlockPos> {
        let mut locs = Vec::new();
        // there is no chunk cache to ask for CachedChunk.BLOCKS_TO_KEEP_TRACK_OF, so every block
        // is untracked
        let untracked: Vec<&str> = filter.blocks().iter().map(|bom| bom.get_block()).collect();

        locs = prune(ctx, locs, filter, max, blacklist, dropped);

        if !untracked.is_empty()
            || (settings().extend_cache_on_threshold && (locs.len() as i64) < max as i64)
        {
            locs.extend(faster_world_scanner::scan_chunk_radius(
                &ctx.world,
                player_feet(&ctx.player, &ctx.world),
                filter,
                max,
                10,
                32,
            )); // maxSearchRadius is NOT sq
        }

        locs.extend_from_slice(already_known);

        prune(ctx, locs, filter, max, blacklist, dropped)
    }

    pub fn is_next_to_air(ctx: &CalculationContext, pos: BetterBlockPos) -> bool {
        let radius = settings().allow_only_exposed_ores_distance;
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    if dx
                        .wrapping_abs()
                        .wrapping_add(dy.wrapping_abs())
                        .wrapping_add(dz.wrapping_abs())
                        <= radius
                        && movement_helper::is_transparent(ctx.get_block(
                            pos.x + dx,
                            pos.y + dy,
                            pos.z + dz,
                        ))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn plausible_to_break(ctx: &CalculationContext, pos: BetterBlockPos) -> bool {
        let state = ctx.bsi.get0_pos(pos);
        if movement_helper::get_mining_duration_ticks_state(ctx, pos.x, pos.y, pos.z, state, true)
            >= COST_INF
        {
            return false;
        }
        if movement_helper::avoid_breaking(&ctx.bsi, pos.x, pos.y, pos.z, state) {
            return false;
        }

        // bedrock above and below makes it implausible, otherwise we're good
        !(ctx.bsi.get0_pos(pos.above()).name == BEDROCK
            && ctx.bsi.get0_pos(pos.below()).name == BEDROCK)
    }
}

impl State {
    /// `mine(int, BlockOptionalMetaLookup)` without the rescan.
    fn mine(&mut self, quantity: i32, filter: Option<Arc<BlockOptionalMetaLookup>>) {
        self.filter = filter;
        if self.filter_filter().is_none() {
            self.filter = None;
        }
        self.desired_quantity = quantity;
        self.known_ore_locations = Vec::new();
        self.blacklist = Vec::new();
        self.branch_point = None;
        self.branch_point_runaway = None;
        self.anticipated_drops = JavaHashMap::new(BetterBlockPos::block_pos_hash_code);
    }

    /// `cancel()`: `onLostControl()`, which is `mine(0, null)`.
    fn cancel(&mut self) {
        self.mine(0, None);
    }

    fn filter_filter(&self) -> Option<Arc<BlockOptionalMetaLookup>> {
        let filter = self.filter.as_ref()?;
        let settings = settings();
        if !settings.allow_break {
            let f = BlockOptionalMetaLookup::new(
                filter
                    .blocks()
                    .iter()
                    .filter(|e| {
                        settings
                            .allow_break_anyway
                            .iter()
                            .any(|b| b == e.get_block())
                    })
                    .cloned()
                    .collect(),
            );
            if f.blocks().is_empty() {
                log_direct(
                    "Unable to mine when allowBreak is false and target block is not in allowBreakAnyway!",
                );
                return None;
            }
            return Some(Arc::new(f));
        }
        Some(Arc::clone(filter))
    }

    fn update_louca_system(&mut self, ctx: &dyn IPlayerContext) {
        let mut copy = JavaHashMap::copy_of(&self.anticipated_drops);
        if let Some(pos) = ctx.get_selected_block()
            && self.known_ore_locations.contains(&pos)
        {
            copy.insert(
                pos,
                java::current_time_millis()
                    .wrapping_add(settings().mine_drop_loiter_duration_ms_thanks_louca),
            );
        }
        // elaborate dance to avoid concurrentmodificationexcepption since rescan thread reads this
        // don't want to slow everything down with a gross lock do we now
        for pos in self.anticipated_drops.keys() {
            if copy
                .get(pos)
                .is_some_and(|&t| t < java::current_time_millis())
            {
                copy.remove(pos);
            }
        }
        self.anticipated_drops = copy;
    }

    fn update_goal(&mut self, baritone: &Baritone) -> Option<PathingCommand> {
        let filter = self.filter_filter()?;

        let settings = settings();
        let legit = settings.legit_mine;
        if !self.known_ore_locations.is_empty() {
            let context = CalculationContext::from_baritone(baritone);
            let mut dropped = self.dropped_items_scan();
            let locs2 = prune(
                &context,
                self.known_ore_locations.clone(),
                &filter,
                settings.mine_max_ore_locations_count,
                &self.blacklist,
                &mut dropped,
            );
            let goal = GoalComposite::new(
                locs2
                    .iter()
                    .map(|&loc| self.coalesce(baritone, loc, &locs2, &context))
                    .collect(),
            );
            self.known_ore_locations = locs2;
            return Some(PathingCommand::new(
                Some(Arc::new(goal)),
                if legit {
                    PathingCommandType::ForceRevalidateGoalAndPath
                } else {
                    PathingCommandType::RevalidateGoalAndPath
                },
            ));
        }
        // we don't know any ore locations at the moment
        if !legit && !settings.explore_for_blocks {
            return None;
        }
        // only when we should explore for blocks or are in legit mode we do this
        let y = settings.legit_mine_y_level;
        let branch_point = *self
            .branch_point
            .get_or_insert_with(|| baritone.player_context.player_feet());
        // TODO shaft mode, mine 1x1 shafts to either side
        // TODO also, see if the GoalRunAway with maintain Y at 11 works even from the surface
        let runaway = self.branch_point_runaway.get_or_insert_with(|| {
            Arc::new(BranchPointRunaway(GoalRunAway::new(
                1.0,
                Some(y),
                vec![branch_point],
            )))
        });
        Some(PathingCommand::new(
            Some(Arc::clone(runaway)),
            PathingCommandType::RevalidateGoalAndPath,
        ))
    }

    fn internal_mining_goal(
        &self,
        pos: BetterBlockPos,
        context: &CalculationContext,
        locs: &[BetterBlockPos],
    ) -> bool {
        // Here, BlockStateInterface is used because the position may be in a cached chunk (the targeted block is one that is kept track of)
        if locs.contains(&pos) {
            return true;
        }
        let state = context.bsi.get0_pos(pos);
        if settings().internal_mining_air_exception && state.air {
            return true;
        }
        self.filter.as_ref().is_some_and(|filter| filter.has(state))
            && MineProcess::plausible_to_break(context, pos)
    }

    fn coalesce(
        &self,
        baritone: &Baritone,
        loc: BetterBlockPos,
        locs: &[BetterBlockPos],
        context: &CalculationContext,
    ) -> Arc<dyn Goal> {
        let assume_vertical_shaft_mine = !baritone
            .bsi
            .as_ref()
            .expect("the block state interface of this tick")
            .get0_pos(loc.above())
            .falls;
        if !settings().force_internal_mining {
            if assume_vertical_shaft_mine {
                // we can get directly below the block
                return Arc::new(GoalThreeBlocks::from_pos(loc));
            } else {
                // we need to get feet or head into the block
                return Arc::new(GoalTwoBlocks::from_pos(loc));
            }
        }
        let upward_goal = self.internal_mining_goal(loc.above(), context, locs);
        let downward_goal = self.internal_mining_goal(loc.below(), context, locs);
        let double_downward_goal = self.internal_mining_goal(loc.below_n(2), context, locs);
        if upward_goal == downward_goal {
            // symmetric
            if double_downward_goal && assume_vertical_shaft_mine {
                // we have a checkerboard like pattern
                // this one, and the one two below it
                // therefore it's fine to path to immediately below this one, since your feet will be in the doubleDownwardGoal
                // but only if assumeVerticalShaftMine
                return Arc::new(GoalThreeBlocks::from_pos(loc));
            } else {
                // this block has nothing interesting two below, but is symmetric vertically so we can get either feet or head into it
                return Arc::new(GoalTwoBlocks::from_pos(loc));
            }
        }
        if upward_goal {
            // downwardGoal known to be false
            // ignore the gap then potential doubleDownward, because we want to path feet into this one and head into upwardGoal
            return Arc::new(GoalBlock::from_pos(loc));
        }
        // upwardGoal known to be false, downwardGoal known to be true
        if double_downward_goal && assume_vertical_shaft_mine {
            // this block and two below it are goals
            // path into the center of the one below, because that includes directly below this one
            return Arc::new(GoalTwoBlocks::from_pos(loc.below()));
        }
        // upwardGoal false, downwardGoal true, doubleDownwardGoal false
        // just this block and the one immediately below, no others
        Arc::new(GoalBlock::from_pos(loc.below()))
    }

    /// `droppedItemsScan()`: the item entities are ignored, so only the anticipated drops.
    fn dropped_items_scan(&self) -> Vec<BetterBlockPos> {
        if !settings().mine_scan_dropped_items {
            return Vec::new();
        }
        self.anticipated_drops.keys().copied().collect()
    }

    fn add_nearby(&mut self, baritone: &Baritone) -> bool {
        let ctx = &baritone.player_context;
        let mut dropped = self.dropped_items_scan();
        self.known_ore_locations.extend_from_slice(&dropped);
        let player_feet = ctx.player_feet();
        let bsi = BlockStateInterface::from_ctx(ctx);

        let Some(filter) = self.filter_filter() else {
            return false;
        };

        let search_dist = 10;
        let faked_block_reach_distance = 20.0; // at least 10 * sqrt(3) with some extra space to account for positioning within the block
        let include_diagonals = settings().legit_mine_include_diagonals;
        let aim = baritone.look_behavior.get_aim_processor();
        for x in player_feet.x - search_dist..=player_feet.x + search_dist {
            for y in player_feet.y - search_dist..=player_feet.y + search_dist {
                for z in player_feet.z - search_dist..=player_feet.z + search_dist {
                    // crucial to only add blocks we can see because otherwise this
                    // is an x-ray and it'll get caught
                    if filter.has(bsi.get0(x, y, z)) {
                        let pos = BetterBlockPos::new(x, y, z);
                        if (include_diagonals
                            && self
                                .known_ore_locations
                                .iter()
                                .any(|ore| ore.distance_sq(&pos) <= 2.0)) // sq means this is pytha dist <= sqrt(2)
                            || rotation_utils::reachable_distance(
                                ctx,
                                aim,
                                pos,
                                faked_block_reach_distance,
                            )
                            .is_some()
                        {
                            self.known_ore_locations.push(pos);
                        }
                    }
                }
            }
        }
        self.known_ore_locations = prune(
            &CalculationContext::from_baritone(baritone),
            std::mem::take(&mut self.known_ore_locations),
            &filter,
            settings().mine_max_ore_locations_count,
            &self.blacklist,
            &mut dropped,
        );
        true
    }
}

/// `rescan(List<BlockPos>, CalculationContext)`
fn rescan(state: &Mutex<State>, already: Vec<BetterBlockPos>, context: &CalculationContext) {
    let (filter, blacklist, mut dropped) = {
        let s = lock(state);
        let Some(filter) = s.filter_filter() else {
            return;
        };
        if settings().legit_mine {
            return;
        }
        let dropped = s.dropped_items_scan();
        (filter, s.blacklist.clone(), dropped)
    };
    let mut locs = MineProcess::search_world(
        context,
        &filter,
        settings().mine_max_ore_locations_count,
        &already,
        &blacklist,
        &mut dropped,
    );
    locs.extend(dropped);
    let mut s = lock(state);
    if locs.is_empty() && !settings().explore_for_blocks {
        let message = format!("No locations for {filter} known, cancelling");
        log_direct(&message);
        if settings().notification_on_mine_fail {
            log_notification(&message, true);
        }
        s.cancel();
        return;
    }
    s.known_ore_locations = locs;
}

fn prune(
    ctx: &CalculationContext,
    locs2: Vec<BetterBlockPos>,
    filter: &BlockOptionalMetaLookup,
    max: i32,
    blacklist: &[BetterBlockPos],
    dropped: &mut Vec<BetterBlockPos>,
) -> Vec<BetterBlockPos> {
    dropped.retain(|drop| {
        !locs2.iter().any(|&pos| {
            pos.distance_sq(drop) <= 9.0
                && filter.has(ctx.get(pos.x, pos.y, pos.z))
                && MineProcess::plausible_to_break(ctx, pos)
        }) // TODO maybe drop also has to be supported? no lava below?
    });
    let settings = settings();
    let min_y = settings
        .min_y_level_while_mining
        .wrapping_add(ctx.world.dimension().min_y);
    let player_pos = ctx.player.block_position();
    let mut seen = FxHashSet::default();
    let mut locs: Vec<BetterBlockPos> = locs2
        .into_iter()
        .filter(|pos| seen.insert(*pos))
        // remove any that are within loaded chunks that aren't actually what we want
        .filter(|pos| {
            !ctx.bsi.world_contains_loaded_chunk(pos.x, pos.z)
                || filter.has(ctx.get(pos.x, pos.y, pos.z))
                || dropped.contains(pos)
        })
        // remove any that are implausible to mine (encased in bedrock, or touching lava)
        .filter(|&pos| MineProcess::plausible_to_break(ctx, pos))
        .filter(|&pos| {
            if settings.allow_only_exposed_ores {
                MineProcess::is_next_to_air(ctx, pos)
            } else {
                true
            }
        })
        .filter(|pos| pos.y >= min_y)
        .filter(|pos| pos.y <= settings.max_y_level_while_mining)
        .filter(|pos| !blacklist.contains(pos))
        .collect();
    locs.sort_by(|a, b| java::double_compare(player_pos.distance_sq(a), player_pos.distance_sq(b)));

    if locs.len() as i64 > max as i64 {
        let max = usize::try_from(max)
            .unwrap_or_else(|_| panic!("IllegalArgumentException: fromIndex(0) > toIndex({max})"));
        locs.truncate(max);
    }
    locs
}

impl IBaritoneProcess for MineProcess {
    fn is_active(&mut self, _baritone: &mut Baritone) -> bool {
        lock(&self.state).filter.is_some()
    }

    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        calc_failed: bool,
        is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        let state = Arc::clone(&self.state);
        let mut s = lock(&state);
        let settings = settings();
        if s.desired_quantity > 0 {
            let filter = s.filter.as_ref().expect("NullPointerException: filter");
            let inventory = &baritone.player_context.player().inventory;
            let curr = (0..Inventory::INVENTORY_SIZE)
                .map(|i| inventory.get_item(i))
                .filter(|stack| filter.has_stack(stack))
                .fold(0i32, |sum, stack| sum.wrapping_add(stack.count));
            if curr >= s.desired_quantity {
                log_direct(&format!("Have {curr} valid items"));
                s.cancel();
                return None;
            }
        }
        if calc_failed {
            if !s.known_ore_locations.is_empty() && settings.blacklist_closest_on_failure {
                let message = format!(
                    "Unable to find any path to {}, blacklisting presumably unreachable closest instance...",
                    filter_string(s.filter.as_ref())
                );
                log_direct(&message);
                if settings.notification_on_mine_fail {
                    log_notification(&message, true);
                }
                let feet = baritone.player_context.player_feet();
                if let Some(&closest) = s
                    .known_ore_locations
                    .iter()
                    .min_by(|a, b| java::double_compare(feet.distance_sq(a), feet.distance_sq(b)))
                {
                    s.blacklist.push(closest);
                }
                let State {
                    known_ore_locations,
                    blacklist,
                    ..
                } = &mut *s;
                known_ore_locations.retain(|pos| !blacklist.contains(pos));
            } else {
                let message = format!(
                    "Unable to find any path to {}, canceling mine",
                    filter_string(s.filter.as_ref())
                );
                log_direct(&message);
                if settings.notification_on_mine_fail {
                    log_notification(&message, true);
                }
                s.cancel();
                return None;
            }
        }

        s.update_louca_system(&baritone.player_context);
        let mine_goal_update_interval = settings.mine_goal_update_interval;
        let curr = s.known_ore_locations.clone();
        if mine_goal_update_interval != 0 && {
            let tick_count = self.tick_count;
            self.tick_count = tick_count.wrapping_add(1);
            tick_count.wrapping_rem(mine_goal_update_interval) == 0
        } {
            // big brain
            let context = CalculationContext::from_baritone_thread(baritone, true);
            let state = Arc::clone(&self.state);
            let already = curr.clone();
            baritone
                .executor
                .execute(move || rescan(&state, already, &context));
        }
        if settings.legit_mine && !s.add_nearby(baritone) {
            s.cancel();
            return None;
        }
        let ctx = &baritone.player_context;
        let feet = ctx.player_feet();
        let above = feet.above();
        let shaft = curr
            .iter()
            .filter(|pos| pos.x == feet.x && pos.z == feet.z)
            .filter(|pos| pos.y >= feet.y)
            .filter(|&&pos| !BlockStateInterface::get(ctx, pos).air) // after breaking a block, it takes mineGoalUpdateInterval ticks for it to actually update this list =(
            .min_by(|a, b| java::double_compare(above.distance_sq(a), above.distance_sq(b)))
            .copied();
        baritone.input_override_handler.clear_all_keys();
        if let Some(pos) = shaft
            && baritone.player_context.player().on_ground
        {
            let bsi = baritone
                .bsi
                .as_ref()
                .expect("the block state interface of this tick");
            if !movement_helper::avoid_breaking(bsi, pos.x, pos.y, pos.z, bsi.get0_pos(pos)) {
                let ctx = &baritone.player_context;
                let rot =
                    rotation_utils::reachable(ctx, baritone.look_behavior.get_aim_processor(), pos);
                if let Some(rot) = rot
                    && is_safe_to_cancel
                {
                    baritone.look_behavior.update_target(ctx, rot, true);
                    let world = Arc::clone(ctx.world());
                    movement_helper::switch_to_best_tool_for(
                        &mut baritone.player_context,
                        world.get_block_state(pos),
                    );
                    let ctx = &baritone.player_context;
                    if ctx.is_looking_at(pos) || ctx.player_rotations().is_really_close_to(&rot) {
                        baritone
                            .input_override_handler
                            .set_input_force_state(Input::ClickLeft, true);
                    }
                    return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
                }
            }
        }
        let command = s.update_goal(baritone);
        if command.is_none() {
            // none in range
            // maybe say something in chat? (ahem impact)
            s.cancel();
            return None;
        }
        command
    }

    fn is_temporary(&self) -> bool {
        false
    }

    fn on_lost_control(&mut self, _baritone: &mut Baritone) {
        lock(&self.state).cancel();
    }

    fn display_name0(&mut self, _baritone: &mut Baritone) -> String {
        format!("Mine {}", filter_string(lock(&self.state).filter.as_ref()))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `new GoalRunAway(1, y, branchPoint) { ... }`: never reached, so the path goes as far from
/// the branch point as it can.
#[derive(Clone, Debug, PartialEq)]
struct BranchPointRunaway(GoalRunAway);

impl Goal for BranchPointRunaway {
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

impl fmt::Display for BranchPointRunaway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// `GoalTwoBlocks` that also takes the block below its lower block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GoalThreeBlocks {
    x: i32,
    y: i32,
    z: i32,
}

impl GoalThreeBlocks {
    fn from_pos(pos: BetterBlockPos) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }
}

impl Goal for GoalThreeBlocks {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        x == self.x
            && (y == self.y || y == self.y.wrapping_sub(1) || y == self.y.wrapping_sub(2))
            && z == self.z
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let x_diff = x.wrapping_sub(self.x);
        let y_diff = y.wrapping_sub(self.y);
        let z_diff = z.wrapping_sub(self.z);
        GoalBlock::calculate(
            x_diff as f64,
            if y_diff < -1 {
                y_diff + 2
            } else if y_diff == -1 {
                0
            } else {
                y_diff
            },
            z_diff as f64,
        )
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }

    fn as_goal_render_pos(&self) -> Option<&dyn IGoalRenderPos> {
        Some(self)
    }
}

impl IGoalRenderPos for GoalThreeBlocks {
    fn get_goal_pos(&self) -> BetterBlockPos {
        BetterBlockPos::new(self.x, self.y, self.z)
    }
}

impl fmt::Display for GoalThreeBlocks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalThreeBlocks{{x={},y={},z={}}}",
            maybe_censor(self.x),
            maybe_censor(self.y),
            maybe_censor(self.z)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_three_blocks() {
        let goal = GoalThreeBlocks::from_pos(BetterBlockPos::new(0, 10, 0));
        assert!(goal.is_in_goal(0, 10, 0));
        assert!(goal.is_in_goal(0, 8, 0));
        assert!(!goal.is_in_goal(0, 7, 0));
        assert!(!goal.is_in_goal(0, 11, 0));
        assert_eq!(goal.heuristic(0, 9, 0), 0.0);
        assert_eq!(goal.heuristic(0, 8, 0), 0.0);
        assert_eq!(goal.heuristic(0, 5, 0), GoalBlock::calculate(0.0, -3, 0.0));
        assert_eq!(goal.heuristic(0, 13, 0), GoalBlock::calculate(0.0, 3, 0.0));
        assert!(goal.equals(&GoalThreeBlocks::from_pos(BetterBlockPos::new(0, 10, 0))));
        assert!(!goal.equals(&GoalTwoBlocks::new(0, 10, 0)));
        assert_eq!(goal.to_string(), "GoalThreeBlocks{x=0,y=10,z=0}");
    }

    #[test]
    fn branch_point_runaway() {
        let from = vec![BetterBlockPos::new(1, 2, 3)];
        let goal = BranchPointRunaway(GoalRunAway::new(1.0, Some(11), from.clone()));
        assert!(!goal.is_in_goal(100, 11, 100));
        assert_eq!(goal.heuristic_at_goal(), f64::NEG_INFINITY);
        assert_eq!(
            goal.heuristic(5, 6, 7),
            GoalRunAway::new(1.0, Some(11), from.clone()).heuristic(5, 6, 7)
        );
        assert!(goal.equals(&BranchPointRunaway(GoalRunAway::new(
            1.0,
            Some(11),
            from.clone()
        ))));
        assert!(!goal.equals(&GoalRunAway::new(1.0, Some(11), from)));
        assert_eq!(
            goal.to_string(),
            "GoalRunAwayFromMaintainY y=11, [BetterBlockPos{x=1,y=2,z=3}]"
        );
    }
}
