// Ported from baritone src/main/java/baritone/pathing/path/PathExecutor.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The executor does not hold its `PathingBehavior`; ticking it takes the `Baritone`.
// `trySplice` and `cutIfTooLong` consume the executor and return the one to keep, which is
// itself when upstream returns `this`. Positions in the path are `i32` like upstream, since
// `pathPosition` runs past the end (`cancel` sets it to `length + 3`).

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::calc::IPath;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::helper::{log_debug, println};
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext, rotation_utils, vec_utils};
use crate::host::Fluid;
use crate::mc::Vec3;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movements::MovementDescend;
use crate::pathing::movement::{CalculationContext, Movement, MovementKind};
use crate::pathing::path::{CutoffPath, SplicedPath};
use crate::settings::settings;
use crate::utils::BlockStateInterface;

const MAX_MAX_DIST_FROM_PATH: f64 = 3.0;
const MAX_DIST_FROM_PATH: f64 = 2.0;

/// Default value is equal to 10 seconds. It's find to decrease it, but it must be at least 5.5s (110 ticks).
/// For more information, see issue #102.
///
/// See <https://github.com/cabaletta/baritone/issues/102> and <https://i.imgur.com/5s5GLnI.png> (Anime)
const MAX_TICKS_AWAY: f64 = 200.0;

/// Behavior to execute a precomputed path
#[derive(Debug)]
pub struct PathExecutor {
    path: Box<dyn IPath>,
    path_position: i32,
    ticks_away: i32,
    ticks_on_current: i32,
    current_movement_original_cost_estimate: Option<f64>,
    cost_estimate_index: Option<i32>,
    failed: bool,
    recalc_bp: bool,
    to_break: FxHashSet<BetterBlockPos>,
    to_place: FxHashSet<BetterBlockPos>,
    to_walk_into: FxHashSet<BetterBlockPos>,

    sprint_next_tick: bool,
}

fn is_traverse(m: &Movement) -> bool {
    matches!(m.kind(), MovementKind::Traverse(_))
}

fn is_ascend(m: &Movement) -> bool {
    matches!(m.kind(), MovementKind::Ascend(_))
}

fn is_descend(m: &Movement) -> bool {
    matches!(m.kind(), MovementKind::Descend(_))
}

impl PathExecutor {
    pub fn new(path: Box<dyn IPath>) -> Self {
        Self {
            path,
            path_position: 0,
            ticks_away: 0,
            ticks_on_current: 0,
            current_movement_original_cost_estimate: None,
            cost_estimate_index: None,
            failed: false,
            recalc_bp: true,
            to_break: FxHashSet::default(),
            to_place: FxHashSet::default(),
            to_walk_into: FxHashSet::default(),
            sprint_next_tick: false,
        }
    }

    fn length(&self) -> i32 {
        self.path.length() as i32
    }

    fn movement(&self, i: i32) -> &Movement {
        &self.path.movements()[usize::try_from(i).expect("IndexOutOfBoundsException")]
    }

    fn movement_mut(&mut self, i: i32) -> &mut Movement {
        &mut self.path.movements_mut()[usize::try_from(i).expect("IndexOutOfBoundsException")]
    }

