// Ported from baritone src/main/java/baritone/Baritone.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// One `Baritone` per player, driven by the host. It owns what upstream's `Baritone` wires
// together (the player context, the behaviors, the pathing control manager, the processes)
// and exposes the client events the mixins fire as methods:
//
// - `on_tick` at the start of the client tick (`MixinMinecraft.runTick`), with the player,
//   world of this tick already set;
// - during the player's `aiStep`: `player_movement_input` (through the input override handler)
//   for the movement input while the player's `baritone_input` is set,
//   `on_player_sprint_state` when the client reads the sprint key, and
//   `on_player_rotation_move` when it turns movement input into motion (jumping and
//   `moveRelative`);
// - `on_player_update(Pre)` at the end of `LocalPlayer.tick`'s `super.tick()`, so after the
//   player moved: the look behavior sets the player's rotation here;
// - `on_send_rotation` for every rotation `LocalPlayer.sendChanges` sends to the server, then
//   `on_player_update(Post)`;
// - `on_ride_tick` at the start of `LocalPlayer.rideTick`;
// - `on_world_event` when the client changes worlds.
//
// Baritone writes back to the player it was given (selected slot, sprinting, flying, rotation,
// `baritone_input`) and acts through the host's `IPlayerController`. The mixins' other changes
// are the host's: no `mayfly` and no elytra start while `is_pathing()`, key bindings handled
// even with a screen open while `is_pathing()` (`passEvents`), and no auto jump while
// pathing (which Baritone does through `Options::auto_jump`; the client copies the option into
// the player in `sendPosition`, so a change takes effect on the next tick's move).
//
// The world provider is not ported; of its cache, only which chunks are cached is kept
// (`cached_world`, for the explore process), and the host has no chunk events, so `on_tick`
// marks the loaded chunks cached.
//
// Background work (`Baritone.getExecutor()`) runs on `executor`, one per `Baritone` where
// upstream shares a static pool, so a host (or a test) can wait for this Baritone's work.
//
// The other entities are ignored, as if there were none.
//
// Not ported: the game directory, the command manager, the selection manager, `openClick`,
// the builder, the follow and the elytra process.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use crate::api::event::events::tick_event;
use crate::api::event::events::r#type::EventState;
use crate::api::event::events::{PathEvent, RotationMoveEvent};
use crate::api::process::IBaritoneProcess;
use crate::api::utils::{IPlayerContext, IPlayerController};
use crate::behavior::look::ForkableRandom;
use crate::behavior::{InventoryBehavior, LookBehavior, PathingBehavior};
use crate::cache::CachedWorld;
use crate::event::GameEventHandler;
use crate::host::{Player, World};
use crate::java::Executor;
use crate::process::{
    BackfillProcess, CustomGoalProcess, ExploreProcess, FarmProcess, GetToBlockProcess,
    InventoryPauserProcess, MineProcess,
};
use crate::utils::player::BaritonePlayerContext;
use crate::utils::{BlockStateInterface, InputOverrideHandler, PathingControlManager};

pub struct Baritone {
    pub(crate) game_event_handler: GameEventHandler,

    pub(crate) pathing_behavior: PathingBehavior,
    pub(crate) look_behavior: LookBehavior,
    pub(crate) inventory_behavior: InventoryBehavior,
    pub(crate) input_override_handler: InputOverrideHandler,

    /// Registered processes, in registration order. A process is taken out (`None`) while it
    /// runs `on_tick` or `on_lost_control`, so from there it cannot look itself up or cancel
    /// everything (which reaches every process); no upstream process does either.
    pub(crate) processes: Vec<Option<Box<dyn IBaritoneProcess>>>,

    pub(crate) pathing_control_manager: PathingControlManager,

    pub(crate) player_context: BaritonePlayerContext,

    /// `getWorldProvider().getCurrentWorld().getCachedWorld()`
    pub(crate) cached_world: CachedWorld,

    /// `Baritone.getExecutor()`
    pub(crate) executor: Executor,

    pub bsi: Option<BlockStateInterface>,
}

impl Baritone {
    pub fn new(player_controller: Box<dyn IPlayerController>) -> Self {
        Self::with_look_random(player_controller, ForkableRandom::new())
    }

