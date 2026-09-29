//! A deterministic stand-in for the client around a `Baritone`: a simplified Minecraft game
//! mode (block breaking and placing) behind the `IPlayerController`, and simplified player
//! movement (input, jumping, friction, gravity, collision with block shapes, sneaking at
//! edges).
//!
//! `tools/refgen/src/refgen/ExecRefGen.java` runs the same simulation around the real upstream
//! Baritone, operation for operation; keep the two in sync. The simulation is not meant to be
//! Minecraft: it only has to be the same on both sides, and good enough to walk, jump, fall,
//! break and place.
//!
//! A tick: Baritone's tick (`on_tick`), then the background work the processes started during
//! it (rescans), then the player's tick (`on_player_update(Pre)`, input, jump, movement with
//! stepping up, the rotation sent to the server, `on_player_update(Post)`), all while holding
//! `pathPlanLock`; then the harness waits for a running calculation to finish, applies the
//! blocks the game mode broke and placed during the tick, and loads the chunks around the
//! player (if the scenario loads chunks). Holding the lock and deferring the world changes
//! keep the calculation thread from racing the tick.

use std::any::Any;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use baritone::Baritone;
use baritone::api::event::events::PathEvent;
use baritone::api::event::events::rotation_move_event::{self, RotationMoveEvent};
use baritone::api::event::events::r#type::EventState;
use baritone::api::utils::BetterBlockPos;
use baritone::api::utils::i_player_controller::{
    ContainerInput, GameType, IPlayerController, InteractionResult,
};
use baritone::api::utils::input::Input;
use baritone::behavior::look::ForkableRandom;
use baritone::host::{Chunk, Climbable, Half, InteractionHand, ItemStack, Openable, Player, World};
use baritone::java::max_f64;
use baritone::java::min_f64;
use baritone::mc::{Aabb, BlockHitResult, Direction, Vec3, mth};
use baritone::utils::ToolSet;

use super::TABLE;

// region game mode

/// A simplified `MultiPlayerGameMode`: destroy progress per tick is `ToolSet.calculateSpeedVsBlock`
/// of the held item, a block breaks when the progress reaches 1 (at once when the first tick
/// does), then the next one waits 5 ticks. Right clicking with a throwaway block places it.
#[derive(Debug, Default)]
pub struct GameMode {
    /// `isDestroying` (what Baritone's `isHittingBlock` accessor reads and writes)
    pub destroying: bool,
    pub destroy_pos: BetterBlockPos,
    pub destroy_progress: f64,
    pub destroy_delay: i32,
    /// Block changes made this tick, applied after it.
    pub pending: Vec<(BetterBlockPos, u32)>,
    /// What Baritone asked for this tick.
    pub actions: Vec<String>,
}

impl GameMode {
    fn speed(player: &Player, state: &baritone::host::BlockState) -> f64 {
        ToolSet::calculate_speed_vs_block(player.get_item_in_hand(InteractionHand::MainHand), state)
    }

    fn destroy(&mut self, pos: BetterBlockPos) {
        self.pending.push((pos, TABLE.air().id));
    }

    fn start_destroy_block(&mut self, player: &Player, world: &World, pos: BetterBlockPos) -> bool {
        if !self.destroying || pos != self.destroy_pos {
            let state = world.get_block_state(pos);
            let not_air = !state.air;
            if not_air && Self::speed(player, state) >= 1.0 {
                self.destroy(pos);
            } else {
                self.destroying = true;
                self.destroy_pos = pos;
                self.destroy_progress = 0.0;
            }
        }
        true
    }

    fn continue_destroy_block(
        &mut self,
        player: &Player,
        world: &World,
        pos: BetterBlockPos,
    ) -> bool {
        if self.destroy_delay > 0 {
            self.destroy_delay -= 1;
            return true;
        }
        if self.destroying && pos == self.destroy_pos {
            let state = world.get_block_state(pos);
            if state.air {
                self.destroying = false;
                return false;
            }
            self.destroy_progress += Self::speed(player, state);
            if self.destroy_progress >= 1.0 {
                self.destroying = false;
                self.destroy(pos);
                self.destroy_progress = 0.0;
                self.destroy_delay = 5;
            }
            return true;
        }
        self.start_destroy_block(player, world, pos)
    }

