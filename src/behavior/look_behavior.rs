// Ported from baritone src/main/java/baritone/behavior/LookBehavior.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `serverRotation` lives in the player context (`BaritonePlayerContext::server_rotation`),
// which is what reads it. `onSendPacket` is `on_send_rotation`: the host calls it with the
// rotation of every `ServerboundMovePlayerPacket` that has one. The random offsets come from a
// clock-seeded `ForkableRandom` like upstream; `with_random` seeds it for reproducible runs.
//
// Unlike upstream, every target is `Target.Mode.SERVER`: the rotation is only ever set silently
// for the tick and the player's rotation is restored afterwards. The settings upstream picks the
// mode with (`freeLook`, `blockFreeLook`, `antiCheatCompatibility`, the smooth and elytra look
// ones) are not ported.

use crate::api::behavior::look::{IAimProcessor, ITickableAimProcessor};
use crate::api::event::events::RotationMoveEvent;
use crate::api::event::events::tick_event;
use crate::api::event::events::r#type::EventState;
use crate::api::utils::{IPlayerContext, Rotation};
use crate::behavior::look::ForkableRandom;
use crate::java::round_f32;
use crate::settings::settings;
use crate::utils::player::BaritonePlayerContext;

pub struct LookBehavior {
    /// The current look target, may be `None`.
    target: Option<Rotation>,

    /// The last player rotation. Used to restore the player's angle after the silent rotation.
    prev_rotation: Option<Rotation>,

    processor: AbstractAimProcessor,
}

impl Default for LookBehavior {
    fn default() -> Self {
        Self::new()
    }
}

impl LookBehavior {
    pub fn new() -> Self {
        Self::with_random(ForkableRandom::new())
    }

    /// A look behavior whose aim processor draws from `rand`.
    pub fn with_random(rand: ForkableRandom) -> Self {
        Self {
            target: None,
            prev_rotation: None,
            processor: AbstractAimProcessor::new(rand),
        }
    }

    pub fn update_target(&mut self, rotation: Rotation) {
        self.target = Some(rotation);
    }

    pub fn get_aim_processor(&self) -> &dyn IAimProcessor {
        &self.processor
    }

    pub fn on_tick(&mut self, event_type: tick_event::Type) {
        if event_type == tick_event::Type::In {
            self.processor.tick();
        }
    }

    pub fn on_player_update(&mut self, ctx: &mut BaritonePlayerContext, state: EventState) {
        let Some(target) = self.target else {
            return;
        };

        match state {
            EventState::Pre => {
                let player = ctx.player();
                self.prev_rotation = Some(Rotation::new(player.y_rot, player.x_rot));
                let actual = self.processor.peek_rotation(ctx, target);
                set_y_rot(ctx, actual.get_yaw());
                set_x_rot(ctx, actual.get_pitch());
            }
            EventState::Post => {
                // Reset the player's rotations back to their original values
                if let Some(prev_rotation) = self.prev_rotation {
                    set_y_rot(ctx, prev_rotation.get_yaw());
                    set_x_rot(ctx, prev_rotation.get_pitch());
                    //ctx.player().xRotO = prevRotation.getPitch();
                    //ctx.player().yRotO = prevRotation.getYaw();
                    self.prev_rotation = None;
                }
                // The target is done being used for this game tick, so it can be invalidated
                self.target = None;
            }
        }
    }

    /// `onSendPacket(PacketEvent)` for a `ServerboundMovePlayerPacket.Rot` or `.PosRot`.
    pub fn on_send_rotation(&mut self, ctx: &mut BaritonePlayerContext, y_rot: f32, x_rot: f32) {
        ctx.server_rotation = Some(Rotation::new(y_rot, x_rot));
    }

    /// `onWorldEvent(WorldEvent)`
    pub fn on_world_event(&mut self, ctx: &mut BaritonePlayerContext) {
        ctx.server_rotation = None;
        self.target = None;
    }

    pub fn pig(&self, ctx: &mut BaritonePlayerContext) {
        if let Some(target) = self.target {
            let actual = self.processor.peek_rotation(ctx, target);
            set_y_rot(ctx, actual.get_yaw());
        }
    }

    pub fn get_effective_rotation(&self, ctx: &BaritonePlayerContext) -> Option<Rotation> {
        ctx.get_effective_rotation()
    }

    pub fn on_player_rotation_move(&self, ctx: &dyn IPlayerContext, event: &mut RotationMoveEvent) {
        if let Some(target) = self.target {
            let actual = self.processor.peek_rotation(ctx, target);
            event.set_yaw(actual.get_yaw());
            event.set_pitch(actual.get_pitch());
        }
    }
}

/// `Entity.setYRot(float)`: ignores non-finite values.
fn set_y_rot(ctx: &mut BaritonePlayerContext, y_rot: f32) {
    if y_rot.is_finite() {
        ctx.player_mut().y_rot = y_rot;
    }
}

