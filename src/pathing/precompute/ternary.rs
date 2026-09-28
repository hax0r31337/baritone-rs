// Ported from baritone src/main/java/baritone/pathing/precompute/Ternary.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use serde::{Deserialize, Serialize};

/// Serialized as `"yes"`, `"maybe"`, `"no"` (host block state table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ternary {
    Yes,
    Maybe,
    No,
}
