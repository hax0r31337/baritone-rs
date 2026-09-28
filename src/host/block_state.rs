//! The block state table (the "trait table"): what the port knows about each block state.
//!
//! Upstream asks Minecraft's `BlockState` and `Block` classes (`instanceof`, `Blocks.X`, state
//! properties); the port reads these traits instead. The host sends the table once per session
//! and whenever its registry changes. `docs/trait-mapping.md` maps every upstream check to the
//! field that replaces it, and says which setting overrides Rust applies on top.
//!
//! A table with Java semantics, exported from a real 26.3 client, is in
//! `tests/fixtures/reference/blocks.json.gz` (`tools/refgen`).

use std::collections::BTreeMap;
use std::collections::hash_map::Entry;
use std::fmt;

use rustc_hash::FxHashMap;

use serde::{Deserialize, Serialize};

use crate::api::utils::BetterBlockPos;
use crate::mc::{Aabb, Direction, Vec3, VoxelShape, mth};
use crate::pathing::precompute::Ternary;

/// The fluid in a block state, waterlogging and bubble columns included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fluid {
    #[default]
    Empty,
    /// Still or flowing water (`Fluids.WATER` / `Fluids.FLOWING_WATER`).
    Water,
    /// Still or flowing lava (`Fluids.LAVA` / `Fluids.FLOWING_LAVA`).
    Lava,
}

/// Blocks a player can climb, and scaffolding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Climbable {
    Ladder,
    Vine,
    /// Weeping and twisting vines, both the tip and the plant.
    NetherVine,
    /// Not climbable for upstream's `isClimbable`.
    Scaffolding,
}

/// `net.minecraft.world.level.block.state.properties.SlabType`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlabType {
    Top,
    Bottom,
    Double,
}

/// `net.minecraft.world.level.block.state.properties.Half`. Doors map their lower half to
/// `Bottom`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Half {
    Top,
    #[default]
    Bottom,
}

/// Stairs shape, as far as upstream reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stairs {
    pub half: Half,
    /// `SHAPE` is `INNER_LEFT` or `INNER_RIGHT`.
    pub inner_corner: bool,
}

/// Blocks that open and close.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Openable {
    Door,
    FenceGate,
    TrapDoor,
}

/// `BlockBehaviour.OffsetType`, without `NONE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetType {
    Xz,
    Xyz,
}

/// A block whose shapes move with its position (flowers, bamboo, pointed dripstone):
/// `BlockBehaviour.Properties.offsetType` with the block's `getMaxHorizontalOffset()` and
/// `getMaxVerticalOffset()`, and which of its shapes the block moves by it
/// (`shape.move(state.getOffset(pos))`). Some blocks are only drawn offset (short grass).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockOffset {
    #[serde(rename = "type")]
    pub offset_type: OffsetType,
    pub max_horizontal: f32,
    pub max_vertical: f32,
    /// The outline shape moves.
    #[serde(default)]
    pub outline: bool,
    /// The collision shape moves.
    #[serde(default)]
    pub collision: bool,
}

impl BlockOffset {
    /// The offset function `offsetType` installs, evaluated at `pos`.
    pub fn evaluate(&self, pos: BetterBlockPos) -> Vec3 {
        let seed = mth::get_seed(pos.x, 0, pos.z);
        let max_horizontal_offset = self.max_horizontal as f64;
        let x = mth::clamp(
            (((seed & 15) as f32 / 15.0f32) as f64 - 0.5) * 0.5,
            -max_horizontal_offset,
            max_horizontal_offset,
        );
        let z = mth::clamp(
            (((seed >> 8 & 15) as f32 / 15.0f32) as f64 - 0.5) * 0.5,
            -max_horizontal_offset,
            max_horizontal_offset,
        );
        match self.offset_type {
            OffsetType::Xz => Vec3::new(x, 0.0, z),
            OffsetType::Xyz => {
                let y =
                    (((seed >> 4 & 15) as f32 / 15.0f32) as f64 - 1.0) * self.max_vertical as f64;
                Vec3::new(x, y, z)
            }
        }
    }
}

/// Blocks that change the player's speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedKind {
    SoulSand,
    Honey,
    Cobweb,
}

