// Ported from baritone src/main/java/baritone/pathing/movement/Movement.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The subclasses are the variants of `MovementKind`; each keeps its own fields in the struct of
// its file under `movements/`, and `Movement` dispatches the overridden methods to them.
// Movements do not hold the `IBaritone`/`IPlayerContext`: execution (phase 4) passes the
// player context in. Missing until phase 4: `update`, `prepared`, `safeToCancel`, `reset`,
// `updateState`, `playerInValidPosition`, `toBreak`/`toPlace`/`toWalkInto` and their caches,
// `toBreakAll`, the `MovementState`.

use rustc_hash::FxHashSet;

use crate::api::pathing::movement::IMovement;
use crate::api::utils::BetterBlockPos;
use crate::mc::Direction;
use crate::pathing::movement::CalculationContext;
use crate::pathing::movement::movements::{
    MovementAscend, MovementDescend, MovementDiagonal, MovementDownward, MovementFall,
    MovementParkour, MovementPillar, MovementTraverse,
};

pub const HORIZONTALS_BUT_ALSO_DOWN_____SO_EVERY_DIRECTION_EXCEPT_UP: [Direction; 5] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
    Direction::Down,
];

/// The subclass of a [`Movement`], with its own fields.
#[derive(Clone, Debug, PartialEq)]
pub enum MovementKind {
    Ascend(MovementAscend),
    Descend(MovementDescend),
    Diagonal(MovementDiagonal),
    Downward(MovementDownward),
    Fall(MovementFall),
    Parkour(MovementParkour),
    Pillar(MovementPillar),
    Traverse(MovementTraverse),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Movement {
    pub(crate) src: BetterBlockPos,

    pub(crate) dest: BetterBlockPos,

    /// The positions that need to be broken before this movement can ensue
    pub(crate) positions_to_break: Box<[BetterBlockPos]>,

    /// The position where we need to place a block before this movement can ensue
    pub(crate) position_to_place: Option<BetterBlockPos>,

    cost: Option<f64>,

    valid_positions_cached: Option<FxHashSet<BetterBlockPos>>,

    calculated_while_loaded: Option<bool>,

    pub(crate) kind: MovementKind,
}

impl Movement {
    /// `Movement(IBaritone, BetterBlockPos, BetterBlockPos, BetterBlockPos[], BetterBlockPos)`;
    /// `position_to_place` is `None` for the constructor without it.
    pub(crate) fn new(
        src: BetterBlockPos,
        dest: BetterBlockPos,
        to_break: Box<[BetterBlockPos]>,
        to_place: Option<BetterBlockPos>,
        kind: MovementKind,
    ) -> Self {
        Self {
            src,
            dest,
            positions_to_break: to_break,
            position_to_place: to_place,
            cost: None,
            valid_positions_cached: None,
            calculated_while_loaded: None,
            kind,
        }
    }

    /// The subclass and its fields.
    pub fn kind(&self) -> &MovementKind {
        &self.kind
    }

    /// The subclass and its fields.
    pub fn kind_mut(&mut self) -> &mut MovementKind {
        &mut self.kind
    }

    /// The upstream class name (`"MovementTraverse"`).
    pub fn class_name(&self) -> &'static str {
        match self.kind {
            MovementKind::Ascend(_) => "MovementAscend",
            MovementKind::Descend(_) => "MovementDescend",
            MovementKind::Diagonal(_) => "MovementDiagonal",
            MovementKind::Downward(_) => "MovementDownward",
            MovementKind::Fall(_) => "MovementFall",
            MovementKind::Parkour(_) => "MovementParkour",
            MovementKind::Pillar(_) => "MovementPillar",
            MovementKind::Traverse(_) => "MovementTraverse",
        }
    }

    /// `getCost()`: panics if the cost was never calculated or set (upstream unboxes `null`).
    pub fn get_cost(&self) -> f64 {
        self.cost.expect("NullPointerException: movement cost")
    }

