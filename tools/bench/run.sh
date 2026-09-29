#!/usr/bin/env bash
# Benchmarks path calculation of upstream Baritone (the real classes, as tools/refgen compiles
# them) against the port, on worlds the vanilla 26.3 dedicated server generates from random
# seeds: an Overworld and a Nether square of 24x24 chunks per seed. Both sides run the same
# queries; the port's side checks that it finds the same paths and prints the comparison
# (also written to target/bench/report.md).
#
# Usage: MINECRAFT_EULA=true tools/bench/run.sh [options] [path/to/baritone]   (default: ../baritone)
#   --seeds a,b,c   world seeds (default: 3 random ones)
#   --queries N     queries per world (default 20)
#   --jit-min N     JIT warm-up passes over every query of every world before anything is
#                   timed, at least (default 3); more until the JIT stops compiling and pass
#                   times settle, at most --jit-max (default 10). Upstream only; the port has no JIT
#   --warmup N      untimed passes over a world's queries before its timed ones, on both
#                   sides (default 1)
#   --reps N        timed passes; the report takes the median (default 5)
#   --budget N      nodes a calculation may consider before it is cancelled (default 500000)
#
# Generating a world runs the Minecraft server, which needs you to agree to the Minecraft EULA
# (https://aka.ms/MinecraftEULA); MINECRAFT_EULA=true says you do. Generated worlds are cached
# in target/bench-cache (per seed), so later runs with the same seeds skip the server.
# Needs: what tools/refgen/run.sh needs, and cargo.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SEEDS=""
QUERIES=20
WARMUP=1
JIT_MIN=3
JIT_MAX=10
REPS=5
BUDGET=500000
BARITONE_ARG=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --seeds) SEEDS="$2"; shift 2 ;;
        --queries) QUERIES="$2"; shift 2 ;;
        --warmup) WARMUP="$2"; shift 2 ;;
        --reps) REPS="$2"; shift 2 ;;
        --budget) BUDGET="$2"; shift 2 ;;
        --jit-min) JIT_MIN="$2"; shift 2 ;;
        --jit-max) JIT_MAX="$2"; shift 2 ;;
        -*) echo "unknown option $1" >&2; exit 1 ;;
        *) BARITONE_ARG="$1"; shift ;;
    esac
done
if [[ -z "$SEEDS" ]]; then
    SEEDS="$RANDOM$RANDOM,$RANDOM$RANDOM,$RANDOM$RANDOM"
fi
BARITONE="$(cd "${BARITONE_ARG:-$ROOT/../baritone}" && pwd)"
BUILD="$ROOT/target/bench-build"
BENCH_CACHE="$ROOT/target/bench-cache"
OUT="$ROOT/target/bench"

source "$ROOT/tools/refgen/common.sh"

SERVER_SHA1=33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c
SERVER="$BENCH_CACHE/$SERVER_SHA1-server.jar"
mkdir -p "$BENCH_CACHE"
if [[ ! -f "$SERVER" ]]; then
    curl -sSfL -o "$SERVER.part" "https://piston-data.mojang.com/v1/objects/$SERVER_SHA1/server.jar"
    echo "$SERVER_SHA1  $SERVER.part" | sha1sum -c --quiet -
    mv "$SERVER.part" "$SERVER"
fi

compile_refgen "$BUILD" "$ROOT/tools/bench/src"
cargo build --release --example bench --manifest-path "$ROOT/Cargo.toml"

rm -rf "$OUT"
mkdir -p "$OUT"
echo "seeds $SEEDS"
java -Xms8G -Xmx8G -cp "$BUILD:$CP" refgen.Bench "$SERVER" "$BENCH_CACHE" "$OUT" "$SEEDS" "$QUERIES" "$WARMUP" "$REPS" "$BUDGET" "$JIT_MIN" "$JIT_MAX"
"$ROOT/target/release/examples/bench" "$OUT"
