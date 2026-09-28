// Ported from baritone src/api/java/baritone/api/utils/BlockOptionalMeta.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Resolved against the host's block state table instead of the registries. The block is its
// name. The matching states are the block's states whose properties have the selected
// values: `matches(state)` compares the name and those properties, which is exactly
// upstream's set of states, and keeps working with any table that has the same blocks. The
// items a block drops are the table's `drops` (upstream rolls the block's loot table in a
// stub server level, `ServerLevelStub`, which is not ported), and item stacks match by item:
// upstream's `getBaritoneHash()` minus the damage is the item's hash.
//
// A bad selector is an `Err(IllegalArgumentException)` with upstream's message. An unknown
// property name, where upstream throws a `NullPointerException`, and a repeated one, where
// Guava's `ImmutableMap.Builder` throws, get messages of their own; so does an invalid value,
// whose message upstream prints with Minecraft's `Property.toString()`. Integer property
// values parse like `Integer.parseInt` (`"+7"` is `7`), for ASCII digits.

use std::fmt;

use rustc_hash::FxHashSet;

use crate::api::utils::block_utils;
use crate::host::{BlockState, BlockStateTable, ItemStack};
use crate::java::{self, IllegalArgumentException};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockOptionalMeta {
    /// The block's name.
    block: String,
    /// exists so toString() can return something more useful than a list of all blockstates
    properties_description: String,
    /// Selected property values, in the selector's order.
    properties: Vec<(String, String)>,
    /// Item ids of the drops of the matching states.
    stack_hashes: FxHashSet<String>,
}

impl BlockOptionalMeta {
    /// `BlockOptionalMeta(Block)`: every state of `block`'s block.
    pub fn from_block(table: &BlockStateTable, block: &BlockState) -> Self {
        let mut bom = Self {
            block: block.name.clone(),
            properties_description: "{}".to_owned(),
            properties: Vec::new(),
            stack_hashes: FxHashSet::default(),
        };
        bom.stack_hashes = bom.get_stack_hashes(table);
        bom
    }

    /// `BlockOptionalMeta(String)`: `id`, `id[]` or `id[key=value,...]`.
    pub fn from_selector(
        table: &BlockStateTable,
        selector: &str,
    ) -> Result<Self, IllegalArgumentException> {
        let Some((id, props)) = parse_selector(selector) else {
            return Err(IllegalArgumentException(
                "invalid block selector".to_owned(),
            ));
        };

        let block = block_utils::string_to_block_required(table, id)?;

        let properties = match props {
            None => Vec::new(),
            Some(props) => Self::parse_properties(table, block, props)?,
        };

        let mut bom = Self {
            block: block.name.clone(),
            properties_description: match props {
                None => "{}".to_owned(),
                Some(props) => format!("{{{}}}", props.replace('=', ":")),
            },
            properties,
            stack_hashes: FxHashSet::default(),
        };
        bom.stack_hashes = bom.get_stack_hashes(table);
        Ok(bom)
    }

    fn parse_properties(
        table: &BlockStateTable,
        block: &BlockState,
        raw: &str,
    ) -> Result<Vec<(String, String)>, IllegalArgumentException> {
        let mut properties: Vec<(String, String)> = Vec::new();
        for pair in java::split(raw, ',') {
            let parts = java::split(pair, '=');
            if parts.len() != 2 {
                return Err(IllegalArgumentException(format!(
                    "\"{pair}\" is not a valid property-value pair"
                )));
            }
            let raw_key = parts[0];
            let raw_value = parts[1];
            let values: Vec<&str> = table
                .get_possible_states(&block.name)
                .filter_map(|state| state.properties.get(raw_key))
                .map(String::as_str)
                .collect();
            if values.is_empty() {
                return Err(IllegalArgumentException(format!(
                    "\"{raw_key}\" is not a property of Block{{{}}}",
                    block.name
                )));
            }
            let Some(value) = property_value(&values, raw_value) else {
                return Err(IllegalArgumentException(format!(
                    "\"{raw_value}\" is not a valid value for {raw_key} on Block{{{}}}",
                    block.name
                )));
            };
            if let Some((_, existing)) = properties.iter().find(|(k, _)| k == raw_key) {
                return Err(IllegalArgumentException(format!(
                    "Multiple entries with same key: {raw_key}={value} and {raw_key}={existing}"
                )));
            }
            properties.push((raw_key.to_owned(), value.to_owned()));
        }
        Ok(properties)
    }

    /// `getStackHashes(Set<BlockState>)`: the drops of the block, if any state matches.
    fn get_stack_hashes(&self, table: &BlockStateTable) -> FxHashSet<String> {
        if self.get_any_block_state(table).is_none() {
            return FxHashSet::default();
        }
        drops(table, &self.block).iter().cloned().collect()
    }

