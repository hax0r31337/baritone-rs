#!/usr/bin/env bash
# Regenerates tests/fixtures/reference/{math_goals.json,blocks.json.gz,paths.json.gz}
# and src/mc/mth_tables.rs by running the real upstream Baritone classes against the real
# Minecraft client jar (registries bootstrapped, block tags bound from its data pack).
#
# Usage: tools/refgen/run.sh [path/to/baritone]   (default: ../baritone)
# Needs: java/javac 25+, curl, sha1sum. Downloads (~70 MB) are cached in target/refgen-cache.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BARITONE="$(cd "${1:-$ROOT/../baritone}" && pwd)"
COMMIT="$(cat "$ROOT/UPSTREAM")"
MINECRAFT=26.3
CACHE="$ROOT/target/refgen-cache"
BUILD="$ROOT/target/refgen-build"
OUT="$ROOT/tests/fixtures/reference/math_goals.json"
BLOCKS_OUT="$ROOT/tests/fixtures/reference/blocks.json.gz"
PATHS_OUT="$ROOT/tests/fixtures/reference/paths.json.gz"
MTH_TABLES="$ROOT/src/mc/mth_tables.rs"

head="$(git -C "$BARITONE" rev-parse HEAD)"
if [[ "$head" != "$COMMIT" ]]; then
    echo "error: $BARITONE is at $head, UPSTREAM says $COMMIT" >&2
    exit 1
fi

# sha1 url
JARS=(
    "e877b6a07acd633fb3bb475002175cec036e7b87 https://piston-data.mojang.com/v1/objects/e877b6a07acd633fb3bb475002175cec036e7b87/client.jar"
    "efc0e34ede4e3204eaefb84a00e55e8c86634382 https://libraries.minecraft.net/com/google/code/gson/gson/2.14.0/gson-2.14.0.jar"
    "aeaffd00d57023a2c947393ed251f0354f0985fc https://libraries.minecraft.net/com/google/guava/failureaccess/1.0.3/failureaccess-1.0.3.jar"
    "c376b13067cc99a5774403530953f7b05a91e218 https://libraries.minecraft.net/com/google/guava/guava/33.6.0-jre/guava-33.6.0-jre.jar"
    "add8754cda96cf0cd840441632875648836f8a71 https://libraries.minecraft.net/com/mojang/authlib/10.0.77/authlib-10.0.77.jar"
    "3373d1e7bf00c8b99bed1ea4efb8c47344e4a887 https://libraries.minecraft.net/com/mojang/brigadier/1.3.11/brigadier-1.3.11.jar"
    # Minecraft.<clinit> (reached through Baritone's Helper) creates a tracy section category
    "cc2ad81342001b4281c305a298d7f50332354058 https://libraries.minecraft.net/com/mojang/jtracy/1.14.38/jtracy-1.14.38.jar"
    "b6b2ae770c02e0c1eb90f9985b151e9085a38d0b https://libraries.minecraft.net/com/mojang/datafixerupper/10.0.21/datafixerupper-10.0.21.jar"
    "351cea64a5233361327d8d54c44277041beed97f https://libraries.minecraft.net/com/mojang/logging/1.7.12/logging-1.7.12.jar"
    "05397ef65dcb60670e0e0dba4854522873af3070 https://libraries.minecraft.net/io/netty/netty-buffer/4.2.16.Final/netty-buffer-4.2.16.Final.jar"
    "01a01d45a9efc228c6d747525848b065e0656b5f https://libraries.minecraft.net/io/netty/netty-codec-base/4.2.16.Final/netty-codec-base-4.2.16.Final.jar"
    "c2bc7fa5acfa2afc777e944d84f0aa65559cc81e https://libraries.minecraft.net/io/netty/netty-common/4.2.16.Final/netty-common-4.2.16.Final.jar"
    "a6cff377eecc19c2037bf31568a6d7106b50ba1f https://libraries.minecraft.net/it/unimi/dsi/fastutil/8.5.18/fastutil-8.5.18.jar"
    "65897b3e5731220962e659e001904af3c3cbeba9 https://libraries.minecraft.net/org/apache/commons/commons-lang3/3.20.0/commons-lang3-3.20.0.jar"
    "ad52af0ecf054a7e3f275a2e180ee06d9c490951 https://libraries.minecraft.net/org/apache/logging/log4j/log4j-api/2.26.0/log4j-api-2.26.0.jar"
    "438e036486bad66b189bff385dd07dea4f74a146 https://libraries.minecraft.net/org/joml/joml/1.10.9/joml-1.10.9.jar"
    "7425a601c1c7ec76645a78d22b8c6a627edee507 https://libraries.minecraft.net/org/jspecify/jspecify/1.0.0/jspecify-1.0.0.jar"
    "d9e58ac9c7779ba3bf8142aff6c830617a7fe60f https://libraries.minecraft.net/org/slf4j/slf4j-api/2.0.17/slf4j-api-2.0.17.jar"
    # compile-only dependencies of upstream classes that javac pulls in through -sourcepath
    "1d71ed0f8310e92117bd78ffa1a766e026b39d97 https://babbaj.github.io/maven/dev/babbaj/nether-pathfinder/1.6/nether-pathfinder-1.6.jar"
    "25ea2e8b0c338a877313bd4672d3fe056ea78f0d https://repo1.maven.org/maven2/com/google/code/findbugs/jsr305/3.0.2/jsr305-3.0.2.jar"
    "3f2bd4ba11c4162733c13cc90ca7c7ea09967102 https://repo1.maven.org/maven2/commons-io/commons-io/2.7/commons-io-2.7.jar"
    "7af6a669488450c4a07c2c3254e2151df42d7d04 https://repo1.maven.org/maven2/org/jetbrains/annotations/24.1.0/annotations-24.1.0.jar"
)

