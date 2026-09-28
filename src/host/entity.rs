//! The other entities in the world, as far as ported code looks at them (`ctx.entities()`,
//! `level.getEntitiesOfClass`).

use std::fmt;

use serde::{Deserialize, Serialize};

use super::ItemStack;
use crate::api::utils::BetterBlockPos;
use crate::mc::{Aabb, Vec3};

/// An entity other than the local player.
///
/// Two entities are the same entity (Java `equals`, which is identity for entities) when their
/// `id`s are equal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Entity {
    /// `getId()`
    pub id: i32,
    /// Entity type id (`"minecraft:zombie"`). Class checks read it: `FallingBlockEntity` is
    /// `minecraft:falling_block`, `Spider` is `minecraft:spider` or `minecraft:cave_spider`,
    /// `ZombifiedPiglin` and `Enderman` are their own types, `ItemEntity` is `minecraft:item`.
    pub type_id: String,
    /// `position()`
    pub position: Vec3,
    /// `getBoundingBox()`
    pub bounding_box: Aabb,
    /// `onGround()`
    pub on_ground: bool,
    /// `isAlive()`: false while a dead mob plays its death animation.
    pub alive: bool,
    /// `blocksBuilding`: blocks placing a block into its box (living entities, boats,
    /// minecarts, falling blocks, primed TNT, end crystals). False for spectators, which
    /// `Level.getEntities` leaves out.
    pub blocks_building: bool,
    /// `instanceof Mob`
    pub mob: bool,
    /// A zombified piglin that was hurt by someone (`getLastHurtByMob() != null`).
    pub provoked: bool,
    /// An enderman that is angry (`isCreepy()`).
    pub creepy: bool,
    /// `ItemEntity.getItem()`, for item entities.
    pub item: Option<ItemStack>,
}

impl Default for Entity {
    fn default() -> Self {
        Self {
            id: 0,
            type_id: String::new(),
            position: Vec3::ZERO,
            bounding_box: Aabb::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            on_ground: false,
            alive: true,
            blocks_building: false,
            mob: false,
            provoked: false,
            creepy: false,
            item: None,
        }
    }
}

impl Entity {
    pub const FALLING_BLOCK: &str = "minecraft:falling_block";
    pub const SPIDER: &str = "minecraft:spider";
    pub const CAVE_SPIDER: &str = "minecraft:cave_spider";
    pub const ZOMBIFIED_PIGLIN: &str = "minecraft:zombified_piglin";
    pub const ENDERMAN: &str = "minecraft:enderman";
    pub const ITEM: &str = "minecraft:item";

    /// `blockPosition()`
    pub fn block_position(&self) -> BetterBlockPos {
        BetterBlockPos::from_f64(self.position.x, self.position.y, self.position.z)
    }

    /// `instanceof Spider` (cave spiders are spiders)
    pub fn is_spider(&self) -> bool {
        self.type_id == Self::SPIDER || self.type_id == Self::CAVE_SPIDER
    }

    /// `instanceof ItemEntity`, with its item.
    pub fn as_item_entity(&self) -> Option<&ItemStack> {
        if self.type_id == Self::ITEM {
            self.item.as_ref()
        } else {
            None
        }
    }

    /// `distanceToSqr(Vec3)`
    pub fn distance_to_sqr(&self, pos: Vec3) -> f64 {
        let dx = self.position.x - pos.x;
        let dy = self.position.y - pos.y;
        let dz = self.position.z - pos.z;
        dx * dx + dy * dy + dz * dz
    }
}

impl fmt::Display for Entity {
    /// Like `Entity.toString()`: the type, id and position.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}[id={}, x={:.2}, y={:.2}, z={:.2}]",
            self.type_id, self.id, self.position.x, self.position.y, self.position.z
        )
    }
}