    /// Tick this executor
    ///
    /// Returns true if a movement just finished (and the player is therefore in a "stable" state, like,
    /// not sneaking out over lava), false otherwise
    pub fn on_tick(&mut self, baritone: &mut Baritone) -> bool {
        if self.path_position == self.length() - 1 {
            self.path_position += 1;
        }
        if self.path_position >= self.length() {
            return true; // stop bugging me, I'm done
        }
        let where_am_i = baritone.player_context.player_feet();
        let path_position = self.path_position;
        if !self
            .movement_mut(path_position)
            .get_valid_positions()
            .contains(&where_am_i)
        {
            let mut i = 0;
            while i < self.path_position && i < self.length() {
                //this happens for example when you lag out and get teleported back a couple blocks
                if self
                    .movement_mut(i)
                    .get_valid_positions()
                    .contains(&where_am_i)
                {
                    let previous_pos = self.path_position;
                    self.path_position = i;
                    for j in self.path_position..=previous_pos {
                        self.movement_mut(j).reset();
                    }
                    self.on_change_in_path_position(baritone);
                    self.on_tick(baritone);
                    return false;
                }
                i += 1;
            }
            let mut i = self.path_position + 3;
            while i < self.length() - 1 {
                //dont check pathPosition+1. the movement tells us when it's done (e.g. sneak placing)
                // also don't check pathPosition+2 because reasons
                if self
                    .movement_mut(i)
                    .get_valid_positions()
                    .contains(&where_am_i)
                {
                    if i - self.path_position > 2 {
                        log_debug(&format!(
                            "Skipping forward {} steps, to {}",
                            i - self.path_position,
                            i
                        ));
                    }
                    //System.out.println("Double skip sundae");
                    self.path_position = i - 1;
                    self.on_change_in_path_position(baritone);
                    self.on_tick(baritone);
                    return false;
                }
                i += 1;
            }
        }
        let status = self.closest_path_pos(baritone);
        if self.possibly_off_path(baritone, status, MAX_DIST_FROM_PATH) {
            self.ticks_away += 1;
            println(&format!(
                "FAR AWAY FROM PATH FOR {} TICKS. Current distance: {}. Threshold: {}",
                self.ticks_away, status.0, MAX_DIST_FROM_PATH
            ));
            if self.ticks_away as f64 > MAX_TICKS_AWAY {
                log_debug("Too far away from path for too long, cancelling path");
                self.cancel(baritone);
                return false;
            }
        } else {
            self.ticks_away = 0;
        }
        if self.possibly_off_path(baritone, status, MAX_MAX_DIST_FROM_PATH) {
            // ok, stop right away, we're way too far.
            log_debug("too far from path");
            self.cancel(baritone);
            return false;
        }
        //long start = System.nanoTime() / 1000000L;
        let bsi = BlockStateInterface::from_ctx(&baritone.player_context);
        for i in self.path_position - 10..self.path_position + 10 {
            if i < 0 || i >= self.path.movements().len() as i32 {
                continue;
            }
            let m = self.movement_mut(i);
            let prev_break = m.to_break(&bsi);
            let prev_place = m.to_place(&bsi);
            let prev_walk_into = m.to_walk_into(&bsi);
            m.reset_block_cache();
            let changed_break = prev_break != m.to_break(&bsi);
            let changed_place = prev_place != m.to_place(&bsi);
            let changed_walk_into = prev_walk_into != m.to_walk_into(&bsi);
            if changed_break {
                self.recalc_bp = true;
            }
            if changed_place {
                self.recalc_bp = true;
            }
            if changed_walk_into {
                self.recalc_bp = true;
            }
        }
        if self.recalc_bp {
            let mut new_break = FxHashSet::default();
            let mut new_place = FxHashSet::default();
            let mut new_walk_into = FxHashSet::default();
            for i in self.path_position..self.path.movements().len() as i32 {
                let m = self.movement_mut(i);
                new_break.extend(m.to_break(&bsi));
                new_place.extend(m.to_place(&bsi));
                new_walk_into.extend(m.to_walk_into(&bsi));
            }
            self.to_break = new_break;
            self.to_place = new_place;
            self.to_walk_into = new_walk_into;
            self.recalc_bp = false;
        }
        /*long end = System.nanoTime() / 1000000L;
        if (end - start > 0) {
            System.out.println("Recalculating break and place took " + (end - start) + "ms");
        }*/
        if self.path_position < self.path.movements().len() as i32 - 1 {
            let next = self.movement(self.path_position + 1).get_dest();
            if !baritone
                .bsi
                .as_ref()
                .expect("NullPointerException: bsi")
                .world_contains_loaded_chunk(next.x, next.z)
            {
                log_debug("Pausing since destination is at edge of loaded chunks");
                self.clear_keys(baritone);
                return true;
            }
        }
        let can_cancel = self.movement(self.path_position).safe_to_cancel(baritone);
        if self.cost_estimate_index != Some(self.path_position) {
            self.cost_estimate_index = Some(self.path_position);
            // do this only once, when the movement starts, and deliberately get the cost as cached when this path was calculated, not the cost as it is right now
            self.current_movement_original_cost_estimate =
                Some(self.movement(self.path_position).get_cost());
            let mut i = 1;
            while i < settings().cost_verification_lookahead
                && self.path_position + i < self.length() - 1
            {
                let context = secret_internal_get_calculation_context(baritone);
                if self
                    .movement(self.path_position + i)
                    .calculate_cost(context)
                    >= COST_INF
                    && can_cancel
                {
                    log_debug(
                        "Something has changed in the world and a future movement has become impossible. Cancelling.",
                    );
                    self.cancel(baritone);
                    return true;
                }
                i += 1;
            }
        }
        let path_position = self.path_position;
        let current_cost = {
            let context = baritone
                .pathing_behavior
                .secret_internal_get_calculation_context()
                .expect("NullPointerException: context");
            self.movement_mut(path_position).recalculate_cost(context)
        };
        if current_cost >= COST_INF && can_cancel {
            log_debug(
                "Something has changed in the world and this movement has become impossible. Cancelling.",
            );
            self.cancel(baritone);
            return true;
        }
        let original = self
            .current_movement_original_cost_estimate
            .expect("NullPointerException: currentMovementOriginalCostEstimate");
        if !self.movement(self.path_position).calculated_while_loaded()
            && current_cost - original > settings().max_cost_increase
            && can_cancel
        {
            // don't do this if the movement was calculated while loaded
            // that means that this isn't a cache error, it's just part of the path interfering with a later part
            log_debug(&format!(
                "Original cost {original} current cost {current_cost}. Cancelling."
            ));
            self.cancel(baritone);
            return true;
        }
        if self.should_pause(baritone) {
            log_debug("Pausing since current best path is a backtrack");
            self.clear_keys(baritone);
            return true;
        }
        let movement_status = self.movement_mut(path_position).update(baritone);
        if movement_status == MovementStatus::Unreachable
            || movement_status == MovementStatus::Failed
        {
            log_debug(&format!(
                "Movement returns status {}",
                movement_status.name()
            ));
            self.cancel(baritone);
            return true;
        }
        if movement_status == MovementStatus::Success {
            //System.out.println("Movement done, next path");
            self.path_position += 1;
            self.on_change_in_path_position(baritone);
            self.on_tick(baritone);
            return true;
        } else {
            self.sprint_next_tick = self.should_sprint_next_tick(baritone);
            if !self.sprint_next_tick {
                baritone.player_context.player_mut().sprinting = false; // letting go of control doesn't make you stop sprinting actually
            }
            self.ticks_on_current += 1;
            if self.ticks_on_current as f64
                > self
                    .current_movement_original_cost_estimate
                    .expect("NullPointerException: currentMovementOriginalCostEstimate")
                    + settings().movement_timeout_ticks as f64
            {
                // only cancel if the total time has exceeded the initial estimate
                // as you break the blocks required, the remaining cost goes down, to the point where
                // ticksOnCurrent is greater than recalculateCost + 100
                // this is why we cache cost at the beginning, and don't recalculate for this comparison every tick
                log_debug(&format!(
                    "This movement has taken too long ({} ticks, expected {}). Cancelling.",
                    self.ticks_on_current,
                    self.current_movement_original_cost_estimate.unwrap()
                ));
                self.cancel(baritone);
                return true;
            }
        }
        can_cancel // movement is in progress, but if it reports cancellable, PathingBehavior is good to cut onto the next path
    }

