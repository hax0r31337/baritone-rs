// Ported from baritone src/main/java/baritone/pathing/movement/CalculationContext.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Built from what the host sends instead of `IBaritone`: a world snapshot and the player
// (`new`), or the player context's (`from_baritone`). The `baritone` field is not kept, since
// movements no longer hold one. The active `ActionCosts` are captured here, so a calculation
// keeps one set of costs throughout, like the settings below.
//
// Upstream shares one context between the tick thread and the calculation it starts. The
// port's `BlockStateInterface` and `ToolSet` caches are single-threaded, so the calculation
// gets a clone: same snapshot and settings, its own caches, the same `PrecomputedData`.
//
// Upstream's `forUseOnAnotherThread` copy of the chunk map still sees block updates in the
// chunks it copied; a snapshot does not. Processes rebuild their context every tick, so the
// context execution reads is at most a tick old.
//
// Code that reaches the player through the context's `baritone` (`MineProcess.searchWorld`)
// reads `player`, the snapshot the context was built from. The one subclass that is ported,
// `GetToBlockProcess.GetToBlockCalculationContext`, is a context whose `kind` says which
// method it overrides.

use std::sync::Arc;

use crate::Baritone;
use crate::api::pathing::movement::{ActionCosts, COST_INF, action_costs};
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::host::{BlockState, Inventory, Player, World};
use crate::pathing::precompute::PrecomputedData;
use crate::settings::settings;
use crate::utils::pathing::BetterWorldBorder;
use crate::utils::{BlockStateInterface, ToolSet};

/// `Items.WATER_BUCKET`
pub(crate) const STACK_BUCKET_WATER: &str = "minecraft:water_bucket";

/// Which class a [`CalculationContext`] is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalculationContextKind {
    #[default]
    CalculationContext,
    /// `GetToBlockProcess.GetToBlockCalculationContext`: `breakCostMultiplierAt` is always 1.
    GetToBlock,
}

#[derive(Clone)]
pub struct CalculationContext {
    pub kind: CalculationContextKind,
    pub safe_for_threaded_use: bool,
    /// The world snapshot `bsi` reads (upstream: the live client world).
    pub world: Arc<World>,
    /// The player the context was built from (upstream reaches the live one through
    /// `baritone`).
    pub player: Arc<Player>,
    pub bsi: BlockStateInterface,
    pub tool_set: ToolSet,
    pub has_water_bucket: bool,
    pub has_throwaway: bool,
    pub can_sprint: bool,
    /// protected because you should call the function instead
    pub(crate) place_block_cost: f64,
    pub allow_break: bool,
    /// Block names.
    pub allow_break_anyway: Vec<String>,
    pub allow_parkour: bool,
    pub allow_parkour_place: bool,
    pub allow_jump_at_build_limit: bool,
    pub allow_parkour_ascend: bool,
    pub assume_walk_on_water: bool,
    pub allow_fall_into_lava: bool,
    pub frost_walker: i32,
    pub allow_diagonal_descend: bool,
    pub allow_diagonal_ascend: bool,
    pub allow_downward: bool,
    pub min_fall_height: i32,
    pub max_fall_height_no_water: i32,
    pub max_fall_height_bucket: i32,
    pub water_walk_speed: f64,
    pub break_block_additional_cost: f64,
    pub backtrack_cost_favoring_coefficient: f64,
    pub jump_penalty: f64,
    pub walk_on_water_one_penalty: f64,
    pub allow_walk_on_magma_blocks: bool,
    pub world_border: BetterWorldBorder,

    pub precomputed_data: Arc<PrecomputedData>,

    /// The `ActionCosts` constants, captured when the context is created.
    pub costs: Arc<ActionCosts>,
}

impl CalculationContext {
    /// `CalculationContext(IBaritone)`
    pub fn from_baritone(baritone: &Baritone) -> Self {
        Self::from_baritone_thread(baritone, false)
    }

    /// `CalculationContext(IBaritone, boolean)`
    pub fn from_baritone_thread(baritone: &Baritone, for_use_on_another_thread: bool) -> Self {
        let ctx = &baritone.player_context;
        Self::new(
            Arc::clone(ctx.world()),
            Arc::new(ctx.player().clone()),
            baritone.inventory_behavior.has_generic_throwaway(ctx),
            for_use_on_another_thread,
        )
    }

