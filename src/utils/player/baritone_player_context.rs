// Ported from baritone src/main/java/baritone/utils/player/BaritonePlayerContext.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Holds what the host sent (the player, the world, the other entities, the options) and the
// host's player controller, where upstream reads them from `Minecraft`. `playerRotations()`
// asks `LookBehavior.getEffectiveRotation()`, which reads `LookBehavior.serverRotation`; the
// context keeps that rotation (`server_rotation`) because it is what reads it.

use std::sync::Arc;

use crate::api::utils::i_player_controller::{IPlayerController, PlayerController};
use crate::api::utils::{IPlayerContext, Rotation, ray_trace_utils};
use crate::host::{Entity, Options, Player, World};
use crate::mc::BlockHitResult;
use crate::settings::settings;

/// Implementation of [`IPlayerContext`] that provides information about the primary player.
pub struct BaritonePlayerContext {
    pub(crate) player: Option<Player>,
    pub(crate) world: Option<Arc<World>>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) options: Options,
    pub(crate) player_controller: Box<dyn IPlayerController>,
    /// `LookBehavior.serverRotation`: the rotation known to the server.
    pub(crate) server_rotation: Option<Rotation>,
}

impl BaritonePlayerContext {
    pub fn new(player_controller: Box<dyn IPlayerController>) -> Self {
        Self {
            player: None,
            world: None,
            entities: Vec::new(),
            options: Options::default(),
            player_controller,
            server_rotation: None,
        }
    }

    /// `LookBehavior.getEffectiveRotation()`
    pub fn get_effective_rotation(&self) -> Option<Rotation> {
        if settings().free_look {
            return self.server_rotation;
        }
        // If freeLook isn't on, just defer to the player's actual rotations
        None
    }
}

impl IPlayerContext for BaritonePlayerContext {
    fn options(&self) -> &Options {
        &self.options
    }

    fn options_mut(&mut self) -> &mut Options {
        &mut self.options
    }

    fn is_in_world(&self) -> bool {
        self.player.is_some() && self.world.is_some()
    }

    fn player(&self) -> &Player {
        self.player.as_ref().expect("NullPointerException: player")
    }

    fn player_mut(&mut self) -> &mut Player {
        self.player.as_mut().expect("NullPointerException: player")
    }

    fn player_controller(&mut self) -> PlayerController<'_> {
        PlayerController {
            controller: &mut *self.player_controller,
            player: self.player.as_mut().expect("NullPointerException: player"),
            world: self.world.as_mut().expect("NullPointerException: world"),
        }
    }

    fn player_controller_ref(&self) -> &dyn IPlayerController {
        &*self.player_controller
    }

    fn world(&self) -> &Arc<World> {
        self.world.as_ref().expect("NullPointerException: world")
    }

    fn entities(&self) -> &[Entity] {
        &self.entities
    }

    fn player_rotations(&self) -> Rotation {
        self.get_effective_rotation().unwrap_or_else(|| {
            let player = self.player();
            Rotation::new(player.y_rot, player.x_rot)
        })
    }

    fn object_mouse_over(&self) -> BlockHitResult {
        ray_trace_utils::ray_trace_towards(
            self.player(),
            self.world(),
            self.player_rotations(),
            self.player_controller.get_block_reach_distance(),
        )
    }
}