/// `Entity.setXRot(float)`: ignores non-finite values, keeps the pitch within ±90.
fn set_x_rot(ctx: &mut BaritonePlayerContext, x_rot: f32) {
    if x_rot.is_finite() {
        // Math.clamp(xRot % 360.0F, -90.0F, 90.0F)
        ctx.player_mut().x_rot = (x_rot % 360.0f32).clamp(-90.0f32, 90.0f32);
    }
}

/// `AimProcessor` (the look behavior's own processor, whose previous rotation is the player's)
/// and `AbstractAimProcessor`'s forks (which remember the rotation they returned last).
#[derive(Clone, Debug)]
struct AbstractAimProcessor {
    rand: ForkableRandom,
    random_yaw_offset: f64,
    random_pitch_offset: f64,
    /// `None` for `AimProcessor`, whose `getPrevRotation()` is `ctx.playerRotations()`.
    prev: Option<Rotation>,
}

impl AbstractAimProcessor {
    fn new(rand: ForkableRandom) -> Self {
        Self {
            rand,
            random_yaw_offset: 0.0,
            random_pitch_offset: 0.0,
            prev: None,
        }
    }

    fn get_prev_rotation(&self, ctx: &dyn IPlayerContext) -> Rotation {
        match self.prev {
            Some(prev) => prev,
            // Implementation will use LookBehavior.serverRotation
            None => ctx.player_rotations(),
        }
    }

    /// Nudges the player's pitch to a regular level. (Between `-20` and `10`, increments are by `1`)
    fn nudge_to_level(pitch: f32) -> f32 {
        if pitch < -20.0 {
            return pitch + 1.0;
        } else if pitch > 10.0 {
            return pitch - 1.0;
        }
        pitch
    }

    fn calculate_mouse_move(ctx: &dyn IPlayerContext, current: f32, target: f32) -> f32 {
        let delta = target - current;
        let delta_px = Self::angle_to_mouse(ctx, delta); // yes, even the mouse movements use double
        current + Self::mouse_to_angle(ctx, delta_px)
    }

    fn angle_to_mouse(ctx: &dyn IPlayerContext, angle_delta: f32) -> f64 {
        let min_angle_change = Self::mouse_to_angle(ctx, 1.0);
        round_f32(angle_delta / min_angle_change) as f64
    }

    fn mouse_to_angle(ctx: &dyn IPlayerContext, mouse_delta: f64) -> f32 {
        // casting float literals to double gets us the precise values used by mc
        let f = ctx.options().sensitivity * 0.6f32 as f64 + 0.2f32 as f64;
        (mouse_delta * f * f * f * 8.0) as f32 * 0.15f32 // yes, one double and one float scaling factor
    }
}

impl IAimProcessor for AbstractAimProcessor {
    fn peek_rotation(&self, ctx: &dyn IPlayerContext, rotation: Rotation) -> Rotation {
        let prev = self.get_prev_rotation(ctx);

        let mut desired_yaw = rotation.get_yaw();
        let mut desired_pitch = rotation.get_pitch();

        // In other words, the target doesn't care about the pitch, so it used playerRotations().getPitch()
        // and it's safe to adjust it to a normal level
        if desired_pitch == prev.get_pitch() {
            desired_pitch = Self::nudge_to_level(desired_pitch);
        }

        desired_yaw = (desired_yaw as f64 + self.random_yaw_offset) as f32;
        desired_pitch = (desired_pitch as f64 + self.random_pitch_offset) as f32;

        Rotation::new(
            Self::calculate_mouse_move(ctx, prev.get_yaw(), desired_yaw),
            Self::calculate_mouse_move(ctx, prev.get_pitch(), desired_pitch),
        )
        .clamp()
    }

    fn fork(&self, ctx: &dyn IPlayerContext) -> Box<dyn ITickableAimProcessor> {
        Box::new(Self {
            rand: self.rand.fork(),
            random_yaw_offset: self.random_yaw_offset,
            random_pitch_offset: self.random_pitch_offset,
            prev: Some(self.get_prev_rotation(ctx)),
        })
    }
}

impl ITickableAimProcessor for AbstractAimProcessor {
    fn tick(&mut self) {
        let settings = settings();
        // randomLooking
        self.random_yaw_offset = (self.rand.next_double() - 0.5) * settings.random_looking;
        self.random_pitch_offset = (self.rand.next_double() - 0.5) * settings.random_looking;

        // randomLooking113
        let mut random = self.rand.next_double() - 0.5;
        if random.abs() < 0.1 {
            random *= 4.0;
        }
        self.random_yaw_offset += random * settings.random_looking113;
    }

    fn advance(&mut self, ticks: i32) {
        for _ in 0..ticks {
            self.tick();
        }
    }

    fn next_rotation(&mut self, ctx: &dyn IPlayerContext, rotation: Rotation) -> Rotation {
        let actual = self.peek_rotation(ctx, rotation);
        self.tick();
        if self.prev.is_some() {
            // a fork remembers what it returned
            self.prev = Some(actual);
        }
        actual
    }
}
