// Ported from baritone src/main/java/baritone/pathing/movement/movements/MovementParkour.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Missing until phase 4: safeToCancel, updateState.

use rustc_hash::FxHashSet;

use crate::api::pathing::movement::COST_INF;
use crate::api::utils::BetterBlockPos;
use crate::host::Fluid;
use crate::mc::Direction;
use crate::pathing::movement::movement::{
    HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP, MovementKind,
};
use crate::pathing::movement::movement_helper as mh;
use crate::pathing::movement::movements::{is_magma, is_soul_sand};
use crate::pathing::movement::{CalculationContext, Movement};
use crate::utils::BlockStateInterface;
use crate::utils::pathing::MutableMoveResult;

#[derive(Clone, Debug, PartialEq)]
pub struct MovementParkour {
    pub(crate) direction: Direction,
    pub(crate) dist: i32,
    pub(crate) ascend: bool,
}

impl MovementParkour {
    fn new(src: BetterBlockPos, dist: i32, dir: Direction, ascend: bool) -> Movement {
        let ascend_n = if ascend { 1 } else { 0 };
        Movement::new(
            src,
            src.relative_n(dir, dist).above_n(ascend_n),
            Box::new([]),
            Some(src.relative_n(dir, dist).below_n(1 - ascend_n)),
            MovementKind::Parkour(MovementParkour {
                direction: dir,
                dist,
                ascend,
            }),
        )
    }

    pub fn get_direction(&self) -> Direction {
        self.direction
    }

    pub fn get_dist(&self) -> i32 {
        self.dist
    }

    pub fn is_ascend(&self) -> bool {
        self.ascend
    }

    /// `cost(CalculationContext, BetterBlockPos, Direction)`: the movement from `src` in
    /// `direction`, whatever its cost.
    pub fn cost_pos(
        context: &CalculationContext,
        src: BetterBlockPos,
        direction: Direction,
    ) -> Movement {
        let mut res = MutableMoveResult::new();
        Self::cost(context, src.x, src.y, src.z, direction, &mut res);
        let dist = res
            .x
            .wrapping_sub(src.x)
            .wrapping_abs()
            .wrapping_add(res.z.wrapping_sub(src.z).wrapping_abs());
        Self::new(src, dist, direction, res.y > src.y)
    }

