// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementDownward.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `reset` is `Movement::reset`.

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::movement::{COST_INF, MovementStatus};
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::pathing::movement::movement::MovementKind;
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movements::is_ladder_or_vine;
use crate::pathing::movement::{CalculationContext, Movement, MovementState};

#[derive(Clone, Debug, PartialEq)]
pub struct MovementDownward {
    pub num_ticks: i32,
}

impl MovementDownward {
    pub fn new(start: BetterBlockPos, end: BetterBlockPos) -> Movement {
        Movement::new(
            start,
            end,
            Box::new([end]),
            None,
            MovementKind::Downward(MovementDownward { num_ticks: 0 }),
        )
    }

    pub(crate) fn calculate_cost(m: &Movement, context: &CalculationContext) -> f64 {
        Self::cost(context, m.src.x, m.src.y, m.src.z)
    }

    pub(crate) fn calculate_valid_positions(m: &Movement) -> FxHashSet<BetterBlockPos> {
        FxHashSet::from_iter([m.src, m.dest])
    }

    pub fn cost(context: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
        if !context.allow_downward {
            return COST_INF;
        }
        if !mh::can_walk_on(context, x, y - 2, z) {
            return COST_INF;
        }
        let down = context.get(x, y - 1, z);
        let down_block = down;
        if is_ladder_or_vine(down_block) {
            context.costs.ladder_down_one_cost
        } else {
            // we're standing on it, while it might be block falling, it'll be air by the time we get here in the movement
            context.costs.fall_n_blocks_cost[1]
                + mh::get_mining_duration_ticks_state(context, x, y - 1, z, down, false)
        }
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

        if baritone.player_context.player_feet() == m.dest {
            state.set_status(MovementStatus::Success);
            return;
        } else if !m.player_in_valid_position(baritone) {
            state.set_status(MovementStatus::Unreachable);
            return;
        }
        let ctx = &baritone.player_context;
        let position = ctx.player().position;
        let diff_x = position.x - (m.dest.x as f64 + 0.5);
        let diff_z = position.z - (m.dest.z as f64 + 0.5);
        let ab = (diff_x * diff_x + diff_z * diff_z).sqrt();

        let MovementKind::Downward(this) = &mut m.kind else {
            unreachable!()
        };
        let num_ticks = this.num_ticks;
        this.num_ticks = num_ticks.wrapping_add(1);
        if num_ticks < 10 && ab < 0.2 {
            return;
        }
        mh::move_towards(ctx, state, m.positions_to_break[0]);
    }
}
