// Ported from baritone src/api/java/baritone/api/behavior/look/ITickableAimProcessor.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::api::behavior::look::IAimProcessor;
use crate::api::utils::Rotation;
use crate::api::utils::i_player_context::IPlayerContext;

pub trait ITickableAimProcessor: IAimProcessor {
    /// Advances the internal state of this aim processor by a single tick.
    fn tick(&mut self);

    /// Calls [`ITickableAimProcessor::tick`] the specified number of times.
    fn advance(&mut self, ticks: i32);

    /// Returns the actual rotation as provided by [`IAimProcessor::peek_rotation`], and then automatically advances the
    /// internal state by one [`ITickableAimProcessor::tick`].
    fn next_rotation(&mut self, ctx: &dyn IPlayerContext, rotation: Rotation) -> Rotation;
}
