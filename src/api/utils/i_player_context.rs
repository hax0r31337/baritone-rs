// Ported from baritone src/api/java/baritone/api/utils/IPlayerContext.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `minecraft()` is `options()`, the one part of `Minecraft` ported code reads. `worldData()`
// (the chunk cache) and `viewerPos()` (rendering) are not ported. `player()` and `world()`
// panic when there is none, where upstream would throw a `NullPointerException`; callers that
// care check `is_in_world` first, like upstream's null checks. `entities()` leaves out the
// local player, which upstream's includes; callers that went through it skip the player
// anyway. `entitiesStream()` is `entities().iter()`.

use std::sync::Arc;

use crate::api::utils::i_player_controller::{IPlayerController, PlayerController};
use crate::api::utils::{BetterBlockPos, Rotation};
use crate::host::{Entity, Options, Player, World};
use crate::mc::{BlockHitResult, HitResultType, Vec3};

pub trait IPlayerContext {
    /// `minecraft().options`
    fn options(&self) -> &Options;

    /// `minecraft().options`, for the options Baritone changes.
    fn options_mut(&mut self) -> &mut Options;

    /// There is a player and a world (upstream: `player() != null && world() != null`).
    fn is_in_world(&self) -> bool;

    fn player(&self) -> &Player;

    fn player_mut(&mut self) -> &mut Player;

    /// `playerController()`, with the player and world it acts on.
    fn player_controller(&mut self) -> PlayerController<'_>;

    /// `playerController()`, for the methods that only read.
    fn player_controller_ref(&self) -> &dyn IPlayerController;

    fn world(&self) -> &Arc<World>;

    /// `entities()`: every entity but the local player.
    fn entities(&self) -> &[Entity];

    fn object_mouse_over(&self) -> BlockHitResult;

    fn player_feet(&self) -> BetterBlockPos {
        player_feet(self.player(), self.world())
    }

    fn player_feet_as_vec(&self) -> Vec3 {
        let position = self.player().position;
        Vec3::new(position.x, position.y, position.z)
    }

    fn player_head(&self) -> Vec3 {
        let player = self.player();
        Vec3::new(
            player.position.x,
            player.position.y + player.eye_height as f64,
            player.position.z,
        )
    }

    fn player_motion(&self) -> Vec3 {
        self.player().delta_movement
    }

    fn player_rotations(&self) -> Rotation {
        let player = self.player();
        Rotation::new(player.y_rot, player.x_rot)
    }

    /// Returns the block that the crosshair is currently placed over. Updated once per tick.
    fn get_selected_block(&self) -> Option<BetterBlockPos> {
        let result = self.object_mouse_over();
        if result.get_type() == HitResultType::Block {
            return Some(result.get_block_pos());
        }
        None
    }

    fn is_looking_at(&self, pos: BetterBlockPos) -> bool {
        self.get_selected_block() == Some(pos)
    }
}

/// `playerFeet()` of `player` in `world`, for code that works on a snapshot of the two.
pub fn player_feet(player: &Player, world: &World) -> BetterBlockPos {
    // TODO find a better way to deal with soul sand!!!!!
    let feet = BetterBlockPos::from_f64(
        player.position.x,
        player.position.y + 0.1251,
        player.position.z,
    );

    // sometimes when calling this from another thread or while world is null, it'll throw a NullPointerException
    // that causes the game to immediately crash
    //
    // so of course crashing on 2b is horribly bad due to queue times and logout spot
    // catch the NPE and ignore it if it does happen
    //
    // this does not impact performance at all since we're not null checking constantly
    // if there is an exception, the only overhead is Java generating the exception object... so we can ignore it
    if world.get_block_state(feet).slab.is_some() {
        return feet.above();
    }

    feet
}
