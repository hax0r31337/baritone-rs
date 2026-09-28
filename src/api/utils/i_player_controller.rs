// Ported from baritone src/api/java/baritone/api/utils/IPlayerController.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Upstream chains to Minecraft's `MultiPlayerGameMode` (`BaritonePlayerController`); here the
// host implements it. The game mode breaks and places blocks on the client and tells the
// server, so the methods that do so get the player and the world the client would change:
// what the implementation changes, the rest of the tick sees. Methods take the player and the
// world as `&mut` even where upstream passes neither, because the game mode reads the client's
// own. `PlayerController` bundles an implementation with them, so call sites read like
// upstream's `ctx.playerController().x(...)`.
//
// Not upstream: `set_destroy_delay` (the `IPlayerControllerMP` accessor BlockBreakHelper casts
// the game mode to), `swing` (`LocalPlayer.swing`) and `disconnect` (`ClientLevel.disconnect`).
// They are client actions too.

use std::sync::Arc;

use crate::api::utils::BetterBlockPos;
use crate::host::{InteractionHand, Player, World};
use crate::mc::{BlockHitResult, Direction};
use crate::settings::settings;

/// `net.minecraft.world.level.GameType`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GameType {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameType {
    pub fn is_creative(self) -> bool {
        self == GameType::Creative
    }
}

/// `net.minecraft.world.InteractionResult`. Upstream compares against the `SUCCESS` constant
/// by identity, so the successes are told apart: `SUCCESS` is `Success`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InteractionResult {
    Success,
    SuccessServer,
    Consume,
    Fail,
    Pass,
    TryEmptyHandInteraction,
}

/// `net.minecraft.world.inventory.ContainerInput` (the click type of a window click)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContainerInput {
    Pickup,
    QuickMove,
    Swap,
    Clone,
    Throw,
    QuickCraft,
    PickupAll,
}

/// See the module docs.
pub trait IPlayerController: Send {
    fn sync_held_item(&mut self, player: &mut Player);

    fn has_broken_block(&self) -> bool;

    fn on_player_damage_block(
        &mut self,
        player: &mut Player,
        world: &mut Arc<World>,
        pos: BetterBlockPos,
        side: Direction,
    ) -> bool;

    fn reset_block_removing(&mut self, player: &mut Player, world: &mut Arc<World>);

    fn window_click(
        &mut self,
        player: &mut Player,
        window_id: i32,
        slot_id: i32,
        mouse_button: i32,
        input_type: ContainerInput,
    );

    fn get_game_type(&self) -> GameType;

    fn process_right_click_block(
        &mut self,
        player: &mut Player,
        world: &mut Arc<World>,
        hand: InteractionHand,
        result: &BlockHitResult,
    ) -> InteractionResult;

    fn process_right_click(
        &mut self,
        player: &mut Player,
        world: &mut Arc<World>,
        hand: InteractionHand,
    ) -> InteractionResult;

    fn click_block(
        &mut self,
        player: &mut Player,
        world: &mut Arc<World>,
        loc: BetterBlockPos,
        face: Direction,
    ) -> bool;

    fn set_hitting_block(&mut self, hitting_block: bool);

    fn get_block_reach_distance(&self) -> f64 {
        if self.get_game_type().is_creative() {
            5.0f32 as f64
        } else {
            settings().block_reach_distance as f64
        }
    }

    /// `((IPlayerControllerMP) mc.gameMode).setDestroyDelay(int)`
    fn set_destroy_delay(&mut self, destroy_delay: i32);

    /// `LocalPlayer.swing(InteractionHand, SwingAnimation.DEFAULT, false)`
    fn swing(&mut self, player: &mut Player, hand: InteractionHand);

    /// `ClientLevel.disconnect(Component)`
    fn disconnect(&mut self, reason: &str);
}

/// A controller with the player and world it acts on: what `ctx.playerController()` hands
/// out. The methods are upstream's `IPlayerController` signatures.
pub struct PlayerController<'a> {
    pub controller: &'a mut dyn IPlayerController,
    pub player: &'a mut Player,
    pub world: &'a mut Arc<World>,
}

impl PlayerController<'_> {
    pub fn sync_held_item(&mut self) {
        self.controller.sync_held_item(self.player);
    }

    pub fn has_broken_block(&self) -> bool {
        self.controller.has_broken_block()
    }

    pub fn on_player_damage_block(&mut self, pos: BetterBlockPos, side: Direction) -> bool {
        self.controller
            .on_player_damage_block(self.player, self.world, pos, side)
    }

    pub fn reset_block_removing(&mut self) {
        self.controller
            .reset_block_removing(self.player, self.world);
    }

    /// `windowClick(int, int, int, ContainerInput, Player)`: the player is the context's.
    pub fn window_click(
        &mut self,
        window_id: i32,
        slot_id: i32,
        mouse_button: i32,
        input_type: ContainerInput,
    ) {
        self.controller
            .window_click(self.player, window_id, slot_id, mouse_button, input_type);
    }

    pub fn get_game_type(&self) -> GameType {
        self.controller.get_game_type()
    }

    /// `processRightClickBlock(LocalPlayer, Level, InteractionHand, BlockHitResult)`: the
    /// player and world are the context's.
    pub fn process_right_click_block(
        &mut self,
        hand: InteractionHand,
        result: &BlockHitResult,
    ) -> InteractionResult {
        self.controller
            .process_right_click_block(self.player, self.world, hand, result)
    }

    /// `processRightClick(LocalPlayer, Level, InteractionHand)`: the player and world are the
    /// context's.
    pub fn process_right_click(&mut self, hand: InteractionHand) -> InteractionResult {
        self.controller
            .process_right_click(self.player, self.world, hand)
    }

    pub fn click_block(&mut self, loc: BetterBlockPos, face: Direction) -> bool {
        self.controller
            .click_block(self.player, self.world, loc, face)
    }

    pub fn set_hitting_block(&mut self, hitting_block: bool) {
        self.controller.set_hitting_block(hitting_block);
    }

    pub fn get_block_reach_distance(&self) -> f64 {
        self.controller.get_block_reach_distance()
    }

    pub fn set_destroy_delay(&mut self, destroy_delay: i32) {
        self.controller.set_destroy_delay(destroy_delay);
    }

    pub fn swing(&mut self, hand: InteractionHand) {
        self.controller.swing(self.player, hand);
    }

    pub fn disconnect(&mut self, reason: &str) {
        self.controller.disconnect(reason);
    }
}
