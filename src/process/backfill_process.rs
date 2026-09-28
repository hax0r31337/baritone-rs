// Ported from baritone src/main/java/baritone/process/BackfillProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `blocksToReplace` iterates in Java `HashMap` order (`java::JavaHashMap`) and holds state
// ids. `Blocks.AIR` is the table's air block; an unloaded chunk is upstream's
// `EmptyLevelChunk`. Upstream turns `backfill` off when `allowParkour` is on, which here
// replaces the settings (`update_settings`). If the table has no dirt, placement is judged
// for a full block, which is what dirt is.

use std::any::Any;
use std::sync::Arc;

use crate::Baritone;
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::helper::log_direct;
use crate::api::utils::input::Input;
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::host::BlockState;
use crate::java::{self, JavaHashMap};
use crate::mc::Aabb;
use crate::pathing::movement::MovementState;
use crate::pathing::movement::movement_helper::{self, PlaceResult};
use crate::process::builder_process;
use crate::settings::{settings, update_settings};

#[derive(Debug)]
pub struct BackfillProcess {
    /// Positions and the state ids they had when Baritone looked at them.
    pub blocks_to_replace: JavaHashMap<BetterBlockPos, u32>,
}

impl Default for BackfillProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl BackfillProcess {
    pub fn new() -> Self {
        Self {
            blocks_to_replace: JavaHashMap::new(BetterBlockPos::block_pos_hash_code),
        }
    }

    #[allow(non_snake_case)]
    fn am_I_breaking_a_block_HMMMMMMM(&mut self, baritone: &Baritone) {
        let ctx = &baritone.player_context;
        let Some(selected) = ctx.get_selected_block() else {
            return;
        };
        if !baritone.pathing_behavior.is_pathing() {
            return;
        }
        self.blocks_to_replace
            .insert(selected, ctx.world().get_block_state(selected).id);
    }

    pub fn to_fill_in(&self, baritone: &Baritone) -> Vec<BetterBlockPos> {
        let ctx = &baritone.player_context;
        let world = ctx.world();
        let full_block;
        let dirt = match world.table().get_default_state("minecraft:dirt") {
            Some(dirt) => dirt,
            None => {
                full_block = BlockState {
                    collision_shape: vec![Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)],
                    ..BlockState::default()
                };
                &full_block
            }
        };
        let feet = ctx.player_feet();
        let mut to_fill_in: Vec<BetterBlockPos> = self
            .blocks_to_replace
            .keys()
            .copied()
            .filter(|&pos| is_air_block(ctx, pos))
            .filter(|&pos| builder_process::placement_plausible(ctx, pos, dirt))
            .filter(|&pos| !part_of_current_movement(baritone, pos))
            .collect();
        // Comparator.comparingDouble(playerFeet::distSqr).reversed()
        to_fill_in.sort_by(|a, b| java::double_compare(feet.distance_sq(b), feet.distance_sq(a)));
        to_fill_in
    }
}

/// `getBlockState(pos).getBlock() == Blocks.AIR`
fn is_air_block(ctx: &dyn IPlayerContext, pos: BetterBlockPos) -> bool {
    let world = ctx.world();
    world.get_block_state(pos).default_state == world.table().air().default_state
}

fn part_of_current_movement(baritone: &Baritone, pos: BetterBlockPos) -> bool {
    baritone.pathing_behavior.with_current(|exec| {
        let Some(exec) = exec else {
            return false;
        };
        if exec.finished() || exec.failed() {
            return false;
        }
        let movement = &exec.get_path().movements()[exec.get_position() as usize];
        movement.to_break_all().contains(&pos)
    })
}

impl IBaritoneProcess for BackfillProcess {
    fn is_active(&mut self, baritone: &mut Baritone) -> bool {
        if !baritone.player_context.is_in_world() {
            return false;
        }
        if !settings().backfill {
            return false;
        }
        if settings().allow_parkour {
            log_direct("Backfill cannot be used with allowParkour true");
            update_settings(|settings| settings.backfill = false);
            return false;
        }
        let world = Arc::clone(baritone.player_context.world());
        let positions: Vec<BetterBlockPos> = self.blocks_to_replace.keys().copied().collect();
        for pos in positions {
            if !world.has_chunk(pos.x >> 4, pos.z >> 4)
                || !is_air_block(&baritone.player_context, pos)
            {
                self.blocks_to_replace.remove(&pos);
            }
        }
        self.am_I_breaking_a_block_HMMMMMMM(baritone);
        baritone.input_override_handler.clear_all_keys();

        !self.to_fill_in(baritone).is_empty()
    }

    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        _calc_failed: bool,
        is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        if !is_safe_to_cancel {
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        }
        baritone.input_override_handler.clear_all_keys();
        for to_place in self.to_fill_in(baritone) {
            let mut fake = MovementState::default();
            match movement_helper::attempt_to_place_a_block(
                &mut fake, baritone, to_place, false, false,
            ) {
                PlaceResult::NoOption => continue,
                PlaceResult::ReadyToPlace => {
                    baritone
                        .input_override_handler
                        .set_input_force_state(Input::ClickRight, true);
                    return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
                }
                PlaceResult::Attempting => {
                    // patience
                    let rotation = fake
                        .get_target()
                        .get_rotation()
                        .expect("NoSuchElementException: No value present");
                    baritone
                        .look_behavior
                        .update_target(&baritone.player_context, rotation, true);
                    return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
                }
            }
        }
        Some(PathingCommand::new(None, PathingCommandType::Defer)) // cede to other process
    }

    fn is_temporary(&self) -> bool {
        true
    }

    fn on_lost_control(&mut self, _baritone: &mut Baritone) {
        if !self.blocks_to_replace.is_empty() {
            self.blocks_to_replace.clear();
        }
    }

    fn priority(&self) -> f64 {
        5.0
    }

    fn display_name0(&mut self, _baritone: &mut Baritone) -> String {
        "Backfill".to_owned()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