    fn place(
        &mut self,
        player: &mut Player,
        world: &World,
        hit: &BlockHitResult,
    ) -> InteractionResult {
        let selected = player.inventory.selected;
        let item = player.inventory.get_item(selected);
        // doors, gates and trapdoors open and close, unless sneaking with something in hand
        let clicked = world.get_block_state(hit.get_block_pos());
        if clicked.properties.contains_key("open") && !(player.crouching && !item.is_empty()) {
            let pos = hit.get_block_pos();
            self.pending.push((pos, toggled(clicked)));
            // a door's other half opens with it
            if clicked.openable == Some(Openable::Door) {
                let other = if clicked.half == Half::Top {
                    pos.below()
                } else {
                    pos.above()
                };
                let other_state = world.get_block_state(other);
                if other_state.name == clicked.name {
                    self.pending.push((other, toggled(other_state)));
                }
            }
            return InteractionResult::Success;
        }
        // containers open, unless sneaking with something in hand
        if CONTAINERS.contains(&clicked.name.as_str()) && !(player.crouching && !item.is_empty()) {
            player.container_open = true;
            return InteractionResult::Success;
        }
        // bone meal ages what has an age by 3, up to its maximum
        if item.is_item("minecraft:bone_meal") {
            let Some(now) = clicked
                .properties
                .get("age")
                .map(|a| a.parse::<i32>().unwrap())
            else {
                return InteractionResult::Pass;
            };
            let max = TABLE
                .get_possible_states(&clicked.name)
                .map(|s| s.properties["age"].parse::<i32>().unwrap())
                .max()
                .unwrap();
            if now >= max {
                return InteractionResult::Pass;
            }
            let aged = with_property(clicked, "age", &(now + 3).min(max).to_string());
            self.pending.push((hit.get_block_pos(), aged));
            shrink(player, selected);
            return InteractionResult::Success;
        }
        // seeds and the like plant their crop against the clicked face, even inside the player
        if let Some(&(_, plant)) = PLANTS.iter().find(|(seed, _)| item.is_item(seed)) {
            let target = if clicked.replaceable {
                hit.get_block_pos()
            } else {
                hit.get_block_pos().relative(hit.get_direction())
            };
            if !world.get_block_state(target).replaceable {
                return InteractionResult::Fail;
            }
            self.pending
                .push((target, TABLE.get_default_state(plant).unwrap().id));
            shrink(player, selected);
            return InteractionResult::Success;
        }
        let Some(block) = TABLE
            .get_default_state(item.get_item())
            .filter(|_| !item.is_empty())
        else {
            return InteractionResult::Pass;
        };
        let block = block.id;
        let target = if clicked.replaceable {
            hit.get_block_pos()
        } else {
            hit.get_block_pos().relative(hit.get_direction())
        };
        if !world.get_block_state(target).replaceable {
            return InteractionResult::Fail;
        }
        let target_box = Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0).move_pos(target);
        if player.bounding_box.intersects(&target_box) {
            return InteractionResult::Fail;
        }
        self.pending.push((target, block));
        shrink(player, selected);
        InteractionResult::Success
    }
}

/// Blocks whose menu opens on a right click.
const CONTAINERS: [&str; 6] = [
    "minecraft:crafting_table",
    "minecraft:furnace",
    "minecraft:blast_furnace",
    "minecraft:chest",
    "minecraft:trapped_chest",
    "minecraft:ender_chest",
];

/// Items that plant a block other than their own.
const PLANTS: [(&str, &str); 8] = [
    ("minecraft:wheat_seeds", "minecraft:wheat"),
    ("minecraft:carrot", "minecraft:carrots"),
    ("minecraft:potato", "minecraft:potatoes"),
    ("minecraft:beetroot_seeds", "minecraft:beetroots"),
    ("minecraft:nether_wart", "minecraft:nether_wart"),
    ("minecraft:cocoa_beans", "minecraft:cocoa"),
    ("minecraft:melon_seeds", "minecraft:melon_stem"),
    ("minecraft:pumpkin_seeds", "minecraft:pumpkin_stem"),
];

/// Takes one item from the selected stack.
fn shrink(player: &mut Player, selected: i32) {
    let stack = &mut player.inventory.items[selected as usize];
    stack.count -= 1;
    if stack.count <= 0 {
        *stack = ItemStack::empty();
    }
}

/// The same block with `property` set to `value`.
fn with_property(state: &baritone::host::BlockState, property: &str, value: &str) -> u32 {
    let mut properties = state.properties.clone();
    properties.insert(property.to_owned(), value.to_owned());
    TABLE
        .iter()
        .find(|s| s.name == state.name && s.properties == properties)
        .unwrap()
        .id
}