    /// `closestPathPos(IPath)`: the distance and the position.
    fn closest_path_pos(&mut self, baritone: &Baritone) -> (f64, Option<BetterBlockPos>) {
        let mut best = -1.0;
        let mut best_pos = None;
        let position = baritone.player_context.player().position;
        for movement in self.path.movements_mut() {
            for &pos in movement.get_valid_positions() {
                let dist = vec_utils::entity_distance_to_center(position, pos);
                if dist < best || best == -1.0 {
                    best = dist;
                    best_pos = Some(pos);
                }
            }
        }
        (best, best_pos)
    }

    fn should_pause(&self, baritone: &Baritone) -> bool {
        let Some(current) = baritone.pathing_behavior.get_in_progress() else {
            return false;
        };
        let ctx = &baritone.player_context;
        if !ctx.player().on_ground {
            return false;
        }
        if !mh::can_walk_on_ctx(ctx, ctx.player_feet().below()) {
            // we're in some kind of sketchy situation, maybe parkouring
            return false;
        }
        if !mh::can_walk_through_ctx(ctx, ctx.player_feet())
            || !mh::can_walk_through_ctx(ctx, ctx.player_feet().above())
        {
            // suffocating?
            return false;
        }
        if !self.movement(self.path_position).safe_to_cancel(baritone) {
            return false;
        }
        let Some(positions) = current.best_path_so_far() else {
            return false;
        };
        if positions.len() < 3 {
            return false; // not long enough yet to justify pausing, its far from certain we'll actually take this route
        }
        // the first block of the next path will always overlap
        // no need to pause our very last movement when it would have otherwise cleanly exited with MovementStatus SUCCESS
        positions[1..].contains(&ctx.player_feet())
    }

