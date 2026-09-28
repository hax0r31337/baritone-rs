// Ported from baritone src/main/java/baritone/utils/ToolSet.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Items and the player are the host's (`crate::host::player`): a tool's speed comes from its
// tool rules, enchantment bonuses are precomputed by the host. The cache is keyed by the
// block's default state, which is how the port identifies a `Block`.

use std::cell::RefCell;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::host::{BlockState, BlockStateTable, ItemStack, Player};
use crate::settings::settings;

/// A cached list of the best tools on the hotbar for any block
///
/// Owned by one thread at a time (`Send`, not `Sync`), like the calculation context it
/// belongs to. A clone starts with a copy of the cache.
#[derive(Clone, Debug)]
pub struct ToolSet {
    /// A cache mapping a `Block` (its default state id) to how long it will take to break
    /// with this toolset, given the optimum tool is used.
    break_strength_cache: RefCell<FxHashMap<u32, f64>>,

    /// `backendCalculation`: the potion amplifier it multiplies `getBestDestructionTime` by,
    /// `None` when `considerPotionEffects` was off.
    amplifier: Option<f64>,

    player: Arc<Player>,

    /// The table the states passed in come from; gives each block's default state.
    table: Arc<BlockStateTable>,
}

/// Used for evaluating the material cost of a tool.
/// see [`ToolSet::get_material_cost`]
/// Prefer tools with lower material cost (lower index in this list).
const MATERIAL_TAGS_PRIORITY_LIST: [&str; 6] = [
    "minecraft:wooden_tool_materials",
    "minecraft:stone_tool_materials",
    "minecraft:iron_tool_materials",
    "minecraft:gold_tool_materials",
    "minecraft:diamond_tool_materials",
    "minecraft:netherite_tool_materials",
];

/// `ItemTags.SWORDS`
const SWORDS: &str = "minecraft:swords";

impl ToolSet {
    pub fn new(player: Arc<Player>, table: Arc<BlockStateTable>) -> Self {
        let mut tool_set = Self {
            break_strength_cache: RefCell::new(FxHashMap::default()),
            amplifier: None,
            player,
            table,
        };
        if settings().consider_potion_effects {
            tool_set.amplifier = Some(tool_set.potion_amplifier());
        }
        tool_set
    }

    /// Using the best tool on the hotbar, how fast we can mine this block
    ///
    /// Returns the speed of how fast we'll mine it. 1/(time in ticks)
    pub fn get_str_vs_block(&self, state: &BlockState) -> f64 {
        let block = state.default_state;
        if let Some(&cached) = self.break_strength_cache.borrow().get(&block) {
            return cached;
        }
        let best = self.get_best_destruction_time(state);
        let value = match self.amplifier {
            Some(amplifier) => amplifier * best,
            None => best,
        };
        self.break_strength_cache.borrow_mut().insert(block, value);
        value
    }

    /// Evaluate the material cost of a possible tool.
    /// If all else is equal, we want to prefer the tool with the lowest material cost.
    /// i.e. we want to prefer a wooden pickaxe over a stone pickaxe, if all else is equal.
    ///
    /// Returns values from 0 up. (The tags hold the tools' repair materials, not the tools, so
    /// a tool itself gets -1; kept as upstream.)
    fn get_material_cost(item_stack: &ItemStack) -> i32 {
        for (i, tag) in MATERIAL_TAGS_PRIORITY_LIST.iter().enumerate() {
            if item_stack.is_tag(tag) {
                return i as i32;
            }
        }
        -1
    }

    pub fn has_silk_touch(&self, stack: &ItemStack) -> bool {
        // silk touch enchantment is still special cased as affecting block drops
        // not possible to add custom attribute via datapack
        stack.silk_touch
    }

    /// Calculate which tool on the hotbar is best for mining, depending on an override setting,
    /// related to auto tool movement cost, it will either return current selected slot, or the best slot.
    ///
    /// Returns an int containing the index in the tools array that worked best.
    ///
    /// `getBestSlot(Block, boolean)`; upstream takes the `Block`, the port any of its states.
    pub fn get_best_slot(&self, b: &BlockState, prefer_silk_touch: bool) -> i32 {
        self.get_best_slot_pathing(b, prefer_silk_touch, false)
    }

