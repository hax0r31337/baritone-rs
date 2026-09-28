// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementAscend.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `reset` is `Movement::reset`.

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::java::max_f64;
use crate::mc::Direction;
use crate::pathing::movement::movement::{
    HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP, MovementKind,
};
use crate::pathing::movement::movement_helper::{self as mh, PlaceResult};
use crate::pathing::movement::movements::{is_magma, is_soul_sand};
use crate::pathing::movement::{CalculationContext, Movement, MovementState};
use crate::settings::settings;
use crate::utils::BlockStateInterface;

#[derive(Clone, Debug, PartialEq)]
pub struct MovementAscend {
    pub ticks_without_placement: i32,
}

impl MovementAscend {
    pub fn new(src: BetterBlockPos, dest: BetterBlockPos) -> Movement {
        Movement::new(
            src,
            dest,
            Box::new([dest, src.above_n(2), dest.above()]),
            Some(dest.below()),
            MovementKind::Ascend(MovementAscend {
                ticks_without_placement: 0,
            }),
        )
    }

    pub(crate) fn calculate_cost(m: &Movement, context: &CalculationContext) -> f64 {
        Self::cost(context, m.src.x, m.src.y, m.src.z, m.dest.x, m.dest.z)
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        let prior = m.src.subtract(m.get_direction()).above(); // sometimes we back up to place the block, also sprint ascends, also skip descend to straight ascend
        FxHashSet::from_iter([m.src, m.src.above(), m.dest, prior, prior.above()])
    }

    pub fn cost(
        context: &CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        dest_x: i32,
        dest_z: i32,
    ) -> f64 {
        let costs = &*context.costs;
        let to_place = context.get(dest_x, y, dest_z);
        let mut additional_placement_cost = 0.0;
        if !mh::can_walk_on_state(context, dest_x, y, dest_z, to_place) {
            additional_placement_cost = context.cost_of_placing_at(dest_x, y, dest_z, to_place);
            if additional_placement_cost >= COST_INF {
                return COST_INF;
            }
            if !mh::is_replaceable(dest_x, y, dest_z, to_place, &context.bsi) {
                return COST_INF;
            }
            let mut found_place_option = false;
            for dir in HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP {
                let against_x = dest_x + dir.get_step_x();
                let against_y = y + dir.get_step_y();
                let against_z = dest_z + dir.get_step_z();
                if against_x == x && against_z == z {
                    // we might be able to backplace now, but it doesn't matter because it will have been broken by the time we'd need to use it
                    continue;
                }
                if mh::can_place_against(&context.bsi, against_x, against_y, against_z) {
                    found_place_option = true;
                    break;
                }
            }
            if !found_place_option {
                // didn't find a valid place =(
                return COST_INF;
            }
        }
        let src_up2 = context.get(x, y + 2, z); // used lower down anyway
        if context.get(x, y + 3, z).falls
            && (mh::can_walk_through(context, x, y + 1, z) || !src_up2.falls)
        {
            //it would fall on us and possibly suffocate us
            // HOWEVER, we assume that we're standing in the start position
            // that means that src and src.up(1) are both air
            // maybe they aren't now, but they will be by the time this starts
            // if the lower one is can't walk through and the upper one is falling, that means that by standing on src
            // (the presupposition of this Movement)
            // we have necessarily already cleared the entire FallingBlock stack
            // on top of our head

            // as in, if we have a block, then two FallingBlocks on top of it
            // and that block is x, y+1, z, and we'd have to clear it to even start this movement
            // we don't need to worry about those FallingBlocks because we've already cleared them
            return COST_INF;
            // you may think we only need to check srcUp2, not srcUp
            // however, in the scenario where glitchy world gen where unsupported sand / gravel generates
            // it's possible srcUp is AIR from the start, and srcUp2 is falling
            // and in that scenario, when we arrive and break srcUp2, that lets srcUp3 fall on us and suffocate us
        }
        let src_down = context.get(x, y - 1, z);
        if mh::is_climbable(src_down) {
            return COST_INF;
        }
        // we can jump from soul sand, but not from a bottom slab
        let jumping_from_bottom_slab = mh::is_bottom_slab(src_down);
        let jumping_to_bottom_slab = mh::is_bottom_slab(to_place);
        if jumping_from_bottom_slab && !jumping_to_bottom_slab {
            return COST_INF; // the only thing we can ascend onto from a bottom slab is another bottom slab
        }
        let mut walk;
        if jumping_to_bottom_slab {
            if jumping_from_bottom_slab {
                walk = max_f64(costs.jump_one_block_cost, costs.walk_one_block_cost); // we hit space immediately on entering this action
                walk += context.jump_penalty;
            } else {
                walk = costs.walk_one_block_cost; // we don't hit space we just walk into the slab
            }
        } else {
            // jumpingFromBottomSlab must be false
            if is_soul_sand(to_place) {
                walk = costs.walk_one_over_soul_sand_cost;
            } else if is_magma(to_place) {
                walk = costs.sneak_one_block_cost;
            } else {
                walk = max_f64(costs.jump_one_block_cost, costs.walk_one_block_cost);
            }
            walk += context.jump_penalty;
        }

        let mut total_cost = walk + additional_placement_cost;
        // start with srcUp2 since we already have its state
        // includeFalling isn't needed because of the falling check above -- if srcUp3 is falling we will have already exited with COST_INF if we'd actually have to break it
        total_cost += mh::get_mining_duration_ticks_state(context, x, y + 2, z, src_up2, false);
        if total_cost >= COST_INF {
            return COST_INF;
        }
        total_cost += mh::get_mining_duration_ticks(context, dest_x, y + 1, dest_z, false);
        if total_cost >= COST_INF {
            return COST_INF;
        }
        total_cost += mh::get_mining_duration_ticks(context, dest_x, y + 2, dest_z, true);
        total_cost
    }

