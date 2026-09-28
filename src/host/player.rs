//! The local player as the host describes it: inventory, effects, and the equipment data path
//! calculation reads.
//!
//! Stands in for Minecraft's `LocalPlayer`, `Inventory` and `ItemStack` in ported code. Items
//! carry what upstream asks of them (tool rules, tags, damage); the host computes enchantment
//! effects, because enchantments are data driven. Position, rotation and movement state come
//! with the execution code (phase 4).

use serde::{Deserialize, Serialize};

use super::BlockState;

/// `HolderSet<Block>`: the blocks a tool rule applies to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockSet {
    /// A block tag (`HolderSet.Named`), matched against [`BlockState::tags`].
    Tag(String),
    /// Blocks by id (`HolderSet.Direct`).
    Blocks(Vec<String>),
}

impl BlockSet {
    /// `BlockState.is(HolderSet)`
    pub fn contains(&self, state: &BlockState) -> bool {
        match self {
            BlockSet::Tag(tag) => state.tags.iter().any(|t| t == tag),
            BlockSet::Blocks(blocks) => blocks.contains(&state.name),
        }
    }
}

/// `Tool.Rule`
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRule {
    pub blocks: BlockSet,
    #[serde(default)]
    pub speed: Option<f32>,
    #[serde(default)]
    pub correct_for_drops: Option<bool>,
}

/// The `minecraft:tool` item component (`net.minecraft.world.item.component.Tool`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub rules: Vec<ToolRule>,
    pub default_mining_speed: f32,
}

impl Tool {
    pub fn get_mining_speed(&self, state: &BlockState) -> f32 {
        for rule in &self.rules {
            if let Some(speed) = rule.speed
                && rule.blocks.contains(state)
            {
                return speed;
            }
        }
        self.default_mining_speed
    }

    pub fn is_correct_for_drops(&self, state: &BlockState) -> bool {
        for rule in &self.rules {
            if let Some(correct) = rule.correct_for_drops
                && rule.blocks.contains(state)
            {
                return correct;
            }
        }
        false
    }
}

/// An item stack, as far as ported code looks at it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemStack {
    /// Item id (`"minecraft:diamond_pickaxe"`); `"minecraft:air"` for an empty stack.
    pub name: String,
    pub count: i32,
    /// The `minecraft:damage` component.
    pub damage: i32,
    /// The `minecraft:max_damage` component, 0 without.
    pub max_damage: i32,
    /// The `minecraft:tool` component.
    pub tool: Option<Tool>,
    /// Item tags the item is in. The host must send at least the ones ported code checks:
    /// `minecraft:swords` and the `minecraft:*_tool_materials` tags (`ToolSet`).
    pub tags: Vec<String>,
    /// What the item's enchantments add to mining speed: the `MINING_EFFICIENCY` attribute
    /// effect of the first enchantment that has one, at the enchantment's level (Efficiency:
    /// `level² + 1`). `None` without such an enchantment.
    pub mining_efficiency: Option<f32>,
    /// Enchanted with Silk Touch at a level above 0.
    pub silk_touch: bool,
}

impl Default for ItemStack {
    /// `ItemStack.EMPTY`
    fn default() -> Self {
        Self::empty()
    }
}

impl ItemStack {
    pub const AIR: &str = "minecraft:air";

    /// `ItemStack.EMPTY`
    pub fn empty() -> Self {
        Self {
            name: Self::AIR.to_owned(),
            count: 0,
            damage: 0,
            max_damage: 0,
            tool: None,
            tags: Vec::new(),
            mining_efficiency: None,
            silk_touch: false,
        }
    }

    /// `new ItemStack(item)`: one plain item.
    pub fn of(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            count: 1,
            ..Self::empty()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.name == Self::AIR || self.count <= 0
    }

    /// `is(TagKey<Item>)`
    pub fn is_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// `getItem() == item`
    pub fn is_item(&self, name: &str) -> bool {
        if self.is_empty() {
            name == Self::AIR
        } else {
            self.name == name
        }
    }

