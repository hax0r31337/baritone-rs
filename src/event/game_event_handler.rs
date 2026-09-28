// Ported from baritone src/main/java/baritone/event/GameEventHandler.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The listeners are fixed: the behaviors in registration order (look, pathing, inventory, input
// override), then the pathing control manager's post tick. `WaypointBehavior` and the world
// provider (chunk cache) are not ported, and no kept listener handles chunk, block change or
// chat events. Path events, which only outside listeners receive, are collected for the host
// (`Baritone::take_path_events`).

use crate::Baritone;
use crate::api::event::events::tick_event;
use crate::api::event::events::r#type::EventState;
use crate::api::event::events::{PathEvent, RotationMoveEvent};
use crate::behavior::{InventoryBehavior, PathingBehavior};
use crate::utils::{BlockStateInterface, PathingControlManager};

#[derive(Debug, Default)]
pub struct GameEventHandler {
    path_events: Vec<PathEvent>,
}

impl GameEventHandler {
    pub fn on_tick(baritone: &mut Baritone, event_type: tick_event::Type) {
        if event_type == tick_event::Type::In {
            baritone.bsi = Some(BlockStateInterface::from_ctx(&baritone.player_context));
        } else {
            baritone.bsi = None;
        }
        baritone.look_behavior.on_tick(event_type);
        PathingBehavior::on_tick(baritone, event_type);
        InventoryBehavior::on_tick(baritone, event_type);
        let Baritone {
            input_override_handler,
            player_context,
            pathing_behavior,
            ..
        } = baritone;
        input_override_handler.on_tick(player_context, pathing_behavior, event_type);
        PathingControlManager::on_tick(baritone, event_type);
    }

    pub fn on_player_update(baritone: &mut Baritone, state: EventState) {
        baritone
            .look_behavior
            .on_player_update(&mut baritone.player_context, state);
        baritone
            .pathing_behavior
            .on_player_update(&mut baritone.player_context, state);
    }

    pub fn on_player_sprint_state(baritone: &Baritone) -> Option<bool> {
        baritone.pathing_behavior.on_player_sprint_state()
    }

    pub fn on_player_rotation_move(baritone: &Baritone, event: &mut RotationMoveEvent) {
        baritone
            .look_behavior
            .on_player_rotation_move(&baritone.player_context, event);
    }

    pub fn on_world_event(baritone: &mut Baritone, _state: EventState) {
        baritone
            .look_behavior
            .on_world_event(&mut baritone.player_context);
    }

    pub fn on_path_event(&mut self, event: PathEvent) {
        self.path_events.push(event);
    }

    /// The path events since the last call.
    pub fn take_path_events(&mut self) -> Vec<PathEvent> {
        std::mem::take(&mut self.path_events)
    }
}