/// One block state: stands in for Minecraft's `BlockState` in ported code.
///
/// Fields without a default must be sent. The three tri-states are computed by the host the
/// way upstream's `MovementHelper.*BlockState` would, with upstream's default settings except
/// that `blocksToAvoid` is empty; `MovementHelper` applies the actual settings on top.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockState {
    /// Index in the table (the host state id). Set by [`BlockStateTable::new`].
    #[serde(skip)]
    pub id: u32,
    /// Block id (`"minecraft:oak_stairs"`). Setting block lists hold these.
    pub name: String,
    /// State properties by name (`"facing" => "north"`).
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    /// This is its block's default state (`Block.defaultBlockState()`). A block without one
    /// uses its first state.
    #[serde(default, rename = "default")]
    pub is_default: bool,
    /// Id of its block's default state. Set by [`BlockStateTable::new`]. Two states belong to
    /// the same block (upstream's `Block`) exactly when their `default_state` is equal.
    #[serde(skip)]
    pub default_state: u32,
    /// Block tags the state is in (`"minecraft:mineable/pickaxe"`). The host must send at least
    /// the tags that item tool rules refer to ([`crate::host::BlockSet::Tag`]); sorted.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Air, cave air, void air (Java `AirBlock`).
    #[serde(default)]
    pub air: bool,
    /// `MovementHelper.canWalkOnBlockState`
    pub can_walk_on: Ternary,
    /// `MovementHelper.canWalkThroughBlockState`
    pub can_walk_through: Ternary,
    /// `MovementHelper.fullyPassableBlockState`
    pub fully_passable: Ternary,

    #[serde(default)]
    pub fluid: Fluid,
    /// `FluidState.isSource()`
    #[serde(default)]
    pub fluid_source: bool,
    /// `FluidState.getAmount()`: 8 for a source or falling fluid, 1-7 for flowing, 0 for none.
    #[serde(default)]
    pub fluid_amount: u8,
    /// The block itself is a liquid (Java `LiquidBlock`: water, lava), not waterlogged.
    #[serde(default)]
    pub liquid_block: bool,
    /// `BlockBehaviour.liquid()`: like `liquid_block`, but also bubble columns.
    #[serde(default)]
    pub liquid: bool,

    #[serde(default)]
    pub climbable: Option<Climbable>,
    /// Affected by gravity (Java `FallingBlock`).
    #[serde(default)]
    pub falls: bool,
    /// Hurts or traps whoever walks into it: cactus, sweet berry bush, fire, end portal, cobweb,
    /// bubble column. Fluids and `hot_floor` are added by `MovementHelper.avoidWalkingInto`.
    #[serde(default)]
    pub avoid_walking_into: bool,
    /// Hurts whoever stands on it without sneaking (magma).
    #[serde(default)]
    pub hot_floor: bool,
    /// Java `BaseFireBlock` (fire, soul fire).
    #[serde(default)]
    pub fire: bool,
    /// Breaking it causes trouble: ice (turns into water), infested blocks.
    #[serde(default)]
    pub avoid_breaking: bool,
    /// `canBeReplaced()`
    #[serde(default)]
    pub replaceable: bool,
    /// A side face can be clicked to place against: `normal_cube`, glass, stained glass.
    #[serde(default)]
    pub can_place_against: bool,
    /// `MovementHelper.isBlockNormalCube`: a full collision cube, excluding bamboo, moving
    /// pistons, scaffolding, shulker boxes, pointed dripstone and amethyst clusters.
    #[serde(default)]
    pub normal_cube: bool,
    /// `isPathfindable(PathComputationType.LAND)`
    #[serde(default)]
    pub pathfindable_land: bool,

    #[serde(default)]
    pub slab: Option<SlabType>,
    #[serde(default)]
    pub stairs: Option<Stairs>,
    #[serde(default)]
    pub openable: Option<Openable>,
    /// Open state of an `openable`.
    #[serde(default)]
    pub open: bool,
    /// Half of an `openable` (trapdoor top/bottom, door upper/lower).
    #[serde(default)]
    pub half: Half,
    /// `HORIZONTAL_FACING` of any block that has it (doors, gates, trapdoors, ladders, ...).
    #[serde(default)]
    pub facing: Option<Direction>,
    /// An `openable` that opens with a right click (false for iron doors and trapdoors).
    #[serde(default)]
    pub hand_openable: bool,
    /// Snow layer count, 0 when this is not a snow layer.
    #[serde(default)]
    pub snow_layers: u8,
    #[serde(default)]
    pub farmland: bool,
    #[serde(default)]
    pub speed_kind: Option<SpeedKind>,
    #[serde(default)]
    pub lily_pad: bool,
    /// Java `CarpetBlock`.
    #[serde(default)]
    pub carpet: bool,
    /// Java `LeavesBlock`.
    #[serde(default)]
    pub leaves: bool,
    /// Chest, trapped chest, ender chest.
    #[serde(default)]
    pub chest_like: bool,
    /// Java `BonemealableBlock` whose `isValidBonemealTarget` and `isBonemealSuccess` both
    /// hold for this state alone in an otherwise empty world, with the luckiest roll: bone
    /// meal would grow it. Upstream asks the world; the host evaluates the state on its own.
    /// Farm does not read it for bamboo, whose check it ports (the stalk's height and stage).
    #[serde(default)]
    pub bonemealable: bool,

    /// Item ids the block drops: its loot table rolled for the block's default state with a
    /// netherite pickaxe and loot seed 1 (`BlockOptionalMeta.drops`). Read from the block's
    /// default state.
    #[serde(default)]
    pub drops: Vec<String>,

    /// `getDestroySpeed`; negative means unbreakable.
    #[serde(default)]
    pub hardness: f32,
    /// `requiresCorrectToolForDrops()`
    #[serde(default)]
    pub requires_tool: bool,

    /// Collision boxes, relative to the block's origin.
    #[serde(default)]
    pub collision_shape: Vec<Aabb>,
    /// Outline (selection) boxes, relative to the block's origin.
    #[serde(default)]
    pub outline_shape: Vec<Aabb>,
    /// `getInteractionShape`: boxes that only change the face a raytrace reports (hopper,
    /// cauldron, composter, scaffolding).
    #[serde(default)]
    pub interaction_shape: Vec<Aabb>,
    /// The block's offset (`getOffset`), and which of the shapes above move with it; those
    /// are stored unmoved.
    #[serde(default)]
    pub offset: Option<BlockOffset>,
}

