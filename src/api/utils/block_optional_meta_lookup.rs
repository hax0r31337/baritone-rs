// Ported from baritone src/api/java/baritone/api/utils/BlockOptionalMetaLookup.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `has(BlockState)` asks each `BlockOptionalMeta`, which is the same as upstream's set of all
// their states (see `block_optional_meta.rs`).

use std::fmt;

use rustc_hash::FxHashSet;

use crate::api::utils::BlockOptionalMeta;
use crate::host::{BlockState, BlockStateTable, ItemStack};
use crate::java::IllegalArgumentException;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockOptionalMetaLookup {
    /// Block names.
    block_set: FxHashSet<String>,
    /// Item ids.
    stack_hashes: FxHashSet<String>,
    boms: Vec<BlockOptionalMeta>,
}

impl BlockOptionalMetaLookup {
    /// `BlockOptionalMetaLookup(BlockOptionalMeta...)`
    pub fn new(boms: Vec<BlockOptionalMeta>) -> Self {
        let mut blocks = FxHashSet::default();
        let mut stacks = FxHashSet::default();
        for bom in &boms {
            blocks.insert(bom.get_block().to_owned());
            stacks.extend(bom.stack_hashes().iter().cloned());
        }
        Self {
            block_set: blocks,
            stack_hashes: stacks,
            boms,
        }
    }

    /// `BlockOptionalMetaLookup(Block...)` and `BlockOptionalMetaLookup(List<Block>)`
    pub fn from_blocks<'a>(
        table: &BlockStateTable,
        blocks: impl IntoIterator<Item = &'a BlockState>,
    ) -> Self {
        Self::new(
            blocks
                .into_iter()
                .map(|block| BlockOptionalMeta::from_block(table, block))
                .collect(),
        )
    }

    /// `BlockOptionalMetaLookup(String...)`: block selectors.
    pub fn from_selectors<S: AsRef<str>>(
        table: &BlockStateTable,
        blocks: &[S],
    ) -> Result<Self, IllegalArgumentException> {
        Ok(Self::new(
            blocks
                .iter()
                .map(|block| BlockOptionalMeta::from_selector(table, block.as_ref()))
                .collect::<Result<_, _>>()?,
        ))
    }

    /// `has(Block)`
    pub fn has_block(&self, block: &BlockState) -> bool {
        self.block_set.contains(&block.name)
    }

    /// `has(BlockState)`
    pub fn has(&self, state: &BlockState) -> bool {
        self.boms.iter().any(|bom| bom.matches(state))
    }

    /// `has(ItemStack)`
    pub fn has_stack(&self, stack: &ItemStack) -> bool {
        self.stack_hashes.contains(stack.get_item())
    }

    pub fn blocks(&self) -> &[BlockOptionalMeta] {
        &self.boms
    }
}

impl fmt::Display for BlockOptionalMetaLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let boms: Vec<String> = self.boms.iter().map(ToString::to_string).collect();
        write!(f, "BlockOptionalMetaLookup{{[{}]}}", boms.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::utils::block_optional_meta::tests::table;

    #[test]
    fn lookup() {
        let table = table();
        let lookup =
            BlockOptionalMetaLookup::from_selectors(&table, &["stone", "furnace[lit=true]"])
                .unwrap();
        assert!(lookup.has(table.get(1)));
        assert!(lookup.has(table.get(2)));
        assert!(!lookup.has(table.get(4)));
        assert!(lookup.has_block(table.get(4)));
        assert!(!lookup.has_block(table.get(0)));
        assert!(lookup.has_stack(&ItemStack::of("minecraft:cobblestone")));
        assert!(!lookup.has_stack(&ItemStack::of("minecraft:furnace")));
        assert_eq!(
            lookup.to_string(),
            "BlockOptionalMetaLookup{[BlockOptionalMeta{block=Block{minecraft:stone},properties={}}, \
             BlockOptionalMeta{block=Block{minecraft:furnace},properties={lit:true}}]}"
        );
        assert_eq!(
            BlockOptionalMetaLookup::from_blocks(&table, [table.get(1)]),
            BlockOptionalMetaLookup::from_selectors(&table, &["stone"]).unwrap()
        );
        assert!(BlockOptionalMetaLookup::from_selectors(&table, &["stone", "dirt"]).is_err());
        let empty = BlockOptionalMetaLookup::from_selectors::<&str>(&table, &[]).unwrap();
        assert!(empty.blocks().is_empty());
        assert_eq!(empty.to_string(), "BlockOptionalMetaLookup{[]}");
    }
}
