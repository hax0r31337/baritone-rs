//! Shared by the execution tests: the Java-semantics block state table and the client
//! simulation.

#![allow(dead_code)] // each test crate uses a different part

pub mod sim;

use std::io::Read;
use std::sync::{Arc, LazyLock};

use baritone::host::BlockStateTable;
use flate2::read::GzDecoder;
use serde::Deserialize;

/// The 26.3 block state table from `fixtures/reference/blocks.json.gz`.
pub static TABLE: LazyLock<Arc<BlockStateTable>> = LazyLock::new(|| {
    #[derive(Deserialize)]
    struct Blocks {
        table: BlockStateTable,
    }
    let mut json = String::new();
    GzDecoder::new(&include_bytes!("../fixtures/reference/blocks.json.gz")[..])
        .read_to_string(&mut json)
        .unwrap();
    let blocks: Blocks = serde_json::from_str(&json).unwrap();
    Arc::new(blocks.table)
});

/// The id of `name`'s default state.
pub fn block(name: &str) -> u32 {
    TABLE
        .get_default_state(name)
        .unwrap_or_else(|| panic!("no block {name}"))
        .id
}