impl Default for BlockState {
    /// An unnamed state with every flag off and all tri-states `No`.
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            properties: BTreeMap::new(),
            is_default: false,
            default_state: 0,
            tags: Vec::new(),
            air: false,
            can_walk_on: Ternary::No,
            can_walk_through: Ternary::No,
            fully_passable: Ternary::No,
            fluid: Fluid::Empty,
            fluid_source: false,
            fluid_amount: 0,
            liquid_block: false,
            liquid: false,
            climbable: None,
            falls: false,
            avoid_walking_into: false,
            hot_floor: false,
            fire: false,
            avoid_breaking: false,
            replaceable: false,
            can_place_against: false,
            normal_cube: false,
            pathfindable_land: false,
            slab: None,
            stairs: None,
            openable: None,
            open: false,
            half: Half::Bottom,
            facing: None,
            hand_openable: false,
            snow_layers: 0,
            farmland: false,
            speed_kind: None,
            lily_pad: false,
            carpet: false,
            leaves: false,
            chest_like: false,
            bonemealable: false,
            drops: Vec::new(),
            hardness: 0.0,
            requires_tool: false,
            collision_shape: Vec::new(),
            outline_shape: Vec::new(),
            interaction_shape: Vec::new(),
            offset: None,
        }
    }
}

impl BlockState {
    /// `getOffset(BlockPos)`: zero for blocks without an offset.
    pub fn get_offset(&self, pos: BetterBlockPos) -> Vec3 {
        match &self.offset {
            Some(offset) => offset.evaluate(pos),
            None => Vec3::ZERO,
        }
    }

    /// `getShape(BlockGetter, BlockPos)`: the outline shape at `pos`.
    pub fn get_shape(&self, pos: BetterBlockPos) -> VoxelShape<'_> {
        let offset = self
            .offset
            .filter(|offset| offset.outline)
            .map(|offset| offset.evaluate(pos));
        VoxelShape::new(&self.outline_shape, offset)
    }

    /// `getCollisionShape(BlockGetter, BlockPos)`
    pub fn get_collision_shape(&self, pos: BetterBlockPos) -> VoxelShape<'_> {
        let offset = self
            .offset
            .filter(|offset| offset.collision)
            .map(|offset| offset.evaluate(pos));
        VoxelShape::new(&self.collision_shape, offset)
    }

    /// `getInteractionShape(BlockGetter, BlockPos)`: never moved.
    pub fn get_interaction_shape(&self, _pos: BetterBlockPos) -> VoxelShape<'_> {
        VoxelShape::new(&self.interaction_shape, None)
    }
}