    /// A Baritone whose look behavior draws its random offsets from `rand`.
    pub fn with_look_random(
        player_controller: Box<dyn IPlayerController>,
        rand: ForkableRandom,
    ) -> Self {
        let mut baritone = Self {
            game_event_handler: GameEventHandler::default(),
            pathing_behavior: PathingBehavior::new(),
            look_behavior: LookBehavior::with_random(rand),
            inventory_behavior: InventoryBehavior::new(),
            input_override_handler: InputOverrideHandler::new(),
            processes: Vec::new(),
            pathing_control_manager: PathingControlManager::new(),
            // Define this before behaviors try and get it, or else it will be null and the builds will fail!
            player_context: BaritonePlayerContext::new(player_controller),
            cached_world: CachedWorld::new(),
            executor: Executor::new(),
            bsi: None,
        };
        baritone.register_process(Box::new(MineProcess::new()));
        baritone.register_process(Box::new(CustomGoalProcess::new())); // very high iq
        baritone.register_process(Box::new(GetToBlockProcess::new()));
        baritone.register_process(Box::new(ExploreProcess::new()));
        baritone.register_process(Box::new(FarmProcess::new()));
        baritone.register_process(Box::new(InventoryPauserProcess::new()));
        baritone.register_process(Box::new(BackfillProcess::new()));
        baritone
    }

    /// `registerProcess(Function<Baritone, T>)`
    pub fn register_process(&mut self, process: Box<dyn IBaritoneProcess>) {
        self.processes.push(Some(process));
        let index = self.processes.len() - 1;
        PathingControlManager::register_process(self, index);
    }

    /// The process at `index` in registration order. Panics while it is running.
    pub fn process(&self, index: usize) -> &dyn IBaritoneProcess {
        self.processes[index]
            .as_deref()
            .expect("the process is running")
    }

