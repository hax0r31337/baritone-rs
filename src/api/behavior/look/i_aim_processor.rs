// Ported from baritone src/api/java/baritone/api/behavior/look/IAimProcessor.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Upstream's processors hold the player context; the port passes it to each call.

use crate::api::behavior::look::ITickableAimProcessor;
use crate::api::utils::Rotation;
use crate::api::utils::i_player_context::IPlayerContext;

pub trait IAimProcessor {
    /// Returns the actual rotation that will be used when the desired rotation is requested. The returned rotation
    /// always reflects what would happen in the upcoming tick. In other words, it is a pure function, and no internal
    /// state changes. If simulation of the rotation states beyond the next tick is required, then a
    /// [`IAimProcessor::fork`] should be created.
    fn peek_rotation(&self, ctx: &dyn IPlayerContext, desired: Rotation) -> Rotation;

    /// Returns a copy of this [`IAimProcessor`] which has its own internal state and is manually tickable.
    fn fork(&self, ctx: &dyn IPlayerContext) -> Box<dyn ITickableAimProcessor>;
}
