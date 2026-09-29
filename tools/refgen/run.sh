#!/usr/bin/env bash
# Regenerates tests/fixtures/reference/{math_goals.json,blocks.json.gz,paths.json.gz,exec.json.gz}
# and src/mc/mth_tables.rs by running the real upstream Baritone classes against the real
# Minecraft client jar (registries bootstrapped, block tags bound from its data pack).
#
# Usage: tools/refgen/run.sh [path/to/baritone]   (default: ../baritone)
# Needs: java/javac 25+, curl, sha1sum. Downloads (~70 MB) are cached in target/refgen-cache.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BARITONE="$(cd "${1:-$ROOT/../baritone}" && pwd)"
BUILD="$ROOT/target/refgen-build"
OUT="$ROOT/tests/fixtures/reference/math_goals.json"
BLOCKS_OUT="$ROOT/tests/fixtures/reference/blocks.json.gz"
PATHS_OUT="$ROOT/tests/fixtures/reference/paths.json.gz"
EXEC_OUT="$ROOT/tests/fixtures/reference/exec.json.gz"
MTH_TABLES="$ROOT/src/mc/mth_tables.rs"

source "$ROOT/tools/refgen/common.sh"

compile_refgen "$BUILD"
java -cp "$BUILD:$CP" refgen.RefGen "$COMMIT" "$MINECRAFT" "$OUT" "$MTH_TABLES" "$BLOCKS_OUT" "$PATHS_OUT" "$EXEC_OUT"
echo "wrote $OUT, $BLOCKS_OUT, $PATHS_OUT, $EXEC_OUT and $MTH_TABLES"
