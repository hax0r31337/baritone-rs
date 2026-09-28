// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementFall.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Missing until phase 4: willPlaceBucket, updateState, getDirectionVec, prepared, safeToCancel.

use rustc_hash::FxHashSet;

use crate::api::pathing::movement::COST_INF;
use crate::api::utils::BetterBlockPos;
use crate::pathing::movement::movement::MovementKind;
use crate::pathing::movement::movements::MovementDescend;
use crate::pathing::movement::{CalculationContext, Movement};
use crate::utils::pathing::MutableMoveResult;

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
