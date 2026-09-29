// Ported from baritone src/api/java/baritone/api/pathing/movement/ActionCosts.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Upstream this is an interface of constants. The port makes it a runtime struct so Bedrock
// can supply its own tuning; `ActionCosts::java()` has the upstream values, and the active
// instance is process-global like upstream's constants (`action_costs()`).

use std::cell::RefCell;
use std::sync::{Arc, LazyLock};

use arc_swap::{ArcSwap, Cache, Guard};

/// don't make this Double.MAX_VALUE because it's added to other things, maybe other COST_INFs,
/// and that would make it overflow to negative
///
/// A sentinel rather than a tuning value, so it stays a constant.
pub const COST_INF: f64 = 1000000.0;

/// Number of entries in [`ActionCosts::fall_n_blocks_cost`].
pub const FALL_N_BLOCKS_COST_LEN: usize = 4097;

/// Movement costs, measured roughly in ticks.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionCosts {
    /// These costs are measured roughly in ticks btw
    pub walk_one_block_cost: f64,
    pub walk_one_in_water_cost: f64,
    /// 0.4 in BlockSoulSand but effectively about half
    pub walk_one_over_soul_sand_cost: f64,
    pub ladder_up_one_cost: f64,
    pub ladder_down_one_cost: f64,
    pub sneak_one_block_cost: f64,
    pub sprint_one_block_cost: f64,
    pub sprint_multiplier: f64,
    /// To walk off an edge you need to walk 0.5 to the edge then 0.3 to start falling off
    pub walk_off_block_cost: f64,
    /// To walk the rest of the way to be centered on the new block
    pub center_after_fall_cost: f64,

    /// `FALL_N_BLOCKS_COST[n]`: ticks to fall `n` blocks, `n` in `0..4097`.
    pub fall_n_blocks_cost: Box<[f64]>,

    pub fall_1_25_blocks_cost: f64,
    pub fall_0_25_blocks_cost: f64,
    /// When you hit space, you get enough upward velocity to go 1.25 blocks
    /// Then, you fall the remaining 0.25 to get on the surface, on block higher.
    /// Since parabolas are symmetric, the amount of time it takes to ascend up from 1 to 1.25
    /// will be the same amount of time that it takes to fall back down from 1.25 to 1.
    /// And the same applies to the overall shape, if it takes X ticks to fall back down 1.25 blocks,
    /// it will take X ticks to reach the peak of your 1.25 block leap.
    /// Therefore, the part of your jump from y=0 to y=1.25 takes distanceToTicks(1.25) ticks,
    /// and the sub-part from y=1 to y=1.25 takes distanceToTicks(0.25) ticks.
    /// Therefore, the other sub-part, from y=0 to y-1, takes distanceToTicks(1.25)-distanceToTicks(0.25) ticks.
    /// That's why JUMP_ONE_BLOCK_COST = FALL_1_25_BLOCKS_COST - FALL_0_25_BLOCKS_COST
    pub jump_one_block_cost: f64,
}

impl ActionCosts {
    /// The upstream (Java Edition) values.
    pub fn java() -> Self {
        let walk_one_block_cost = 20.0 / 4.317; // 4.633
        let sprint_one_block_cost = 20.0 / 5.612; // 3.564
        let walk_off_block_cost = walk_one_block_cost * 0.8; // 3.706
        let fall_1_25_blocks_cost = Self::distance_to_ticks(1.25);
        let fall_0_25_blocks_cost = Self::distance_to_ticks(0.25);
        Self {
            walk_one_block_cost,
            walk_one_in_water_cost: 20.0 / 2.2, // 9.091
            walk_one_over_soul_sand_cost: walk_one_block_cost * 2.0,
            ladder_up_one_cost: 20.0 / 2.35,  // 8.511
            ladder_down_one_cost: 20.0 / 3.0, // 6.667
            sneak_one_block_cost: 20.0 / 1.3, // 15.385
            sprint_one_block_cost,
            sprint_multiplier: sprint_one_block_cost / walk_one_block_cost, // 0.769
            walk_off_block_cost,
            center_after_fall_cost: walk_one_block_cost - walk_off_block_cost, // 0.927
            fall_n_blocks_cost: Self::generate_fall_n_blocks_cost(),
            fall_1_25_blocks_cost,
            fall_0_25_blocks_cost,
            jump_one_block_cost: fall_1_25_blocks_cost - fall_0_25_blocks_cost,
        }
    }

    pub fn generate_fall_n_blocks_cost() -> Box<[f64]> {
        (0..FALL_N_BLOCKS_COST_LEN)
            .map(|i| Self::distance_to_ticks(i as f64))
            .collect()
    }

    pub fn velocity(ticks: i32) -> f64 {
        (0.98f64.powf(ticks as f64) - 1.0) * -3.92
    }

    pub fn old_formula(ticks: f64) -> f64 {
        -3.92 * (99.0 - 49.5 * (0.98f64.powf(ticks) + 1.0) - ticks)
    }

    pub fn distance_to_ticks(distance: f64) -> f64 {
        if distance == 0.0 {
            return 0.0; // Avoid 0/0 NaN
        }
        let mut tmp_distance = distance;
        let mut tick_count: i32 = 0;
        loop {
            let fall_distance = Self::velocity(tick_count);
            if tmp_distance <= fall_distance {
                return tick_count as f64 + tmp_distance / fall_distance;
            }
            tmp_distance -= fall_distance;
            tick_count += 1;
        }
    }
}

impl Default for ActionCosts {
    fn default() -> Self {
        Self::java()
    }
}

static ACTION_COSTS: LazyLock<ArcSwap<ActionCosts>> =
    LazyLock::new(|| ArcSwap::from_pointee(ActionCosts::java()));

/// The active costs (upstream's `ActionCosts` constants).
pub fn action_costs() -> Guard<Arc<ActionCosts>> {
    ACTION_COSTS.load()
}

type ActionCostsCache = Cache<&'static ArcSwap<ActionCosts>, Arc<ActionCosts>>;

thread_local! {
    static ACTION_COSTS_CACHE: RefCell<ActionCostsCache> =
        RefCell::new(Cache::new(LazyLock::force(&ACTION_COSTS)));
}

/// Runs `f` on the active costs, like `settings::with_settings`: the next call sees a
/// replacement, and a call only compares a pointer. `f` must not call `with_action_costs` again.
#[inline]
pub fn with_action_costs<R>(f: impl FnOnce(&ActionCosts) -> R) -> R {
    ACTION_COSTS_CACHE.with(|cache| f(cache.borrow_mut().load()))
}

/// Replaces the active costs. Calculations already running keep the costs they captured.
pub fn set_action_costs(costs: ActionCosts) {
    ACTION_COSTS.store(Arc::new(costs));
}