    /// `getBlock()`: the block's name.
    pub fn get_block(&self) -> &str {
        &self.block
    }

    /// `matches(Block)`
    pub fn matches_block(&self, block: &BlockState) -> bool {
        block.name == self.block
    }

    /// `matches(BlockState)`
    pub fn matches(&self, blockstate: &BlockState) -> bool {
        blockstate.name == self.block
            && self
                .properties
                .iter()
                .all(|(key, value)| blockstate.properties.get(key) == Some(value))
    }

    /// `matches(ItemStack)`
    pub fn matches_stack(&self, stack: &ItemStack) -> bool {
        self.stack_hashes.contains(stack.get_item())
    }

    /// `getAnyBlockState()`: the matching state with the lowest id.
    pub fn get_any_block_state<'a>(&self, table: &'a BlockStateTable) -> Option<&'a BlockState> {
        table
            .get_possible_states(&self.block)
            .find(|state| self.matches(state))
    }

    /// `getAllBlockStates()`, in id order.
    pub fn get_all_block_states<'a>(
        &'a self,
        table: &'a BlockStateTable,
    ) -> impl Iterator<Item = &'a BlockState> {
        table
            .get_possible_states(&self.block)
            .filter(|state| self.matches(state))
    }

    /// `stackHashes()`: the item ids that match.
    pub fn stack_hashes(&self) -> &FxHashSet<String> {
        &self.stack_hashes
    }
}

impl fmt::Display for BlockOptionalMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BlockOptionalMeta{{block=Block{{{}}},properties={}}}",
            self.block, self.properties_description
        )
    }
}

/// `drops(Block)`: what the block drops, from its default state's traits.
fn drops<'a>(table: &'a BlockStateTable, block: &str) -> &'a [String] {
    table
        .get_default_state(block)
        .map_or(&[], |state| &state.drops)
}

/// The value among `values` that `Property.getValue(raw)` parses `raw` to: the one spelled
/// the same, or for an integer property the one with the same number.
fn property_value<'a>(values: &[&'a str], raw: &str) -> Option<&'a str> {
    if let Some(&value) = values.iter().find(|&&v| v == raw) {
        return Some(value);
    }
    let number: i32 = raw.parse().ok()?;
    if !values.iter().all(|v| v.parse::<i32>().is_ok()) {
        return None;
    }
    values
        .iter()
        .find(|v| v.parse::<i32>() == Ok(number))
        .copied()
}

