// Ported from baritone src/api/java/baritone/api/event/events/RotationMoveEvent.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::api::utils::Rotation;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RotationMoveEvent {
    /// The type of event
    event_type: Type,

    original: Rotation,

    /// The yaw rotation
    yaw: f32,

    /// The pitch rotation
    pitch: f32,
}

impl RotationMoveEvent {
    pub fn new(event_type: Type, yaw: f32, pitch: f32) -> Self {
        Self {
            event_type,
            original: Rotation::new(yaw, pitch),
            yaw,
            pitch,
        }
    }

    pub fn get_original(&self) -> Rotation {
        self.original
    }

    /// Set the yaw movement rotation
    pub fn set_yaw(&mut self, yaw: f32) {
        self.yaw = yaw;
    }

    /// The yaw movement rotation
    pub fn get_yaw(&self) -> f32 {
        self.yaw
    }

    /// Set the pitch movement rotation
    pub fn set_pitch(&mut self, pitch: f32) {
        self.pitch = pitch;
    }

    /// The pitch movement rotation
    pub fn get_pitch(&self) -> f32 {
        self.pitch
    }

    /// The type of the event
    pub fn get_type(&self) -> Type {
        self.event_type
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// Called when the player's motion is updated.
    MotionUpdate,

    /// Called when the player jumps.
    Jump,
}
