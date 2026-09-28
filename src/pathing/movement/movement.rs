// Ported from baritone src/main/java/baritone/pathing/movement/Movement.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The subclasses are the variants of `MovementKind`; each keeps its own fields in the struct of
// its file under `movements/`, and `Movement` dispatches the overridden methods to them.
// Movements do not hold the `IBaritone`/`IPlayerContext`: execution passes the `Baritone` in.
// `updateState` changes the state it is given where upstream changes and returns it; the
// subclasses that call `super.updateState` / `super.prepared` call `update_state_default` /
// `prepared_default`.

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::movement::{IMovement, MovementStatus};
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext, rotation_utils, vec_utils};
use crate::behavior::PathingBehavior;
use crate::host::Entity;
use crate::mc::{Aabb, Direction};
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movement_state::MovementTarget;
use crate::pathing::movement::movements::{
    MovementAscend, MovementDescend, MovementDiagonal, MovementDownward, MovementFall,
    MovementParkour, MovementPillar, MovementTraverse,
};
use crate::pathing::movement::{CalculationContext, MovementState};
use crate::settings::settings;
use crate::utils::BlockStateInterface;

pub const HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP: [Direction; 5] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
    Direction::Down,
];

/// The subclass of a [`Movement`], with its own fields.
#[derive(Clone, Debug, PartialEq)]
pub enum MovementKind {
    Ascend(MovementAscend),
    Descend(MovementDescend),
    Diagonal(MovementDiagonal),
    Downward(MovementDownward),
    Fall(MovementFall),
    Parkour(MovementParkour),
    Pillar(MovementPillar),
    Traverse(MovementTraverse),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Movement {
    current_state: MovementState,

    pub(crate) src: BetterBlockPos,

    pub(crate) dest: BetterBlockPos,

    /// The positions that need to be broken before this movement can ensue
    pub(crate) positions_to_break: Box<[BetterBlockPos]>,

    /// The position where we need to place a block before this movement can ensue
    pub(crate) position_to_place: Option<BetterBlockPos>,

    cost: Option<f64>,

    pub to_break_cached: Option<Vec<BetterBlockPos>>,
    pub to_place_cached: Option<Vec<BetterBlockPos>>,
    pub to_walk_into_cached: Option<Vec<BetterBlockPos>>,

    valid_positions_cached: Option<FxHashSet<BetterBlockPos>>,

    calculated_while_loaded: Option<bool>,

    pub(crate) kind: MovementKind,
}

impl Movement {
    /// `Movement(IBaritone, BetterBlockPos, BetterBlockPos, BetterBlockPos[], BetterBlockPos)`;
    /// `position_to_place` is `None` for the constructor without it.
    pub(crate) fn new(
        src: BetterBlockPos,
        dest: BetterBlockPos,
        to_break: Box<[BetterBlockPos]>,
        to_place: Option<BetterBlockPos>,
        kind: MovementKind,
    ) -> Self {
        Self {
            current_state: MovementState::default(),
            src,
            dest,
            positions_to_break: to_break,
            position_to_place: to_place,
            cost: None,
            to_break_cached: None,
            to_place_cached: None,
            to_walk_into_cached: None,
            valid_positions_cached: None,
            calculated_while_loaded: None,
            kind,
        }
    }

    /// The subclass and its fields.
    pub fn kind(&self) -> &MovementKind {
        &self.kind
    }

    /// The subclass and its fields.
    pub fn kind_mut(&mut self) -> &mut MovementKind {
        &mut self.kind
    }

    /// The upstream class name (`"MovementTraverse"`).
    pub fn class_name(&self) -> &'static str {
        match self.kind {
            MovementKind::Ascend(_) => "MovementAscend",
            MovementKind::Descend(_) => "MovementDescend",
            MovementKind::Diagonal(_) => "MovementDiagonal",
            MovementKind::Downward(_) => "MovementDownward",
            MovementKind::Fall(_) => "MovementFall",
            MovementKind::Parkour(_) => "MovementParkour",
            MovementKind::Pillar(_) => "MovementPillar",
            MovementKind::Traverse(_) => "MovementTraverse",
        }
    }

    /// `getCost()`: panics if the cost was never calculated or set (upstream unboxes `null`).
    pub fn get_cost(&self) -> f64 {
        self.cost.expect("NullPointerException: movement cost")
    }

    /// `getCost(CalculationContext)`
    pub fn get_cost_context(&mut self, context: &CalculationContext) -> f64 {
        match self.cost {
            Some(cost) => cost,
            None => {
                let cost = self.calculate_cost(context);
                self.cost = Some(cost);
                cost
            }
        }
    }

