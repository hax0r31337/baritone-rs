// Ported from baritone src/main/java/baritone/behavior/PathingBehavior.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The two locks are Java monitors (`ReentrantMutex`): `pathPlanLock` guards the plan (the
// current and next executors, and the fields the calculation thread writes), `pathCalcLock`
// the start of a calculation. `inProgress` (volatile upstream) is a `SearchHandle` behind a
// plain mutex. Calculations run on a new thread each (upstream: a cached thread pool) with a
// clone of the calculation context. Hosts that want finished calculations to take effect only
// between ticks hold `path_plan_lock()` around the tick, like the reference tests do.
//
// `getCurrent()` / `getNext()` hand out references upstream; here they are `with_current` /
// `with_next`, which run a closure under the plan lock. Path events go to the `Baritone`'s
// event list (`GameEventHandler.onPathEvent`). Rendering (`onRenderPass`) is not ported, and
// `isSafeToCancel`'s elytra check is `true` (no elytra process).

use std::cell::RefCell;
use std::sync::{Arc, Mutex, PoisonError};

use crate::Baritone;
use crate::api::event::events::PathEvent;
use crate::api::event::events::tick_event;
use crate::api::event::events::r#type::EventState;
use crate::api::pathing::calc::{IPath, IPathFinder};
use crate::api::pathing::goals::{Goal, GoalXZ};
use crate::api::process::PathingCommand;
use crate::api::utils::helper::{log_debug, log_direct};
use crate::api::utils::path_calculation_result::Type;
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::java::{ReentrantMutex, double_compare};
use crate::pathing::calc::{AStarPathFinder, SearchHandle};
use crate::pathing::movement::CalculationContext;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::path::PathExecutor;
use crate::settings::settings;
use crate::utils::PathingControlManager;
use crate::utils::pathing::Favoring;

/// What `pathPlanLock` guards.
#[derive(Debug, Default)]
pub struct Plan {
    current: Option<PathExecutor>,
    next: Option<PathExecutor>,

    /*eta*/
    ticks_elapsed_so_far: i32,
    start_position: Option<BetterBlockPos>,

    expected_segment_start: Option<BetterBlockPos>,
}

type PlanLock = ReentrantMutex<RefCell<Plan>>;

pub struct PathingBehavior {
    path_plan_lock: Arc<PlanLock>,
    path_calc_lock: Arc<ReentrantMutex<()>>,
    in_progress: Arc<Mutex<Option<SearchHandle>>>,
    to_dispatch: Arc<Mutex<Vec<PathEvent>>>,

    goal: Option<Arc<dyn Goal>>,
    context: Option<CalculationContext>,

    safe_to_cancel: bool,
    pause_requested_last_tick: bool,
    unpaused_last_tick: bool,
    paused_this_tick: bool,
    cancel_requested: bool,
    calc_failed_last_tick: bool,

    last_auto_jump: bool,
}