/// The same block with its `open` property flipped.
fn toggled(state: &baritone::host::BlockState) -> u32 {
    let open = state.properties["open"] == "true";
    with_property(state, "open", &(!open).to_string())
}

/// Blocks the simulation climbs: ladders, vines, weeping and twisting vines, scaffolding.
fn climbable(state: &baritone::host::BlockState) -> bool {
    state.climbable.is_some()
}

/// The host side of `IPlayerController`, over a shared `GameMode`.
pub struct Controller(pub Arc<Mutex<GameMode>>);

impl Controller {
    fn game_mode(&self) -> MutexGuard<'_, GameMode> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn pos(p: BetterBlockPos) -> String {
    format!("{},{},{}", p.x, p.y, p.z)
}

impl IPlayerController for Controller {
    fn sync_held_item(&mut self, player: &mut Player) {
        let selected = player.inventory.selected;
        self.game_mode().actions.push(format!("sync {selected}"));
    }

    fn has_broken_block(&self) -> bool {
        !self.game_mode().destroying
    }

    fn on_player_damage_block(
        &mut self,
        player: &mut Player,
        world: &mut Arc<World>,
        pos: BetterBlockPos,
        side: Direction,
    ) -> bool {
        let mut gm = self.game_mode();
        let result = gm.continue_destroy_block(player, world, pos);
        gm.actions.push(format!(
            "damage {} {} {result}",
            self::pos(pos),
            side.get_name()
        ));
        result
    }

    fn reset_block_removing(&mut self, _player: &mut Player, _world: &mut Arc<World>) {
        let mut gm = self.game_mode();
        gm.destroying = false;
        gm.destroy_progress = 0.0;
        gm.actions.push("reset".to_owned());
    }

    fn window_click(
        &mut self,
        player: &mut Player,
        window_id: i32,
        slot_id: i32,
        mouse_button: i32,
        input_type: ContainerInput,
    ) {
        assert_eq!(window_id, 0);
        assert_eq!(input_type, ContainerInput::Swap);
        // the inventory menu: hotbar slots are 36-44, main slots 9-35
        let slot = if slot_id >= 36 { slot_id - 36 } else { slot_id };
        player.inventory.swap(slot, mouse_button);
        self.game_mode()
            .actions
            .push(format!("swap {slot_id} {mouse_button}"));
    }

    fn get_game_type(&self) -> GameType {
        GameType::Survival
    }

    fn process_right_click_block(
        &mut self,
        player: &mut Player,
        world: &mut Arc<World>,
        hand: InteractionHand,
        result: &BlockHitResult,
    ) -> InteractionResult {
        let mut gm = self.game_mode();
        let r = match hand {
            InteractionHand::MainHand => gm.place(player, world, result),
            InteractionHand::OffHand => InteractionResult::Pass,
        };
        gm.actions.push(format!(
            "use {hand:?} {} {} {r:?}",
            pos(result.get_block_pos()),
            result.get_direction().get_name()
        ));
        r
    }

    fn process_right_click(
        &mut self,
        _player: &mut Player,
        _world: &mut Arc<World>,
        hand: InteractionHand,
    ) -> InteractionResult {
        self.game_mode().actions.push(format!("use item {hand:?}"));
        InteractionResult::Pass
    }

    fn click_block(
        &mut self,
        player: &mut Player,
        world: &mut Arc<World>,
        loc: BetterBlockPos,
        face: Direction,
    ) -> bool {
        let mut gm = self.game_mode();
        let result = gm.start_destroy_block(player, world, loc);
        gm.actions
            .push(format!("click {} {}", pos(loc), face.get_name()));
        result
    }

    fn set_hitting_block(&mut self, hitting_block: bool) {
        self.game_mode().destroying = hitting_block;
    }

    fn set_destroy_delay(&mut self, destroy_delay: i32) {
        self.game_mode().destroy_delay = destroy_delay;
    }

    fn swing(&mut self, _player: &mut Player, hand: InteractionHand) {
        self.game_mode().actions.push(format!("swing {hand:?}"));
    }

    fn disconnect(&mut self, reason: &str) {
        self.game_mode()
            .actions
            .push(format!("disconnect {reason}"));
    }
}

// endregion

// region movement

