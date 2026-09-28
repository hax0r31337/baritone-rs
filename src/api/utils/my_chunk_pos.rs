// Ported from baritone src/api/java/baritone/api/utils/MyChunkPos.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use serde::{Deserialize, Serialize};

/// Need a non obfuscated chunkpos that can be loaded from json without resorting to reflection
///
/// Deserialized like Gson does: missing fields are 0, unknown fields are ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct MyChunkPos {
    pub x: i32,
    pub z: i32,
}

impl fmt::Display for MyChunkPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {}", self.x, self.z)
    }
}