    /// Runs `f` on the process at `index`, taken out of the `Baritone` meanwhile. The process
    /// is put back even if `f` panics, so it stays registered like upstream's after an
    /// exception.
    pub fn with_process<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut dyn IBaritoneProcess, &mut Baritone) -> R,
    ) -> R {
        let mut process = self.processes[index]
            .take()
            .expect("the process is running");
        let result = panic::catch_unwind(AssertUnwindSafe(|| f(&mut *process, self)));
        self.processes[index] = Some(process);
        result.unwrap_or_else(|payload| panic::resume_unwind(payload))
    }

    fn find_process<T: 'static>(&self) -> Option<&T> {
        self.processes
            .iter()
            .flatten()
            .find_map(|p| p.as_any().downcast_ref::<T>())
    }

    fn find_process_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.processes
            .iter_mut()
            .flatten()
            .find_map(|p| p.as_any_mut().downcast_mut::<T>())
    }

    /// Runs `f` on the registered process of type `T`, taken out of the `Baritone` meanwhile
    /// (see [`Self::with_process`]), for a process method that reaches the `Baritone`.
    pub(crate) fn with_process_of<T: 'static, R>(
        &mut self,
        f: impl FnOnce(&mut T, &mut Baritone) -> R,
    ) -> R {
        let index = self
            .processes
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.as_any().is::<T>()))
            .expect("the process is registered and not running");
        self.with_process(index, |process, baritone| {
            let process = process
                .as_any_mut()
                .downcast_mut::<T>()
                .expect("the process at its index");
            f(process, baritone)
        })
    }

    /// `Baritone.getExecutor()`: where processes run their background work.
    pub fn get_executor(&self) -> &Executor {
        &self.executor
    }

    /// `getWorldProvider().getCurrentWorld().getCachedWorld()`
    pub fn get_cached_world(&self) -> &CachedWorld {
        &self.cached_world
    }

    pub fn get_pathing_control_manager(&self) -> &PathingControlManager {
        &self.pathing_control_manager
    }

    pub fn get_input_override_handler(&self) -> &InputOverrideHandler {
        &self.input_override_handler
    }

    pub fn get_custom_goal_process(&self) -> &CustomGoalProcess {
        self.find_process()
            .expect("the custom goal process is running")
    }

    pub fn get_custom_goal_process_mut(&mut self) -> &mut CustomGoalProcess {
        self.find_process_mut()
            .expect("the custom goal process is running")
    }

    pub fn get_player_context(&self) -> &BaritonePlayerContext {
        &self.player_context
    }

    pub fn get_inventory_behavior(&self) -> &InventoryBehavior {
        &self.inventory_behavior
    }

    pub fn get_look_behavior(&self) -> &LookBehavior {
        &self.look_behavior
    }

    pub fn get_look_behavior_mut(&mut self) -> &mut LookBehavior {
        &mut self.look_behavior
    }

    pub fn get_inventory_pauser_process(&self) -> &InventoryPauserProcess {
        self.find_process()
            .expect("the inventory pauser process is running")
    }

    pub fn get_inventory_pauser_process_mut(&mut self) -> &mut InventoryPauserProcess {
        self.find_process_mut()
            .expect("the inventory pauser process is running")
    }

    pub fn get_mine_process(&self) -> &MineProcess {
        self.find_process().expect("the mine process is running")
    }

    pub fn get_get_to_block_process(&self) -> &GetToBlockProcess {
        self.find_process()
            .expect("the get to block process is running")
    }

    pub fn get_explore_process(&self) -> &ExploreProcess {
        self.find_process().expect("the explore process is running")
    }

    pub fn get_explore_process_mut(&mut self) -> &mut ExploreProcess {
        self.find_process_mut()
            .expect("the explore process is running")
    }

    pub fn get_farm_process(&self) -> &FarmProcess {
        self.find_process().expect("the farm process is running")
    }

    pub fn get_pathing_behavior(&self) -> &PathingBehavior {
        &self.pathing_behavior
    }

    // region host state

    /// Sets the player, `None` when there is none (in menus).
    pub fn set_player(&mut self, player: Option<Player>) {
        self.player_context.player = player;
    }

    /// The player as Baritone left it: the host applies what Baritone changed.
    pub fn player(&self) -> Option<&Player> {
        self.player_context.player.as_ref()
    }

    pub fn player_mut(&mut self) -> Option<&mut Player> {
        self.player_context.player.as_mut()
    }

    /// Sets the world, `None` when there is none.
    pub fn set_world(&mut self, world: Option<Arc<World>>) {
        self.player_context.world = world;
    }

    pub fn world(&self) -> Option<&Arc<World>> {
        self.player_context.world.as_ref()
    }

    /// The world, for changes (copy on write when a calculation still reads it).
    pub fn world_mut(&mut self) -> Option<&mut World> {
        self.player_context.world.as_mut().map(Arc::make_mut)
    }

    pub fn options_mut(&mut self) -> &mut crate::host::Options {
        self.player_context.options_mut()
    }

    // endregion

    // region events

    /// `TickEvent`, `IN` when there is a player and a world.
    pub fn on_tick(&mut self) {
        // ChunkEvent: upstream caches a chunk when the client loads it
        if let Some(world) = &self.player_context.world {
            for (x, z) in world.loaded_chunks() {
                self.cached_world.queue_for_packing(x, z);
            }
        }
        let event_type = if self.player_context.is_in_world() {
            tick_event::Type::In
        } else {
            tick_event::Type::Out
        };
        GameEventHandler::on_tick(self, event_type);
    }

    /// `PlayerUpdateEvent`
    pub fn on_player_update(&mut self, state: EventState) {
        GameEventHandler::on_player_update(self, state);
    }

    /// `SprintStateEvent`: whether the sprint key is down, if Baritone decides.
    pub fn on_player_sprint_state(&self) -> Option<bool> {
        GameEventHandler::on_player_sprint_state(self)
    }

    /// `RotationMoveEvent`
    pub fn on_player_rotation_move(&self, event: &mut RotationMoveEvent) {
        GameEventHandler::on_player_rotation_move(self, event);
    }

    /// `PacketEvent` for a `ServerboundMovePlayerPacket` with a rotation.
    pub fn on_send_rotation(&mut self, y_rot: f32, x_rot: f32) {
        self.look_behavior
            .on_send_rotation(&mut self.player_context, y_rot, x_rot);
    }

    /// `MixinClientPlayerEntity.updateRidden`: at the start of `LocalPlayer.rideTick`, turns the
    /// player toward the look target so the mount steers.
    pub fn on_ride_tick(&mut self) {
        self.look_behavior.pig(&mut self.player_context);
    }

    /// `WorldEvent`
    pub fn on_world_event(&mut self, state: EventState) {
        GameEventHandler::on_world_event(self, state);
    }

    /// The path events since the last call (`IGameEventListener.onPathEvent`).
    pub fn take_path_events(&mut self) -> Vec<PathEvent> {
        self.game_event_handler.take_path_events()
    }

    // endregion
}