/// Standing and crouching player boxes and eye heights (`Player.POSES`).
const WIDTH: f32 = 0.6;
const STANDING_HEIGHT: f32 = 1.8;
const CROUCHING_HEIGHT: f32 = 1.5;

fn player_box(pos: Vec3, crouching: bool) -> Aabb {
    let w = (WIDTH / 2.0) as f64;
    let h = if crouching {
        CROUCHING_HEIGHT
    } else {
        STANDING_HEIGHT
    } as f64;
    Aabb::new(pos.x - w, pos.y, pos.z - w, pos.x + w, pos.y + h, pos.z + w)
}

/// The collision boxes of every block the box may touch, in world coordinates.
fn colliders(world: &World, area: &Aabb) -> Vec<Aabb> {
    let mut boxes = Vec::new();
    for x in mth::floor(area.min_x) - 1..=mth::floor(area.max_x) + 1 {
        for y in mth::floor(area.min_y) - 1..=mth::floor(area.max_y) + 1 {
            for z in mth::floor(area.min_z) - 1..=mth::floor(area.max_z) + 1 {
                let pos = BetterBlockPos::new(x, y, z);
                for b in world
                    .get_block_state(pos)
                    .get_collision_shape(pos)
                    .to_aabbs()
                {
                    boxes.push(b.move_pos(pos));
                }
            }
        }
    }
    boxes
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
    Z,
}

fn min_of(b: &Aabb, axis: Axis) -> f64 {
    match axis {
        Axis::X => b.min_x,
        Axis::Y => b.min_y,
        Axis::Z => b.min_z,
    }
}

fn max_of(b: &Aabb, axis: Axis) -> f64 {
    match axis {
        Axis::X => b.max_x,
        Axis::Y => b.max_y,
        Axis::Z => b.max_z,
    }
}

/// How far `bb` can move by `d` along `axis` before it hits one of `boxes`.
fn collide_axis(axis: Axis, bb: &Aabb, boxes: &[Aabb], mut d: f64) -> f64 {
    if d.abs() < 1.0E-7 {
        return 0.0;
    }
    for b in boxes {
        let overlaps = |a: Axis| min_of(b, a) < max_of(bb, a) && max_of(b, a) > min_of(bb, a);
        let others = match axis {
            Axis::X => overlaps(Axis::Y) && overlaps(Axis::Z),
            Axis::Y => overlaps(Axis::X) && overlaps(Axis::Z),
            Axis::Z => overlaps(Axis::X) && overlaps(Axis::Y),
        };
        if !others {
            continue;
        }
        if d > 0.0 {
            if min_of(b, axis) >= max_of(bb, axis) - 1.0E-7 {
                d = min_f64(d, min_of(b, axis) - max_of(bb, axis));
            }
        } else if max_of(b, axis) <= min_of(bb, axis) + 1.0E-7 {
            d = max_f64(d, max_of(b, axis) - min_of(bb, axis));
        }
    }
    d
}

fn move_box(bb: &Aabb, axis: Axis, d: f64) -> Aabb {
    match axis {
        Axis::X => bb.move_by(d, 0.0, 0.0),
        Axis::Y => bb.move_by(0.0, d, 0.0),
        Axis::Z => bb.move_by(0.0, 0.0, d),
    }
}

/// `Entity.collide`: Y first, then the larger of X and Z.
fn collide(world: &World, bb: &Aabb, delta: Vec3) -> Vec3 {
    let area = Aabb::new(
        min_f64(bb.min_x, bb.min_x + delta.x),
        min_f64(bb.min_y, bb.min_y + delta.y),
        min_f64(bb.min_z, bb.min_z + delta.z),
        max_f64(bb.max_x, bb.max_x + delta.x),
        max_f64(bb.max_y, bb.max_y + delta.y),
        max_f64(bb.max_z, bb.max_z + delta.z),
    );
    let boxes = colliders(world, &area);
    let order = if delta.x.abs() < delta.z.abs() {
        [Axis::Y, Axis::Z, Axis::X]
    } else {
        [Axis::Y, Axis::X, Axis::Z]
    };
    let mut bb = *bb;
    let mut out = [0.0; 3];
    for axis in order {
        let d = match axis {
            Axis::X => delta.x,
            Axis::Y => delta.y,
            Axis::Z => delta.z,
        };
        let d = collide_axis(axis, &bb, &boxes, d);
        bb = move_box(&bb, axis, d);
        out[axis as usize] = d;
    }
    Vec3::new(out[0], out[1], out[2])
}