    pub fn cost(
        context: &CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        dir: Direction,
        res: &mut MutableMoveResult,
    ) {
        let costs = &*context.costs;
        if !context.allow_parkour {
            return;
        }
        let dimension = context.world.dimension();
        // Level.getMaxY()
        if !context.allow_jump_at_build_limit && y >= dimension.min_y + dimension.height - 1 {
            return;
        }
        let x_diff = dir.get_step_x();
        let z_diff = dir.get_step_z();
        if !mh::fully_passable(context, x + x_diff, y, z + z_diff) {
            // most common case at the top -- the adjacent block isn't air
            return;
        }
        let adj = context.get(x + x_diff, y - 1, z + z_diff);
        if mh::can_walk_on_state(context, x + x_diff, y - 1, z + z_diff, adj) {
            // don't parkour if we could just traverse (for now)
            // second most common case -- we could just traverse not parkour
            return;
        }
        if mh::avoid_walking_into(adj) && adj.fluid != Fluid::Water {
            // magma sucks
            return;
        }
        if !mh::fully_passable(context, x + x_diff, y + 1, z + z_diff) {
            return;
        }
        if !mh::fully_passable(context, x + x_diff, y + 2, z + z_diff) {
            return;
        }
        if !mh::fully_passable(context, x, y + 2, z) {
            return;
        }
        let standing_on = context.get(x, y - 1, z);
        if mh::is_climbable(standing_on)
            || standing_on.stairs.is_some()
            || mh::is_bottom_slab(standing_on)
        {
            return;
        }
        // we can't jump from (frozen) water with assumeWalkOnWater because we can't be sure it will be frozen
        if context.assume_walk_on_water && standing_on.fluid != Fluid::Empty {
            return;
        }
        if context.get(x, y, z).fluid != Fluid::Empty {
            return; // can't jump out of water
        }
        #[allow(clippy::if_same_then_else)] // upstream's structure
        let max_jump = if context.allow_walk_on_magma_blocks && is_magma(standing_on) {
            2
        } else if is_soul_sand(standing_on) {
            2 // 1 block gap
        } else if context.can_sprint {
            4
        } else {
            3
        };

        // check parkour jumps from smallest to largest for obstacles/walls and landing positions
        let mut verified_max_jump = 1; // i - 1 (when i = 2)
        for i in 2..=max_jump {
            let dest_x = x + x_diff * i;
            let dest_z = z + z_diff * i;

            // check head/feet
            if !mh::fully_passable(context, dest_x, y + 1, dest_z) {
                break;
            }
            if !mh::fully_passable(context, dest_x, y + 2, dest_z) {
                break;
            }

            // check for ascend landing position
            let dest_into = context.bsi.get0(dest_x, y, dest_z);
            if !mh::fully_passable_state(context, dest_x, y, dest_z, dest_into) {
                if i <= 3
                    && context.allow_parkour_ascend
                    && context.can_sprint
                    && mh::can_walk_on_state(context, dest_x, y, dest_z, dest_into)
                    && Self::check_overshoot_safety(
                        &context.bsi,
                        dest_x + x_diff,
                        y + 1,
                        dest_z + z_diff,
                    )
                {
                    res.x = dest_x;
                    res.y = y + 1;
                    res.z = dest_z;
                    res.cost = i as f64 * costs.sprint_one_block_cost + context.jump_penalty;
                    return;
                }
                break;
            }

            // check for flat landing position
            let landing_on = context.bsi.get0(dest_x, y - 1, dest_z);
            // farmland needs to be canWalkOn otherwise farm can never work at all, but we want to specifically disallow ending a jump on farmland haha
            // frostwalker works here because we can't jump from possibly unfrozen water
            if (!landing_on.farmland
                && mh::can_walk_on_state(context, dest_x, y - 1, dest_z, landing_on))
                || (16.min(context.frost_walker.wrapping_add(2)) >= i
                    && mh::can_use_frost_walker(context, landing_on))
            {
                if Self::check_overshoot_safety(&context.bsi, dest_x + x_diff, y, dest_z + z_diff) {
                    res.x = dest_x;
                    res.y = y;
                    res.z = dest_z;
                    res.cost = Self::cost_from_jump_distance(costs, i) + context.jump_penalty;
                    return;
                }
                break;
            }

            if !mh::fully_passable(context, dest_x, y + 3, dest_z) {
                break;
            }

            verified_max_jump = i;
        }

        // parkour place starts here
        if !context.allow_parkour_place {
            return;
        }
        // check parkour jumps from largest to smallest for positions to place blocks
        let mut i = verified_max_jump;
        while i > 1 {
            let dest_x = x + i * x_diff;
            let dest_z = z + i * z_diff;
            let to_replace = context.get(dest_x, y - 1, dest_z);
            let place_cost = context.cost_of_placing_at(dest_x, y - 1, dest_z, to_replace);
            if place_cost >= COST_INF
                || !mh::is_replaceable(dest_x, y - 1, dest_z, to_replace, &context.bsi)
                || !Self::check_overshoot_safety(&context.bsi, dest_x + x_diff, y, dest_z + z_diff)
            {
                i -= 1;
                continue;
            }
            for dir in HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP {
                let against_x = dest_x + dir.get_step_x();
                let against_y = y - 1 + dir.get_step_y();
                let against_z = dest_z + dir.get_step_z();
                if against_x == dest_x - x_diff && against_z == dest_z - z_diff {
                    // we can't turn around that fast
                    continue;
                }
                if mh::can_place_against(&context.bsi, against_x, against_y, against_z) {
                    res.x = dest_x;
                    res.y = y;
                    res.z = dest_z;
                    res.cost =
                        Self::cost_from_jump_distance(costs, i) + place_cost + context.jump_penalty;
                    return;
                }
            }
            i -= 1;
        }
    }

    fn check_overshoot_safety(bsi: &BlockStateInterface, x: i32, y: i32, z: i32) -> bool {
        // we're going to walk into these two blocks after the landing of the parkour anyway, so make sure they aren't avoidWalkingInto
        !mh::avoid_walking_into(bsi.get0(x, y, z)) && !mh::avoid_walking_into(bsi.get0(x, y + 1, z))
    }

    fn cost_from_jump_distance(
        costs: &crate::api::pathing::movement::ActionCosts,
        dist: i32,
    ) -> f64 {
        match dist {
            2 => costs.walk_one_block_cost * 2.0, // IDK LOL
            3 => costs.walk_one_block_cost * 3.0,
            4 => costs.sprint_one_block_cost * 4.0,
            _ => panic!("LOL {dist}"),
        }
    }

    pub(crate) fn calculate_cost(&self, m: &Movement, context: &CalculationContext) -> f64 {
        let mut res = MutableMoveResult::new();
        Self::cost(context, m.src.x, m.src.y, m.src.z, self.direction, &mut res);
        if res.x != m.dest.x || res.y != m.dest.y || res.z != m.dest.z {
            return COST_INF;
        }
        res.cost
    }

    pub(crate) fn calculate_valid_positions(&self, m: &Movement) -> FxHashSet<BetterBlockPos> {
        let mut set = FxHashSet::default();
        for i in 0..=self.dist {
            for y in 0..2 {
                set.insert(m.src.relative_n(self.direction, i).above_n(y));
            }
        }
        set
    }
}
