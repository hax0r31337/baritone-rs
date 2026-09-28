// Ported from baritone src/api/java/baritone/api/utils/BlockUtils.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Blocks are looked up in the host's block state table instead of `BuiltInRegistries.BLOCK`;
// a `Block` is its default state. The table indexes block names, so upstream's resource cache
// is not needed.

use crate::host::{BlockState, BlockStateTable};
use crate::java::IllegalArgumentException;

pub fn block_to_string(block: &BlockState) -> String {
    match block.name.strip_prefix("minecraft:") {
        // normally, only write the part after the minecraft:
        Some(path) => path.to_owned(),
        // Baritone is running on top of forge with mods installed, perhaps?
        None => block.name.clone(), // include the namespace with the colon
    }
}

pub fn string_to_block_required<'a>(
    table: &'a BlockStateTable,
    name: &str,
) -> Result<&'a BlockState, IllegalArgumentException> {
    string_to_block_nullable(table, name)
        .ok_or_else(|| IllegalArgumentException(format!("Invalid block name {name}")))
}

pub fn string_to_block_nullable<'a>(
    table: &'a BlockStateTable,
    name: &str,
) -> Option<&'a BlockState> {
    // Identifier.tryParse: an empty namespace is minecraft's
    let id = match name.split_once(':') {
        Some(("", path)) => format!("minecraft:{path}"),
        Some(_) => name.to_owned(),
        None => format!("minecraft:{name}"),
    };
    table.get_default_state(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> BlockStateTable {
        let state = |name: &str| BlockState {
            name: name.to_owned(),
            air: name == "minecraft:air",
            ..BlockState::default()
        };
        BlockStateTable::new(
            vec![
                state("minecraft:air"),
                state("minecraft:stone"),
                state("mod:thing"),
            ],
            0,
        )
        .unwrap()
    }

    #[test]
    fn names() {
        let table = table();
        assert_eq!(block_to_string(table.get(1)), "stone");
        assert_eq!(block_to_string(table.get(2)), "mod:thing");
        for name in ["stone", "minecraft:stone", ":stone"] {
            assert_eq!(string_to_block_nullable(&table, name).unwrap().id, 1);
        }
        assert_eq!(string_to_block_nullable(&table, "mod:thing").unwrap().id, 2);
        assert!(string_to_block_nullable(&table, "thing").is_none());
        assert_eq!(
            string_to_block_required(&table, "dirt"),
            Err(IllegalArgumentException(
                "Invalid block name dirt".to_owned()
            ))
        );
    }
}