/// The player's step height (`Attributes.STEP_HEIGHT`).
const STEP: f64 = 0.6;

/// `Entity.collide` with stepping up: a move on the ground that runs into something at most
/// [`STEP`] high goes up, across and back down instead, if that gets further.
fn collide_and_step(world: &World, bb: &Aabb, delta: Vec3, on_ground: bool) -> Vec3 {
    let movement = collide(world, bb, delta);
    let horizontal = movement.x != delta.x || movement.z != delta.z;
    let on_ground_after = on_ground || (movement.y != delta.y && delta.y < 0.0);
    if on_ground_after && horizontal {
        let up = collide(world, bb, Vec3::new(0.0, STEP, 0.0)).y;
        let raised = bb.move_by(0.0, up, 0.0);
        let across = collide(world, &raised, Vec3::new(delta.x, 0.0, delta.z));
        let down = collide(
            world,
            &raised.move_by(across.x, 0.0, across.z),
            Vec3::new(0.0, movement.y - up, 0.0),
        );
        let stepped = Vec3::new(across.x, up + down.y, across.z);
        if stepped.x * stepped.x + stepped.z * stepped.z
            > movement.x * movement.x + movement.z * movement.z
        {
            return stepped;
        }
    }
    movement
}

/// `Entity.canFallAtLeast`: nothing below the box within `min_height`.
fn can_fall_at_least(world: &World, bb: &Aabb, dx: f64, dz: f64, min_height: f64) -> bool {
    let area = Aabb::new(
        bb.min_x + 1.0E-7 + dx,
        bb.min_y - min_height - 1.0E-7,
        bb.min_z + 1.0E-7 + dz,
        bb.max_x - 1.0E-7 + dx,
        bb.min_y,
        bb.max_z - 1.0E-7 + dz,
    );
    !colliders(world, &area).iter().any(|b| b.intersects(&area))
}

/// `Player.maybeBackOffFromEdge`, with a step height of 0.6.
fn back_off_from_edge(world: &World, bb: &Aabb, delta: Vec3) -> Vec3 {
    let max_down_step = 0.6f32 as f64;
    let mut dx = delta.x;
    let mut dz = delta.z;
    let step_x = dx.signum_java() * 0.05;
    let step_z = dz.signum_java() * 0.05;
    while dx != 0.0 && can_fall_at_least(world, bb, dx, 0.0, max_down_step) {
        if dx.abs() <= 0.05 {
            dx = 0.0;
            break;
        }
        dx -= step_x;
    }
    while dz != 0.0 && can_fall_at_least(world, bb, 0.0, dz, max_down_step) {
        if dz.abs() <= 0.05 {
            dz = 0.0;
            break;
        }
        dz -= step_z;
    }
    while dx != 0.0 && dz != 0.0 && can_fall_at_least(world, bb, dx, dz, max_down_step) {
        if dx.abs() <= 0.05 {
            dx = 0.0;
        } else {
            dx -= step_x;
        }
        if dz.abs() <= 0.05 {
            dz = 0.0;
        } else {
            dz -= step_z;
        }
    }
    Vec3::new(dx, delta.y, dz)
}

trait SignumJava {
    fn signum_java(self) -> f64;
}

impl SignumJava for f64 {
    /// `Math.signum(double)`: 0 stays 0.
    fn signum_java(self) -> f64 {
        if self == 0.0 || self.is_nan() {
            self
        } else if self > 0.0 {
            1.0
        } else {
            -1.0
        }
    }
}

/// `Entity.getInputVector`
fn input_vector(input: Vec3, speed: f32, y_rot: f32) -> Vec3 {
    let length = input.length_sqr();
    if length < 1.0E-7 {
        return Vec3::ZERO;
    }
    let movement = if length > 1.0 {
        // Vec3.normalize
        let dist = (input.x * input.x + input.y * input.y + input.z * input.z).sqrt();
        if dist < 1.0E-5f32 as f64 {
            Vec3::ZERO
        } else {
            Vec3::new(input.x / dist, input.y / dist, input.z / dist)
        }
    } else {
        input
    }
    .scale(speed as f64);
    let sin = mth::sin((y_rot * (std::f64::consts::PI / 180.0) as f32) as f64);
    let cos = mth::cos((y_rot * (std::f64::consts::PI / 180.0) as f32) as f64);
    Vec3::new(
        movement.x * cos as f64 - movement.z * sin as f64,
        movement.y,
        movement.z * cos as f64 + movement.x * sin as f64,
    )
}