/// Why a block state table was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableError {
    /// The host speaks another table version.
    Version { expected: u32, actual: u32 },
    /// The table has more states than a `u32` id can address.
    TooLarge,
    /// The designated air state does not exist or is not air.
    Air(u32),
    /// A state has no name.
    Unnamed(u32),
    /// A second default state for the same block.
    DefaultState(u32),
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TableError::Version { expected, actual } => {
                write!(f, "block state table version {actual}, expected {expected}")
            }
            TableError::TooLarge => f.write_str("block state table has too many states"),
            TableError::Air(id) => write!(f, "air state {id} is missing or not air"),
            TableError::Unnamed(id) => write!(f, "block state {id} has no name"),
            TableError::DefaultState(id) => {
                write!(f, "block state {id} is a second default state of its block")
            }
        }
    }
}

impl std::error::Error for TableError {}

/// All block states, indexed by host state id. Replaces `Block.BLOCK_STATE_REGISTRY`.
///
/// Serialized as `{"version": 4, "air": <id>, "states": [...]}`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "TableData")]
pub struct BlockStateTable {
    states: Box<[BlockState]>,
    air: u32,
    /// The ids of each block's states, in id order, by block name.
    blocks: FxHashMap<String, Vec<u32>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TableData {
    version: u32,
    air: u32,
    states: Vec<BlockState>,
}

impl TryFrom<TableData> for BlockStateTable {
    type Error = TableError;

    fn try_from(data: TableData) -> Result<Self, TableError> {
        if data.version != Self::VERSION {
            return Err(TableError::Version {
                expected: Self::VERSION,
                actual: data.version,
            });
        }
        Self::new(data.states, data.air)
    }
}

impl BlockStateTable {
    /// Bumped whenever a field is added or changes meaning.
    pub const VERSION: u32 = 4;

    /// Builds the table; `states[i]` gets id `i`. `air` is the state returned for unloaded
    /// chunks, empty sections and positions outside the world's height (`Blocks.AIR`).
    pub fn new(mut states: Vec<BlockState>, air: u32) -> Result<Self, TableError> {
        if u32::try_from(states.len()).is_err() {
            return Err(TableError::TooLarge);
        }
        // default state per block name: the flagged one, else the first
        let mut defaults: FxHashMap<&str, (u32, bool)> = FxHashMap::default();
        for (id, state) in states.iter().enumerate() {
            let id = id as u32;
            if state.name.is_empty() {
                return Err(TableError::Unnamed(id));
            }
            match defaults.entry(&state.name) {
                Entry::Vacant(e) => {
                    e.insert((id, state.is_default));
                }
                Entry::Occupied(mut e) => {
                    if state.is_default {
                        if e.get().1 {
                            return Err(TableError::DefaultState(id));
                        }
                        e.insert((id, true));
                    }
                }
            }
        }
        let default_states: Vec<u32> = states.iter().map(|s| defaults[s.name.as_str()].0).collect();
        for (id, (state, default_state)) in states.iter_mut().zip(default_states).enumerate() {
            state.id = id as u32;
            state.default_state = default_state;
        }
        if !states.get(air as usize).is_some_and(|s| s.air) {
            return Err(TableError::Air(air));
        }
        let mut blocks: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        for state in &states {
            blocks.entry(state.name.clone()).or_default().push(state.id);
        }
        Ok(Self {
            states: states.into_boxed_slice(),
            air,
            blocks,
        })
    }

    /// The state with this id. Panics if there is none; ids from a [`crate::host::World`] are
    /// always valid.
    #[inline]
    pub fn get(&self, id: u32) -> &BlockState {
        &self.states[id as usize]
    }

    /// The state with this id, if any.
    pub fn try_get(&self, id: u32) -> Option<&BlockState> {
        self.states.get(id as usize)
    }