impl Default for PathingBehavior {
    fn default() -> Self {
        Self::new()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn queue_path_event_to(to_dispatch: &Mutex<Vec<PathEvent>>, event: PathEvent) {
    lock(to_dispatch).push(event);
}

impl PathingBehavior {
    pub fn new() -> Self {
        Self {
            path_plan_lock: Arc::new(ReentrantMutex::new(RefCell::new(Plan::default()))),
            path_calc_lock: Arc::new(ReentrantMutex::new(())),
            in_progress: Arc::new(Mutex::new(None)),
            to_dispatch: Arc::new(Mutex::new(Vec::new())),
            goal: None,
            context: None,
            safe_to_cancel: false,
            pause_requested_last_tick: false,
            unpaused_last_tick: false,
            paused_this_tick: false,
            cancel_requested: false,
            calc_failed_last_tick: false,
            last_auto_jump: false,
        }
    }

    /// `pathPlanLock`. Holding it (`lock()`) keeps finished calculations from taking effect.
    pub fn path_plan_lock(&self) -> Arc<ReentrantMutex<RefCell<Plan>>> {
        Arc::clone(&self.path_plan_lock)
    }

    fn queue_path_event(&self, event: PathEvent) {
        queue_path_event_to(&self.to_dispatch, event);
    }

    fn dispatch_events(baritone: &mut Baritone) {
        let curr: Vec<PathEvent> =
            std::mem::take(&mut *lock(&baritone.pathing_behavior.to_dispatch));
        baritone.pathing_behavior.calc_failed_last_tick = curr.contains(&PathEvent::CalcFailed);
        for event in curr {
            baritone.game_event_handler.on_path_event(event);
        }
    }

    pub fn on_tick(baritone: &mut Baritone, event_type: tick_event::Type) {
        Self::dispatch_events(baritone);
        if event_type == tick_event::Type::Out {
            Self::secret_internal_segment_cancel(baritone);
            PathingControlManager::cancel_everything(baritone);
            return;
        }

        let expected_segment_start = Self::path_start(&baritone.player_context);
        baritone
            .pathing_behavior
            .path_plan_lock
            .lock()
            .borrow_mut()
            .expected_segment_start = Some(expected_segment_start);
        PathingControlManager::pre_tick(baritone);
        Self::tick_path(baritone);
        {
            let plan = baritone.pathing_behavior.path_plan_lock.lock();
            let mut plan = plan.borrow_mut();
            plan.ticks_elapsed_so_far = plan.ticks_elapsed_so_far.wrapping_add(1);
        }
        Self::dispatch_events(baritone);
    }

    /// `onPlayerSprintState(SprintStateEvent)`: what the sprint key should read this tick, if
    /// Baritone decides.
    pub fn on_player_sprint_state(&self) -> Option<bool> {
        if self.is_pathing() {
            return Some(self.with_current(|current| {
                current
                    .expect("NullPointerException: current")
                    .is_sprinting()
            }));
        }
        None
    }

    fn tick_path(baritone: &mut Baritone) {
        let pb = &mut baritone.pathing_behavior;
        pb.paused_this_tick = false;
        if pb.pause_requested_last_tick && pb.safe_to_cancel {
            pb.pause_requested_last_tick = false;
            if pb.unpaused_last_tick {
                baritone.input_override_handler.clear_all_keys();
                baritone
                    .input_override_handler
                    .get_block_break_helper()
                    .stop_breaking_block(&mut baritone.player_context);
            }
            let pb = &mut baritone.pathing_behavior;
            pb.unpaused_last_tick = false;
            pb.paused_this_tick = true;
            return;
        }
        pb.unpaused_last_tick = true;
        if pb.cancel_requested {
            pb.cancel_requested = false;
            baritone.input_override_handler.clear_all_keys();
        }
        let plan_lock = Arc::clone(&baritone.pathing_behavior.path_plan_lock);
        let plan_guard = plan_lock.lock();
        {
            let calc_lock = Arc::clone(&baritone.pathing_behavior.path_calc_lock);
            let _calc = calc_lock.lock();
            if let Some(in_progress) = baritone.pathing_behavior.get_in_progress() {
                // we are calculating
                // are we calculating the right thing though? 🤔
                let calc_from = in_progress.get_start();
                let current_best = in_progress.best_path_so_far();
                let plan = plan_guard.borrow();
                let feet = baritone.player_context.player_feet();
                let expected = plan.expected_segment_start;
                if plan
                    .current
                    .as_ref()
                    .is_none_or(|current| current.get_path().get_dest() != calc_from) // if current ends in inProgress's start, then we're ok
                    && calc_from != feet
                    && Some(calc_from) != expected // if current starts in our playerFeet or pathStart, then we're ok
                    && current_best.as_ref().is_none_or(|best| {
                        !best.contains(&feet) && !expected.is_some_and(|e| best.contains(&e))
                    })
                {
                    // when it was *just* started, currentBest will be empty so we need to also check calcFrom since that's always present
                    in_progress.cancel(); // cancellation doesn't dispatch any events
                }
            }
        }
        let mut plan = plan_guard.borrow_mut();
        let Some(current) = plan.current.as_mut() else {
            return;
        };
        baritone.pathing_behavior.safe_to_cancel = current.on_tick(baritone);
        if current.failed() || current.finished() {
            plan.current = None;
            let goal = baritone.pathing_behavior.goal.clone();
            let feet = baritone.player_context.player_feet();
            if goal.as_ref().is_none_or(|goal| goal.is_in_goal_pos(feet)) {
                log_debug(&format!(
                    "All done. At {}",
                    goal.as_ref().map_or("null".to_owned(), |g| g.to_string())
                ));
                baritone
                    .pathing_behavior
                    .queue_path_event(PathEvent::AtGoal);
                plan.next = None;
                if settings().disconnect_on_arrival {
                    baritone
                        .player_context
                        .player_controller()
                        .disconnect("[Baritone] Arrived at goal!");
                }
                return;
            }
            let expected = plan.expected_segment_start;
            if let Some(next) = &plan.next
                && !next.get_path().positions().contains(&feet)
                && !expected.is_some_and(|e| next.get_path().positions().contains(&e))
            {
                // can contain either one
                // if the current path failed, we may not actually be on the next one, so make sure
                log_debug("Discarding next path as it does not contain current position");
                // for example if we had a nicely planned ahead path that starts where current ends
                // that's all fine and good
                // but if we fail in the middle of current
                // we're nowhere close to our planned ahead path
                // so need to discard it sadly.
                baritone
                    .pathing_behavior
                    .queue_path_event(PathEvent::DiscardNext);
                plan.next = None;
            }
            if let Some(next) = plan.next.take() {
                log_debug("Continuing on to planned next path");
                baritone
                    .pathing_behavior
                    .queue_path_event(PathEvent::ContinuingOntoPlannedNext);
                let current = plan.current.insert(next);
                current.on_tick(baritone); // don't waste a tick doing nothing, get started right away
                return;
            }
            // at this point, current just ended, but we aren't in the goal and have no plan for the future
            let calc_lock = Arc::clone(&baritone.pathing_behavior.path_calc_lock);
            let _calc = calc_lock.lock();
            if baritone.pathing_behavior.get_in_progress().is_some() {
                baritone
                    .pathing_behavior
                    .queue_path_event(PathEvent::PathFinishedNextStillCalculating);
                return;
            }
            // we aren't calculating
            baritone
                .pathing_behavior
                .queue_path_event(PathEvent::CalcStarted);
            let start = plan.expected_segment_start;
            drop(plan);
            Self::find_path_in_new_thread(baritone, start, true, None);
            return;
        }
        // at this point, we know current is in progress
        if baritone.pathing_behavior.safe_to_cancel
            && let Some(next) = plan.next.as_mut()
            && next.snipsnapifpossible(baritone)
        {
            // a movement just ended; jump directly onto the next path
            log_debug("Splicing into planned next path early...");
            baritone
                .pathing_behavior
                .queue_path_event(PathEvent::SplicingOntoNextEarly);
            let next = plan.next.take();
            let current = plan.current.insert(next.unwrap());
            current.on_tick(baritone);
            return;
        }
        if settings().splice_path {
            let current = plan.current.take().unwrap();
            let spliced = current.try_splice(plan.next.as_ref());
            plan.current = Some(spliced);
        }
        let current_dest = plan.current.as_ref().unwrap().get_path().get_dest();
        if plan
            .next
            .as_ref()
            .is_some_and(|next| current_dest == next.get_path().get_dest())
        {
            plan.next = None;
        }
        let calc_lock = Arc::clone(&baritone.pathing_behavior.path_calc_lock);
        let _calc = calc_lock.lock();
        if baritone.pathing_behavior.get_in_progress().is_some() {
            // if we aren't calculating right now
            return;
        }
        if plan.next.is_some() {
            // and we have no plan for what to do next
            return;
        }
        if baritone
            .pathing_behavior
            .goal
            .as_ref()
            .is_none_or(|goal| goal.is_in_goal_pos(current_dest))
        {
            // and this path doesn't get us all the way there
            return;
        }
        let current = plan.current.as_ref().unwrap();
        if ticks_remaining_in_segment(current, false) < settings().planning_tick_lookahead as f64 {
            // and this path has 7.5 seconds or less left
            // don't include the current movement so a very long last movement (e.g. descend) doesn't trip it up
            // if we actually included current, it wouldn't start planning ahead until the last movement was done, if the last movement took more than 7.5 seconds on its own
            log_debug("Path almost over. Planning ahead...");
            baritone
                .pathing_behavior
                .queue_path_event(PathEvent::NextSegmentCalcStarted);
            Self::find_path_in_new_thread(
                baritone,
                Some(current_dest),
                false,
                Some(current.get_path()),
            );
        }
    }

    /// `onPlayerUpdate(PlayerUpdateEvent)`
    pub fn on_player_update(&mut self, ctx: &mut dyn IPlayerContext, state: EventState) {
        if self.has_path() {
            match state {
                EventState::Pre => {
                    self.last_auto_jump = ctx.options().auto_jump;
                    ctx.options_mut().auto_jump = false;
                }
                EventState::Post => {
                    ctx.options_mut().auto_jump = self.last_auto_jump;
                }
            }
        }
    }

    pub fn secret_internal_set_goal(&mut self, goal: Option<Arc<dyn Goal>>) {
        self.goal = goal;
    }

    pub fn secret_internal_set_goal_and_path(
        baritone: &mut Baritone,
        command: &PathingCommand,
    ) -> bool {
        baritone
            .pathing_behavior
            .secret_internal_set_goal(command.goal.clone());
        baritone.pathing_behavior.context = Some(match &command.desired_calc_context {
            Some(context) => context.clone(),
            None => CalculationContext::from_baritone_thread(baritone, true),
        });
        let Some(goal) = baritone.pathing_behavior.goal.clone() else {
            return false;
        };
        if goal.is_in_goal_pos(baritone.player_context.player_feet()) {
            return false;
        }
        let plan_lock = Arc::clone(&baritone.pathing_behavior.path_plan_lock);
        let plan = plan_lock.lock();
        if plan.borrow().current.is_some() {
            return false;
        }
        let calc_lock = Arc::clone(&baritone.pathing_behavior.path_calc_lock);
        let _calc = calc_lock.lock();
        if baritone.pathing_behavior.get_in_progress().is_some() {
            return false;
        }
        baritone
            .pathing_behavior
            .queue_path_event(PathEvent::CalcStarted);
        let start = plan.borrow().expected_segment_start;
        Self::find_path_in_new_thread(baritone, start, true, None);
        true
    }

    pub fn get_goal(&self) -> Option<&Arc<dyn Goal>> {
        self.goal.as_ref()
    }

    /// `hasPath()`: `getCurrent() != null`
    pub fn has_path(&self) -> bool {
        self.with_current(|current| current.is_some())
    }

    pub fn is_pathing(&self) -> bool {
        self.has_path() && !self.paused_this_tick
    }

    /// `getCurrent()`, under the plan lock.
    pub fn with_current<R>(&self, f: impl FnOnce(Option<&PathExecutor>) -> R) -> R {
        let plan = self.path_plan_lock.lock();
        let plan = plan.borrow();
        f(plan.current.as_ref())
    }

    /// `getNext()`, under the plan lock.
    pub fn with_next<R>(&self, f: impl FnOnce(Option<&PathExecutor>) -> R) -> R {
        let plan = self.path_plan_lock.lock();
        let plan = plan.borrow();
        f(plan.next.as_ref())
    }

    pub fn get_in_progress(&self) -> Option<SearchHandle> {
        lock(&self.in_progress).clone()
    }

    pub fn is_safe_to_cancel(&self) -> bool {
        if !self.has_path() {
            // !baritone.getElytraProcess().isActive() || ...: there is no elytra process
            return true;
        }
        self.safe_to_cancel
    }

    pub fn request_pause(&mut self) {
        self.pause_requested_last_tick = true;
    }

    pub fn cancel_segment_if_safe(baritone: &mut Baritone) -> bool {
        if baritone.pathing_behavior.is_safe_to_cancel() {
            Self::secret_internal_segment_cancel(baritone);
            return true;
        }
        false
    }

    pub fn cancel_everything(baritone: &mut Baritone) -> bool {
        let do_it = baritone.pathing_behavior.is_safe_to_cancel();
        if do_it {
            Self::secret_internal_segment_cancel(baritone);
        }
        PathingControlManager::cancel_everything(baritone); // regardless of if we can stop the current segment, we can still stop the processes
        do_it
    }

    /// NOT exposed on public api
    pub fn calc_failed_last_tick(&self) -> bool {
        self.calc_failed_last_tick
    }

    pub fn soft_cancel_if_safe(&mut self) {
        {
            let plan_lock = Arc::clone(&self.path_plan_lock);
            let plan = plan_lock.lock();
            if let Some(in_progress) = self.get_in_progress() {
                in_progress.cancel(); // only cancel ours
            }
            if !self.is_safe_to_cancel() {
                return;
            }
            let mut plan = plan.borrow_mut();
            plan.current = None;
            plan.next = None;
        }
        self.cancel_requested = true;
        // do everything BUT clear keys
    }

    /// just cancel the current path
    pub fn secret_internal_segment_cancel(baritone: &mut Baritone) {
        baritone
            .pathing_behavior
            .queue_path_event(PathEvent::Canceled);
        let plan_lock = Arc::clone(&baritone.pathing_behavior.path_plan_lock);
        let plan = plan_lock.lock();
        if let Some(in_progress) = baritone.pathing_behavior.get_in_progress() {
            in_progress.cancel();
        }
        let mut plan = plan.borrow_mut();
        if plan.current.is_some() {
            plan.current = None;
            plan.next = None;
            baritone.input_override_handler.clear_all_keys();
            baritone
                .input_override_handler
                .get_block_break_helper()
                .stop_breaking_block(&mut baritone.player_context);
        }
    }

    /// exposed on public api because :sob:
    pub fn force_cancel(baritone: &mut Baritone) {
        Self::cancel_everything(baritone);
        Self::secret_internal_segment_cancel(baritone);
        let calc_lock = Arc::clone(&baritone.pathing_behavior.path_calc_lock);
        let _calc = calc_lock.lock();
        *lock(&baritone.pathing_behavior.in_progress) = None;
    }

    pub fn secret_internal_get_calculation_context(&self) -> Option<&CalculationContext> {
        self.context.as_ref()
    }

    pub fn estimated_ticks_to_goal(&self, ctx: &dyn IPlayerContext) -> Option<f64> {
        let current_pos = ctx.player_feet();
        let plan_lock = self.path_plan_lock.lock();
        let (Some(goal), Some(start_position)) =
            (self.goal.as_ref(), plan_lock.borrow().start_position)
        else {
            return None;
        };
        if goal.is_in_goal_pos(ctx.player_feet()) {
            let mut plan = plan_lock.borrow_mut();
            // resetEstimatedTicksToGoal()
            plan.ticks_elapsed_so_far = 0;
            plan.start_position = plan.expected_segment_start;
            return Some(0.0);
        }
        let ticks_elapsed_so_far = plan_lock.borrow().ticks_elapsed_so_far;
        if ticks_elapsed_so_far == 0 {
            return None;
        }
        let current = goal.heuristic(current_pos.x, current_pos.y, current_pos.z);
        let start = goal.heuristic(start_position.x, start_position.y, start_position.z);
        if current == start {
            // can't check above because current and start can be equal even if currentPos and startPosition are not
            return None;
        }
        let eta = (current - goal.heuristic_at_goal()).abs() * ticks_elapsed_so_far as f64
            / (start - current).abs();
        Some(eta)
    }

    /// See issue #209
    ///
    /// Returns the starting `BlockPos` for a new path
    pub fn path_start(ctx: &dyn IPlayerContext) -> BetterBlockPos {
        // TODO move to a helper or util class
        let feet = ctx.player_feet();
        if !mh::can_walk_on_ctx(ctx, feet.below()) {
            if ctx.player().on_ground {
                let player_x = ctx.player().position.x;
                let player_z = ctx.player().position.z;
                let mut closest = Vec::with_capacity(9);
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        closest.push(BetterBlockPos::new(feet.x + dx, feet.y, feet.z + dz));
                    }
                }
                // List.sort is stable; Comparator.comparingDouble orders like Double.compare
                closest.sort_by(|a, b| {
                    let key = |pos: &BetterBlockPos| {
                        ((pos.x as f64 + 0.5) - player_x) * ((pos.x as f64 + 0.5) - player_x)
                            + ((pos.z as f64 + 0.5) - player_z) * ((pos.z as f64 + 0.5) - player_z)
                    };
                    double_compare(key(a), key(b))
                });
                for &possible_support in closest.iter().take(4) {
                    let x_dist = ((possible_support.x as f64 + 0.5) - player_x).abs();
                    let z_dist = ((possible_support.z as f64 + 0.5) - player_z).abs();
                    if x_dist > 0.8 && z_dist > 0.8 {
                        // can't possibly be sneaking off of this one, we're too far away
                        continue;
                    }
                    if mh::can_walk_on_ctx(ctx, possible_support.below())
                        && mh::can_walk_through_ctx(ctx, possible_support)
                        && mh::can_walk_through_ctx(ctx, possible_support.above())
                    {
                        // this is plausible
                        //logDebug("Faking path start assuming player is standing off the edge of a block");
                        return possible_support;
                    }
                }
            } else {
                // !onGround
                // we're in the middle of a jump
                if mh::can_walk_on_ctx(ctx, feet.below().below()) {
                    //logDebug("Faking path start assuming player is midair and falling");
                    return feet.below();
                }
            }
        }
        feet
    }

