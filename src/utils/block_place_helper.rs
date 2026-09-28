// Ported from baritone src/main/java/baritone/utils/BlockPlaceHelper.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::api::utils::IPlayerContext;
use crate::api::utils::i_player_controller::InteractionResult;
use crate::host::InteractionHand;
use crate::mc::HitResultType;
use crate::settings::settings;

/// base ticks between places caused by tick logic
const BASE_PLACE_DELAY: i32 = 1;

#[derive(Clone, Debug, Default)]
pub struct BlockPlaceHelper {
    right_click_timer: i32,
}

impl BlockPlaceHelper {
    pub fn tick(&mut self, ctx: &mut dyn IPlayerContext, right_click_requested: bool) {
        if self.right_click_timer > 0 {
            self.right_click_timer -= 1;
            return;
        }
        let mouse_over = ctx.object_mouse_over();
        if !right_click_requested
            || ctx.player().hands_busy
            || mouse_over.get_type() != HitResultType::Block
        {
            return;
        }
        self.right_click_timer = settings().right_click_speed.wrapping_sub(BASE_PLACE_DELAY);
        for hand in InteractionHand::VALUES {
            let mut controller = ctx.player_controller();
            if controller.process_right_click_block(hand, &mouse_over) == InteractionResult::Success
            {
                controller.swing(hand);
                return;
            }
            if !controller.player.get_item_in_hand(hand).is_empty()
                && controller.process_right_click(hand) == InteractionResult::Success
            {
                return;
            }
        }
    }
}
