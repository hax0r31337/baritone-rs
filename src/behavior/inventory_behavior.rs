// Ported from baritone src/main/java/baritone/behavior/InventoryBehavior.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The functions that may move items reach `InventoryPauserProcess` and the player controller,
// so they take the whole `Baritone`. `selectThrowawayForLocation` first asks
// `BuilderProcess.placeAt`, which is null unless a schematic is being built; the builder is not
// ported, so those two branches are left out. An item "has" the tool component when its stack
// does (upstream asks the item's default components; vanilla stacks carry the same).

use std::hash::{BuildHasher, Hasher};

use crate::Baritone;
use crate::api::event::events::tick_event;
use crate::api::utils::IPlayerContext;
use crate::api::utils::helper::log_debug;
use crate::api::utils::i_player_controller::ContainerInput;
use crate::host::{Inventory, ItemStack, Player};
use crate::settings::settings;
use crate::utils::ToolSet;

/// `Blocks.STONE`
const STONE: &str = "minecraft:stone";

#[derive(Clone, Debug, Default)]
pub struct InventoryBehavior {
    pub(crate) ticks_since_last_inventory_move: i32,
    /// not everything asks every tick, so remember the request while coming to a halt
    pub(crate) last_tick_requested_move: Option<[i32; 2]>,
}

/// `stack.getItem().components().has(DataComponents.TOOL)`
#[cfg(not(feature = "bedrock"))]
fn has_tool(stack: &ItemStack) -> bool {
    !stack.is_empty() && stack.tool.is_some()
}

/// `stack.getItem().components().has(DataComponents.TOOL)`: Bedrock's tools and shears.
#[cfg(feature = "bedrock")]
fn has_tool(stack: &ItemStack) -> bool {
    !stack.is_empty() && crate::host::bedrock_tool::is_tool(stack)
}

impl InventoryBehavior {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_tick(baritone: &mut Baritone, event_type: tick_event::Type) {
        if !settings().allow_inventory {
            return;
        }
        if event_type == tick_event::Type::Out {
            return;
        }
        if baritone.player_context.player().container_open {
            // we have a crafting table or a chest or something open
            return;
        }
        let this = &mut baritone.inventory_behavior;
        this.ticks_since_last_inventory_move = this.ticks_since_last_inventory_move.wrapping_add(1);
        let player = baritone.player_context.player();
        if Self::first_valid_throwaway(player) >= 9 {
            // aka there are none on the hotbar, but there are some in main inventory
            let first = Self::first_valid_throwaway(player);
            Self::request_swap_with_hot_bar(baritone, first, 8);
        }
        let pick = Self::best_tool_against(&baritone.player_context, STONE);
        if pick >= 9 {
            Self::request_swap_with_hot_bar(baritone, pick, 0);
        }
        if let Some([in_inventory, in_hotbar]) =
            baritone.inventory_behavior.last_tick_requested_move
        {
            log_debug(&format!(
                "Remembering to move {in_inventory} {in_hotbar} from a previous tick"
            ));
            Self::request_swap_with_hot_bar(baritone, in_inventory, in_hotbar);
        }
    }

    pub fn attempt_to_put_on_hotbar(
        baritone: &mut Baritone,
        in_main_invy: i32,
        disallowed_hotbar: impl Fn(i32) -> bool,
    ) -> bool {
        let destination =
            Self::get_temp_hotbar_slot(baritone.player_context.player(), disallowed_hotbar);
        if let Some(destination) = destination
            && !Self::request_swap_with_hot_bar(baritone, in_main_invy, destination)
        {
            return false;
        }
        true
    }