    /// `Blocks.AIR.defaultBlockState()`
    #[inline]
    pub fn air(&self) -> &BlockState {
        self.get(self.air)
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &BlockState> {
        self.states.iter()
    }

    /// `Blocks.X.defaultBlockState()` for the block named `name`, if the table has it.
    pub fn get_default_state(&self, name: &str) -> Option<&BlockState> {
        self.blocks
            .get(name)
            .map(|ids| self.get(self.get(ids[0]).default_state))
    }

    /// `block.getStateDefinition().getPossibleStates()` for the block named `name`, in id
    /// order; empty if the table has no such block.
    pub fn get_possible_states(&self, name: &str) -> impl Iterator<Item = &BlockState> {
        self.blocks
            .get(name)
            .into_iter()
            .flatten()
            .map(|&id| self.get(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str) -> BlockState {
        BlockState {
            name: name.to_owned(),
            ..BlockState::default()
        }
    }

    #[test]
    fn ids_are_indices() {
        let air = BlockState {
            air: true,
            ..state("minecraft:air")
        };
        let table = BlockStateTable::new(vec![state("minecraft:stone"), air], 1).unwrap();
        assert_eq!(table.len(), 2);
        assert_eq!(table.get(0).id, 0);
        assert_eq!(table.air().id, 1);
        assert_eq!(table.air().name, "minecraft:air");
        assert!(table.try_get(2).is_none());
    }

    #[test]
    fn rejects_bad_tables() {
        assert_eq!(
            BlockStateTable::new(vec![state("minecraft:stone")], 0),
            Err(TableError::Air(0))
        );
        assert_eq!(
            BlockStateTable::new(vec![state("minecraft:stone")], 1),
            Err(TableError::Air(1))
        );
        let air = BlockState {
            air: true,
            ..state("")
        };
        assert_eq!(
            BlockStateTable::new(vec![air], 0),
            Err(TableError::Unnamed(0))
        );
        let air = BlockState {
            air: true,
            ..state("minecraft:air")
        };
        let default = |name| BlockState {
            is_default: true,
            ..state(name)
        };
        assert_eq!(
            BlockStateTable::new(vec![air, default("minecraft:a"), default("minecraft:a")], 0),
            Err(TableError::DefaultState(2))
        );
    }

    #[test]
    fn default_states() {
        let air = BlockState {
            air: true,
            ..state("minecraft:air")
        };
        let default = |name| BlockState {
            is_default: true,
            ..state(name)
        };
        let table = BlockStateTable::new(
            vec![
                air,
                state("minecraft:a"),
                default("minecraft:a"),
                state("minecraft:b"),
                state("minecraft:a"),
                state("minecraft:b"),
            ],
            0,
        )
        .unwrap();
        let defaults: Vec<u32> = table.iter().map(|s| s.default_state).collect();
        // flagged default for a, first state for b
        assert_eq!(defaults, [0, 2, 2, 3, 2, 3]);
        assert_eq!(table.get_default_state("minecraft:a").unwrap().id, 2);
        assert_eq!(table.get_default_state("minecraft:b").unwrap().id, 3);
        assert!(table.get_default_state("minecraft:c").is_none());
        let ids = |name| {
            table
                .get_possible_states(name)
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("minecraft:a"), [1, 2, 4]);
        assert_eq!(ids("minecraft:b"), [3, 5]);
        assert_eq!(ids("minecraft:c"), [0u32; 0]);
    }

    #[test]
    fn deserialize() {
        let json = r#"{
            "version": 4,
            "air": 0,
            "states": [
                {"name": "minecraft:air", "air": true, "can_walk_on": "no",
                 "can_walk_through": "yes", "fully_passable": "yes"},
                {"name": "minecraft:oak_slab", "properties": {"type": "bottom", "waterlogged": "true"},
                 "can_walk_on": "maybe", "can_walk_through": "no", "fully_passable": "no",
                 "fluid": "water", "fluid_source": true, "fluid_amount": 8, "slab": "bottom",
                 "hardness": 2.0, "collision_shape": [[0, 0, 0, 1, 0.5, 1]],
                 "default": true, "tags": ["minecraft:mineable/axe", "minecraft:slabs"],
                 "drops": ["minecraft:oak_slab"]}
            ]
        }"#;
        let table: BlockStateTable = serde_json::from_str(json).unwrap();
        let slab = table.get(1);
        assert_eq!(slab.id, 1);
        assert_eq!(slab.properties["type"], "bottom");
        assert_eq!(slab.can_walk_on, Ternary::Maybe);
        assert_eq!(slab.fluid, Fluid::Water);
        assert_eq!(slab.slab, Some(SlabType::Bottom));
        assert_eq!(
            slab.collision_shape,
            [Aabb::new(0.0, 0.0, 0.0, 1.0, 0.5, 1.0)]
        );
        assert!(slab.is_default);
        assert_eq!(slab.default_state, 1);
        assert_eq!(slab.tags, ["minecraft:mineable/axe", "minecraft:slabs"]);
        assert_eq!(slab.drops, ["minecraft:oak_slab"]);
        assert!(!slab.bonemealable);

        let wrong_version = json.replace("\"version\": 4", "\"version\": 3");
        assert!(serde_json::from_str::<BlockStateTable>(&wrong_version).is_err());
        let unknown_field = json.replace("\"air\": true", "\"air\": true, \"airy\": true");
        assert!(serde_json::from_str::<BlockStateTable>(&unknown_field).is_err());
        let missing_tri_state = json.replace("\"can_walk_on\": \"no\",", "");
        assert!(serde_json::from_str::<BlockStateTable>(&missing_tri_state).is_err());
    }
}