    fn possibly_off_path(
        &self,
        baritone: &Baritone,
        status: (f64, Option<BetterBlockPos>),
        leniency: f64,
    ) -> bool {
        let distance_from_path = status.0;
        if distance_from_path > leniency {
            // when we're midair in the middle of a fall, we're very far from both the beginning and the end, but we aren't actually off path
            if let MovementKind::Fall(_) = self.movement(self.path_position).kind() {
                let fall_dest = self.path.positions()[(self.path_position + 1) as usize]; // .get(pathPosition) is the block we fell off of
                vec_utils::entity_flat_distance_to_center(
                    baritone.player_context.player().position,
                    fall_dest,
                ) >= leniency // ignore Y by using flat distance
            } else {
                true
            }
        } else {
            false
        }
    }

    /// Regardless of current path position, snap to the current player feet if possible
    ///
    /// Returns whether or not it was possible to snap to the current player feet
    pub fn snipsnapifpossible(&mut self, baritone: &mut Baritone) -> bool {
        let ctx = &baritone.player_context;
        if !ctx.player().on_ground
            && ctx.world().get_block_state(ctx.player_feet()).fluid == Fluid::Empty
        {
            // if we're falling in the air, and not in water, don't splice
            return false;
        } else {
            // we are either onGround or in liquid
            if ctx.player().delta_movement.y < -0.1 {
                // if we are strictly moving downwards (not stationary)
                // we could be falling through water, which could be unsafe to splice
                return false; // so don't
            }
        }
        let feet = ctx.player_feet();
        let Some(index) = self.path.positions().iter().position(|&p| p == feet) else {
            return false;
        };
        self.path_position = index as i32; // jump directly to current position
        self.clear_keys(baritone);
        true
    }