    pub(crate) fn update_state(
        m: &mut Movement,
        baritone: &mut Baritone,
        state: &mut MovementState,
    ) {
        if baritone.player_context.player_feet().y < m.src.y {
            // this check should run even when in preparing state (breaking blocks)
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        m.update_state_default(baritone, state);
        // TODO incorporate some behavior from ActionClimb (specifically how it waited until it was at most 1.2 blocks away before starting to jump
        // for efficiency in ascending minimal height staircases, which is just repeated MovementAscend, so that it doesn't bonk its head on the ceiling repeatedly)
        if state.get_status() != MovementStatus::Running {
            return;
        }

        let (src, dest) = (m.src, m.dest);
        let direction = m.get_direction();
        let ctx = &baritone.player_context;
        if ctx.player_feet() == dest || ctx.player_feet() == dest.offset(direction.below()) {
            state.set_status(MovementStatus::Success);
            return;
        }

        let position_to_place = m.position_to_place.expect("positionToPlace");
        let jumping_onto = BlockStateInterface::get(ctx, position_to_place);
        if !mh::can_walk_on_ctx_state(ctx, position_to_place, jumping_onto) {
            let MovementKind::Ascend(this) = &mut m.kind else {
                unreachable!()
            };
            this.ticks_without_placement = this.ticks_without_placement.wrapping_add(1);
            if mh::attempt_to_place_a_block(state, baritone, dest.below(), false, true)
                == PlaceResult::ReadyToPlace
            {
                state.set_input(Input::Sneak, true);
                if baritone.player_context.player().crouching {
                    state.set_input(Input::ClickRight, true);
                }
            }
            if this.ticks_without_placement > 10 {
                // After 10 ticks without placement, we might be standing in the way, move back
                state.set_input(Input::MoveBack, true);
            }

            return;
        }
        mh::move_towards(ctx, state, dest);

        state.set_input(
            Input::Sneak,
            settings().allow_walk_on_magma_blocks && is_magma(jumping_onto),
        );

        if mh::is_bottom_slab(jumping_onto)
            && !mh::is_bottom_slab(BlockStateInterface::get(ctx, src.below()))
        {
            return; // don't jump while walking from a non double slab into a bottom slab
        }

        if settings().assume_step || ctx.player_feet() == src.above() {
            // no need to hit space if we're already jumping
            return;
        }

        let x_axis = src.x.wrapping_sub(dest.x).wrapping_abs(); // either 0 or 1
        let z_axis = src.z.wrapping_sub(dest.z).wrapping_abs(); // either 0 or 1
        let position = ctx.player().position;
        let flat_dist_to_next = x_axis as f64 * ((dest.x as f64 + 0.5) - position.x).abs()
            + z_axis as f64 * ((dest.z as f64 + 0.5) - position.z).abs();
        let side_dist = z_axis as f64 * ((dest.x as f64 + 0.5) - position.x).abs()
            + x_axis as f64 * ((dest.z as f64 + 0.5) - position.z).abs();

        let delta_movement = ctx.player().delta_movement;
        let lateral_motion = x_axis as f64 * delta_movement.z + z_axis as f64 * delta_movement.x;
        if lateral_motion.abs() > 0.1 {
            return;
        }

        if Self::head_bonk_clear(m, ctx) {
            state.set_input(Input::Jump, true);
            return;
        }

        if flat_dist_to_next > 1.2 || side_dist > 0.2 {
            return;
        }

        // Once we are pointing the right way and moving, start jumping
        // This is slightly more efficient because otherwise we might start jumping before moving, and fall down without moving onto the block we want to jump onto
        // Also wait until we are close enough, because we might jump and hit our head on an adjacent block
        state.set_input(Input::Jump, true);
    }

    pub fn head_bonk_clear(m: &Movement, ctx: &dyn IPlayerContext) -> bool {
        let start_up = m.src.above_n(2);
        for i in 0..4 {
            let check = start_up.relative(Direction::from_2d_data_value(i));
            if !mh::can_walk_through_ctx(ctx, check) {
                // We might bonk our head
                return false;
            }
        }
        true
    }

    pub(crate) fn safe_to_cancel(&self, state: &MovementState) -> bool {
        // if we had to place, don't allow pause
        state.get_status() != MovementStatus::Running || self.ticks_without_placement == 0
    }
}
