# Upstream block checks → host traits

This maps every place in the kept upstream code (baritone `25111dae`, MC 26.3) that asks about Minecraft
block, fluid or block-state identity or API to the host trait that replaces it. Use it when porting
upstream diffs. The traits are the fields of `crate::host::BlockState` (`src/host/block_state.rs`,
table version 2); the field list is at the end. Item and player checks map to `crate::host::player`
(Table D). Locations are `File.java:line`; every file name used
here is unique in the kept scope. `MH` = `MovementHelper.java`.

`tools/refgen` exports the whole table from a real 26.3 client (`tests/fixtures/reference/blocks.json.gz`),
and `tests/reference_block_states.rs` checks the Rust side of every row it can reach (the tri-states,
the MovementHelper block checks, PrecomputedData, BetterWorldBorder) against the real upstream code, for
every state and in random worlds. When a new check needs a new field: add it to `BlockState`, export it
in `BlockRefGen.traits`, bump `BlockStateTable::VERSION`, and add a row here. `tests/reference_paths.rs`
covers the movement rows through the real upstream path finder (every `Moves` result and whole paths).
Identity checks that several movements share (`Blocks.SOUL_SAND`, `MAGMA_BLOCK`, `WATER`,
`LADDER || VINE`) are helpers in `src/pathing/movement/movements/mod.rs`.

## Caveats

- **Tri-states depend on settings.** `PrecomputedData` is rebuilt for each `CalculationContext`
  (`CalculationContext.java:95`), and the `*BlockState` functions read settings. The host sends
  `can_walk_on` / `can_walk_through` / `fully_passable` computed with upstream defaults
  (`allowWalkOnMagmaBlocks=false`, `allowVines=false`, `assumeWalkOnLava=false`,
  `allowWalkOnBottomSlab=true`) and `blocksToAvoid=[]`. `movement_helper::*_block_state` apply the
  actual settings on top:
  - `can_walk_on`: `hot_floor && normal_cube && allowWalkOnMagmaBlocks` → YES;
    `climbable ∈ {Vine, NetherVine} && allowVines` → YES; `fluid == Lava && assumeWalkOnLava` and the
    host said NO → MAYBE; `slab == Bottom && fluid == Empty && !allowWalkOnBottomSlab` → NO.
  - `can_walk_through`: `!air && blocksToAvoid ∋ name` → NO. The default `blocksToAvoid` is `[tripwire]`
    (Settings.java:240).
  - `fully_passable`: no settings are involved.
- **No chunk cache.** Upstream's snow checks return true when the chunk is not loaded (MH:198,313),
  for snow read from the cache. The port reads air for unloaded chunks, so these branches are
  unreachable; they are kept for fidelity.
- **Upstream quirks to keep, not fix:**
  - `MovementDiagonal.java:169` tests `cuttingOver1` for magma a second time; it should test `cuttingOver2`.
  - `fully_passable` uses `SkullBlock`, which covers floor skulls only. `can_walk_through` uses
    `AbstractSkullBlock`, which also covers wall skulls.
  - `CauldronBlock` is the empty cauldron only.
  - `ToolSet` caches break speed per `Block` and uses the block's default state.
- **Two differences from the plan's vocabulary:**
  - Upstream `isClimbable` does **not** include scaffolding.
  - Upstream `avoidWalkingInto` does **not** include powder snow. Powder snow only forces
    `can_walk_through = NO`.

## Table A: Block identities (`Blocks.X`)