    /// In a new thread, pathfind to target blockpos
    ///
    /// Upstream reads `current` for the timeouts and the previous path; the port's caller
    /// passes the current path, since it holds the plan.
    fn find_path_in_new_thread(
        baritone: &mut Baritone,
        start: Option<BetterBlockPos>,
        talk_about_it: bool,
        current_path: Option<&dyn IPath>,
    ) {
        let pb = &baritone.pathing_behavior;
        // this must be called with synchronization on pathCalcLock!
        // actually, we can check this, muahaha
        if !pb.path_calc_lock.is_held_by_current_thread() {
            panic!("Must be called with synchronization on pathCalcLock");
            // why do it this way? it's already indented so much that putting the whole thing in a synchronized(pathCalcLock) was just too much lol
        }
        if pb.get_in_progress().is_some() {
            panic!("Already doing it"); // should have been checked by caller
        }
        let context = pb.context.as_ref().expect("NullPointerException: context");
        if !context.safe_for_threaded_use {
            panic!("Improper context thread safety level");
        }
        let Some(goal) = pb.goal.clone() else {
            log_debug("no goal"); // TODO should this be an exception too? definitely should be checked by caller
            return;
        };
        let start = start.expect("NullPointerException: start");
        let settings = settings();
        let (primary_timeout, failure_timeout) = if current_path.is_none() {
            (settings.primary_timeout_ms, settings.failure_timeout_ms)
        } else {
            (
                settings.plan_ahead_primary_timeout_ms,
                settings.plan_ahead_failure_timeout_ms,
            )
        };
        drop(settings);
        let context = context.clone();
        let mut pathfinder = Self::create_pathfinder(
            &baritone.player_context,
            start,
            Arc::clone(&goal),
            current_path,
            context,
        );
        let pathfinder_goal = pathfinder.get_goal();
        if !Arc::ptr_eq(pathfinder_goal, &goal) && !pathfinder_goal.equals(&*goal) {
            // will return the exact same object if simplification didn't happen
            log_debug(&format!("Simplifying {goal:?} to GoalXZ due to distance"));
        }
        let pb = &baritone.pathing_behavior;
        *lock(&pb.in_progress) = Some(pathfinder.search().handle());
        let plan_lock = Arc::clone(&pb.path_plan_lock);
        let calc_lock = Arc::clone(&pb.path_calc_lock);
        let in_progress = Arc::clone(&pb.in_progress);
        let to_dispatch = Arc::clone(&pb.to_dispatch);
        std::thread::Builder::new()
            .name("Baritone path calculation".to_owned())
            .spawn(move || {
                if talk_about_it {
                    log_debug(&format!(
                        "Starting to search for path from {start} to {goal}"
                    ));
                }

                let calc_result = pathfinder.calculate(primary_timeout, failure_timeout);
                let plan = plan_lock.lock();
                let mut plan = plan.borrow_mut();
                let result_type = calc_result.get_type();
                let executor = calc_result.into_path().map(PathExecutor::new);
                if plan.current.is_none() {
                    match executor {
                        Some(executor) => {
                            let expected = plan.expected_segment_start;
                            if expected
                                .is_some_and(|e| executor.get_path().positions().contains(&e))
                            {
                                queue_path_event_to(
                                    &to_dispatch,
                                    PathEvent::CalcFinishedNowExecuting,
                                );
                                plan.current = Some(executor);
                                // resetEstimatedTicksToGoal(start)
                                plan.ticks_elapsed_so_far = 0;
                                plan.start_position = Some(start);
                            } else {
                                log_debug(
                                    "Warning: discarding orphan path segment with incorrect start",
                                );
                            }
                        }
                        None => {
                            if result_type != Type::Cancellation && result_type != Type::Exception {
                                // don't dispatch CALC_FAILED on cancellation
                                queue_path_event_to(&to_dispatch, PathEvent::CalcFailed);
                            }
                        }
                    }
                } else if plan.next.is_none() {
                    match executor {
                        Some(executor) => {
                            let current_dest = plan.current.as_ref().unwrap().get_path().get_dest();
                            if executor.get_path().get_src() == current_dest {
                                queue_path_event_to(
                                    &to_dispatch,
                                    PathEvent::NextSegmentCalcFinished,
                                );
                                plan.next = Some(executor);
                            } else {
                                log_debug(
                                    "Warning: discarding orphan next segment with incorrect start",
                                );
                            }
                        }
                        None => queue_path_event_to(&to_dispatch, PathEvent::NextCalcFailed),
                    }
                } else {
                    //throw new IllegalStateException("I have no idea what to do with this path");
                    // no point in throwing an exception here, and it gets it stuck with inProgress being not null
                    log_direct("Warning: PathingBehaivor illegal state! Discarding invalid path!");
                }
                if talk_about_it && let Some(current) = &plan.current {
                    let path = current.get_path();
                    if goal.is_in_goal_pos(path.get_dest()) {
                        log_debug(&format!(
                            "Finished finding a path from {start} to {goal}. {} nodes considered",
                            path.get_num_nodes_considered()
                        ));
                    } else {
                        log_debug(&format!(
                            "Found path segment from {start} towards {goal}. {} nodes considered",
                            path.get_num_nodes_considered()
                        ));
                    }
                }
                let _calc = calc_lock.lock();
                *lock(&in_progress) = None;
            })
            .expect("failed to spawn the path calculation thread");
    }

