//! The other entities in the world, as far as ported code looks at them (`ctx.entities()`,
//! `level.getEntitiesOfClass`).

use serde::{Deserialize, Serialize};

use crate::api::utils::BetterBlockPos;
use crate::mc::{Aabb, Vec3};

/// An entity other than the local player.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Entity {
    /// `getId()`
    pub id: i32,
    /// Entity type id (`"minecraft:zombie"`). Class checks read it: `FallingBlockEntity` is
    /// `minecraft:falling_block`, `Spider` is `minecraft:spider` or `minecraft:cave_spider`,
    /// `ZombifiedPiglin` and `Enderman` are their own types.
    pub type_id: String,
    /// `position()`
    pub position: Vec3,
    /// `getBoundingBox()`
    pub bounding_box: Aabb,
    /// `instanceof Mob`
    pub mob: bool,
    /// A zombified piglin that was hurt by someone (`getLastHurtByMob() != null`).
    pub provoked: bool,
    /// An enderman that is angry (`isCreepy()`).
    pub creepy: bool,
}

impl Default for Entity {
    fn default() -> Self {
        Self {
            id: 0,
            type_id: String::new(),
            position: Vec3::ZERO,
            bounding_box: Aabb::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            mob: false,
            provoked: false,
            creepy: false,
        }
    }
}

impl Entity {
    pub const FALLING_BLOCK: &str = "minecraft:falling_block";
    pub const SPIDER: &str = "minecraft:spider";
    pub const CAVE_SPIDER: &str = "minecraft:cave_spider";
    pub const ZOMBIFIED_PIGLIN: &str = "minecraft:zombified_piglin";
    pub const ENDERMAN: &str = "minecraft:enderman";

    /// `blockPosition()`
    pub fn block_position(&self) -> BetterBlockPos {
        BetterBlockPos::from_f64(self.position.x, self.position.y, self.position.z)
    }

    /// `instanceof Spider` (cave spiders are spiders)
    pub fn is_spider(&self) -> bool {
        self.type_id == Self::SPIDER || self.type_id == Self::CAVE_SPIDER
    }
}
