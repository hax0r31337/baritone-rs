// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementFall.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The Nether check (`dimension() == Level.NETHER`) is the world's `water_evaporates`.

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext, Rotation, rotation_utils, vec_utils};
use crate::host::{Climbable, Fluid, Inventory};
use crate::mc::{Direction, Vec3};
use crate::pathing::movement::calculation_context::STACK_BUCKET_WATER;
use crate::pathing::movement::movement::MovementKind;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movement_state::MovementTarget;
use crate::pathing::movement::movements::{MovementDescend, is_magma};
use crate::pathing::movement::{CalculationContext, Movement, MovementState};
use crate::utils::pathing::MutableMoveResult;

/// `Items.BUCKET`
const STACK_BUCKET_EMPTY: &str = "minecraft:bucket";

#[derive(Clone, Debug, PartialEq)]
pub struct MovementFall;

impl MovementFall {
    pub fn new(src: BetterBlockPos, dest: BetterBlockPos) -> Movement {
        Movement::new(
            src,
            dest,
            Self::build_positions_to_break(src, dest),
            None,
            MovementKind::Fall(MovementFall),
        )
    }

    pub(crate) fn calculate_cost(m: &Movement, context: &CalculationContext) -> f64 {
        let mut result = MutableMoveResult::new();
        MovementDescend::cost(
            context,
            m.src.x,
            m.src.y,
            m.src.z,
            m.dest.x,
            m.dest.z,
            &mut result,
        );
        if result.y != m.dest.y {
            return COST_INF; // doesn't apply to us, this position is a descend not a fall
        }
        result.cost
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        let mut set = FxHashSet::default();
        set.insert(m.src);
        let mut y = m.src.y.wrapping_sub(m.dest.y);
        while y >= 0 {
            set.insert(m.dest.above_n(y));
            y -= 1;
        }
        set
    }

    fn will_place_bucket(m: &Movement, baritone: &Baritone) -> bool {
        let context = CalculationContext::from_baritone(baritone);
        let mut result = MutableMoveResult::new();
        let (src, dest) = (m.src, m.dest);
        MovementDescend::dynamic_fall_cost(
            &context,
            src.x,
            src.y,
            src.z,
            dest.x,
            dest.z,
            0.0,
            context.get(dest.x, src.y - 2, dest.z),
            &mut result,
        )
    }