    fn create_pathfinder(
        ctx: &dyn IPlayerContext,
        start: BetterBlockPos,
        goal: Arc<dyn Goal>,
        previous: Option<&dyn IPath>,
        context: CalculationContext,
    ) -> AStarPathFinder {
        let mut transformed = goal;
        if settings().simplify_unloaded_y_coord
            && let Some(render_pos) = transformed.as_goal_render_pos()
        {
            let pos = render_pos.get_goal_pos();
            if !context.bsi.world_contains_loaded_chunk(pos.x, pos.z) {
                transformed = Arc::new(GoalXZ::new(pos.x, pos.z));
            }
        }
        let favoring = Favoring::from_ctx(ctx, previous, &context);
        let feet = ctx.player_feet();
        let mut real_start = start;
        let sub = feet.subtract(real_start);
        if feet.y == real_start.y && sub.x.wrapping_abs() <= 1 && sub.z.wrapping_abs() <= 1 {
            real_start = feet;
        }
        AStarPathFinder::new(
            real_start,
            start.x,
            start.y,
            start.z,
            transformed,
            favoring,
            context,
        )
    }
}

/// `IPathingBehavior.ticksRemainingInSegment(boolean)` for the current executor.
fn ticks_remaining_in_segment(current: &PathExecutor, include_current_movement: bool) -> f64 {
    let start = if include_current_movement {
        current.get_position()
    } else {
        current.get_position() + 1
    };
    current
        .get_path()
        .ticks_remaining_from(usize::try_from(start).unwrap_or(usize::MAX))
}