/// Upstream's `^(?<id>.+?)(?:\[(?<properties>.+?)?\])?$` with `Matcher.find()`: the id and the
/// properties text (`None` when absent or `[]`), or `None` if the selector does not match.
/// `.` does not match line terminators, and `$` also matches before a final one.
fn parse_selector(selector: &str) -> Option<(&str, Option<&str>)> {
    const TERMINATORS: [char; 5] = ['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}'];
    let body = selector
        .strip_suffix("\r\n")
        .or_else(|| selector.strip_suffix(TERMINATORS))
        .unwrap_or(selector);
    if body.is_empty() || body.contains(TERMINATORS) {
        return None;
    }
    // the lazy id stops at the first '[' (after at least one character) from which
    // `\[.*\]` reaches the end
    if body.ends_with(']')
        && let Some(open) = body[..body.len() - 1]
            .char_indices()
            .skip(1)
            .find(|&(_, c)| c == '[')
            .map(|(i, _)| i)
    {
        let props = &body[open + 1..body.len() - 1];
        return Some((&body[..open], (!props.is_empty()).then_some(props)));
    }
    Some((body, None))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use super::*;

    pub(crate) fn table() -> BlockStateTable {
        let state = |name: &str, props: &[(&str, &str)]| BlockState {
            name: name.to_owned(),
            air: name == "minecraft:air",
            properties: props
                .iter()
                .map(|&(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<BTreeMap<_, _>>(),
            drops: match name {
                "minecraft:stone" => vec!["minecraft:cobblestone".to_owned()],
                "minecraft:wheat" => vec![
                    "minecraft:wheat".to_owned(),
                    "minecraft:wheat_seeds".to_owned(),
                ],
                _ => Vec::new(),
            },
            ..BlockState::default()
        };
        let mut states = vec![state("minecraft:air", &[]), state("minecraft:stone", &[])];
        for lit in ["true", "false"] {
            for facing in ["north", "south"] {
                states.push(state(
                    "minecraft:furnace",
                    &[("facing", facing), ("lit", lit)],
                ));
            }
        }
        for age in 0..8 {
            states.push(state("minecraft:wheat", &[("age", &age.to_string())]));
        }
        BlockStateTable::new(states, 0).unwrap()
    }

    #[test]
    fn selector_syntax() {
        assert_eq!(parse_selector("stone"), Some(("stone", None)));
        assert_eq!(parse_selector("stone[]"), Some(("stone", None)));
        assert_eq!(
            parse_selector("furnace[lit=true]"),
            Some(("furnace", Some("lit=true")))
        );
        assert_eq!(parse_selector("a[b]c]"), Some(("a", Some("b]c"))));
        assert_eq!(parse_selector("a[b"), Some(("a[b", None)));
        assert_eq!(parse_selector("[a]"), Some(("[a]", None)));
        assert_eq!(parse_selector("a[[b]]"), Some(("a", Some("[b]"))));
        assert_eq!(parse_selector("stone\n"), Some(("stone", None)));
        assert_eq!(parse_selector("stone\r\n"), Some(("stone", None)));
        assert_eq!(parse_selector("st\none"), None);
        assert_eq!(parse_selector(""), None);
        assert_eq!(parse_selector("\n"), None);
    }

    #[test]
    fn whole_block() {
        let table = table();
        let bom = BlockOptionalMeta::from_selector(&table, "furnace").unwrap();
        assert_eq!(bom.get_block(), "minecraft:furnace");
        assert_eq!(bom.get_all_block_states(&table).count(), 4);
        assert_eq!(
            bom.to_string(),
            "BlockOptionalMeta{block=Block{minecraft:furnace},properties={}}"
        );
        let stone = BlockOptionalMeta::from_block(&table, table.get(1));
        assert_eq!(
            stone,
            BlockOptionalMeta::from_selector(&table, "minecraft:stone[]").unwrap()
        );
        assert!(stone.matches(table.get(1)));
        assert!(!stone.matches(table.get(2)));
        assert!(stone.matches_stack(&ItemStack::of("minecraft:cobblestone")));
        assert!(!stone.matches_stack(&ItemStack::of("minecraft:stone")));
        assert!(!stone.matches_stack(&ItemStack::empty()));
    }

    #[test]
    fn properties() {
        let table = table();
        let lit = BlockOptionalMeta::from_selector(&table, "furnace[lit=true]").unwrap();
        let states: Vec<u32> = lit.get_all_block_states(&table).map(|s| s.id).collect();
        assert_eq!(states, [2, 3]);
        assert_eq!(
            lit.to_string(),
            "BlockOptionalMeta{block=Block{minecraft:furnace},properties={lit:true}}"
        );
        assert!(lit.matches_block(table.get(5)));
        assert!(!lit.matches(table.get(5)));
        let one =
            BlockOptionalMeta::from_selector(&table, "furnace[lit=false,facing=south]").unwrap();
        assert_eq!(one.get_any_block_state(&table).unwrap().id, 5);
        // Integer.parseInt
        let ripe = BlockOptionalMeta::from_selector(&table, "wheat[age=+7]").unwrap();
        assert_eq!(
            ripe.get_any_block_state(&table).unwrap().properties["age"],
            "7"
        );
        assert_eq!(ripe.stack_hashes().len(), 2);
        // a trailing comma is dropped by String.split
        assert!(BlockOptionalMeta::from_selector(&table, "furnace[lit=true,]").is_ok());
    }

    #[test]
    fn errors() {
        let table = table();
        let err = |s: &str| BlockOptionalMeta::from_selector(&table, s).unwrap_err().0;
        assert_eq!(err(""), "invalid block selector");
        assert_eq!(err("dirt"), "Invalid block name dirt");
        assert_eq!(
            err("furnace[lit]"),
            "\"lit\" is not a valid property-value pair"
        );
        assert_eq!(
            err("furnace[lit=true,,facing=north]"),
            "\"\" is not a valid property-value pair"
        );
        assert_eq!(
            err("furnace[lit=]"),
            "\"lit=\" is not a valid property-value pair"
        );
        assert_eq!(
            err("furnace[lit=maybe]"),
            "\"maybe\" is not a valid value for lit on Block{minecraft:furnace}"
        );
        assert_eq!(
            err("wheat[age=8]"),
            "\"8\" is not a valid value for age on Block{minecraft:wheat}"
        );
        assert_eq!(
            err("furnace[color=red]"),
            "\"color\" is not a property of Block{minecraft:furnace}"
        );
        assert_eq!(
            err("furnace[lit=true,lit=false]"),
            "Multiple entries with same key: lit=false and lit=true"
        );
    }
}