    /// `CalculationContext(IBaritone, boolean)`, with the world and player the host sent, and
    /// `InventoryBehavior.hasGenericThrowaway()`.
    pub fn new(
        world: Arc<World>,
        player: Arc<Player>,
        has_generic_throwaway: bool,
        for_use_on_another_thread: bool,
    ) -> Self {
        let settings = settings();
        let costs = Arc::clone(&action_costs());
        let precomputed_data = Arc::new(PrecomputedData::new(world.table()));
        let bsi = BlockStateInterface::new(Arc::clone(&world));
        let tool_set = ToolSet::new(Arc::clone(&player), Arc::clone(world.table()));
        let has_throwaway = settings.allow_place && has_generic_throwaway;
        let has_water_bucket = settings.allow_water_bucket_fall
            && Inventory::is_hotbar_slot(
                player
                    .get_inventory()
                    .find_slot_matching_item(STACK_BUCKET_WATER),
            )
            && !world.dimension().water_evaporates;
        let can_sprint = settings.allow_sprint && player.food_level > 6;
        // todo: technically there can now be datapack enchants that replace blocks with any other at any range
        let frost_walker = player.frost_walker;
        let water_speed_multiplier: f32 = player.water_movement_efficiency.unwrap_or(1.0);
        let water_walk_speed = costs.walk_one_in_water_cost
            * (1.0f32 - water_speed_multiplier) as f64
            + costs.walk_one_block_cost * water_speed_multiplier as f64;
        // why cache these things here, why not let the movements just get directly from settings?
        // because if some movements are calculated one way and others are calculated another way,
        // then you get a wildly inconsistent path that isn't optimal for either scenario.
        let world_border = BetterWorldBorder::new(&world.border());
        Self {
            kind: CalculationContextKind::CalculationContext,
            safe_for_threaded_use: for_use_on_another_thread,
            world,
            player,
            bsi,
            tool_set,
            has_water_bucket,
            has_throwaway,
            can_sprint,
            place_block_cost: settings.block_placement_penalty,
            allow_break: settings.allow_break,
            allow_break_anyway: settings.allow_break_anyway.clone(),
            allow_parkour: settings.allow_parkour,
            allow_parkour_place: settings.allow_parkour_place,
            allow_jump_at_build_limit: settings.allow_jump_at_build_limit,
            allow_parkour_ascend: settings.allow_parkour_ascend,
            assume_walk_on_water: settings.assume_walk_on_water,
            allow_fall_into_lava: false, // Super secret internal setting for ElytraBehavior
            frost_walker,
            allow_diagonal_descend: settings.allow_diagonal_descend,
            allow_diagonal_ascend: settings.allow_diagonal_ascend,
            allow_downward: settings.allow_downward,
            min_fall_height: 3, // Minimum fall height used by MovementFall
            max_fall_height_no_water: settings.max_fall_height_no_water,
            max_fall_height_bucket: settings.max_fall_height_bucket,
            water_walk_speed,
            break_block_additional_cost: settings.block_break_additional_penalty,
            backtrack_cost_favoring_coefficient: settings.backtrack_cost_favoring_coefficient,
            jump_penalty: settings.jump_penalty,
            walk_on_water_one_penalty: settings.walk_on_water_one_penalty,
            allow_walk_on_magma_blocks: settings.allow_walk_on_magma_blocks,
            world_border,
            precomputed_data,
            costs,
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> &BlockState {
        self.bsi.get0(x, y, z) // laughs maniacally
    }

    #[inline]
    pub fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.bsi.is_loaded(x, z)
    }

    /// `get(BlockPos)`
    pub fn get_pos(&self, pos: BetterBlockPos) -> &BlockState {
        self.get(pos.x, pos.y, pos.z)
    }

    /// Upstream returns the `Block`; the port returns the state, which carries the block's
    /// traits.
    #[inline]
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> &BlockState {
        self.get(x, y, z)
    }

    pub fn cost_of_placing_at(&self, x: i32, y: i32, z: i32, current: &BlockState) -> f64 {
        if !self.has_throwaway {
            // only true if allowPlace is true, see constructor
            return COST_INF;
        }
        if self.is_possibly_protected(x, y, z) {
            return COST_INF;
        }
        if !self.world_border.can_place_at(x, z) {
            return COST_INF;
        }
        let settings = settings();
        if !settings.allow_place_in_fluids_source && current.fluid_source {
            return COST_INF;
        }
        if !settings.allow_place_in_fluids_flow
            && current.fluid != crate::host::Fluid::Empty
            && !current.fluid_source
        {
            return COST_INF;
        }
        self.place_block_cost
    }

    pub fn break_cost_multiplier_at(&self, x: i32, y: i32, z: i32, current: &BlockState) -> f64 {
        if self.kind == CalculationContextKind::GetToBlock {
            return 1.0;
        }
        if !self.allow_break && !self.allow_break_anyway.contains(&current.name) {
            return COST_INF;
        }
        if self.is_possibly_protected(x, y, z) {
            return COST_INF;
        }
        1.0
    }

    pub fn place_bucket_cost(&self) -> f64 {
        self.place_block_cost // shrug
    }

    pub fn is_possibly_protected(&self, _x: i32, _y: i32, _z: i32) -> bool {
        // TODO more protection logic here; see #220
        false
    }
}