// endregion

/// The upstream class name of a process.
pub fn process_class(process: &dyn Any) -> &'static str {
    use baritone::process::*;
    if process.is::<BackfillProcess>() {
        "BackfillProcess"
    } else if process.is::<CustomGoalProcess>() {
        "CustomGoalProcess"
    } else if process.is::<ExploreProcess>() {
        "ExploreProcess"
    } else if process.is::<FarmProcess>() {
        "FarmProcess"
    } else if process.is::<GetToBlockProcess>() {
        "GetToBlockProcess"
    } else if process.is::<InventoryPauserProcess>() {
        "InventoryPauserProcess"
    } else if process.is::<MineProcess>() {
        "MineProcess"
    } else {
        panic!("unknown process")
    }
}

/// What happened in a tick, for comparing runs.
#[derive(Clone, Debug, PartialEq)]
pub struct TickRecord {
    pub position: [u64; 3],
    pub delta_movement: [u64; 3],
    pub rotation: [u32; 2],
    pub on_ground: bool,
    pub crouching: bool,
    pub sprinting: bool,
    pub selected: i32,
    /// The inputs Baritone forces, in `Input` order.
    pub inputs: Vec<(Input, bool)>,
    pub baritone_input: bool,
    pub actions: Vec<String>,
    pub events: Vec<PathEvent>,
    /// The current path's position and length, if pathing.
    pub path: Option<(i32, usize)>,
    /// The class of the current movement, if pathing and not past the end.
    pub movement: Option<&'static str>,
    /// The most recent pathing command: its type and goal.
    pub command: Option<String>,
    /// The class of the process that had control this tick.
    pub in_control: Option<&'static str>,
}

pub struct Sim {
    pub baritone: Baritone,
    pub game_mode: Arc<Mutex<GameMode>>,
    /// The shift key of the last input (`isShiftKeyDown`), which decides crouching.
    prev_shift: bool,
    /// `LivingEntity.noJumpDelay`
    jump_delay: i32,
    /// The rotation last sent to the server.
    last_sent: Option<(f32, f32)>,
    pub ticks: u32,
    /// Every chunk the world can load, and the radius around the player's chunk that loads
    /// after every tick.
    pub loading: Option<(World, i32)>,
}

impl Sim {
    pub fn new(world: World, mut player: Player, seed: i64) -> Self {
        let game_mode = Arc::new(Mutex::new(GameMode::default()));
        let mut baritone = Baritone::with_look_random(
            Box::new(Controller(Arc::clone(&game_mode))),
            ForkableRandom::with_seed(seed),
        );
        player.old_position = player.position;
        player.bounding_box = player_box(player.position, player.crouching);
        baritone.set_player(Some(player));
        baritone.set_world(Some(Arc::new(world)));
        Self {
            baritone,
            game_mode,
            prev_shift: false,
            jump_delay: 0,
            last_sent: None,
            ticks: 0,
            loading: None,
        }
    }

    /// Loads the chunks of `source` within `radius` of the player's chunk after every tick.
    pub fn load_chunks_around(&mut self, source: World, radius: i32) {
        self.loading = Some((source, radius));
    }

    fn load_chunks(&mut self) {
        let Some((source, radius)) = &self.loading else {
            return;
        };
        let feet = self.player().block_position();
        let (cx, cz) = (feet.x >> 4, feet.z >> 4);
        let world = self.baritone.world_mut().unwrap();
        for x in cx - radius..=cx + radius {
            for z in cz - radius..=cz + radius {
                if let Some(chunk) = source.get_chunk(x, z)
                    && !world.has_chunk(x, z)
                {
                    world.load_chunk(x, z, Chunk::clone(chunk)).unwrap();
                }
            }
        }
    }

    pub fn player(&self) -> &Player {
        self.baritone.player().unwrap()
    }

    pub fn world(&self) -> &Arc<World> {
        self.baritone.world().unwrap()
    }

