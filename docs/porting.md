# Porting conventions

How upstream Java maps to this crate. Follow these when porting new files or upstream diffs, so
that the next diff can be ported mechanically too. The upstream commit is in `UPSTREAM`.

## Layout and names

- One Rust file per upstream file. `src/api/java/baritone/api/a/B.java` → `src/api/a/b.rs`,
  `src/main/java/baritone/a/B.java` → `src/a/b.rs`, `api/Settings.java` → `src/settings.rs`.
  File names are the class name in snake_case (`IGoalRenderPos` → `i_goal_render_pos.rs`).
- First line: `// Ported from baritone <path> @ <commit>`. If only part of the file is ported,
  say which parts are missing and in which phase they come, right below it.
- Types keep their upstream names. Methods and fields are upstream names in snake_case, including
  `get_` prefixes (`get_yaw`, `get_goal_pos`).
- Java overloads: the most-used overload keeps the plain name. The others get a suffix for what
  sets them apart, and a doc comment with the Java signature:
  - `_pos` for the `BlockPos` form (`is_in_goal_pos`, `heuristic_pos`, `long_hash_pos`)
  - `_n` for the amount form (`above_n(amt)`, `relative_n(dir, dist)`)
  - `_f64` / `_f32` for a numeric type variant (`mth::wrap_degrees_f64`)
  - a descriptive name when the difference is semantic: `Goal::heuristic()` is
    `heuristic_at_goal()`, the 2-argument `calcRotationFromVec3d` is
    `calc_rotation_from_vec3d_absolute`
  - for `MovementHelper`-style overloads that differ in how they reach the world, the
    `CalculationContext` form is the plain name, `_bsi` is the `BlockStateInterface` form and
    `_ctx` the `IPlayerContext` form; `_state` marks the form with an extra `BlockState`
    argument (`can_walk_on(context, x, y, z)`, `can_walk_on_bsi(bsi, x, y, z)`,
    `can_walk_on_bsi_state(bsi, x, y, z, state)`). A function with a single form keeps the
    plain name whatever its arguments (`avoid_breaking(bsi, ...)`).
- Constructors are `new`; other constructors are `from_pos`, `from_f64`, ... A constructor that
  takes a nullable `Integer` takes `Option<i32>` instead of being split in two.

## Minecraft types

- `BlockPos` is `BetterBlockPos` everywhere.
- `Mth`, `Vec3`, `Direction`, `AABB` (`Aabb`) live in `crate::mc`, ported from the 26.3 client
  jar's bytecode and verified bit for bit. Only math/geometry goes there.
- Block and item classes are never ported. Every `Blocks.X`, `instanceof XxxBlock` and
  block-state query maps to a host trait; see `docs/trait-mapping.md`, and add a row there for
  any new check.
- `BlockState` is `crate::host::BlockState`, the host's traits for one state, passed as
  `&BlockState` (what `BlockStateInterface::get0` returns). `state.getBlock() == Blocks.X` and
  `instanceof` become trait reads (`state.fluid`, `state.carpet`); comparing two states
  (`state == other`) compares `id`. Upstream parameters of type `Block` take `&BlockState`.
- `Block.BLOCK_STATE_REGISTRY` is `crate::host::BlockStateTable`; the state id is the index.
- The client world (`ClientLevel`, `ClientChunkCache`, `LevelChunk`, `WorldBorder`,
  `dimensionType()`) is `crate::host::World`, read through an `Arc<World>` snapshot. A
  `BlockStateInterface` owns one snapshot, which replaces `createThreadSafeCopy`.
- The chunk cache (`CachedRegion`, `WorldData`) is not ported. Where upstream falls back to it,
  the port does what upstream does without world data (air, not loaded).
- `Pair<A, B>` is a tuple, nullable values are `Option`.

## Java numeric semantics

- `int`/`long` arithmetic that can overflow uses `wrapping_*`. `Math.abs(int)` is
  `wrapping_abs`. `>>` on signed types matches Java; `>>>` casts to unsigned first.