    /// `getBestSlot(Block, boolean, boolean)`
    pub fn get_best_slot_pathing(
        &self,
        b: &BlockState,
        prefer_silk_touch: bool,
        pathing_calculation: bool,
    ) -> i32 {
        let settings = settings();
        let inventory = self.player.get_inventory();

        /*
        If we actually want know what efficiency our held item has instead of the best one
        possible, this lets us make pathing depend on the actual tool to be used (if auto tool is disabled)
        */
        if !settings.auto_tool && pathing_calculation {
            return inventory.get_selected_slot();
        }

        let mut best = 0;
        let mut highest_speed = f64::NEG_INFINITY;
        let mut lowest_cost = i32::MIN;
        let mut best_silk_touch = false;
        let block_state = self.table.get(b.default_state);
        for i in 0..9 {
            let item_stack = inventory.get_item(i);
            if !settings.use_sword_to_mine && item_stack.is_tag(SWORDS) {
                continue;
            }

            if settings.item_saver
                && item_stack
                    .get_damage_value()
                    .wrapping_add(settings.item_saver_threshold)
                    >= item_stack.get_max_damage()
                && item_stack.get_max_damage() > 1
            {
                continue;
            }
            let speed = Self::calculate_speed_vs_block(item_stack, block_state);
            let silk_touch = self.has_silk_touch(item_stack);
            if speed > highest_speed {
                highest_speed = speed;
                best = i;
                lowest_cost = Self::get_material_cost(item_stack);
                best_silk_touch = silk_touch;
            } else if speed == highest_speed {
                let cost = Self::get_material_cost(item_stack);
                if (cost < lowest_cost && (silk_touch || !best_silk_touch))
                    || (prefer_silk_touch && !best_silk_touch && silk_touch)
                {
                    highest_speed = speed;
                    best = i;
                    lowest_cost = cost;
                    best_silk_touch = silk_touch;
                }
            }
        }
        best
    }

    /// Calculate how effectively a block can be destroyed
    ///
    /// Returns a double containing the destruction ticks with the best tool
    fn get_best_destruction_time(&self, b: &BlockState) -> f64 {
        let stack = self
            .player
            .get_inventory()
            .get_item(self.get_best_slot_pathing(b, false, true));
        Self::calculate_speed_vs_block(stack, self.table.get(b.default_state))
            * self.avoidance_multiplier(b)
    }

    fn avoidance_multiplier(&self, b: &BlockState) -> f64 {
        let settings = settings();
        if settings.blocks_to_avoid_breaking.contains(&b.name) {
            settings.avoid_breaking_multiplier
        } else {
            1.0
        }
    }

    /// Calculates how long would it take to mine the specified block given the best tool
    /// in this toolset is used. A negative value is returned if the specified block is unbreakable.
    ///
    /// `item` is the item to mine it with, `state` the blockstate to be mined. Returns how long
    /// it would take in ticks.
    pub fn calculate_speed_vs_block(item: &ItemStack, state: &BlockState) -> f64 {
        // the host sends -1 where getDestroySpeed(null, null) throws
        let hardness = state.hardness;
        if hardness < 0.0 {
            return -1.0;
        }

        let mut speed = item.get_destroy_speed(state);
        if speed > 1.0
            && let Some(efficiency) = item.mining_efficiency
        {
            speed += efficiency;
        }

        speed /= hardness;
        if !state.requires_tool || (!item.is_empty() && item.is_correct_tool_for_drops(state)) {
            (speed / 30.0) as f64
        } else {
            (speed / 100.0) as f64
        }
    }

