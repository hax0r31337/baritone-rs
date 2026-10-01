//! Bedrock Edition's tools and break speed, for the `bedrock` feature.
//!
//! Not from upstream. Bedrock items have no `minecraft:tool` component: whether an item is the
//! right tool, and how fast it digs, follow from the item's tags (`minecraft:is_pickaxe`,
//! `minecraft:iron_tier`, ...) and the block's (`minecraft:is_pickaxe_item_destructible`,
//! `minecraft:iron_tier_destructible`, ...).

use super::{BlockState, ItemStack};

const IS_SHEARS: &str = "minecraft:is_shears";
const IS_PICKAXE: &str = "minecraft:is_pickaxe";
const IS_AXE: &str = "minecraft:is_axe";
const IS_SHOVEL: &str = "minecraft:is_shovel";
const IS_HOE: &str = "minecraft:is_hoe";
const IS_SWORD: &str = "minecraft:is_sword";

/// Pickaxe tiers that dig each tier's blocks: a block of `DIGGING_TIERS[i]`'s tier needs a
/// pickaxe of that tier or one after it.
const DIGGING_TIERS: [&str; 4] = [
    "minecraft:stone_tier",
    "minecraft:iron_tier",
    "minecraft:diamond_tier",
    "minecraft:netherite_tier",
];

/// The block tags that say a block needs a pickaxe of `DIGGING_TIERS[i]`, highest tier first.
const DESTRUCTIBLE_TIERS: [(&str, usize); 3] = [
    ("minecraft:diamond_tier_destructible", 2),
    ("minecraft:iron_tier_destructible", 1),
    ("minecraft:stone_tier_destructible", 0),
];

/// A right tool's speed by its tier; the first tag the item has wins.
const TIER_SPEEDS: [(&str, f32); 6] = [
    ("minecraft:wooden_tier", 2.0),
    ("minecraft:stone_tier", 4.0),
    ("minecraft:iron_tier", 6.0),
    ("minecraft:diamond_tier", 8.0),
    ("minecraft:golden_tier", 12.0),
    ("minecraft:netherite_tier", 9.0),
];

/// The speed of a right tool without a tier tag.
const FALLBACK_TOOL_SPEED: f32 = 2.0;

/// Java's `DataComponents.TOOL` holders: `minecraft:is_tool` (pickaxes, axes, shovels, hoes,
/// swords, the mace) and shears.
pub(crate) fn is_tool(item: &ItemStack) -> bool {
    item.is_tag("minecraft:is_tool") || item.is_tag(IS_SHEARS)
}

fn block_is(state: &BlockState, tag: &str) -> bool {
    state.tags.iter().any(|t| t == tag)
}

/// Whether `item` is the right tool for `state`: its kind digs the block and, for a pickaxe, its
/// tier is high enough.
pub(crate) fn is_correct_tool(item: &ItemStack, state: &BlockState) -> bool {
    if item.is_tag(IS_SHEARS) {
        block_is(state, "minecraft:is_shears_item_destructible")
    } else if item.is_tag(IS_PICKAXE) {
        if !block_is(state, "minecraft:is_pickaxe_item_destructible") {
            return false;
        }
        match DESTRUCTIBLE_TIERS
            .iter()
            .find(|(tag, _)| block_is(state, tag))
        {
            Some(&(_, tier)) => DIGGING_TIERS[tier..].iter().any(|t| item.is_tag(t)),
            None => true,
        }
    } else if item.is_tag(IS_AXE) {
        block_is(state, "minecraft:is_axe_item_destructible")
    } else if item.is_tag(IS_SHOVEL) {
        block_is(state, "minecraft:is_shovel_item_destructible")
    } else if item.is_tag(IS_HOE) {
        block_is(state, "minecraft:is_hoe_item_destructible")
    } else if item.is_tag(IS_SWORD) {
        block_is(state, "minecraft:is_sword_item_destructible")
    } else {
        false
    }
}