| Upstream | Proposed trait | Locations | What the check does |
|---|---|---|---|
| `AIR` | `air` + the table's air state (`BlockStateTable::air`) | BlockStateInterface.java:56,101,170-171 | Returned for out-of-range Y, unloaded chunks and empty sections |
| `BAMBOO` | `name` | FarmProcess.java:91,134 | Farm pickup item list; harvest target |
| `BEDROCK` | `name` | MineProcess.java:497 | Bedrock both above and below → `plausibleToBreak` is false |
| `BEETROOTS` | `name` + `properties.age` | FarmProcess.java:120 | Crop harvest target (max age 3) |
| `BIG_DRIPLEAF` | `can_walk_through` (NO) | MH:149 | Forced not walk-through |
| `BLAST_FURNACE` | `name` | GetToBlockProcess.java:246 | Right-click container on arrival |
| `BUBBLE_COLUMN` | `can_walk_through` (NO), `avoid_walking_into`, `can_walk_on` | MH:146,380,410 | Walk-through NO; avoid; excluded from the normal-cube walk-on YES |
| `CACTUS` | `avoid_walking_into`; `name` | MH:375; FarmProcess.java:92,143 | Two uses: damage avoidance, and farm pickup/harvest |
| `CARROTS` | `name` + `properties.age` | FarmProcess.java:118 | Crop harvest target (max age 7) |
| `CHEST` | `chest_like`; `can_walk_on` (YES) | MH:422; GetToBlockProcess.java:246,254 | Two uses: walk-on YES (0.875-high shape), and GetToBlock right-click / clear the block on top |
| `COBWEB` | `can_walk_through` (NO), `fully_passable` (NO), `avoid_walking_into` | MH:146,241,379 | Slows movement: never walk through, never fully passable, avoid |
| `COCOA` | `can_walk_through` (NO), `fully_passable` (NO); `name` + `properties.age` | MH:146,244; FarmProcess.java:124 | Two uses: passability, and farm harvest when age ≥ 2 |
| `CRAFTING_TABLE` | `name` | GetToBlockProcess.java:246 | Right-click container on arrival |
| `DIRT_PATH` | `can_walk_on` (YES) | MH:419 | Walkable although shorter than a full block (`farmland`/dirt-path flag; only used inside the tri-state) |
| `ENDER_CHEST` | `chest_like`; `can_walk_on` (YES) | MH:422; GetToBlockProcess.java:246,254 | Same as `CHEST` |
| `END_PORTAL` | `can_walk_through` (NO), `avoid_walking_into` | MH:146,378 | Never path into an end portal |
| `END_ROD` | `can_walk_through` (NO) | MH:146 | Forced not walk-through |
| `FARMLAND` | `farmland`; `can_walk_on` (YES); `name` | MH:419; MovementParkour.java:146; FarmProcess.java:203,234 | Three uses: walk-on YES, parkour must not land on it (trampling), farm plant target |
| `FURNACE` | `name` | GetToBlockProcess.java:246 | Right-click container on arrival |
| `GLASS` | `can_walk_on` (YES), `can_place_against` | MH:425,586 | Two uses: walk-on YES, can place against |
| `HONEY_BLOCK` | `can_walk_through` (NO), `can_walk_on` (`speed_kind=Honey`) | MH:146,410 | Walk-through NO; excluded from the normal-cube walk-on YES |
| `ICE` | `avoid_breaking` | MH:75 | Breaking ice turns it into water |
| `IRON_DOOR` | `hand_openable = false` | MH:160; MovementTraverse.java:228 | Two uses: walk-through NO (the door cannot be opened), and Traverse does not right-click it |
| `JUNGLE_LOG` | `name` | FarmProcess.java:204,246 | Cocoa planting support |
| `LADDER` | `climbable = Ladder`; `fully_passable` (NO); `can_walk_on` (YES); `facing` | MH:243,416,595; MovementDescend.java:117; MovementDownward.java:66; MovementFall.java:170; MovementParkour.java:272 | Several uses: climbable, never fully passable, always walk-on, Descend/Downward/Parkour ladder cases, Fall reads `FACING` to steer away |
| `LARGE_FERN` | `replaceable` | MH:319 | Forced replaceable (double plant). `canBeReplaced()` is already true in 26.3 (refgen asserts it), so the port drops the check |
| `LAVA` | `liquid_block && fluid == Lava` | MH:857 | `isTransparent` (MineProcess exposed-ore check) |
| `LILY_PAD` | `lily_pad` | MH:456; MovementPillar.java:100; MovementTraverse.java:165 | Water under a lily pad can be walked on; cannot pillar or backplace while standing on one over fluid |
| `MAGMA_BLOCK` | `hot_floor` (new) | MH:374,410; MovementAscend.java:136,194; MovementDescend.java:260; MovementDiagonal.java:146,160,165,169,280; MovementFall.java:99; MovementParkour.java:105,265; MovementTraverse.java:102,224 | Several uses, gated by `allowWalkOnMagmaBlocks`: avoid_walking_into, walk-on, sneak cost, sneak input, no diagonal cut-over, parkour max jump 2 |
| `MELON` | `name` | FarmProcess.java:81,122 | Farm pickup item; harvest target |
| `NETHER_PORTAL` | `name` | GetToBlockProcess.java:239 | `enterPortal`: walk into the portal rather than stop next to it |
| `NETHER_WART` | `name` + `properties.age` | FarmProcess.java:123 | Harvest when age ≥ 3 |
| `POINTED_DRIPSTONE` | `can_walk_through` (NO) | MH:146 | Forced not walk-through |
| `POTATOES` | `name` + `properties.age` | FarmProcess.java:119 | Crop harvest target (max age 7) |
| `POWDER_SNOW` | `can_walk_through` (NO) | MH:152 | Forced not walk-through (not in `avoid_walking_into`) |
| `PUMPKIN` | `name` | FarmProcess.java:85,121 | Farm pickup item; harvest target |
| `SCAFFOLDING` | `climbable = Scaffolding` | MH:551 | Waterlogged scaffolding still counts as solid (`mustBeSolidToWalkOn`) |
| `SOUL_SAND` | `speed_kind = SoulSand`; `can_walk_on` (YES); `name` | MH:419; MovementAscend.java:134; MovementDescend.java:126; MovementDiagonal.java:144,158; MovementParkour.java:107; MovementTraverse.java:93,100,158,289; FarmProcess.java:206,240 | Several uses: slower walk cost, parkour max jump 2, cannot backplace against it (158), back off from the edge when bridging (289, issue #118), walk-on YES, nether wart soil (farm) |
| `STONE` | `name` lookup → `hardness`/tool data | InventoryBehavior.java:72 | Picks the best pickaxe to keep on the hotbar |
| `SUGAR_CANE` | `name` | FarmProcess.java:90,125 | Farm pickup item; harvest target |
| `SWEET_BERRY_BUSH` | `can_walk_through` (NO), `avoid_walking_into` | MH:146,376 | Damages the player |
| `TALL_GRASS` | `replaceable` | MH:319 | Same as `LARGE_FERN` |
| `TRAPPED_CHEST` | `chest_like`; `can_walk_on` (YES) | MH:422; GetToBlockProcess.java:246,254 | Same as `CHEST` |
| `TRIPWIRE` | `fully_passable` (NO); `name` | MH:240; Settings.java:241 | Never fully passable; also in the default `blocksToAvoid` |
| `TWISTING_VINES` | `climbable = NetherVine` | MH:599 | `isClimbable` |
| `TWISTING_VINES_PLANT` | `climbable = NetherVine` | MH:600 | `isClimbable` |
| `VINE` | `climbable = Vine`; `fully_passable` (NO) | MH:242,596; MovementDescend.java:117; MovementDownward.java:66; MovementParkour.java:272 | Climbable; never fully passable; Descend/Downward/Parkour vine cases (Ladder\|Vine only, not nether vines) |
| `WATER` | `liquid_block && fluid == Water` | MH:858; MovementDiagonal.java:151,223,232; MovementTraverse.java:97 | Several uses: `isTransparent`, walk-on-water penalty, pure water is exempt from `avoidWalkingInto` when edging around. A pure water block only, not waterlogged |
| `WEEPING_VINES` | `climbable = NetherVine` | MH:597 | `isClimbable` |
| `WEEPING_VINES_PLANT` | `climbable = NetherVine` | MH:598 | `isClimbable` |
| `WHEAT` | `name` + `properties.age` | FarmProcess.java:117 | Crop harvest target (max age 7) |

## Table B: Class checks (`instanceof`)

| Upstream | Proposed trait | Locations | What the check does |
|---|---|---|---|
| `AbstractSkullBlock` | `can_walk_through` (NO) | MH:146 | Floor and wall skulls are never walk-through |
| `AirBlock` | `air` | MH:143,235,308,856; MovementPillar.java:90,212; MovementTraverse.java:210; MineProcess.java:120,254; FarmProcess.java:233,248,308,365 | Several uses: tri-state early YES, replaceable, `isTransparent`, pillar penalty, farm open-air checks, mining shaft/air exception |
| `AmethystClusterBlock` | `can_walk_through` (NO), `normal_cube` | MH:146,783 | Includes the buds; excluded from normal cube |
| `AzaleaBlock` | `can_walk_through` (NO), `fully_passable` (NO), `can_walk_on` (YES) | MH:146,245,413 | Azalea and flowering azalea |
| `BambooStalkBlock` | `normal_cube`; `name` | MH:778; FarmProcess.java:138 | Two uses: excluded from normal cube (offset shape), and bamboo replant check |
| `BaseFireBlock` | `fire` (new) | MH:146,239,377; RotationUtils.java:248,290; VecUtils.java:61 | Several uses: never walk through / fully passable, avoid; for aiming, hitting the block below counts as hitting the fire, and the aim point is its bottom |
| `BlockItem` | item → block `name` | InventoryBehavior.java:173,176 | Throwaway that matches the builder's desired state; `placeAt` is always null without BuilderProcess, so this is dead code in the port |
| `BonemealableBlock` | `bonemealable` (new) | FarmProcess.java:259-261 | `isValidBonemealTarget` + `isBonemealSuccess`. Vanilla makes these world-dependent, so the host approximates them |
| `CactusBlock` | `name` | FarmProcess.java:147 | Replant check: cactus below |
| `CarpetBlock` | `carpet` (new) | MH:165,194,456; MovementPillar.java:100; MovementTraverse.java:165 | Walk-through MAYBE → needs walkable below; water under a carpet can be walked on; cannot pillar or backplace |
| `CauldronBlock` | `can_walk_through` (NO) | MH:181 | Empty cauldron only |
| `CropBlock` (cast, `isMaxAge`) | `name` + `properties.age` | FarmProcess.java:117-120,156 | Crop is ripe at max age |
| `DoorBlock` | `openable = Door` (+ `open`, `facing`, `hand_openable`) | MH:158,246,331; MovementTraverse.java:226,227 | Walk-through YES unless iron; never fully passable; runtime open/facing check; right-click to open |
| `EndPortalBlock` | `fully_passable` (NO) | MH:251 | |
| `FallingBlock` | `falls` | MH:93,629; MovementAscend.java:96; MovementDescend.java:138; MovementPillar.java:113,118; MineProcess.java:261 | Several uses: avoid breaking next to an unsupported falling block, add the cost of the falling stack, block the move if something would fall on the player, mining goal shape |
| `FenceGateBlock` | `openable = FenceGate` (+ `open`) | MH:158,247,344; MovementPillar.java:73; MovementTraverse.java:236 | Walk-through YES; never fully passable; cannot pillar into one (#172); right-click to open |
| `FlowingFluid` (fluid type) | `fluid != Empty` | MH:758,764 | `possiblyFlowing` / `isFlowing` guard |
| `InfestedBlock` | `avoid_breaking` | MH:76 | Silverfish |
| `LeavesBlock` | `leaves` (new) | MH:553 | Waterlogged leaves still count as solid |
| `LilyPadBlock` | `lily_pad` | MH:224 | Water under a lily pad is not walk-through |
| `LiquidBlock` | `liquid_block` (new) | MH:100,109,560 | Liquid next to the block → avoid breaking it; liquid above → frost-walker ground is not solid |
| `MovingPistonBlock` | `normal_cube` | MH:779 | Excluded |
| `PointedDripstoneBlock` | `normal_cube` | MH:782 | Excluded |
| `ScaffoldingBlock` | `normal_cube` (`climbable = Scaffolding`) | MH:780 | Excluded |
| `ShulkerBoxBlock` | `can_walk_through` (NO), `fully_passable` (NO), `normal_cube` | MH:146,253,781 | The box can open |
| `SkullBlock` | `fully_passable` (NO) | MH:252 | Floor skulls only (see Caveats) |
| `SlabBlock` | `slab` | MH:146,437,535,639; MovementPillar.java:67; MovementTraverse.java:158,289 | Several uses: walk-through NO, walk-on depends on type, `isBottomSlab`, cannot backplace against a half slab, bridging edge case |
| `SnowLayerBlock` | `snow_layers` | MH:168,198,248,312 | Walk-through MAYBE (at 3+ layers it blocks); never fully passable; replaceable only with 1 layer |
| `StainedGlassBlock` | `can_walk_on` (YES), `can_place_against` | MH:425,586 | Same as `GLASS` |
| `StairBlock` | `stairs` | MH:428,539; MovementParkour.java:94 | Walk-on YES; solid when waterlogged if top or inner corner; no parkour from stairs |
| `SugarCaneBlock` | `name` | FarmProcess.java:129 | Replant check: cane below |
| `TrapDoorBlock` | `openable = TrapDoor` (+ `open`, `half`) | MH:146,250,547 | Walk-through NO; never fully passable; a closed top trapdoor is solid when waterlogged |
| `WaterFluid` (fluid type) | `fluid == Water` | MH:227; MovementFall.java:103; MovementParkour.java:81 | Several uses: still water is walk-through, landing in water in Fall, a water-exempt avoid check |

Skipped as non-block: `Mob`/`Spider`/`ZombifiedPiglin`/`Enderman` (Avoidance.java:76-79), `ItemEntity`,
`BlockHitResult`, packets, `ClientLevel`, Movement*/PathNode/goal classes.

## Table C: State properties and other block-state API

| Upstream | Proposed trait | Locations | What the check does |
|---|---|---|---|
| `LiquidBlock.LEVEL` | `liquid_block && fluid_source` (level 0 ⇔ source) | MH:104,502,522 | Source liquid next to the block → avoid breaking it; frost walker needs source water |
| `FrostedIceBlock.meltsInto()` identity | `liquid_block && fluid == Water && fluid_source` | MH:501,521 | Frost walker target = the default water state |
| `getFluidState().isEmpty()` | `fluid == Empty` | MH:111,174,214,224,249,373,533,610,753; MovementParkour.java:98,101; MovementPillar.java:100; MovementTraverse.java:165; CalculationContext.java:197; PathExecutor.java:323 | "Has any fluid", waterlogged included |
| `getFluidState().getType() == Fluids.*` | `fluid` + `fluid_source` | MH:459 (`FLOWING_WATER`),720-721,737-738 | `isWater` / `isLava` (still or flowing); flowing water above |
| `FluidType.getAmount(fs) != 8` | `fluid_amount` | MH:175,759,767 | Not full (8 means source or falling) → flowing |
| `FluidState.isSource()` | `fluid_source` | CalculationContext.java:194,197 | `allowPlaceInFluidsSource` / `allowPlaceInFluidsFlow` |
| `SnowLayerBlock.LAYERS` | `snow_layers` | MH:206,317 | 3+ layers → not walk-through; exactly 1 → replaceable |
| `SlabBlock.TYPE` | `slab: Bottom\|Top\|Double` | MH:439,536,640; MovementPillar.java:67; MovementTraverse.java:158 | Bottom-slab rules; not `DOUBLE` → cannot backplace |
| `StairBlock.HALF`, `StairBlock.SHAPE` | `stairs.half`, `stairs.inner_corner` (new) | MH:540,543 | Waterlogged stairs are solid if top half or `INNER_LEFT`/`INNER_RIGHT` |
| `TrapDoorBlock.OPEN`, `TrapDoorBlock.HALF` | `open`, `half` | MH:548 | Closed top trapdoor → solid |
| `DoorBlock.OPEN`, `FenceGateBlock.OPEN` | `open` | MH:335,348,357 | Runtime door/gate passability |
| `HorizontalDirectionalBlock.FACING` | `facing` (axis) | MH:356 | Door passability: facing axis vs. the player's approach axis |
| `LadderBlock.FACING` | `facing` | MovementFall.java:171 | Fall steers away from the ladder's wall |
| `NetherWartBlock.AGE`, `CocoaBlock.AGE`, `CropBlock::isMaxAge` | `properties.age` + per-`name` max age | FarmProcess.java:123,124,156 | Ripeness |
| `FallingBlock.isFree(state)` | `air \|\| fire \|\| liquid \|\| replaceable` (`movement_helper::falling_block_is_free`; vanilla: `isAir() \|\| is(BlockTags.FIRE) \|\| liquid() \|\| canBeReplaced()`) | MH:95 | Falling block below is unsupported. `liquid()` also covers bubble columns |
| `isPathfindable(PathComputationType.LAND)` | `pathfindable_land` (new) | MH:184,230,258,286,293 | Fallback for walk-through and fully passable. 184/258 are inside the tri-states; 230/286/293 run at runtime |
| `canBeReplaced()` | `replaceable` | MH:322; MovementPillar.java:212 | Can place into it; pillar decides whether it must break the block first |
| `Block.isShapeFullBlock(getCollisionShape(null,null))` | `normal_cube` (new) | MH:787 (exclusions at 778-783) | `isBlockNormalCube`: a full collision cube with no world context. Position-dependent shapes throw, which counts as false |
| `getCollisionShape(world,pos)` | `collision_shape` | VecUtils.java:51 | Aim point = center of the collision bounds. Empty shape → block center |
| `getShape(world,pos)` (outline), `Shapes.block()` | `outline_shape` | RotationUtils.java:211,213 | Side-offset aim points. Empty shape → unit cube |
| `level.clip(ClipContext.Block.OUTLINE, ClipContext.Fluid.NONE)` | `outline_shape` | RayTraceUtils.java:62 | Raytrace against outline shapes, ignoring fluids |
| `getDestroySpeed(null,null)` | `hardness` | ToolSet.java:210 | < 0 or throws → unbreakable |
| `ItemStack.getDestroySpeed(state)` | the item's `tool` rules matched against `tags` / `name` | ToolSet.java:219 | Tool speed multiplier: the first rule with a speed whose block set holds the state, else the default speed; 1 without a tool |
| `requiresCorrectToolForDrops()`, `ItemStack.isCorrectToolForDrops(state)` | `requires_tool`; the item's `tool` rules | ToolSet.java:234 | Divide by 30 (correct tool) or 100 |
| break data per `Block` from its default state (`getBlock()`, `defaultBlockState()`) | `default_state` (from `default`) | ToolSet.java:97,153,192; InventoryBehavior.java:152 | Cache key is the block, not the state |
| `Block.BLOCK_STATE_REGISTRY` size / `getId` | host state id, table length | PrecomputedData.java:27,73,88,103 | Tri-state cache index |
| settings `List<Block>` membership | `name` | MH:74 (`blocksToDisallowBreaking`),155 (`blocksToAvoid`); ToolSet.java:196 (`blocksToAvoidBreaking`); CalculationContext.java:204, MineProcess.java:529 (`allowBreakAnyway`) | Setting lists hold block names |
| `harvest.block == state.getBlock()`, scan block lists | `name` | FarmProcess.java:172,198-207 | Farm target matching / chunk scan |
| `BlockUtils.stringToBlockRequired` | `name` registry | BlockOptionalMeta.java:98 | Parse the selector id |
| `getStateDefinition().getProperty`, `Property.getValue(String)` | `properties` (keys/values per name) | BlockOptionalMeta.java:123,124 | Parse and validate `[k=v]` |
| `getPossibleStates()`, `state.getValue(prop)` | all state ids with that `name` + `properties` | BlockOptionalMeta.java:86,104,135,137 | Selector → set of states |
| state identity (`hashCode`, `==`) | state id | BlockOptionalMeta.java:145,168,173; BlockOptionalMetaLookup.java:72-78 | `matches` / `has(state)` |
| loot table drops (`getLootTable`, default state, netherite pickaxe) | `drops` (new) | BlockOptionalMeta.java:154,223-256 | Item stacks that count as "mined" (MineProcess.java:78,350) |
| `ctx.world().dimension() == Level.NETHER` (not block data) | world `water_evaporates` (new, world-level) | CalculationContext.java:104; MovementFall.java:105 | No water-bucket falls in the Nether |
| `getEntitiesOfClass(FallingBlockEntity)` (not block data) | entity list | Movement.java:159 | Pause mining while falling blocks are in the air |

## Table D: Items and the player

The player and its items are `crate::host::player` (`Player`, `Inventory`, `ItemStack`, `Tool`). Items
carry what upstream reads from their components and tags; the host evaluates enchantments, which are
data driven. `tools/refgen` exports real 26.3 items (`tests/fixtures/reference/paths.json.gz`), and
`tests/reference_paths.rs` checks ToolSet and the calculation context against upstream.

| Upstream | Port | Locations | What the check does |
|---|---|---|---|
| `DataComponents.TOOL` (`Tool`, `Tool.Rule`) | `ItemStack::tool` (`Tool`, `ToolRule`); `HolderSet<Block>` is `BlockSet::Tag` (matched against `BlockState::tags`) or `BlockSet::Blocks` (names) | ToolSet.java:219,234 | Mining speed and correct tool |
| `itemStack.is(ItemTags.SWORDS)`, `is(ItemTags.*_TOOL_MATERIALS)` | `ItemStack::tags` | ToolSet.java:95,145 | Skip swords; material cost. The material tags hold repair materials, so every tool gets -1 (kept) |
| `getDamageValue()`, `getMaxDamage()` | `damage`, `max_damage` | ToolSet.java:149 | `itemSaver` |
| enchantment `MINING_EFFICIENCY` attribute effect | `ItemStack::mining_efficiency` (host evaluates the first such enchantment at its level) | ToolSet.java:222-231 | Added to speeds above 1 |
| `Enchantments.SILK_TOUCH` level > 0 | `ItemStack::silk_touch` | ToolSet.java:107-117 | Tie break |
| `hasEffect(MobEffects.HASTE / MINING_FATIGUE)`, `getAmplifier()` | `Player::effects` | ToolSet.java:252-266 | Break speed amplifier |
| `getInventory().getItem(i)`, `getSelectedSlot()`, `findSlotMatchingItem`, `isHotbarSlot` | `Inventory` | ToolSet.java:132,143; CalculationContext.java:104 | Hotbar tools; water bucket on the hotbar (matched by item id only) |
| `getFoodData().getFoodLevel()` | `Player::food_level` | CalculationContext.java:105 | Sprinting needs food > 6 |
| equipment `FROST_WALKER` level | `Player::frost_walker` (last found, like upstream) | CalculationContext.java:116-127 | Frost walker level |
| equipment `WATER_MOVEMENT_EFFICIENCY` attribute effect | `Player::water_movement_efficiency`; `None` is upstream's default multiplier 1, so water walking costs the same as land walking without Depth Strider (kept) | CalculationContext.java:134-148 | `waterWalkSpeed` |
| `InventoryBehavior.hasGenericThrowaway()` | a `CalculationContext::new` argument until InventoryBehavior is ported (phase 4) | CalculationContext.java:103 | `hasThrowaway` |

## Trait fields

`crate::host::BlockState`, table version 2. Serialized as JSON with these names; enum values are
snake_case (`"nether_vine"`), `Option` fields are absent or `null` when unset, and every field except
`name` and the three tri-states defaults to false / zero / empty. "(new)" marks fields the plan's
trait table does not list.

| Field | Type | Meaning |
|---|---|---|
| `name`, `properties` | `String`, `BTreeMap<String,String>` | Block id and state properties. Used by BlockOptionalMeta, Farm/Mine/GetToBlock targets and the settings block lists |
| `default` (new, v2) | `bool` | The block's default state. The table derives `default_state` (the id of each state's block's default state; the first state of the block when none is flagged), which stands for the `Block` |
| `tags` (new, v2) | `[String]`, sorted | Block tags the state is in. At least the tags item tool rules refer to (`mineable/*`, `incorrect_for_*_tool`, `sword_efficient`, `leaves`, ...) |
| `air` | `bool` | Java `AirBlock` (air, cave air, void air) |
| `can_walk_on`, `can_walk_through`, `fully_passable` | `Ternary` | Precomputed tri-states. MAYBE → position check in Rust. Setting overrides are listed under Caveats |
| `fluid` | `Empty\|Water\|Lava` | Fluid in this state, waterlogged and bubble column included |
| `fluid_source` | `bool` | `FluidState.isSource()` (the plan's "flowing" is `!fluid_source`) |
| `fluid_amount` | `u8` | `FluidState.getAmount()`: 8 for source or falling, 1-7 for flowing (the plan's "level") |
| `liquid_block` (new) | `bool` | The block itself is a pure liquid (Java `LiquidBlock`: water/lava), not waterlogged |
| `liquid` (new) | `bool` | `BlockBehaviour.liquid()`: `liquid_block` plus bubble columns. Only `FallingBlock.isFree` reads it |
| `climbable` | `Option<Ladder\|Vine\|NetherVine\|Scaffolding>` | `isClimbable` = Ladder\|Vine\|NetherVine. The `NetherVine` variant (weeping/twisting, both parts) is new |
| `falls` | `bool` | Java `FallingBlock` (gravity) |
| `avoid_walking_into` | `bool` | Cactus, sweet berry bush, fire, end portal, cobweb, bubble column. Rust ORs in `fluid != Empty` and `hot_floor && !allowWalkOnMagmaBlocks` |
| `hot_floor` (new) | `bool` | Hurts entities that stand on it unless they sneak (magma) |
| `fire` (new) | `bool` | Java `BaseFireBlock` (fire, soul fire) |
| `avoid_breaking` | `bool` | Ice (turns into water), infested blocks |
| `replaceable` | `bool` | `canBeReplaced()` |
| `can_place_against` | `bool` | `normal_cube \|\| glass \|\| stained glass`. Rust adds the world-border check |
| `normal_cube` (new) | `bool` | `isBlockNormalCube`: full collision cube with no world context, excluding bamboo, moving piston, scaffolding, shulker box, pointed dripstone and amethyst clusters |
| `pathfindable_land` (new) | `bool` | `isPathfindable(LAND)` |
| `slab` | `Option<Top\|Bottom\|Double>` | Slab type |
| `stairs` | `Option<{half: Top\|Bottom, inner_corner: bool}>` | Stairs. `inner_corner` is new (`SHAPE` is `INNER_LEFT`/`INNER_RIGHT`) |
| `openable` | `Option<Door\|FenceGate\|TrapDoor>` | Door, gate or trapdoor |
| `open`, `half` | `bool`, `Top\|Bottom` | Openable state. A door's lower half is `Bottom` |
| `facing` | `Option<Direction>` | `HORIZONTAL_FACING` of every state that has it: doors (MH:356) and `LadderBlock.FACING` (new use) among others |
| `hand_openable` (new) | `bool` | Can be opened with a right-click (false for iron doors and iron trapdoors) |
| `snow_layers` | `u8` | 0 = not a snow layer, otherwise 1-8 |
| `farmland` | `bool` | Farmland (dirt path only feeds `can_walk_on`) |
| `speed_kind` | `Option<SoulSand\|Honey\|Cobweb>` | Only `SoulSand` is read outside the tri-states |
| `lily_pad` | `bool` | Lily pad |
| `carpet` (new) | `bool` | Java `CarpetBlock` |
| `leaves` (new) | `bool` | Java `LeavesBlock` |
| `chest_like` | `bool` | Chest, trapped chest, ender chest |
| `hardness` | `f32` | `getDestroySpeed(null, null)`; < 0 means unbreakable (also when it throws) |
| `requires_tool` | `bool` | `requiresCorrectToolForDrops()` |
| `collision_shape`, `outline_shape` | `Vec<Aabb>` | Aim points, raytrace, position checks. Computed at `BlockPos.ZERO` with no world, so position-offset shapes (flowers, bamboo) are at their zero offset |

Version 2 replaced the planned `harvest_tools` / `required_tier` with `tags` and the items' own tool
rules, which reproduce `Tool.getMiningSpeed` exactly (swords, shears and tier denial rules included).

Not in version 2, added with the code that reads them:

| Field | Type | Meaning | Phase |
|---|---|---|---|
| `drops` (new) | `[item name]` | Default-state loot with a netherite pickaxe (the BlockOptionalMeta stack match). Needs loot tables | 5 |
| `bonemealable` (new) | `bool` | Approximates `isValidBonemealTarget && isBonemealSuccess` | 5 |

Not needed: `bubble_column` and `end_portal` from the plan. Upstream only reads them inside the tri-states
and `avoid_walking_into`, so they stay host-internal.