    pub(crate) fn update_state(
        m: &mut Movement,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) {
        m.update_state_default(baritone, state);
        if state.get_status() != MovementStatus::Running {
            return;
        }

        let (src, dest) = (m.src, m.dest);
        let will_place_bucket = || Self::will_place_bucket(m, baritone);
        let ctx = &baritone.player_context;
        let player_feet = ctx.player_feet();
        let to_dest = rotation_utils::calc_rotation_from_vec3d(
            ctx.player_head(),
            vec_utils::get_block_pos_center(dest),
            ctx.player_rotations(),
        );
        let mut target_rotation = None;
        let dest_state = ctx.world().get_block_state(dest);

        if is_magma(ctx.world().get_block_state(dest.below()))
            && mh::stepping_on_blocks(ctx)
                .into_iter()
                .all(|block| mh::can_walk_through_ctx(ctx, block))
        {
            state.set_input(Input::Sneak, true);
        }

        // WaterFluid: still or flowing water
        let is_water = dest_state.fluid == Fluid::Water;
        if !is_water && will_place_bucket() && player_feet != dest {
            let water_bucket = ctx
                .player()
                .inventory
                .find_slot_matching_item(STACK_BUCKET_WATER);
            if !Inventory::is_hotbar_slot(water_bucket) || ctx.world().dimension().water_evaporates
            {
                state.set_status(MovementStatus::Unreachable);
                return;
            }

            if ctx.player().position.y - (dest.y as f64)
                < ctx.player_controller_ref().get_block_reach_distance()
                && !ctx.player().on_ground
            {
                baritone
                    .player_context
                    .player_mut()
                    .inventory
                    .set_selected_slot(water_bucket);

                target_rotation = Some(Rotation::new(to_dest.get_yaw(), 90.0));

                let ctx = &baritone.player_context;
                if ctx.is_looking_at(dest) || ctx.is_looking_at(dest.below()) {
                    state.set_input(Input::ClickRight, true);
                }
            }
        }
        match target_rotation {
            Some(target_rotation) => state.set_target(MovementTarget::new(target_rotation, true)),
            None => state.set_target(MovementTarget::new(to_dest, false)),
        };
        let ctx = &mut baritone.player_context;
        if player_feet == dest
            && (ctx.player().position.y - (player_feet.y as f64) < 0.094 || is_water)
        {
            // 0.094 because lilypads
            if is_water {
                // only match water, not flowing water (which we cannot pick up with a bucket)
                let empty_bucket = ctx
                    .player()
                    .inventory
                    .find_slot_matching_item(STACK_BUCKET_EMPTY);
                if Inventory::is_hotbar_slot(empty_bucket) {
                    ctx.player_mut().inventory.set_selected_slot(empty_bucket);
                    if ctx.player().delta_movement.y >= 0.0 {
                        state.set_input(Input::ClickRight, true);
                    }
                    return;
                } else if ctx.player().delta_movement.y >= 0.0 {
                    state.set_status(MovementStatus::Success);
                    return;
                } // don't else return state; we need to stay centered because this water might be flowing under the surface
            } else {
                state.set_status(MovementStatus::Success);
                return;
            }
        }
        let player = ctx.player();
        let dest_center = vec_utils::get_block_pos_center(dest); // we are moving to the 0.5 center not the edge (like if we were falling on a ladder)
        if (player.position.x + player.delta_movement.x - dest_center.x).abs() > 0.1
            || (player.position.z + player.delta_movement.z - dest_center.z).abs() > 0.1
        {
            if !player.on_ground && player.delta_movement.y.abs() > 0.4 {
                state.set_input(Input::Sneak, true);
            }
            state.set_input(Input::MoveForward, true);
        }
        let avoid = match Self::avoid(ctx) {
            None => src.subtract(dest),
            Some(avoid) => {
                let (x, y, z) = avoid.get_unit_vec3i();
                let avoid = BetterBlockPos::new(x, y, z);
                let dist = (avoid.x as f64
                    * (dest_center.x - avoid.x as f64 / 2.0 - player.position.x))
                    .abs()
                    + (avoid.z as f64 * (dest_center.z - avoid.z as f64 / 2.0 - player.position.z))
                        .abs();
                if dist < 0.6 {
                    state.set_input(Input::MoveForward, true);
                } else if !player.on_ground {
                    state.set_input(Input::Sneak, false);
                }
                avoid
            }
        };
        if target_rotation.is_none() {
            let dest_center_offset = Vec3::new(
                dest_center.x + 0.125 * avoid.x as f64,
                dest_center.y,
                dest_center.z + 0.125 * avoid.z as f64,
            );
            state.set_target(MovementTarget::new(
                rotation_utils::calc_rotation_from_vec3d(
                    ctx.player_head(),
                    dest_center_offset,
                    ctx.player_rotations(),
                ),
                false,
            ));
        }
    }

    fn avoid(ctx: &dyn IPlayerContext) -> Option<Direction> {
        for i in 0..15 {
            let state = ctx.world().get_block_state(ctx.player_feet().below_n(i));
            if state.climbable == Some(Climbable::Ladder) {
                // LadderBlock.FACING
                return Some(state.facing.expect("IllegalArgumentException: no facing"));
            }
        }
        None
    }

    pub(crate) fn safe_to_cancel(m: &Movement, baritone: &Baritone, state: &MovementState) -> bool {
        // if we haven't started walking off the edge yet, or if we're in the process of breaking blocks before doing the fall
        // then it's safe to cancel this
        baritone.player_context.player_feet() == m.src
            || state.get_status() != MovementStatus::Running
    }

    pub(crate) fn prepared(
        m: &mut Movement,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) -> bool {
        if state.get_status() == MovementStatus::Waiting {
            return true;
        }
        // only break if one of the first three needs to be broken
        // specifically ignore the last one which might be water
        let mut i = 0;
        while i < 4 && i < m.positions_to_break.len() {
            if !mh::can_walk_through_ctx(&baritone.player_context, m.positions_to_break[i]) {
                return m.prepared_default(baritone, state);
            }
            i += 1;
        }
        true
    }

    fn build_positions_to_break(
        src: BetterBlockPos,
        dest: BetterBlockPos,
    ) -> Box<[BetterBlockPos]> {
        let diff_x = src.x.wrapping_sub(dest.x);
        let diff_z = src.z.wrapping_sub(dest.z);
        let diff_y = src.y.wrapping_sub(dest.y).wrapping_abs();
        let len = usize::try_from(diff_y.wrapping_add(2)).expect("NegativeArraySizeException");
        (0..len)
            .map(|i| {
                BetterBlockPos::new(
                    src.x.wrapping_sub(diff_x),
                    src.y.wrapping_add(1).wrapping_sub(i as i32),
                    src.z.wrapping_sub(diff_z),
                )
            })
            .collect()
    }
}
