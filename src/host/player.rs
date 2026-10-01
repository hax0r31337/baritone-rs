//! The local player as the host describes it: inventory, effects, the equipment data path
//! calculation reads, and the entity state execution reads.
//!
//! Stands in for Minecraft's `LocalPlayer`, `Inventory` and `ItemStack` in ported code. Items
//! carry what upstream asks of them (tool rules, tags, damage); the host computes enchantment
//! effects, because enchantments are data driven. With the `bedrock` feature, items have no
//! tool rules: Bedrock derives the tool from item and block tags (`super::bedrock_tool`).
//!
//! The host sends the player every tick. Upstream also writes to the client's player (the
//! selected slot, sprinting, flying, the rotation, the movement input); the port writes to its
//! copy, and the host applies what changed (see `crate::Baritone`).

use serde::{Deserialize, Serialize};

use super::BlockState;
use crate::api::utils::BetterBlockPos;
use crate::mc::{Aabb, Vec3};

/// `HolderSet<Block>`: the blocks a tool rule applies to.
#[cfg(not(feature = "bedrock"))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockSet {
    /// A block tag (`HolderSet.Named`), matched against [`BlockState::tags`].
    Tag(String),
    /// Blocks by id (`HolderSet.Direct`).
    Blocks(Vec<String>),
}

#[cfg(not(feature = "bedrock"))]
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
#[cfg(not(feature = "bedrock"))]
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
#[cfg(not(feature = "bedrock"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub rules: Vec<ToolRule>,
    pub default_mining_speed: f32,
}

#[cfg(not(feature = "bedrock"))]
impl Tool {
    fn get_mining_speed(&self, state: &BlockState) -> f32 {
        for rule in &self.rules {
            if let Some(speed) = rule.speed
                && rule.blocks.contains(state)
            {
                return speed;
            }
        }
        self.default_mining_speed
    }

    fn is_correct_for_drops(&self, state: &BlockState) -> bool {
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
    #[cfg(not(feature = "bedrock"))]
    pub tool: Option<Tool>,
    /// Item tags the item is in. The host must send at least the ones ported code checks:
    /// `minecraft:swords` and the `minecraft:*_tool_materials` tags (`ToolSet`). With the
    /// `bedrock` feature: the `minecraft:is_*` tool kinds (`is_pickaxe`, `is_sword`,
    /// `is_shears`, ...) and the `minecraft:*_tier` tags.
    pub tags: Vec<String>,
    /// What the item's enchantments add to mining speed: the `MINING_EFFICIENCY` attribute
    /// effect of the first enchantment that has one, at the enchantment's level (Efficiency:
    /// `level² + 1`, on Bedrock too). `None` without such an enchantment.
    pub mining_efficiency: Option<f32>,
    /// Enchanted with Silk Touch at a level above 0.
    pub silk_touch: bool,
    /// The stack's components differ from its item's defaults (renamed, enchanted, damaged,
    /// ...). `findSlotMatchingItem` skips such stacks.
    pub components_changed: bool,
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
            #[cfg(not(feature = "bedrock"))]
            tool: None,
            tags: Vec::new(),
            mining_efficiency: None,
            silk_touch: false,
            components_changed: false,
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
        self.get_item() == name
    }