    /// Upstream draws from `new Random()`; the port from `RandomState`'s random keys.
    pub fn get_temp_hotbar_slot(
        player: &Player,
        disallowed_hotbar: impl Fn(i32) -> bool,
    ) -> Option<i32> {
        // we're using 0 and 8 for pickaxe and throwaway
        let mut candidates = Vec::new();
        for i in 1..8 {
            if player.inventory.get_non_equipment_item(i).is_empty() && !disallowed_hotbar(i) {
                candidates.push(i);
            }
        }
        if candidates.is_empty() {
            for i in 1..8 {
                if !disallowed_hotbar(i) {
                    candidates.push(i);
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        let random = std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish();
        Some(candidates[(random % candidates.len() as u64) as usize])
    }

    fn request_swap_with_hot_bar(
        baritone: &mut Baritone,
        in_inventory: i32,
        in_hotbar: i32,
    ) -> bool {
        let this = &mut baritone.inventory_behavior;
        this.last_tick_requested_move = Some([in_inventory, in_hotbar]);
        let settings = settings();
        if this.ticks_since_last_inventory_move < settings.ticks_between_inventory_moves {
            log_debug(&format!(
                "Inventory move requested but delaying {} {}",
                this.ticks_since_last_inventory_move, settings.ticks_between_inventory_moves
            ));
            return false;
        }
        if settings.inventory_move_only_if_stationary
            && !baritone
                .get_inventory_pauser_process_mut()
                .stationary_for_inventory_move()
        {
            log_debug("Inventory move requested but delaying until stationary");
            return false;
        }
        drop(settings);
        // the inventory menu's container id is always 0
        baritone.player_context.player_controller().window_click(
            0,
            if in_inventory < 9 {
                in_inventory + 36
            } else {
                in_inventory
            },
            in_hotbar,
            ContainerInput::Swap,
        );
        let this = &mut baritone.inventory_behavior;
        this.ticks_since_last_inventory_move = 0;
        this.last_tick_requested_move = None;
        true
    }

    fn first_valid_throwaway(player: &Player) -> i32 {
        // TODO offhand idk
        let settings = settings();
        for i in 0..Inventory::INVENTORY_SIZE {
            if settings
                .acceptable_throwaway_items
                .iter()
                .any(|item| player.inventory.get_non_equipment_item(i).get_item() == item)
            {
                return i;
            }
        }
        -1
    }

    /// Upstream takes the `Block`; the port takes its name and uses the table's default state.
    fn best_tool_against(ctx: &dyn IPlayerContext, against: &str) -> i32 {
        let Some(against) = ctx.world().table().get_default_state(against) else {
            return -1;
        };
        let invy = &ctx.player().inventory;
        let settings = settings();
        let mut best_ind = -1;
        let mut best_speed = -1.0;
        for i in 0..Inventory::INVENTORY_SIZE {
            let stack = invy.get_non_equipment_item(i);
            if stack.is_empty() {
                continue;
            }
            if settings.item_saver
                && stack
                    .get_damage_value()
                    .wrapping_add(settings.item_saver_threshold)
                    >= stack.get_max_damage()
                && stack.get_max_damage() > 1
            {
                continue;
            }
            if has_tool(stack) {
                let speed = ToolSet::calculate_speed_vs_block(stack, against); // takes into account enchants
                if speed > best_speed {
                    best_speed = speed;
                    best_ind = i;
                }
            }
        }
        best_ind
    }

    pub fn has_generic_throwaway(&self, ctx: &dyn IPlayerContext) -> bool {
        let settings = settings();
        for item in &settings.acceptable_throwaway_items {
            if Self::has_throwaway(
                ctx.player(),
                |stack| stack.get_item() == item,
                settings.allow_inventory,
            ) {
                return true;
            }
        }
        false
    }

    pub fn select_throwaway_for_location(
        baritone: &mut Baritone,
        select: bool,
        _x: i32,
        _y: i32,
        _z: i32,
    ) -> bool {
        // BuilderProcess.placeAt is always null here, see the file header
        let items = settings().acceptable_throwaway_items.clone();
        for item in &items {
            if Self::throwaway(baritone, select, |stack| stack.get_item() == item) {
                return true;
            }
        }
        false
    }

    /// `throwaway(boolean, Predicate<? super ItemStack>)`
    pub fn throwaway(
        baritone: &mut Baritone,
        select: bool,
        desired: impl Fn(&ItemStack) -> bool,
    ) -> bool {
        let allow_inventory = settings().allow_inventory;
        Self::throwaway_inventory(baritone, select, desired, allow_inventory)
    }

    /// `throwaway(boolean, Predicate<? super ItemStack>, boolean)`
    pub fn throwaway_inventory(
        baritone: &mut Baritone,
        select: bool,
        desired: impl Fn(&ItemStack) -> bool,
        allow_inventory: bool,
    ) -> bool {
        if !select {
            return Self::has_throwaway(baritone.player_context.player(), desired, allow_inventory);
        }
        let p = baritone.player_context.player_mut();
        for i in 0..9 {
            let item = p.inventory.get_non_equipment_item(i);
            // this usage of settings() is okay because it's only called once during pathing
            // (while creating the CalculationContext at the very beginning)
            // and then it's called during execution
            // since this function is never called during cost calculation, we don't need to migrate
            // acceptableThrowawayItems to the CalculationContext
            if desired(item) {
                p.inventory.set_selected_slot(i);
                return true;
            }
        }
        if desired(&p.offhand) {
            // main hand takes precedence over off hand
            // that means that if we have block A selected in main hand and block B in off hand, right clicking places block B
            // we've already checked above ^ and the main hand can't possible have an acceptablethrowawayitem
            // so we need to select in the main hand something that doesn't right click
            // so not a shovel, not a hoe, not a block, etc
            for i in 0..9 {
                let item = p.inventory.get_non_equipment_item(i);
                if item.is_empty() || has_tool(item) {
                    p.inventory.set_selected_slot(i);
                    return true;
                }
            }
        }

        if allow_inventory {
            for i in 9..36 {
                if desired(
                    baritone
                        .player_context
                        .player()
                        .inventory
                        .get_non_equipment_item(i),
                ) {
                    Self::request_swap_with_hot_bar(baritone, i, 7);
                    baritone
                        .player_context
                        .player_mut()
                        .inventory
                        .set_selected_slot(7);
                    return true;
                }
            }
        }

        false
    }

    /// `throwaway(false, desired, allowInventory)`: the same search, selecting nothing.
    fn has_throwaway(
        player: &Player,
        desired: impl Fn(&ItemStack) -> bool,
        allow_inventory: bool,
    ) -> bool {
        for i in 0..9 {
            if desired(player.inventory.get_non_equipment_item(i)) {
                return true;
            }
        }
        if desired(&player.offhand) {
            for i in 0..9 {
                let item = player.inventory.get_non_equipment_item(i);
                if item.is_empty() || has_tool(item) {
                    return true;
                }
            }
        }
        if allow_inventory {
            for i in 9..36 {
                if desired(player.inventory.get_non_equipment_item(i)) {
                    return true;
                }
            }
        }
        false
    }
}