    /// `getDamageValue()`
    pub fn get_damage_value(&self) -> i32 {
        // Mth.clamp(damage, 0, getMaxDamage()) is Math.min(Math.max(damage, 0), max)
        self.damage.max(0).min(self.get_max_damage())
    }

    /// `getMaxDamage()`
    pub fn get_max_damage(&self) -> i32 {
        self.max_damage
    }

    /// `getDestroySpeed(BlockState)`: the tool component's speed, 1 without one.
    pub fn get_destroy_speed(&self, state: &BlockState) -> f32 {
        match &self.tool {
            Some(tool) if !self.is_empty() => tool.get_mining_speed(state),
            _ => 1.0,
        }
    }

    /// `isCorrectToolForDrops(BlockState)`
    pub fn is_correct_tool_for_drops(&self, state: &BlockState) -> bool {
        match &self.tool {
            Some(tool) if !self.is_empty() => tool.is_correct_for_drops(state),
            _ => false,
        }
    }
}

/// `net.minecraft.world.entity.player.Inventory`: the 36 main slots, hotbar first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Inventory {
    /// Main inventory slots; slots past the end are empty.
    pub items: Vec<ItemStack>,
    /// Selected hotbar slot.
    pub selected: i32,
}

impl Inventory {
    /// `Inventory.getSelectionSize()`
    pub const SELECTION_SIZE: i32 = 9;
    /// `Inventory.INVENTORY_SIZE`
    pub const INVENTORY_SIZE: i32 = 36;

    /// `getItem(int)`
    pub fn get_item(&self, slot: i32) -> &ItemStack {
        static EMPTY: std::sync::LazyLock<ItemStack> = std::sync::LazyLock::new(ItemStack::empty);
        usize::try_from(slot)
            .ok()
            .and_then(|slot| self.items.get(slot))
            .unwrap_or(&EMPTY)
    }

    /// `getSelectedSlot()`
    pub fn get_selected_slot(&self) -> i32 {
        self.selected
    }

    /// `Inventory.isHotbarSlot(int)`
    pub fn is_hotbar_slot(slot: i32) -> bool {
        (0..Self::SELECTION_SIZE).contains(&slot)
    }

    /// `findSlotMatchingItem(ItemStack)`: first non-empty main slot with the same item, or -1.
    /// Upstream also compares components; ported code only looks for plain items (a water
    /// bucket), so the port compares the item id.
    pub fn find_slot_matching_item(&self, name: &str) -> i32 {
        for (i, item) in self
            .items
            .iter()
            .take(Self::INVENTORY_SIZE as usize)
            .enumerate()
        {
            if !item.is_empty() && item.name == name {
                return i as i32;
            }
        }
        -1
    }
}

/// An active status effect (`MobEffectInstance`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobEffectInstance {
    /// Effect id (`"minecraft:haste"`).
    pub effect: String,
    pub amplifier: i32,
}

/// The local player (`LocalPlayer`), as far as ported code reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Player {
    pub inventory: Inventory,
    /// `getFoodData().getFoodLevel()`
    pub food_level: i32,
    /// Active status effects.
    pub effects: Vec<MobEffectInstance>,
    /// Frost Walker level of the equipment: upstream walks every `EquipmentSlot` and keeps the
    /// last level it finds. 0 without.
    pub frost_walker: i32,
    /// The `WATER_MOVEMENT_EFFICIENCY` attribute effect of the first enchantment on the
    /// equipment that has one, at its level. Depth Strider's is `LevelBasedValue.Linear` in the
    /// vanilla data: `0.33333334f + 0.33333334f * (level - 1)` in `float`, which equals
    /// `level / 3f` up to level 4 but not above. `None` without.
    pub water_movement_efficiency: Option<f32>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            inventory: Inventory::default(),
            food_level: 20,
            effects: Vec::new(),
            frost_walker: 0,
            water_movement_efficiency: None,
        }
    }
}

impl Player {
    pub const HASTE: &str = "minecraft:haste";
    pub const MINING_FATIGUE: &str = "minecraft:mining_fatigue";