    /// `getItem()`: the item id, air for an empty stack.
    pub fn get_item(&self) -> &str {
        if self.is_empty() {
            Self::AIR
        } else {
            &self.name
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
    #[cfg(not(feature = "bedrock"))]
    pub fn get_destroy_speed(&self, state: &BlockState) -> f32 {
        match &self.tool {
            Some(tool) if !self.is_empty() => tool.get_mining_speed(state),
            _ => 1.0,
        }
    }

    /// `isCorrectToolForDrops(BlockState)`
    #[cfg(not(feature = "bedrock"))]
    pub fn is_correct_tool_for_drops(&self, state: &BlockState) -> bool {
        match &self.tool {
            Some(tool) if !self.is_empty() => tool.is_correct_for_drops(state),
            _ => false,
        }
    }

    /// `getDestroySpeed(BlockState)`: Bedrock's tool speed from the item's tags, before
    /// Efficiency; 1 for anything that does not dig the block faster.
    #[cfg(feature = "bedrock")]
    pub fn get_destroy_speed(&self, state: &BlockState) -> f32 {
        if self.is_empty() {
            return 1.0;
        }
        super::bedrock_tool::tool_speed(self, state)
    }

    /// `isCorrectToolForDrops(BlockState)`: Bedrock's right tool, from item and block tags.
    #[cfg(feature = "bedrock")]
    pub fn is_correct_tool_for_drops(&self, state: &BlockState) -> bool {
        !self.is_empty() && super::bedrock_tool::is_correct_tool(self, state)
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

    /// `setSelectedSlot(int)`: panics outside the hotbar, like upstream's
    /// `IllegalArgumentException`.
    pub fn set_selected_slot(&mut self, slot: i32) {
        if !Self::is_hotbar_slot(slot) {
            panic!("Invalid selected slot");
        }
        self.selected = slot;
    }

    /// `getNonEquipmentItems().get(int)`: panics outside the 36 main slots, like upstream's
    /// `IndexOutOfBoundsException`.
    pub fn get_non_equipment_item(&self, slot: i32) -> &ItemStack {
        if !(0..Self::INVENTORY_SIZE).contains(&slot) {
            panic!("IndexOutOfBoundsException: {slot}");
        }
        self.get_item(slot)
    }

    /// Swaps two main slots, what a `ContainerInput.SWAP` window click between a main slot and
    /// a hotbar slot does to the inventory.
    pub fn swap(&mut self, a: i32, b: i32) {
        let size = a.max(b) as usize + 1;
        if self.items.len() < size {
            self.items.resize(size, ItemStack::empty());
        }
        self.items.swap(a as usize, b as usize);
    }

    /// `Inventory.isHotbarSlot(int)`
    pub fn is_hotbar_slot(slot: i32) -> bool {
        (0..Self::SELECTION_SIZE).contains(&slot)
    }

    /// `findSlotMatchingItem(new ItemStack(item))`: first non-empty main slot holding `name`
    /// with its default components (`isSameItemSameComponents`), or -1.
    pub fn find_slot_matching_item(&self, name: &str) -> i32 {
        for (i, item) in self
            .items
            .iter()
            .take(Self::INVENTORY_SIZE as usize)
            .enumerate()
        {
            if !item.is_empty() && item.name == name && !item.components_changed {
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

/// `InteractionHand`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionHand {
    MainHand,
    OffHand,
}

impl InteractionHand {
    /// `InteractionHand.values()`
    pub const VALUES: [InteractionHand; 2] = [InteractionHand::MainHand, InteractionHand::OffHand];
}

/// The local player (`LocalPlayer`), as far as ported code reads it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Player {
    /// `position()`
    pub position: Vec3,
    /// `xo`, `yo`, `zo`: the position at the end of the previous tick, which
    /// `getEyePosition(1.0F)` interpolates from.
    pub old_position: Vec3,
    /// `getDeltaMovement()`
    pub delta_movement: Vec3,
    /// `getYRot()`, the yaw. Baritone sets it while it looks somewhere (`LookBehavior`).
    pub y_rot: f32,
    /// `getXRot()`, the pitch.
    pub x_rot: f32,
    /// `onGround()`
    pub on_ground: bool,
    /// `horizontalCollision`: the last move was stopped by a wall.
    pub horizontal_collision: bool,
    /// `isCrouching()`: in the crouching pose.
    pub crouching: bool,
    /// `getEyeHeight()`, for the current pose.
    pub eye_height: f32,
    /// `getEyeHeight(Pose.CROUCHING)`
    pub crouching_eye_height: f32,
    /// `getBoundingBox()`
    pub bounding_box: Aabb,
    /// `isInWall()`: the eyes are inside a suffocating block.
    pub in_wall: bool,
    /// `isSprinting()`. Baritone clears it (`setSprinting(false)`).
    pub sprinting: bool,
    /// `getAbilities().flying`. Baritone clears it while it moves.
    pub flying: bool,
    /// `isHandsBusy()`
    pub hands_busy: bool,
    /// `containerMenu != inventoryMenu`: a container (chest, crafting table, ...) is open.
    pub container_open: bool,
    /// `input instanceof PlayerMovementInput`: the client takes its movement input from
    /// Baritone (`InputOverrideHandler`) instead of the keyboard. Baritone sets it.
    pub baritone_input: bool,
    /// `getItemBySlot(EquipmentSlot.OFFHAND)`
    pub offhand: ItemStack,
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
    /// A standing player at the origin with an empty inventory.
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            old_position: Vec3::ZERO,
            delta_movement: Vec3::ZERO,
            y_rot: 0.0,
            x_rot: 0.0,
            on_ground: true,
            horizontal_collision: false,
            crouching: false,
            eye_height: Self::STANDING_EYE_HEIGHT,
            crouching_eye_height: Self::CROUCHING_EYE_HEIGHT,
            bounding_box: Self::standing_box(Vec3::ZERO),
            in_wall: false,
            sprinting: false,
            flying: false,
            hands_busy: false,
            container_open: false,
            baritone_input: false,
            offhand: ItemStack::empty(),
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
    /// Counts as Haste when mining on Bedrock.
    #[cfg(feature = "bedrock")]
    pub const CONDUIT_POWER: &str = "minecraft:conduit_power";

    /// The player's standing eye height (`EntityDimensions.eyeHeight` of `Pose.STANDING`).
    pub const STANDING_EYE_HEIGHT: f32 = 1.62;
    /// The player's crouching eye height.
    pub const CROUCHING_EYE_HEIGHT: f32 = 1.27;

    /// The standing player's box (0.6 wide, 1.8 high) with its feet at `position`
    /// (`EntityDimensions.makeBoundingBox`).
    pub fn standing_box(position: Vec3) -> Aabb {
        let w = (0.6f32 / 2.0f32) as f64;
        let h = 1.8f32 as f64;
        Aabb::new(
            position.x - w,
            position.y,
            position.z - w,
            position.x + w,
            position.y + h,
            position.z + w,
        )
    }

    /// `getX()`
    pub fn get_x(&self) -> f64 {
        self.position.x
    }

    /// `getY()`
    pub fn get_y(&self) -> f64 {
        self.position.y
    }

    /// `getZ()`
    pub fn get_z(&self) -> f64 {
        self.position.z
    }

    /// `blockPosition()`: `BlockPos.containing(position())`.
    pub fn block_position(&self) -> BetterBlockPos {
        BetterBlockPos::from_f64(self.position.x, self.position.y, self.position.z)
    }

    /// `getEyePosition()`
    pub fn get_eye_position(&self) -> Vec3 {
        Vec3::new(
            self.position.x,
            self.position.y + self.eye_height as f64,
            self.position.z,
        )
    }

    /// `getEyePosition(float)`
    pub fn get_eye_position_partial(&self, partial_tick_time: f32) -> Vec3 {
        let a = partial_tick_time as f64;
        Vec3::new(
            crate::mc::mth::lerp(a, self.old_position.x, self.position.x),
            crate::mc::mth::lerp(a, self.old_position.y, self.position.y) + self.eye_height as f64,
            crate::mc::mth::lerp(a, self.old_position.z, self.position.z),
        )
    }

    /// `getItemInHand(InteractionHand)`
    pub fn get_item_in_hand(&self, hand: InteractionHand) -> &ItemStack {
        match hand {
            InteractionHand::MainHand => self.inventory.get_item(self.inventory.selected),
            InteractionHand::OffHand => &self.offhand,
        }
    }

    /// `getInventory()`
    pub fn get_inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// `getEffect(Holder<MobEffect>)`
    pub fn get_effect(&self, effect: &str) -> Option<&MobEffectInstance> {
        self.effects.iter().find(|e| e.effect == effect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Bedrock's tool rules are tested in `bedrock_tool`
    #[cfg(not(feature = "bedrock"))]
    fn state(name: &str, tags: &[&str]) -> BlockState {
        BlockState {
            name: name.to_owned(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..BlockState::default()
        }
    }

    #[cfg(not(feature = "bedrock"))]
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

    #[cfg(not(feature = "bedrock"))]
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

    #[cfg(not(feature = "bedrock"))]
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
        inv.items[4] = ItemStack {
            components_changed: true,
            ..ItemStack::of("minecraft:water_bucket")
        };
        assert_eq!(inv.find_slot_matching_item("minecraft:water_bucket"), 20);
        inv.items[4].components_changed = false;
        assert_eq!(inv.find_slot_matching_item("minecraft:water_bucket"), 4);
        assert!(Inventory::is_hotbar_slot(4));
        assert!(!Inventory::is_hotbar_slot(-1));
    }
}
