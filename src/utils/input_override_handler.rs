// Ported from baritone src/main/java/baritone/utils/InputOverrideHandler.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Swapping the client's movement input is setting the player's `baritone_input`; the host then
// moves the player with `PlayerMovementInput`. There is one Baritone, the primary one, so
// `inControl()`'s "we are not primary (a bot)" case never applies.

use std::collections::BTreeMap;

use crate::api::event::events::tick_event;
use crate::api::utils::IPlayerContext;
use crate::api::utils::input::Input;
use crate::behavior::PathingBehavior;
use crate::utils::player::BaritonePlayerContext;
use crate::utils::{BlockBreakHelper, BlockPlaceHelper, PlayerMovementInput};

/// An interface with the game's control system allowing the ability to
/// force down certain controls, having the same effect as if we were actually
/// physically forcing down the assigned key.
#[derive(Clone, Debug, Default)]
pub struct InputOverrideHandler {
    /// Maps inputs to whether or not we are forcing their state down.
    input_force_state_map: BTreeMap<Input, bool>,

    block_break_helper: BlockBreakHelper,
    block_place_helper: BlockPlaceHelper,
}

impl InputOverrideHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether or not we are forcing down the specified [`Input`].
    pub fn is_input_forced_down(&self, input: Input) -> bool {
        self.input_force_state_map
            .get(&input)
            .copied()
            .unwrap_or(false)
    }

    /// Sets whether or not the specified [`Input`] is being forced down.
    pub fn set_input_force_state(&mut self, input: Input, forced: bool) {
        self.input_force_state_map.insert(input, forced);
    }

    /// Clears the override state for all keys
    pub fn clear_all_keys(&mut self) {
        self.input_force_state_map.clear();
    }

    /// The forced states, in `Input` order.
    pub fn input_force_states(&self) -> &BTreeMap<Input, bool> {
        &self.input_force_state_map
    }

    pub fn on_tick(
        &mut self,
        ctx: &mut BaritonePlayerContext,
        pathing_behavior: &PathingBehavior,
        event_type: tick_event::Type,
    ) {
        if event_type == tick_event::Type::Out {
            return;
        }
        if self.is_input_forced_down(Input::ClickLeft) {
            self.set_input_force_state(Input::ClickRight, false);
        }
        self.block_break_helper
            .tick(ctx, self.is_input_forced_down(Input::ClickLeft));
        self.block_place_helper
            .tick(ctx, self.is_input_forced_down(Input::ClickRight));

        if self.in_control(pathing_behavior) {
            if !ctx.player().baritone_input {
                ctx.player_mut().baritone_input = true;
            }
        } else if ctx.player().baritone_input {
            // allow other movement inputs that aren't this one, e.g. for a freecam
            ctx.player_mut().baritone_input = false;
        }
        // only set it if it was previously incorrect
        // gotta do it this way, or else it constantly thinks you're beginning a double tap W sprint lol
    }

    fn in_control(&self, pathing_behavior: &PathingBehavior) -> bool {
        for input in [
            Input::MoveForward,
            Input::MoveBack,
            Input::MoveLeft,
            Input::MoveRight,
            Input::Sneak,
            Input::Jump,
        ] {
            if self.is_input_forced_down(input) {
                return true;
            }
        }
        // if we are not primary (a bot) we should set the movementinput even when idle (not pathing)
        pathing_behavior.is_pathing()
    }

    pub fn get_block_break_helper(&mut self) -> &mut BlockBreakHelper {
        &mut self.block_break_helper
    }

    /// `PlayerMovementInput.tick()`: the movement input the client takes this tick while the
    /// player's `baritone_input` is set.
    pub fn player_movement_input(&self) -> PlayerMovementInput {
        let mut input = PlayerMovementInput::default();
        input.tick(self);
        input
    }
}
