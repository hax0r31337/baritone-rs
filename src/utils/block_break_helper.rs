// Ported from baritone src/main/java/baritone/utils/BlockBreakHelper.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::api::utils::IPlayerContext;
use crate::host::InteractionHand;
use crate::mc::HitResultType;
use crate::settings::settings;

/// base ticks between block breaks caused by tick logic
const BASE_BREAK_DELAY: i32 = 1;

#[derive(Clone, Debug, Default)]
pub struct BlockBreakHelper {
    was_hitting: bool,
    break_delay_timer: i32,
}

impl BlockBreakHelper {
    pub fn stop_breaking_block(&mut self, ctx: &mut dyn IPlayerContext) {
        // The player controller will never be null, but the player can be
        if ctx.is_in_world() && self.was_hitting {
            let mut controller = ctx.player_controller();
            controller.set_hitting_block(false);
            controller.reset_block_removing();
            self.was_hitting = false;
        }
    }

    pub fn tick(&mut self, ctx: &mut dyn IPlayerContext, is_left_click: bool) {
        if self.break_delay_timer > 0 {
            self.break_delay_timer -= 1;
            return;
        }
        let trace = ctx.object_mouse_over();
        let is_block_trace = trace.get_type() == HitResultType::Block;

        if is_left_click && is_block_trace {
            let mut controller = ctx.player_controller();
            controller.set_hitting_block(self.was_hitting);
            if controller.has_broken_block() {
                controller.sync_held_item();
                controller.click_block(trace.get_block_pos(), trace.get_direction());
                controller.swing(InteractionHand::MainHand);
            } else {
                if controller.on_player_damage_block(trace.get_block_pos(), trace.get_direction()) {
                    controller.swing(InteractionHand::MainHand);
                }
                if controller.has_broken_block() {
                    // block broken this tick
                    // break delay timer only applies for multi-tick block breaks like vanilla
                    self.break_delay_timer =
                        settings().block_break_speed.wrapping_sub(BASE_BREAK_DELAY);
                    // must reset controller's destroy delay to prevent the client from delaying itself unnecessarily
                    controller.set_destroy_delay(0);
                }
            }
            // if true, we're breaking a block. if false, we broke the block this tick
            self.was_hitting = !controller.has_broken_block();
            // this value will be reset by the MC client handling mouse keys
            // since we're not spoofing the click keybind to the client, the client will stop the break if isDestroyingBlock is true
            // we store and restore this value on the next tick to determine if we're breaking a block
            controller.set_hitting_block(false);
        } else {
            self.was_hitting = false;
        }
    }
}