/// The efficiency `item` multiplies the base break speed of `state` by, before the Efficiency
/// enchantment: 1 when the item does not dig the block faster.
pub(crate) fn tool_speed(item: &ItemStack, state: &BlockState) -> f32 {
    let name = state.name.as_str();
    if item.is_tag(IS_SWORD) {
        if name == "minecraft:web" {
            15.0
        } else if matches!(name, "minecraft:bamboo" | "minecraft:bamboo_sapling") {
            // Number.MAX_VALUE: instant
            f32::MAX
        } else if name.ends_with("_leaves")
            || matches!(
                name,
                "minecraft:vine"
                    | "minecraft:glow_lichen"
                    | "minecraft:pumpkin"
                    | "minecraft:carved_pumpkin"
                    | "minecraft:lit_pumpkin"
                    | "minecraft:melon_block"
                    | "minecraft:cocoa"
                    | "minecraft:big_dripleaf"
                    | "minecraft:chorus_plant"
                    | "minecraft:chorus_flower"
            )
        {
            1.5
        } else {
            1.0
        }
    } else if item.is_tag(IS_SHEARS) {
        if name == "minecraft:web" || name.ends_with("_leaves") {
            15.0
        } else if name.ends_with("_wool") {
            5.0
        } else if matches!(name, "minecraft:vine" | "minecraft:glow_lichen") {
            2.0
        } else {
            1.0
        }
    } else if is_correct_tool(item, state) {
        TIER_SPEEDS
            .iter()
            .find(|(tag, _)| item.is_tag(tag))
            .map_or(FALLBACK_TOOL_SPEED, |&(_, speed)| speed)
    } else {
        1.0
    }
}