    pub fn calculate_cost(&self, context: &CalculationContext) -> f64 {
        match &self.kind {
            MovementKind::Ascend(_) => MovementAscend::calculate_cost(self, context),
            MovementKind::Descend(_) => MovementDescend::calculate_cost(self, context),
            MovementKind::Diagonal(_) => MovementDiagonal::calculate_cost(self, context),
            MovementKind::Downward(_) => MovementDownward::calculate_cost(self, context),
            MovementKind::Fall(_) => MovementFall::calculate_cost(self, context),
            MovementKind::Parkour(m) => m.calculate_cost(self, context),
            MovementKind::Pillar(_) => MovementPillar::calculate_cost(self, context),
            MovementKind::Traverse(_) => MovementTraverse::calculate_cost(self, context),
        }
    }

    pub fn recalculate_cost(&mut self, context: &CalculationContext) -> f64 {
        self.cost = None;
        self.get_cost_context(context)
    }

    /// `override(double)` (`override` is reserved in Rust)
    pub fn override_cost(&mut self, cost: f64) {
        self.cost = Some(cost);
    }

    fn calculate_valid_positions(&self) -> FxHashSet<BetterBlockPos> {
        match &self.kind {
            MovementKind::Ascend(_) => MovementAscend::calculate_valid_positions(self),
            MovementKind::Descend(_) => MovementDescend::calculate_valid_positions(self),
            MovementKind::Diagonal(_) => MovementDiagonal::calculate_valid_positions(self),
            MovementKind::Downward(_) => MovementDownward::calculate_valid_positions(self),
            MovementKind::Fall(_) => MovementFall::calculate_valid_positions(self),
            MovementKind::Parkour(m) => m.calculate_valid_positions(self),
            MovementKind::Pillar(_) => MovementPillar::calculate_valid_positions(self),
            MovementKind::Traverse(_) => MovementTraverse::calculate_valid_positions(self),
        }
    }

    pub fn get_valid_positions(&mut self) -> &FxHashSet<BetterBlockPos> {
        if self.valid_positions_cached.is_none() {
            self.valid_positions_cached = Some(self.calculate_valid_positions());
        }
        self.valid_positions_cached.as_ref().unwrap()
    }

    pub(crate) fn player_in_valid_position(&mut self, baritone: &Baritone) -> bool {
        let ctx = &baritone.player_context;
        let feet = ctx.player_feet();
        let path_start = PathingBehavior::path_start(ctx);
        let valid = self.get_valid_positions();
        valid.contains(&feet) || valid.contains(&path_start)
    }

    /// Handles the execution of the latest Movement
    /// State, and offers a Status to the calling class.
    pub fn update(&mut self, baritone: &mut Baritone) -> MovementStatus {
        baritone.player_context.player_mut().flying = false;
        let mut current_state = std::mem::take(&mut self.current_state);
        self.update_state(baritone, &mut current_state);
        let ctx = &mut baritone.player_context;
        if mh::is_liquid_ctx(ctx, ctx.player_feet())
            && ctx.player().position.y < self.dest.y as f64 + 0.6
        {
            current_state.set_input(Input::Jump, true);
        }
        if ctx.player().in_wall {
            if let Some(pos) = ctx.get_selected_block() {
                let state = BlockStateInterface::get(ctx, pos).clone();
                mh::switch_to_best_tool_for(ctx, &state);
            }
            current_state.set_input(Input::ClickLeft, true);
        }

        // If the movement target has to force the new rotations, or we aren't using silent move, then force the rotations
        if let Some(rotation) = current_state.get_target().get_rotation() {
            let force = current_state.get_target().has_to_force_rotations();
            baritone
                .look_behavior
                .update_target(&baritone.player_context, rotation, force);
        }
        let handler = &mut baritone.input_override_handler;
        handler.clear_all_keys();
        for (&input, &forced) in current_state.get_input_states() {
            handler.set_input_force_state(input, forced);
        }
        current_state.get_input_states_mut().clear();

        // If the current status indicates a completed movement
        if current_state.get_status().is_complete() {
            handler.clear_all_keys();
        }

        let status = current_state.get_status();
        self.current_state = current_state;
        status
    }