    /// Calculates any modifier to breaking time based on status effects.
    ///
    /// Returns a double to scale block breaking speed.
    fn potion_amplifier(&self) -> f64 {
        let mut speed = 1.0;
        if let Some(haste) = self.player.get_effect(Player::HASTE) {
            speed *= 1.0 + haste.amplifier.wrapping_add(1) as f64 * 0.2;
        }
        if let Some(fatigue) = self.player.get_effect(Player::MINING_FATIGUE) {
            match fatigue.amplifier {
                0 => speed *= 0.3,
                1 => speed *= 0.09,
                2 => speed *= 0.0027, // you might think that 0.09*0.3 = 0.027 so that should be next, that would make too much sense. it's 0.0027.
                _ => speed *= 0.00081,
            }
        }
        speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{BlockSet, Inventory, MobEffectInstance, Tool, ToolRule};

    fn table() -> Arc<BlockStateTable> {
        let state =
            |name: &str, hardness: f32, requires_tool: bool, tags: &[&str], default| BlockState {
                name: name.to_owned(),
                hardness,
                requires_tool,
                tags: tags.iter().map(|t| t.to_string()).collect(),
                is_default: default,
                ..BlockState::default()
            };
        Arc::new(
            BlockStateTable::new(
                vec![
                    BlockState {
                        air: true,
                        ..state("minecraft:air", 0.0, false, &[], true)
                    },
                    state(
                        "minecraft:stone",
                        1.5,
                        true,
                        &["minecraft:mineable/pickaxe"],
                        true,
                    ),
                    state(
                        "minecraft:dirt",
                        0.5,
                        false,
                        &["minecraft:mineable/shovel"],
                        true,
                    ),
                    state("minecraft:bedrock", -1.0, false, &[], true),
                    // two states: the second is the default, and the cache uses it
                    state("minecraft:log", 99.0, false, &[], false),
                    state(
                        "minecraft:log",
                        2.0,
                        false,
                        &["minecraft:mineable/axe"],
                        true,
                    ),
                ],
                0,
            )
            .unwrap(),
        )
    }

    fn tool(tag: &str, speed: f32) -> ItemStack {
        ItemStack {
            tool: Some(Tool {
                rules: vec![ToolRule {
                    blocks: BlockSet::Tag(tag.to_owned()),
                    speed: Some(speed),
                    correct_for_drops: Some(true),
                }],
                default_mining_speed: 1.0,
            }),
            ..ItemStack::of("minecraft:tool")
        }
    }

    fn player(items: Vec<ItemStack>) -> Arc<Player> {
        Arc::new(Player {
            inventory: Inventory { items, selected: 3 },
            ..Player::default()
        })
    }

    #[test]
    fn speed_vs_block() {
        let table = table();
        let stone = table.get(1);
        let hand = ItemStack::empty();
        // needs a tool: hand speed 1 / 1.5 / 100
        assert_eq!(
            ToolSet::calculate_speed_vs_block(&hand, stone),
            (1.0f32 / 1.5 / 100.0) as f64
        );
        let pick = tool("minecraft:mineable/pickaxe", 6.0);
        assert_eq!(
            ToolSet::calculate_speed_vs_block(&pick, stone),
            (6.0f32 / 1.5 / 30.0) as f64
        );
        let efficient = ItemStack {
            mining_efficiency: Some(26.0),
            ..pick.clone()
        };
        assert_eq!(
            ToolSet::calculate_speed_vs_block(&efficient, stone),
            (32.0f32 / 1.5 / 30.0) as f64
        );
        // efficiency only applies to a speed above 1
        let slow = ItemStack {
            mining_efficiency: Some(26.0),
            ..ItemStack::of("minecraft:stick")
        };
        assert_eq!(
            ToolSet::calculate_speed_vs_block(&slow, table.get(2)),
            (1.0f32 / 0.5 / 30.0) as f64
        );
        assert_eq!(ToolSet::calculate_speed_vs_block(&pick, table.get(3)), -1.0);
    }

    #[test]
    fn best_slot_and_cache() {
        let table = table();
        let mut items = vec![ItemStack::empty(); 9];
        items[2] = tool("minecraft:mineable/axe", 4.0);
        items[5] = tool("minecraft:mineable/pickaxe", 6.0);
        let tools = ToolSet::new(player(items), Arc::clone(&table));
        assert_eq!(tools.get_best_slot(table.get(1), false), 5);
        // any state of the block is looked up by its default state
        assert_eq!(tools.get_best_slot(table.get(4), false), 2);
        assert_eq!(
            tools.get_str_vs_block(table.get(4)),
            (4.0f32 / 2.0 / 30.0) as f64
        );
        assert_eq!(tools.break_strength_cache.borrow().len(), 1);
        assert_eq!(
            tools.get_str_vs_block(table.get(5)),
            (4.0f32 / 2.0 / 30.0) as f64
        );
        assert_eq!(tools.break_strength_cache.borrow().len(), 1);
        // ties keep the first slot
        assert_eq!(tools.get_best_slot(table.get(0), false), 0);
    }

    #[test]
    fn potion_amplifier() {
        let table = table();
        let mut p = Player::default();
        p.effects.push(MobEffectInstance {
            effect: Player::HASTE.into(),
            amplifier: 1,
        });
        p.effects.push(MobEffectInstance {
            effect: Player::MINING_FATIGUE.into(),
            amplifier: 2,
        });
        let tools = ToolSet::new(Arc::new(p), table);
        assert_eq!(tools.potion_amplifier(), 1.0 * (1.0 + 2.0 * 0.2) * 0.0027);
    }
}