    fn should_sprint_next_tick(&mut self, baritone: &mut Baritone) -> bool {
        let requested = baritone
            .input_override_handler
            .is_input_forced_down(Input::Sprint);

        // we'll take it from here, no need for minecraft to see we're holding down control and sprint for us
        baritone
            .input_override_handler
            .set_input_force_state(Input::Sprint, false);

        // first and foremost, if allowSprint is off, or if we don't have enough hunger, don't try and sprint
        if !CalculationContext::from_baritone_thread(baritone, false).can_sprint {
            return false;
        }
        let pp = self.path_position;
        let length = self.length();

        // traverse requests sprinting, so we need to do this check first
        if is_traverse(self.movement(pp)) && pp < length - 3 {
            let next = self.movement(pp + 1);
            if is_ascend(next)
                && sprintable_ascend(
                    &baritone.player_context,
                    self.movement(pp),
                    next,
                    self.movement(pp + 2),
                )
            {
                if skip_now(&baritone.player_context, self.movement(pp)) {
                    log_debug("Skipping traverse to straight ascend");
                    self.path_position += 1;
                    self.on_change_in_path_position(baritone);
                    self.on_tick(baritone);
                    baritone
                        .input_override_handler
                        .set_input_force_state(Input::Jump, true);
                    return true;
                } else {
                    log_debug("Too far to the side to safely sprint ascend");
                }
            }
        }

        // if the movement requested sprinting, then we're done
        if requested {
            return true;
        }

        // however, descend and ascend don't request sprinting, because they don't know the context of what movement comes after it
        if is_descend(self.movement(pp)) {
            if pp < length - 2 {
                // keep this out of onTick, even if that means a tick of delay before it has an effect
                let next = self.movement(pp + 1);
                if mh::can_use_frost_walker_ctx(&baritone.player_context, next.get_dest().below()) {
                    // frostwalker only works if you cross the edge of the block on ground so in some cases we may not overshoot
                    // Since MovementDescend can't know the next movement we have to tell it
                    let next_is_parkour = matches!(next.kind(), MovementKind::Parkour(_));
                    if is_traverse(next) || next_is_parkour {
                        let could_place_instead = settings().allow_place
                            && baritone
                                .inventory_behavior
                                .has_generic_throwaway(&baritone.player_context)
                            && next_is_parkour; // traverse doesn't react fast enough
                        // this is true if the next movement does not ascend or descends and goes into the same cardinal direction (N-NE-E-SE-S-SW-W-NW) as the descend
                        // in that case current.getDirection() is e.g. (0, -1, 1) and next.getDirection() is e.g. (0, 0, 3) so the cross product of (0, 0, 1) and (0, 0, 3) is taken, which is (0, 0, 0) because the vectors are colinear (don't form a plane)
                        // since movements in exactly the opposite direction (e.g. descend (0, -1, 1) and traverse (0, 0, -1)) would also pass this check we also have to rule out that case
                        // we can do that by adding the directions because traverse is always 1 long like descend and parkour can't jump through current.getSrc().down()
                        let current_direction = self.movement(pp).get_direction();
                        let same_flat_direction =
                            current_direction.above().offset(next.get_direction())
                                != BetterBlockPos::ORIGIN
                                && current_direction.above().cross(next.get_direction())
                                    == BetterBlockPos::ORIGIN; // here's why you learn maths in school
                        if same_flat_direction
                            && !could_place_instead
                            && let MovementKind::Descend(current) = self.movement_mut(pp).kind_mut()
                        {
                            current.force_safe_mode();
                        }
                    }
                }
            }
            if MovementDescend::safe_mode(self.movement(pp), &baritone.player_context)
                && !MovementDescend::skip_to_ascend(self.movement(pp), &baritone.player_context)
            {
                log_debug("Sprinting would be unsafe");
                return false;
            }

            if pp < length - 2 {
                let current = self.movement(pp);
                let next = self.movement(pp + 1);
                if is_ascend(next)
                    && current.get_direction().above() == next.get_direction().below()
                {
                    // a descend then an ascend in the same direction
                    self.path_position += 1;
                    self.on_change_in_path_position(baritone);
                    self.on_tick(baritone);
                    // okay to skip clearKeys and / or onChangeInPathPosition here since this isn't possible to repeat, since it's asymmetric
                    log_debug("Skipping descend to straight ascend");
                    return true;
                }
                if can_sprint_from_descend_into(&baritone.player_context, current, next) {
                    if is_descend(next) && pp < length - 3 {
                        let next_next = self.movement(pp + 2);
                        if is_descend(next_next)
                            && !can_sprint_from_descend_into(
                                &baritone.player_context,
                                next,
                                next_next,
                            )
                        {
                            return false;
                        }
                    }
                    if baritone.player_context.player_feet() == current.get_dest() {
                        self.path_position += 1;
                        self.on_change_in_path_position(baritone);
                        self.on_tick(baritone);
                    }

                    return true;
                }
                //logDebug("Turning off sprinting " + movement + " " + next + " " + movement.getDirection() + " " + next.getDirection().down() + " " + next.getDirection().down().equals(movement.getDirection()));
            }
        }
        if is_ascend(self.movement(pp)) && pp != 0 {
            let current = self.movement(pp);
            let prev = self.movement(pp - 1);
            if is_descend(prev) && prev.get_direction().above() == current.get_direction().below() {
                let center = current.get_src().above();
                // playerFeet adds 0.1251 to account for soul sand
                // farmland is 0.9375
                // 0.07 is to account for farmland
                if baritone.player_context.player().position.y >= center.y as f64 - 0.07 {
                    baritone
                        .input_override_handler
                        .set_input_force_state(Input::Jump, false);
                    return true;
                }
            }
            if pp < length - 2
                && is_traverse(prev)
                && sprintable_ascend(
                    &baritone.player_context,
                    prev,
                    current,
                    self.movement(pp + 1),
                )
            {
                return true;
            }
        }
        if let MovementKind::Fall(_) = self.movement(pp).kind()
            && let Some((first, second)) = self.override_fall(&baritone.player_context)
        {
            let fall_dest = second;
            if !self.path.positions().contains(&fall_dest) {
                let current = self.movement(pp);
                panic!(
                    "Fall override at {} returned illegal destination {}",
                    current.get_src(),
                    fall_dest
                );
            }
            if baritone.player_context.player_feet() == fall_dest {
                self.path_position = self
                    .path
                    .positions()
                    .iter()
                    .position(|&p| p == fall_dest)
                    .unwrap() as i32;
                self.on_change_in_path_position(baritone);
                self.on_tick(baritone);
                return true;
            }
            self.clear_keys(baritone);
            let ctx = &baritone.player_context;
            let rotation = rotation_utils::calc_rotation_from_vec3d(
                ctx.player_head(),
                first,
                ctx.player_rotations(),
            );
            baritone.look_behavior.update_target(ctx, rotation, false);
            baritone
                .input_override_handler
                .set_input_force_state(Input::MoveForward, true);
            return true;
        }
        false
    }