mkdir -p "$CACHE"
CP=""
for entry in "${JARS[@]}"; do
    sha1="${entry%% *}"
    url="${entry#* }"
    file="$CACHE/$sha1-$(basename "$url")"
    if [[ ! -f "$file" ]]; then
        curl -sSfL -o "$file.part" "$url"
        echo "$sha1  $file.part" | sha1sum -c --quiet -
        mv "$file.part" "$file"
    fi
    CP="$CP:$file"
done
CP="${CP#:}"

# Real upstream sources under test, plus whatever they reference: javac compiles those from
# -sourcepath on demand. Only BaritoneAPI is stubbed (tools/refgen/stubs), because the real one
# reads the settings file and boots the Baritone provider; the stub hands out a real Settings.
API="$BARITONE/src/api/java/baritone/api"
MAIN="$BARITONE/src/main/java/baritone"
SOURCES=(
    "$API"/pathing/goals/*.java
    "$API/pathing/movement/ActionCosts.java"
    "$API/utils/BetterBlockPos.java"
    "$API/utils/Rotation.java"
    "$API/utils/interfaces/IGoalRenderPos.java"
    "$MAIN/utils/BaritoneMath.java"
    "$MAIN/pathing/movement/MovementHelper.java"
    "$MAIN/pathing/precompute/PrecomputedData.java"
    "$MAIN/utils/BlockStateInterface.java"
    "$MAIN/utils/pathing/BetterWorldBorder.java"
    "$MAIN/pathing/calc/AStarPathFinder.java"
    "$MAIN/pathing/calc/AbstractNodeCostSearch.java"
    "$MAIN/pathing/calc/Path.java"
    "$MAIN/pathing/calc/PathNode.java"
    "$MAIN/pathing/calc/openset/BinaryHeapOpenSet.java"
    "$MAIN/pathing/movement/CalculationContext.java"
    "$MAIN/pathing/movement/Movement.java"
    "$MAIN/pathing/movement/Moves.java"
    "$MAIN"/pathing/movement/movements/*.java
    "$MAIN/pathing/path/CutoffPath.java"
    "$MAIN/utils/ToolSet.java"
    "$MAIN/utils/pathing/Avoidance.java"
    "$MAIN/utils/pathing/Favoring.java"
    "$MAIN/utils/pathing/MutableMoveResult.java"
    "$MAIN/utils/pathing/PathBase.java"
)

rm -rf "$BUILD"
mkdir -p "$BUILD"
javac -nowarn -encoding UTF-8 -d "$BUILD" -cp "$CP" \
    -sourcepath "$BARITONE/src/main/java:$BARITONE/src/api/java:$BARITONE/src/schematica_api/java" \
    $(find "$ROOT/tools/refgen/stubs" "$ROOT/tools/refgen/src" -name '*.java') \
    "${SOURCES[@]}"
java -cp "$BUILD:$CP" refgen.RefGen "$COMMIT" "$MINECRAFT" "$OUT" "$MTH_TABLES" "$BLOCKS_OUT" "$PATHS_OUT"
echo "wrote $OUT, $BLOCKS_OUT, $PATHS_OUT and $MTH_TABLES"