    fn game_mode(&self) -> MutexGuard<'_, GameMode> {
        self.game_mode
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn tick(&mut self) -> TickRecord {
        let plan_lock = self.baritone.get_pathing_behavior().path_plan_lock();
        {
            let _plan = plan_lock.lock();
            self.baritone.on_tick();
            // the background work processes started during the tick
            self.baritone.get_executor().wait_until_idle();
            self.player_tick();
        }
        // let a calculation started this tick finish and take effect
        let started = Instant::now();
        while self
            .baritone
            .get_pathing_behavior()
            .get_in_progress()
            .is_some()
        {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "calculation did not finish"
            );
            std::thread::sleep(Duration::from_micros(200));
        }
        let pending = std::mem::take(&mut self.game_mode().pending);
        let world = self.baritone.world_mut().unwrap();
        for (pos, id) in pending {
            world.set_block(pos.x, pos.y, pos.z, id).unwrap();
        }
        self.load_chunks();
        self.ticks += 1;
        self.record()
    }

    fn player_tick(&mut self) {
        let world = Arc::clone(self.world());
        let player = self.baritone.player_mut().unwrap();
        // Entity.setOldPosAndRot
        player.old_position = player.position;

        let player = self.baritone.player().unwrap();
        let crouching = self.prev_shift;
        let input = if player.baritone_input {
            self.baritone
                .get_input_override_handler()
                .player_movement_input()
        } else {
            Default::default()
        };
        self.prev_shift = input.key_presses.shift;
        let sprint_key = self
            .baritone
            .on_player_sprint_state()
            .unwrap_or(input.key_presses.sprint);
        let sprinting = sprint_key && input.forward_impulse > 1.0E-5 && !crouching;
        let xxa = input.left_impulse * 0.98f32;
        let zza = input.forward_impulse * 0.98f32;

        let mut jump_event =
            RotationMoveEvent::new(rotation_move_event::Type::Jump, player.y_rot, player.x_rot);
        self.baritone.on_player_rotation_move(&mut jump_event);
        let mut motion_event = RotationMoveEvent::new(
            rotation_move_event::Type::MotionUpdate,
            player.y_rot,
            player.x_rot,
        );
        self.baritone.on_player_rotation_move(&mut motion_event);

        let player = self.baritone.player_mut().unwrap();
        player.crouching = crouching;
        player.sprinting = sprinting;
        player.eye_height = if crouching {
            Player::CROUCHING_EYE_HEIGHT
        } else {
            Player::STANDING_EYE_HEIGHT
        };
        let mut bb = player_box(player.position, crouching);
        let mut vel = player.delta_movement;

        // LivingEntity.aiStep: jumping
        if self.jump_delay > 0 {
            self.jump_delay -= 1;
        }
        if input.key_presses.jump {
            if player.on_ground && self.jump_delay == 0 {
                vel = Vec3::new(vel.x, max_f64(0.42f32 as f64, vel.y), vel.z);
                if sprinting {
                    let angle = jump_event.get_yaw() * (std::f64::consts::PI / 180.0) as f32;
                    vel = vel.add(
                        -mth::sin(angle as f64) as f64 * 0.2,
                        0.0,
                        mth::cos(angle as f64) as f64 * 0.2,
                    );
                }
                self.jump_delay = 10;
            }
        } else {
            self.jump_delay = 0;
        }

        // LivingEntity.travelInAir
        let block_friction = if player.on_ground { 0.6f32 } else { 1.0f32 };
        let speed = if player.on_ground {
            let base = if sprinting { 0.13f32 } else { 0.1f32 };
            base * (0.21600002f32 / (block_friction * block_friction * block_friction))
        } else if sprinting {
            0.025999999f32
        } else {
            0.02f32
        };
        vel = vel.add_vec(input_vector(
            Vec3::new(xxa as f64, 0.0, zza as f64),
            speed,
            motion_event.get_yaw(),
        ));
        // LivingEntity.handleOnClimbable
        let in_block = world.get_block_state(player.block_position());
        if climbable(in_block) {
            let max = 0.15f32 as f64;
            let mut yd = max_f64(vel.y, -max);
            if yd < 0.0
                && in_block.climbable != Some(Climbable::Scaffolding)
                && input.key_presses.shift
            {
                yd = 0.0;
            }
            vel = Vec3::new(
                mth::clamp(vel.x, -max, max),
                yd,
                mth::clamp(vel.z, -max, max),
            );
        }

        // Entity.move
        let mut delta = vel;
        if crouching && player.on_ground && delta.y <= 0.0 {
            delta = back_off_from_edge(&world, &bb, delta);
        }
        let movement = collide_and_step(&world, &bb, delta, player.on_ground);
        let x_collision = movement.x != delta.x;
        let y_collision = movement.y != delta.y;
        let z_collision = movement.z != delta.z;
        player.position = player.position.add_vec(movement);
        bb = bb.move_vec(movement);
        player.horizontal_collision = x_collision || z_collision;
        player.on_ground = y_collision && delta.y < 0.0;
        vel = Vec3::new(
            if x_collision { 0.0 } else { vel.x },
            if y_collision { 0.0 } else { vel.y },
            if z_collision { 0.0 } else { vel.z },
        );
        // climbing: pushing against a wall or jumping while in a climbable block
        if (player.horizontal_collision || input.key_presses.jump)
            && climbable(world.get_block_state(player.block_position()))
        {
            vel = Vec3::new(vel.x, 0.2, vel.z);
        }
        let friction = block_friction * 0.91f32;
        vel = Vec3::new(
            vel.x * friction as f64,
            (vel.y - 0.08) * 0.98f32 as f64,
            vel.z * friction as f64,
        );
        player.delta_movement = vel;
        player.bounding_box = bb;

        // isInWall: the eyes are inside a block's collision box
        let eye = player.get_eye_position();
        let eye_block = BetterBlockPos::from_f64(eye.x, eye.y, eye.z);
        player.in_wall = world
            .get_block_state(eye_block)
            .get_collision_shape(eye_block)
            .to_aabbs()
            .any(|b| b.move_pos(eye_block).contains(eye.x, eye.y, eye.z));

        // MixinClientPlayerEntity.onPreUpdate: after super.tick(), so after the move
        self.baritone.on_player_update(EventState::Pre);

        // Minecraft.tick: LocalPlayer.sendChanges -> sendPosition
        let player = self.baritone.player().unwrap();
        let rotation = (player.y_rot, player.x_rot);
        if self.last_sent != Some(rotation) {
            self.last_sent = Some(rotation);
            self.baritone.on_send_rotation(rotation.0, rotation.1);
        }

        self.baritone.on_player_update(EventState::Post);
    }

    fn record(&mut self) -> TickRecord {
        let actions = std::mem::take(&mut self.game_mode().actions);
        let events = self.baritone.take_path_events();
        let player = self.player();
        let bits = |v: Vec3| [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()];
        TickRecord {
            position: bits(player.position),
            delta_movement: bits(player.delta_movement),
            rotation: [player.y_rot.to_bits(), player.x_rot.to_bits()],
            on_ground: player.on_ground,
            crouching: player.crouching,
            sprinting: player.sprinting,
            selected: player.inventory.selected,
            inputs: self
                .baritone
                .get_input_override_handler()
                .input_force_states()
                .iter()
                .map(|(&k, &v)| (k, v))
                .collect(),
            baritone_input: player.baritone_input,
            actions,
            events,
            path: self
                .baritone
                .get_pathing_behavior()
                .with_current(|current| current.map(|c| (c.get_position(), c.get_path().length()))),
            movement: self
                .baritone
                .get_pathing_behavior()
                .with_current(|current| {
                    let current = current?;
                    let movements = current.get_path().movements();
                    let position = usize::try_from(current.get_position()).ok()?;
                    movements.get(position).map(|m| m.class_name())
                }),
            command: self
                .baritone
                .get_pathing_control_manager()
                .most_recent_command()
                .map(|c| {
                    let goal = c
                        .goal
                        .as_ref()
                        .map_or_else(|| "null".to_owned(), ToString::to_string);
                    format!("{} {goal}", c.command_type.name())
                }),
            in_control: self
                .baritone
                .get_pathing_control_manager()
                .most_recent_in_control()
                .map(|index| process_class(self.baritone.process(index).as_any())),
        }
    }

    /// Changes a block between ticks.
    pub fn edit(&mut self, pos: BetterBlockPos, id: u32) {
        let world = self.baritone.world_mut().unwrap();
        world.set_block(pos.x, pos.y, pos.z, id).unwrap();
    }

    /// Moves the player between ticks, like a server correction.
    pub fn teleport(&mut self, position: Vec3) {
        let player = self.baritone.player_mut().unwrap();
        player.position = position;
        player.bounding_box = player_box(position, player.crouching);
        player.delta_movement = Vec3::ZERO;
    }

    pub fn feet(&self) -> BetterBlockPos {
        use baritone::api::utils::IPlayerContext;
        self.baritone.get_player_context().player_feet()
    }
}