    fn override_fall(&self, ctx: &dyn IPlayerContext) -> Option<(Vec3, BetterBlockPos)> {
        let movement = self.movement(self.path_position);
        let dir = movement.get_direction();
        if dir.y < -3 {
            return None;
        }
        if !movement
            .to_break_cached
            .as_ref()
            .expect("NullPointerException: toBreakCached")
            .is_empty()
        {
            return None; // it's breaking
        }
        let flat_dir = BetterBlockPos::new(dir.x, 0, dir.z);
        let mut i = self.path_position + 1;
        'outer: while i < self.length() - 1 && i < self.path_position + 3 {
            let next = self.movement(i);
            if !is_traverse(next) {
                break;
            }
            if flat_dir != next.get_direction() {
                break;
            }
            let mut y = next.get_dest().y;
            while y <= movement.get_src().y + 1 {
                let chk = BetterBlockPos::new(next.get_dest().x, y, next.get_dest().z);
                if !mh::fully_passable_ctx(ctx, chk) {
                    break 'outer;
                }
                y += 1;
            }
            if !mh::can_walk_on_ctx(ctx, next.get_dest().below()) {
                break;
            }
            i += 1;
        }
        i -= 1;
        if i == self.path_position {
            return None; // no valid extension exists
        }
        let len = (i - self.path_position) as f64 - 0.4;
        let dest = movement.get_dest();
        Some((
            Vec3::new(
                flat_dir.x as f64 * len + dest.x as f64 + 0.5,
                dest.y as f64,
                flat_dir.z as f64 * len + dest.z as f64 + 0.5,
            ),
            dest.offset_xyz(
                flat_dir.x * (i - self.path_position),
                0,
                flat_dir.z * (i - self.path_position),
            ),
        ))
    }

    fn on_change_in_path_position(&mut self, baritone: &mut Baritone) {
        self.clear_keys(baritone);
        self.ticks_on_current = 0;
    }

    fn clear_keys(&self, baritone: &mut Baritone) {
        // i'm just sick and tired of this snippet being everywhere lol
        baritone.input_override_handler.clear_all_keys();
    }

    fn cancel(&mut self, baritone: &mut Baritone) {
        self.clear_keys(baritone);
        let Baritone {
            input_override_handler,
            player_context,
            ..
        } = baritone;
        input_override_handler
            .get_block_break_helper()
            .stop_breaking_block(player_context);
        self.path_position = self.length() + 3;
        self.failed = true;
    }

    pub fn get_position(&self) -> i32 {
        self.path_position
    }

    /// `trySplice(PathExecutor)`
    pub fn try_splice(self, next: Option<&PathExecutor>) -> PathExecutor {
        let Some(next) = next else {
            return self.cut_if_too_long();
        };
        match SplicedPath::try_splice(Some(&*self.path), Some(&*next.path), false) {
            Some(path) => {
                if path.get_dest() != next.get_path().get_dest() {
                    panic!(
                        "Path has end {} instead of {} after splicing",
                        path.get_dest(),
                        next.get_path().get_dest()
                    );
                }
                let mut ret = PathExecutor::new(Box::new(path));
                ret.path_position = self.path_position;
                ret.current_movement_original_cost_estimate =
                    self.current_movement_original_cost_estimate;
                ret.cost_estimate_index = self.cost_estimate_index;
                ret.ticks_on_current = self.ticks_on_current;
                ret
            }
            None => self.cut_if_too_long(), // dont actually call cutIfTooLong every tick if we won't actually use it, use a method reference
        }
    }

    fn cut_if_too_long(self) -> PathExecutor {
        let settings = settings();
        if self.path_position > settings.max_path_history_length {
            let cutoff_amt = settings.path_history_cutoff_amount;
            let new_path = CutoffPath::new_range(
                &*self.path,
                usize::try_from(cutoff_amt).expect("IndexOutOfBoundsException"),
                self.path.length() - 1,
            );
            if new_path.get_dest() != self.path.get_dest() {
                panic!(
                    "Path has end {} instead of {} after trimming its start",
                    new_path.get_dest(),
                    self.path.get_dest()
                );
            }
            log_debug(&format!(
                "Discarding earliest segment movements, length cut from {} to {}",
                self.path.length(),
                new_path.length()
            ));
            let mut ret = PathExecutor::new(Box::new(new_path));
            ret.path_position = self.path_position - cutoff_amt;
            ret.current_movement_original_cost_estimate =
                self.current_movement_original_cost_estimate;
            if let Some(cost_estimate_index) = self.cost_estimate_index {
                ret.cost_estimate_index = Some(cost_estimate_index - cutoff_amt);
            }
            ret.ticks_on_current = self.ticks_on_current;
            return ret;
        }
        self
    }

    pub fn get_path(&self) -> &dyn IPath {
        &*self.path
    }

    pub fn failed(&self) -> bool {
        self.failed
    }

    pub fn finished(&self) -> bool {
        self.path_position >= self.length()
    }

    pub fn to_break(&self) -> &FxHashSet<BetterBlockPos> {
        &self.to_break
    }

    pub fn to_place(&self) -> &FxHashSet<BetterBlockPos> {
        &self.to_place
    }

    pub fn to_walk_into(&self) -> &FxHashSet<BetterBlockPos> {
        &self.to_walk_into
    }

    pub fn is_sprinting(&self) -> bool {
        self.sprint_next_tick
    }
}