    /// `getInventory()`
    pub fn get_inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// `hasEffect(Holder<MobEffect>)`
    pub fn has_effect(&self, effect: &str) -> bool {
        self.get_effect(effect).is_some()
    }

    /// `getEffect(Holder<MobEffect>)`
    pub fn get_effect(&self, effect: &str) -> Option<&MobEffectInstance> {
        self.effects.iter().find(|e| e.effect == effect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, tags: &[&str]) -> BlockState {
        BlockState {
            name: name.to_owned(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..BlockState::default()
        }
    }

    fn pickaxe() -> Tool {
        Tool {
            rules: vec![
                ToolRule {
                    blocks: BlockSet::Tag("minecraft:incorrect_for_wooden_tool".into()),
                    speed: None,
                    correct_for_drops: Some(false),
                },
                ToolRule {
                    blocks: BlockSet::Tag("minecraft:mineable/pickaxe".into()),
                    speed: Some(2.0),
                    correct_for_drops: Some(true),
                },
            ],
            default_mining_speed: 1.0,
        }
    }

    #[test]
    fn tool_rules_first_match_wins() {
        let tool = pickaxe();
        let stone = state("minecraft:stone", &["minecraft:mineable/pickaxe"]);
        let iron = state(
            "minecraft:iron_ore",
            &[
                "minecraft:incorrect_for_wooden_tool",
                "minecraft:mineable/pickaxe",
            ],
        );
        let dirt = state("minecraft:dirt", &["minecraft:mineable/shovel"]);
        assert_eq!(tool.get_mining_speed(&stone), 2.0);
        assert!(tool.is_correct_for_drops(&stone));
        // the deny rule has no speed, so the speed comes from the next rule
        assert_eq!(tool.get_mining_speed(&iron), 2.0);
        assert!(!tool.is_correct_for_drops(&iron));
        assert_eq!(tool.get_mining_speed(&dirt), 1.0);
        assert!(!tool.is_correct_for_drops(&dirt));

        let direct = BlockSet::Blocks(vec!["minecraft:cobweb".into()]);
        assert!(direct.contains(&state("minecraft:cobweb", &[])));
        assert!(!direct.contains(&state("minecraft:stone", &[])));
    }

    #[test]
    fn item_stack() {
        let empty = ItemStack::empty();
        assert!(empty.is_empty());
        assert!(empty.is_item(ItemStack::AIR));
        let stone = state("minecraft:stone", &["minecraft:mineable/pickaxe"]);
        let mut pick = ItemStack {
            tool: Some(pickaxe()),
            ..ItemStack::of("minecraft:wooden_pickaxe")
        };
        assert_eq!(pick.get_destroy_speed(&stone), 2.0);
        assert!(pick.is_correct_tool_for_drops(&stone));
        assert_eq!(
            ItemStack::of("minecraft:stick").get_destroy_speed(&stone),
            1.0
        );
        pick.count = 0;
        assert_eq!(pick.get_destroy_speed(&stone), 1.0, "empty stacks are air");
        pick.count = 1;
        pick.max_damage = 59;
        pick.damage = 100;
        assert_eq!(pick.get_damage_value(), 59);
        pick.damage = -3;
        assert_eq!(pick.get_damage_value(), 0);
    }

    #[test]
    fn inventory() {
        let mut inv = Inventory::default();
        assert!(inv.get_item(0).is_empty());
        assert!(inv.get_item(-1).is_empty());
        assert_eq!(inv.find_slot_matching_item("minecraft:water_bucket"), -1);
        inv.items = vec![ItemStack::empty(); 36];
        inv.items[20] = ItemStack::of("minecraft:water_bucket");
        assert_eq!(inv.find_slot_matching_item("minecraft:water_bucket"), 20);
        assert!(!Inventory::is_hotbar_slot(20));
        inv.items[4] = ItemStack::of("minecraft:water_bucket");
        assert_eq!(inv.find_slot_matching_item("minecraft:water_bucket"), 4);
        assert!(Inventory::is_hotbar_slot(4));
        assert!(!Inventory::is_hotbar_slot(-1));
    }
}