- `(int)` / `(long)` of a `double` or `float` is `as i32` / `as i64`. Rust saturates and maps
  NaN to 0, which is exactly Java's `d2i`/`d2l`.
- Keep `float` vs `double` exactly: rotations are `f32`, costs `f64`. When Java compares a
  `float` to a `double` literal (`yawDiff < 0.01`), cast the `f32` to `f64` first.
- `Math.min`/`Math.max` on floating point values use `crate::java::{min_f64, max_f64, min_f32,
  max_f32}`: Java propagates NaN and orders `-0.0` below `0.0`, Rust's `f64::min` does neither.
- `Math.pow(a, b)` is `a.powf(b)`, never `powi` (different rounding). `Math.toRadians` is
  `f64::to_radians` (same constant). `Math.floor`/`Math.ceil`/`Math.sqrt` map directly.
- A Java loop that would spin forever on overflow (`for (int x = min; x <= max; x++)` with
  `max == Integer.MAX_VALUE`) becomes an inclusive range; that is the only accepted difference.

## Classes and globals

- Upstream throws `IllegalStateException` / `IllegalArgumentException` for programmer errors
  (NaN rotation, empty `GoalRunAway`, vertical `GoalStrictDirection`): the port panics with the
  same message.
- Settings: `BaritoneAPI.getSettings().x.value` and `Baritone.settings().x.value` are
  `crate::settings::settings().x`. Only settings read by kept code exist, with upstream
  defaults; add new ones with their javadoc and default, and keep the serialized name equal to
  the upstream field name (the settings tests enforce this).
- `ActionCosts` is a runtime struct (`ActionCosts::java()` = upstream values). Static uses of its
  constants read `action_costs()`; `COST_INF` stays a constant.
- Both globals are `ArcSwap` snapshots: cheap to read, replaced atomically.
- Goals: `Goal` trait. Goals that hold other goals use `Arc<dyn Goal>`, since goals are shared
  between the tick thread and calculation threads. `equals` is `Goal::equals` (and
  `PartialEq for dyn Goal`); `instanceof` is a downcast through `Any`, except
  `instanceof IGoalRenderPos`, which is `Goal::as_goal_render_pos()`. `hashCode` is not ported
  for goals because kept code never hashes them.
- `toString` is `Display`, with upstream's format. Floating point output uses Rust formatting,
  not Java's `Float.toString`; it is for logs only.
- Class hierarchies: `Movement` subclasses become an enum, processes are trait objects.
  `Movement` is a struct with the base class's fields and a `MovementKind` enum; each variant
  is the struct of the subclass's file (`MovementTraverse`) with the subclass's fields. The
  subclass's constructor is `MovementX::new(...) -> Movement`, its static `cost` stays on
  `MovementX`, and `Movement` dispatches the overridden methods (`calculate_cost`, ...) on the
  kind. Movements do not hold the `IBaritone`/`IPlayerContext`; execution passes it in.
- Abstract base classes with one subclass keep their own file and struct, and the subclass
  holds it (`AStarPathFinder.search: AbstractNodeCostSearch`). Abstract methods are passed in
  (`AbstractNodeCostSearch::calculate` takes `calculate0`). `PathBase`'s methods are functions
  that every path's `IPath` implementation calls.
- `IPath`: paths are `Box<dyn IPath>`. Methods that return a new path (`postProcess`,
  `cutoffAtLoadedChunks`, `staticCutoff`) take `self: Box<Self>`, since upstream always replaces
  its reference with the result. Paths hand out `&[Movement]` (the only `IMovement`);
  `CutoffPath` copies the part of the previous path it keeps.
- Object graphs become arenas: `PathNode`s live in the search's `Vec`, `previous` is an index,
  and open sets take the arena in each call.
- `catch (Exception e)` around a whole computation (`AbstractNodeCostSearch.calculate`) is
  `catch_unwind`: the panics that stand in for upstream's exceptions become the same result.