/// Break progress per tick of mining `state` with `item` (the empty stack for the hand), for
/// `ToolSet.calculateSpeedVsBlock`: Bedrock's break speed per second / 20, without status
/// effects, water and being airborne, which upstream leaves out on Java too. Negative if the
/// block is unbreakable.
pub(crate) fn speed_vs_block(item: &ItemStack, state: &BlockState) -> f64 {
    let hardness = state.hardness;
    if hardness < 0.0 {
        return -1.0;
    }
    let right_tool = item.is_correct_tool_for_drops(state);
    let base_time = if right_tool || !state.requires_tool {
        1.5
    } else {
        5.0
    } * hardness as f64;

    let mut efficiency = item.get_destroy_speed(state) as f64;
    if efficiency > 1.0
        && let Some(bonus) = item.mining_efficiency
    {
        efficiency += bonus as f64;
    }
    // progress per second, 20 ticks a second
    1.0 / base_time * efficiency / 20.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(name: &str, hardness: f32, requires_tool: bool, tags: &[&str]) -> BlockState {
        BlockState {
            name: name.to_owned(),
            hardness,
            requires_tool,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..BlockState::default()
        }
    }

    fn item(name: &str, tags: &[&str]) -> ItemStack {
        ItemStack {
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..ItemStack::of(name)
        }
    }

    fn pickaxe(tier: &str) -> ItemStack {
        item(
            &format!("minecraft:{tier}_pickaxe"),
            &[
                "minecraft:digger",
                IS_PICKAXE,
                "minecraft:is_tool",
                &format!("minecraft:{tier}_tier"),
            ],
        )
    }

    // tags and hardness from BedrockData 1.26.50.5
    fn stone() -> BlockState {
        block(
            "minecraft:stone",
            1.5,
            true,
            &["minecraft:is_pickaxe_item_destructible", "minecraft:stone"],
        )
    }

    fn iron_ore() -> BlockState {
        block(
            "minecraft:iron_ore",
            3.0,
            true,
            &[
                "minecraft:is_pickaxe_item_destructible",
                "minecraft:stone_tier_destructible",
                "minecraft:iron_pick_diggable",
                "minecraft:diamond_pick_diggable",
            ],
        )
    }

    fn obsidian() -> BlockState {
        block(
            "minecraft:obsidian",
            35.0,
            true,
            &[
                "minecraft:is_pickaxe_item_destructible",
                "minecraft:diamond_tier_destructible",
                "minecraft:diamond_pick_diggable",
            ],
        )
    }

    fn web() -> BlockState {
        block(
            "minecraft:web",
            4.0,
            true,
            &[
                "minecraft:is_shears_item_destructible",
                "minecraft:is_sword_item_destructible",
            ],
        )
    }

    #[test]
    fn pickaxe_tiers() {
        let (stone, iron_ore, obsidian) = (stone(), iron_ore(), obsidian());
        for (tier, digs) in [
            ("wooden", [true, false, false]),
            ("golden", [true, false, false]),
            ("stone", [true, true, false]),
            ("iron", [true, true, false]),
            ("diamond", [true, true, true]),
            ("netherite", [true, true, true]),
        ] {
            let pick = pickaxe(tier);
            let actual = [&stone, &iron_ore, &obsidian].map(|b| is_correct_tool(&pick, b));
            assert_eq!(actual, digs, "{tier}");
        }
        let dirt = block(
            "minecraft:dirt",
            0.5,
            false,
            &["minecraft:is_shovel_item_destructible"],
        );
        assert!(!is_correct_tool(&pickaxe("diamond"), &dirt));
        assert!(!is_correct_tool(&ItemStack::of("minecraft:stick"), &stone));
    }

    #[test]
    fn speeds() {
        let stone = stone();
        assert_eq!(tool_speed(&pickaxe("wooden"), &stone), 2.0);
        assert_eq!(tool_speed(&pickaxe("golden"), &stone), 12.0);
        assert_eq!(tool_speed(&pickaxe("netherite"), &stone), 9.0);
        // a tier without a speed: the fallback
        assert_eq!(tool_speed(&pickaxe("copper"), &stone), FALLBACK_TOOL_SPEED);
        // the wrong tier is not the right tool
        assert_eq!(tool_speed(&pickaxe("wooden"), &iron_ore()), 1.0);

        let sword = item(
            "minecraft:diamond_sword",
            &[IS_SWORD, "minecraft:is_tool", "minecraft:diamond_tier"],
        );
        let shears = item("minecraft:shears", &[IS_SHEARS]);
        let leaves = block("minecraft:oak_leaves", 0.2, false, &[]);
        let wool = block("minecraft:white_wool", 0.8, false, &[]);
        assert_eq!(tool_speed(&sword, &web()), 15.0);
        assert_eq!(tool_speed(&sword, &leaves), 1.5);
        assert_eq!(tool_speed(&sword, &stone), 1.0);
        assert_eq!(
            tool_speed(&sword, &block("minecraft:bamboo", 1.0, false, &[])),
            f32::MAX
        );
        assert_eq!(tool_speed(&shears, &leaves), 15.0);
        assert_eq!(tool_speed(&shears, &wool), 5.0);
        assert_eq!(tool_speed(&shears, &stone), 1.0);
    }

    #[test]
    fn progress_per_tick() {
        let stone = stone();
        // hand on a block that needs a tool: 1 / (5 * 1.5) / 20
        assert_eq!(
            speed_vs_block(&ItemStack::empty(), &stone),
            1.0 / 7.5 / 20.0
        );
        assert_eq!(
            speed_vs_block(&pickaxe("wooden"), &stone),
            1.0 / 2.25 * 2.0 / 20.0
        );
        let efficient = ItemStack {
            mining_efficiency: Some(26.0),
            ..pickaxe("diamond")
        };
        assert_eq!(speed_vs_block(&efficient, &stone), 1.0 / 2.25 * 34.0 / 20.0);
        // efficiency only adds to a speed above 1
        let stick = ItemStack {
            mining_efficiency: Some(26.0),
            ..ItemStack::of("minecraft:stick")
        };
        assert_eq!(speed_vs_block(&stick, &stone), 1.0 / 7.5 / 20.0);
        let bedrock = block("minecraft:bedrock", -1.0, false, &[]);
        assert_eq!(speed_vs_block(&pickaxe("netherite"), &bedrock), -1.0);
    }
}