    /// `getCost(CalculationContext)`
    pub fn get_cost_context(&mut self, context: &CalculationContext) -> f64 {
        match self.cost {
            Some(cost) => cost,
            None => {
                let cost = self.calculate_cost(context);
                self.cost = Some(cost);
                cost
            }
        }
    }

    pub fn calculate_cost(&self, context: &CalculationContext) -> f64 {
        match &self.kind {
            MovementKind::Ascend(_) => MovementAscend::calculate_cost(self, context),
            MovementKind::Descend(_) => MovementDescend::calculate_cost(self, context),
            MovementKind::Diagonal(_) => MovementDiagonal::calculate_cost(self, context),
            MovementKind::Downward(_) => MovementDownward::calculate_cost(self, context),
            MovementKind::Fall(_) => MovementFall::calculate_cost(self, context),
            MovementKind::Parkour(m) => m.calculate_cost(self, context),
            MovementKind::Pillar(_) => MovementPillar::calculate_cost(self, context),
            MovementKind::Traverse(_) => MovementTraverse::calculate_cost(self, context),
        }
    }

    pub fn recalculate_cost(&mut self, context: &CalculationContext) -> f64 {
        self.cost = None;
        self.get_cost_context(context)
    }

    /// `override(double)` (`override` is reserved in Rust)
    pub fn override_cost(&mut self, cost: f64) {
        self.cost = Some(cost);
    }

    fn calculate_valid_positions(&self) -> FxHashSet<BetterBlockPos> {
        match &self.kind {
            MovementKind::Ascend(_) => MovementAscend::calculate_valid_positions(self),
            MovementKind::Descend(_) => MovementDescend::calculate_valid_positions(self),
            MovementKind::Diagonal(_) => MovementDiagonal::calculate_valid_positions(self),
            MovementKind::Downward(_) => MovementDownward::calculate_valid_positions(self),
            MovementKind::Fall(_) => MovementFall::calculate_valid_positions(self),
            MovementKind::Parkour(m) => m.calculate_valid_positions(self),
            MovementKind::Pillar(_) => MovementPillar::calculate_valid_positions(self),
            MovementKind::Traverse(_) => MovementTraverse::calculate_valid_positions(self),
        }
    }

    pub fn get_valid_positions(&mut self) -> &FxHashSet<BetterBlockPos> {
        if self.valid_positions_cached.is_none() {
            self.valid_positions_cached = Some(self.calculate_valid_positions());
        }
        self.valid_positions_cached.as_ref().unwrap()
    }

    pub fn get_src(&self) -> BetterBlockPos {
        self.src
    }

    pub fn get_dest(&self) -> BetterBlockPos {
        self.dest
    }

    /// `BlockPos getDirection()`
    pub fn get_direction(&self) -> BetterBlockPos {
        self.get_dest().subtract(self.get_src())
    }

    pub fn check_loaded_chunk(&mut self, context: &CalculationContext) {
        self.calculated_while_loaded = Some(
            context
                .bsi
                .world_contains_loaded_chunk(self.dest.x, self.dest.z),
        );
    }

    /// Panics if [`Movement::check_loaded_chunk`] never ran (upstream unboxes `null`).
    pub fn calculated_while_loaded(&self) -> bool {
        self.calculated_while_loaded
            .expect("NullPointerException: calculatedWhileLoaded")
    }
}

impl IMovement for Movement {
    fn get_cost(&self) -> f64 {
        Movement::get_cost(self)
    }

    fn calculated_while_loaded(&self) -> bool {
        Movement::calculated_while_loaded(self)
    }

    fn get_src(&self) -> BetterBlockPos {
        self.src
    }

    fn get_dest(&self) -> BetterBlockPos {
        self.dest
    }

    fn get_direction(&self) -> BetterBlockPos {
        Movement::get_direction(self)
    }
}
