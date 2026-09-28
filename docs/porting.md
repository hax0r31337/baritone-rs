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
- Constructors are `new`; other constructors are `from_pos`, `from_f64`, ... A constructor that
  takes a nullable `Integer` takes `Option<i32>` instead of being split in two.

## Minecraft types

- `BlockPos` is `BetterBlockPos` everywhere.
- `Mth`, `Vec3`, `Direction` live in `crate::mc`, ported from the 26.3 client jar's bytecode
  and verified bit for bit. Only math/geometry goes there.
- Block and item classes are never ported. Every `Blocks.X`, `instanceof XxxBlock` and
  block-state query maps to a host trait; see `docs/trait-mapping.md`, and add a row there for
  any new check.
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

## Verification

`tools/refgen/run.sh` compiles the real upstream classes against the real 26.3 client jar
(stubbing only `BaritoneAPI`, `Settings`, `SettingsUtil`, which would boot the whole client),
runs `RefGen`, and writes `tests/fixtures/reference/math_goals.json`. `tests/reference_*.rs`
replay the fixtures and compare floats bit for bit.

To cover a newly ported pure class: add its upstream source to `SOURCES` in `run.sh`, add a
section to `RefGen.java`, regenerate, and add a replay test. Upstream code that cannot be
compiled standalone may be copied into `RefGen.java` verbatim (see the `RotationUtils` region).
Regenerate after every upstream sync; the replay test fails if the fixture's commit differs from
`UPSTREAM`.
