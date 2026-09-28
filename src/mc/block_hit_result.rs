// Ported from Minecraft 26.3 net/minecraft/world/phys/BlockHitResult.java and HitResult.java
// (client jar bytecode)
//
// The raytraces ported code runs (`BlockGetter.clip`) only ever return block results, so
// `HitResult` is this struct and `getType()` is `MISS` or `BLOCK`.

use crate::api::utils::BetterBlockPos;
use crate::mc::{Direction, Vec3};

/// `HitResult.Type`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HitResultType {
    Miss,
    Block,
    Entity,
}

/// `net.minecraft.world.phys.BlockHitResult`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockHitResult {
    location: Vec3,
    direction: Direction,
    block_pos: BetterBlockPos,
    miss: bool,
    inside: bool,
    world_border_hit: bool,
}

impl BlockHitResult {
    /// `BlockHitResult.miss(Vec3, Direction, BlockPos)`
    pub fn miss(location: Vec3, direction: Direction, pos: BetterBlockPos) -> Self {
        Self {
            location,
            direction,
            block_pos: pos,
            miss: true,
            inside: false,
            world_border_hit: false,
        }
    }

    /// `new BlockHitResult(Vec3, Direction, BlockPos, boolean)`
    pub fn new(location: Vec3, direction: Direction, pos: BetterBlockPos, inside: bool) -> Self {
        Self {
            location,
            direction,
            block_pos: pos,
            miss: false,
            inside,
            world_border_hit: false,
        }
    }

    pub fn with_direction(&self, direction: Direction) -> Self {
        Self { direction, ..*self }
    }

    pub fn get_block_pos(&self) -> BetterBlockPos {
        self.block_pos
    }

    pub fn get_direction(&self) -> Direction {
        self.direction
    }

    /// `HitResult.getType()`
    pub fn get_type(&self) -> HitResultType {
        if self.miss {
            HitResultType::Miss
        } else {
            HitResultType::Block
        }
    }

    pub fn is_inside(&self) -> bool {
        self.inside
    }

    pub fn is_world_border_hit(&self) -> bool {
        self.world_border_hit
    }

    /// `HitResult.getLocation()`
    pub fn get_location(&self) -> Vec3 {
        self.location
    }
}