/// `behavior.secretInternalGetCalculationContext()`
fn secret_internal_get_calculation_context(baritone: &Baritone) -> &CalculationContext {
    baritone
        .pathing_behavior
        .secret_internal_get_calculation_context()
        .expect("NullPointerException: context")
}

fn skip_now(ctx: &dyn IPlayerContext, current: &Movement) -> bool {
    let position = ctx.player().position;
    let direction = current.get_direction();
    let off_target = (direction.x as f64 * (current.get_src().z as f64 + 0.5 - position.z)).abs()
        + (direction.z as f64 * (current.get_src().x as f64 + 0.5 - position.x)).abs();
    if off_target > 0.1 {
        return false;
    }
    // we are centered
    let head_bonk = current.get_src().subtract(direction).above_n(2);
    if mh::fully_passable_ctx(ctx, head_bonk) {
        return true;
    }
    // wait 0.3
    let flat_dist = (direction.x as f64 * (head_bonk.x as f64 + 0.5 - position.x)).abs()
        + (direction.z as f64 * (head_bonk.z as f64 + 0.5 - position.z)).abs();
    flat_dist > 0.8
}

fn sprintable_ascend(
    ctx: &dyn IPlayerContext,
    current: &Movement,
    next: &Movement,
    nextnext: &Movement,
) -> bool {
    if !settings().sprint_ascends {
        return false;
    }
    if current.get_direction() != next.get_direction().below() {
        return false;
    }
    if nextnext.get_direction().x != next.get_direction().x
        || nextnext.get_direction().z != next.get_direction().z
    {
        return false;
    }
    if !mh::can_walk_on_ctx(ctx, current.get_dest().below()) {
        return false;
    }
    if !mh::can_walk_on_ctx(ctx, next.get_dest().below()) {
        return false;
    }
    if !next
        .to_break_cached
        .as_ref()
        .expect("NullPointerException: toBreakCached")
        .is_empty()
    {
        return false; // it's breaking
    }
    for x in 0..2 {
        for y in 0..3 {
            let mut chk = current.get_src().above_n(y);
            if x == 1 {
                chk = chk.offset(current.get_direction());
            }
            if !mh::fully_passable_ctx(ctx, chk) {
                return false;
            }
        }
    }
    if mh::avoid_walking_into(ctx.world().get_block_state(current.get_src().above_n(3))) {
        return false;
    }
    !mh::avoid_walking_into(ctx.world().get_block_state(next.get_dest().above_n(2))) // codacy smh my head
}

fn can_sprint_from_descend_into(
    ctx: &dyn IPlayerContext,
    current: &Movement,
    next: &Movement,
) -> bool {
    if is_descend(next) && next.get_direction() == current.get_direction() {
        return true;
    }
    if !mh::can_walk_on_ctx(ctx, current.get_dest().offset(current.get_direction())) {
        return false;
    }
    if is_traverse(next) && next.get_direction() == current.get_direction() {
        return true;
    }
    matches!(next.kind(), MovementKind::Diagonal(_)) && settings().allow_overshoot_diagonal_descend
}