- `Helper.logDebug`/`logDirect`/`logNotification` and `System.out.println` go to the `log` crate
  (`crate::api::utils::helper`).
- `System.currentTimeMillis()` used for timeouts is `java::current_time_millis()`, a monotonic
  clock.
- Coordinates in movement cost code use plain `+`/`-` for small offsets (`y + 1`, `x + dx * i`):
  positions come from the world, far from `i32` overflow, and release builds wrap like Java
  anyway. Arithmetic on values that are not world positions keeps `wrapping_*`.
- `CalculationContext` is built from the world snapshot and the host's `Player`; it captures the
  `ActionCosts` (`context.costs`), which movements read instead of `action_costs()`.

## Verification

`tools/refgen/run.sh` compiles the real upstream classes against the real 26.3 client jar. The
classes under test are listed in `SOURCES`; javac compiles whatever else they reference from
upstream's source tree (`-sourcepath`). Only `BaritoneAPI` is stubbed, because the real one reads
the settings file and boots the Baritone provider; `Settings` is the real class. `RefGen`
bootstraps the Minecraft registries, binds the block and item tags from the jar's data pack and
binds the item components (from datagen's vanilla registries), so block and item checks behave
as in game. `Bootstrap` redirects `System.out`/`System.err` into log4j, which has no provider
here: `RefGen.main` prints failures to the original stream. It writes:

- `tests/fixtures/reference/math_goals.json`: math, positions, goals (`RefGen`).
- `tests/fixtures/reference/blocks.json.gz`: the Java-semantics block state table, and the
  results of MovementHelper's block checks, PrecomputedData and BetterWorldBorder for every
  state and in random worlds, under two settings configurations (`BlockRefGen`). Upstream code
  that needs a `BlockStateInterface` gets `BlockRefGen.FakeBsi`, a subclass over a map of
  blocks created without running the client-bound constructor.
- `tests/fixtures/reference/paths.json.gz`: path calculations by the real `AStarPathFinder` in
  generated worlds (terrain in an Overworld and a low Nether shape, block noise, flat) under
  four hand-made settings configurations plus random mixes, and six inventories (one
  enchanted), the raw result of every `Moves` at sampled positions, and `ToolSet` speeds and
  slots for every block (`PathRefGen`). Every calculation records the search's map size, most
  recent node and best path so far, so failed searches are compared too; a few targeted
  queries per world reach `Path`'s fake start node, "movement became impossible", the
  load-boundary cutoff, cancellation (by a goal that cancels on its Nth `isInGoal`), timeouts
  (0, which expires at the first check) and `slowPath` (without the delay). Extra move samples
  sit at ledges above pools and inside ladder and vine columns, with settings changed after the
  context was built and with `WalkOffCalculationContext`'s fields. The one branch no fixture
  reaches is `MovementDescend.dynamicFallCost`'s flowing water check: `canWalkThrough` has
  already rejected flowing water there, upstream too. The client objects the code reaches for are stand-ins
  allocated without a constructor (`ClientLevel`, `LocalPlayer`, an array-backed
  `BlockStateInterface`, an `IBaritone` proxy); the `CalculationContext` is allocated the same
  way and its fields are set with the expressions of upstream's constructor, and `PathRefGen`
  fails if upstream adds a field it does not set. Timeouts are large enough that results do not
  depend on speed; `a_star_path_finder.rs` unit tests cover timeouts and cancellation.

`tests/reference_*.rs` replay the fixtures and compare floats bit for bit.

To cover a newly ported class: add its upstream source to `SOURCES` in `run.sh`, add a section
to `RefGen.java`, `BlockRefGen.java` or `PathRefGen.java`, regenerate, and add a replay test. Upstream code that
cannot run standalone may be copied into `RefGen.java` verbatim (see the `RotationUtils`
region). Regenerate after every upstream sync; the replay tests fail if the fixture's commit
differs from `UPSTREAM`.