    pub(crate) fn prepared(&mut self, baritone: &mut Baritone, state: &mut MovementState) -> bool {
        match self.kind {
            MovementKind::Diagonal(_) => MovementDiagonal::prepared(self, baritone, state),
            MovementKind::Fall(_) => MovementFall::prepared(self, baritone, state),
            MovementKind::Pillar(_) => MovementPillar::prepared(self, baritone, state),
            MovementKind::Traverse(_) => MovementTraverse::prepared(self, baritone, state),
            _ => self.prepared_default(baritone, state),
        }
    }

    /// `Movement.prepared(MovementState)`, what the subclasses reach as `super.prepared`.
    // every branch that sets somethingInTheWay returns, so its final check never fires (kept)
    #[allow(unused_assignments)]
    pub(crate) fn prepared_default(
        &mut self,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) -> bool {
        if state.get_status() == MovementStatus::Waiting {
            return true;
        }
        let mut something_in_the_way = false;
        for &block_pos in self.positions_to_break.iter() {
            let ctx = &mut baritone.player_context;
            let falling = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.1, 1.0).move_pos(block_pos);
            if ctx.entities().iter().any(|entity| {
                entity.type_id == Entity::FALLING_BLOCK && entity.bounding_box.intersects(&falling)
            }) && settings().pause_mining_for_falling_blocks
            {
                return false;
            }
            if !mh::can_walk_through_ctx(ctx, block_pos) {
                // can't break air, so don't try
                something_in_the_way = true;
                let block_state = BlockStateInterface::get(ctx, block_pos).clone();
                mh::switch_to_best_tool_for(ctx, &block_state);
                let reachable = rotation_utils::reachable_distance(
                    ctx,
                    baritone.look_behavior.get_aim_processor(),
                    block_pos,
                    ctx.player_controller_ref().get_block_reach_distance(),
                );
                if let Some(rot_towards_block) = reachable {
                    state.set_target(MovementTarget::new(rot_towards_block, true));
                    if ctx.is_looking_at(block_pos)
                        || ctx
                            .player_rotations()
                            .is_really_close_to(&rot_towards_block)
                    {
                        state.set_input(Input::ClickLeft, true);
                    }
                    return false;
                }
                //get rekt minecraft
                //i'm doing it anyway
                //i dont care if theres snow in the way!!!!!!!
                //you dont own me!!!!
                state.set_target(MovementTarget::new(
                    rotation_utils::calc_rotation_from_vec3d(
                        ctx.player_head(),
                        vec_utils::get_block_pos_center(block_pos),
                        ctx.player_rotations(),
                    ),
                    true,
                ));
                // don't check selectedblock on this one, this is a fallback when we can't see any face directly, it's intended to be breaking the "incorrect" block
                state.set_input(Input::ClickLeft, true);
                return false;
            }
        }
        if something_in_the_way {
            // There's a block or blocks that we can't walk through, but we have no target rotation to reach any
            // So don't return true, actually set state to unreachable
            state.set_status(MovementStatus::Unreachable);
            return true;
        }
        true
    }

    pub fn safe_to_cancel(&self, baritone: &Baritone) -> bool {
        self.safe_to_cancel_state(baritone, &self.current_state)
    }

    /// `safeToCancel(MovementState)`
    fn safe_to_cancel_state(&self, baritone: &Baritone, current_state: &MovementState) -> bool {
        match &self.kind {
            MovementKind::Ascend(m) => m.safe_to_cancel(current_state),
            MovementKind::Diagonal(_) => MovementDiagonal::safe_to_cancel(self, baritone),
            MovementKind::Fall(_) => MovementFall::safe_to_cancel(self, baritone, current_state),
            MovementKind::Parkour(_) => MovementParkour::safe_to_cancel(current_state),
            MovementKind::Traverse(_) => {
                MovementTraverse::safe_to_cancel(self, baritone, current_state)
            }
            _ => true,
        }
    }

    pub fn get_src(&self) -> BetterBlockPos {
        self.src
    }

    pub fn get_dest(&self) -> BetterBlockPos {
        self.dest
    }

    pub fn reset(&mut self) {
        self.current_state = MovementState::default();
        match &mut self.kind {
            MovementKind::Ascend(m) => m.ticks_without_placement = 0,
            MovementKind::Descend(m) => {
                m.num_ticks = 0;
                m.force_safe_mode = false;
            }
            MovementKind::Downward(m) => m.num_ticks = 0,
            MovementKind::Traverse(m) => m.was_the_bridge_block_always_there = true,
            _ => {}
        }
    }

    /// Calculate latest movement state. Gets called once a tick.
    pub fn update_state(&mut self, baritone: &mut Baritone, state: &mut MovementState) {
        match self.kind {
            MovementKind::Ascend(_) => MovementAscend::update_state(self, baritone, state),
            MovementKind::Descend(_) => MovementDescend::update_state(self, baritone, state),
            MovementKind::Diagonal(_) => MovementDiagonal::update_state(self, baritone, state),
            MovementKind::Downward(_) => MovementDownward::update_state(self, baritone, state),
            MovementKind::Fall(_) => MovementFall::update_state(self, baritone, state),
            MovementKind::Parkour(_) => MovementParkour::update_state(self, baritone, state),
            MovementKind::Pillar(_) => MovementPillar::update_state(self, baritone, state),
            MovementKind::Traverse(_) => MovementTraverse::update_state(self, baritone, state),
        }
    }

    /// `Movement.updateState(MovementState)`, what the subclasses reach as `super.updateState`.
    pub(crate) fn update_state_default(
        &mut self,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) {
        if !self.prepared(baritone, state) {
            state.set_status(MovementStatus::Prepping);
            return;
        } else if state.get_status() == MovementStatus::Prepping {
            state.set_status(MovementStatus::Waiting);
        }

        if state.get_status() == MovementStatus::Waiting {
            state.set_status(MovementStatus::Running);
        }
    }

    /// `BlockPos getDirection()`
    pub fn get_direction(&self) -> BetterBlockPos {
        self.get_dest().subtract(self.get_src())
    }

    pub fn check_loaded_chunk(&mut self, context: &CalculationContext) {
        self.calculated_while_loaded = Some(
            context
                .bsi
                .world_contains_loaded_chunk(self.dest.x, self.dest.z),
        );
    }

    /// Panics if [`Movement::check_loaded_chunk`] never ran (upstream unboxes `null`).
    pub fn calculated_while_loaded(&self) -> bool {
        self.calculated_while_loaded
            .expect("NullPointerException: calculatedWhileLoaded")
    }

    /// Resets the cache for special break, place, and walk into blocks
    pub fn reset_block_cache(&mut self) {
        self.to_break_cached = None;
        self.to_place_cached = None;
        self.to_walk_into_cached = None;
    }

    pub fn to_break(&mut self, bsi: &BlockStateInterface) -> Vec<BetterBlockPos> {
        if let MovementKind::Diagonal(_) = self.kind {
            return MovementDiagonal::to_break(self, bsi);
        }
        if let Some(to_break_cached) = &self.to_break_cached {
            return to_break_cached.clone();
        }
        let mut result = Vec::new();
        for &position_to_break in self.positions_to_break.iter() {
            if !mh::can_walk_through_bsi(
                bsi,
                position_to_break.x,
                position_to_break.y,
                position_to_break.z,
            ) {
                result.push(position_to_break);
            }
        }
        self.to_break_cached = Some(result.clone());
        result
    }

    pub fn to_place(&mut self, bsi: &BlockStateInterface) -> Vec<BetterBlockPos> {
        if let Some(to_place_cached) = &self.to_place_cached {
            return to_place_cached.clone();
        }
        let mut result = Vec::new();
        if let Some(position_to_place) = self.position_to_place
            && !mh::can_walk_on_bsi(
                bsi,
                position_to_place.x,
                position_to_place.y,
                position_to_place.z,
            )
        {
            result.push(position_to_place);
        }
        self.to_place_cached = Some(result.clone());
        result
    }

    pub fn to_walk_into(&mut self, bsi: &BlockStateInterface) -> Vec<BetterBlockPos> {
        // overridden by movementdiagonal
        if let MovementKind::Diagonal(_) = self.kind {
            return MovementDiagonal::to_walk_into(self, bsi);
        }
        self.to_walk_into_cached
            .get_or_insert_with(Vec::new)
            .clone()
    }

    pub fn to_break_all(&self) -> &[BetterBlockPos] {
        &self.positions_to_break
    }

    /// The state the last [`Movement::update`] left.
    pub fn current_state(&self) -> &MovementState {
        &self.current_state
    }
}

impl IMovement for Movement {
    fn get_cost(&self) -> f64 {
        Movement::get_cost(self)
    }

    fn calculated_while_loaded(&self) -> bool {
        Movement::calculated_while_loaded(self)
    }

    fn get_src(&self) -> BetterBlockPos {
        self.src
    }

    fn get_dest(&self) -> BetterBlockPos {
        self.dest
    }

    fn get_direction(&self) -> BetterBlockPos {
        Movement::get_direction(self)
    }
}
