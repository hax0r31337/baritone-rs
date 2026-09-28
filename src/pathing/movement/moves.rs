// Ported from baritone src/main/java/baritone/pathing/movement/Moves.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::utils::BetterBlockPos;
use crate::mc::Direction;
use crate::pathing::movement::movements::{
    MovementAscend, MovementDescend, MovementDiagonal, MovementDownward, MovementFall,
    MovementParkour, MovementPillar, MovementTraverse,
};
use crate::pathing::movement::{CalculationContext, Movement};
use crate::utils::pathing::MutableMoveResult;

/// An enum of all possible movements attached to all possible directions they could be taken in
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Moves {
    Downward,
    Pillar,
    TraverseNorth,
    TraverseSouth,
    TraverseEast,
    TraverseWest,
    AscendNorth,
    AscendSouth,
    AscendEast,
    AscendWest,
    DescendEast,
    DescendWest,
    DescendNorth,
    DescendSouth,
    DiagonalNortheast,
    DiagonalNorthwest,
    DiagonalSoutheast,
    DiagonalSouthwest,
    ParkourNorth,
    ParkourSouth,
    ParkourEast,
    ParkourWest,
}

/// Offsets and dynamic flags of one move.
struct MoveData {
    x_offset: i32,
    y_offset: i32,
    z_offset: i32,
    dynamic_xz: bool,
    dynamic_y: bool,
}

const fn data(x: i32, y: i32, z: i32, dynamic_xz: bool, dynamic_y: bool) -> MoveData {
    MoveData {
        x_offset: x,
        y_offset: y,
        z_offset: z,
        dynamic_xz,
        dynamic_y,
    }
}

impl Moves {
    /// `Moves.values()`
    pub const VALUES: [Moves; 22] = [
        Moves::Downward,
        Moves::Pillar,
        Moves::TraverseNorth,
        Moves::TraverseSouth,
        Moves::TraverseEast,
        Moves::TraverseWest,
        Moves::AscendNorth,
        Moves::AscendSouth,
        Moves::AscendEast,
        Moves::AscendWest,
        Moves::DescendEast,
        Moves::DescendWest,
        Moves::DescendNorth,
        Moves::DescendSouth,
        Moves::DiagonalNortheast,
        Moves::DiagonalNorthwest,
        Moves::DiagonalSoutheast,
        Moves::DiagonalSouthwest,
        Moves::ParkourNorth,
        Moves::ParkourSouth,
        Moves::ParkourEast,
        Moves::ParkourWest,
    ];

    const fn data(self) -> MoveData {
        match self {
            Moves::Downward => data(0, -1, 0, false, false),
            Moves::Pillar => data(0, 1, 0, false, false),
            Moves::TraverseNorth => data(0, 0, -1, false, false),
            Moves::TraverseSouth => data(0, 0, 1, false, false),
            Moves::TraverseEast => data(1, 0, 0, false, false),
            Moves::TraverseWest => data(-1, 0, 0, false, false),
            Moves::AscendNorth => data(0, 1, -1, false, false),
            Moves::AscendSouth => data(0, 1, 1, false, false),
            Moves::AscendEast => data(1, 1, 0, false, false),
            Moves::AscendWest => data(-1, 1, 0, false, false),
            Moves::DescendEast => data(1, -1, 0, false, true),
            Moves::DescendWest => data(-1, -1, 0, false, true),
            Moves::DescendNorth => data(0, -1, -1, false, true),
            Moves::DescendSouth => data(0, -1, 1, false, true),
            Moves::DiagonalNortheast => data(1, 0, -1, false, true),
            Moves::DiagonalNorthwest => data(-1, 0, -1, false, true),
            Moves::DiagonalSoutheast => data(1, 0, 1, false, true),
            Moves::DiagonalSouthwest => data(-1, 0, 1, false, true),
            Moves::ParkourNorth => data(0, 0, -4, true, true),
            Moves::ParkourSouth => data(0, 0, 4, true, true),
            Moves::ParkourEast => data(4, 0, 0, true, true),
            Moves::ParkourWest => data(-4, 0, 0, true, true),
        }
    }

    #[inline]
    pub const fn dynamic_xz(self) -> bool {
        self.data().dynamic_xz
    }

    #[inline]
    pub const fn dynamic_y(self) -> bool {
        self.data().dynamic_y
    }

    #[inline]
    pub const fn x_offset(self) -> i32 {
        self.data().x_offset
    }

    #[inline]
    pub const fn y_offset(self) -> i32 {
        self.data().y_offset
    }

    #[inline]
    pub const fn z_offset(self) -> i32 {
        self.data().z_offset
    }

    pub fn apply0(self, context: &CalculationContext, src: BetterBlockPos) -> Movement {
        match self {
            Moves::Downward => MovementDownward::new(src, src.below()),
            Moves::Pillar => MovementPillar::new(src, src.above()),
            Moves::TraverseNorth => MovementTraverse::new(src, src.north()),
            Moves::TraverseSouth => MovementTraverse::new(src, src.south()),
            Moves::TraverseEast => MovementTraverse::new(src, src.east()),
            Moves::TraverseWest => MovementTraverse::new(src, src.west()),
            Moves::AscendNorth => {
                MovementAscend::new(src, BetterBlockPos::new(src.x, src.y + 1, src.z - 1))
            }
            Moves::AscendSouth => {
                MovementAscend::new(src, BetterBlockPos::new(src.x, src.y + 1, src.z + 1))
            }
            Moves::AscendEast => {
                MovementAscend::new(src, BetterBlockPos::new(src.x + 1, src.y + 1, src.z))
            }
            Moves::AscendWest => {
                MovementAscend::new(src, BetterBlockPos::new(src.x - 1, src.y + 1, src.z))
            }
            Moves::DescendEast | Moves::DescendWest | Moves::DescendNorth | Moves::DescendSouth => {
                let mut res = MutableMoveResult::new();
                self.apply(context, src.x, src.y, src.z, &mut res);
                if res.y == src.y - 1 {
                    MovementDescend::new(src, BetterBlockPos::new(res.x, res.y, res.z))
                } else {
                    MovementFall::new(src, BetterBlockPos::new(res.x, res.y, res.z))
                }
            }
            Moves::DiagonalNortheast
            | Moves::DiagonalNorthwest
            | Moves::DiagonalSoutheast
            | Moves::DiagonalSouthwest => {
                let mut res = MutableMoveResult::new();
                self.apply(context, src.x, src.y, src.z, &mut res);
                let (dir1, dir2) = match self {
                    Moves::DiagonalNortheast => (Direction::North, Direction::East),
                    Moves::DiagonalNorthwest => (Direction::North, Direction::West),
                    Moves::DiagonalSoutheast => (Direction::South, Direction::East),
                    _ => (Direction::South, Direction::West),
                };
                MovementDiagonal::new(src, dir1, dir2, res.y.wrapping_sub(src.y))
            }
            Moves::ParkourNorth => MovementParkour::cost_pos(context, src, Direction::North),
            Moves::ParkourSouth => MovementParkour::cost_pos(context, src, Direction::South),
            Moves::ParkourEast => MovementParkour::cost_pos(context, src, Direction::East),
            Moves::ParkourWest => MovementParkour::cost_pos(context, src, Direction::West),
        }
    }

    #[inline]
    pub fn apply(
        self,
        context: &CalculationContext,
        x: i32,
        y: i32,
        z: i32,
        result: &mut MutableMoveResult,
    ) {
        match self {
            Moves::DescendEast => MovementDescend::cost(context, x, y, z, x + 1, z, result),
            Moves::DescendWest => MovementDescend::cost(context, x, y, z, x - 1, z, result),
            Moves::DescendNorth => MovementDescend::cost(context, x, y, z, x, z - 1, result),
            Moves::DescendSouth => MovementDescend::cost(context, x, y, z, x, z + 1, result),
            Moves::DiagonalNortheast => {
                MovementDiagonal::cost(context, x, y, z, x + 1, z - 1, result)
            }
            Moves::DiagonalNorthwest => {
                MovementDiagonal::cost(context, x, y, z, x - 1, z - 1, result)
            }
            Moves::DiagonalSoutheast => {
                MovementDiagonal::cost(context, x, y, z, x + 1, z + 1, result)
            }
            Moves::DiagonalSouthwest => {
                MovementDiagonal::cost(context, x, y, z, x - 1, z + 1, result)
            }
            Moves::ParkourNorth => {
                MovementParkour::cost(context, x, y, z, Direction::North, result)
            }
            Moves::ParkourSouth => {
                MovementParkour::cost(context, x, y, z, Direction::South, result)
            }
            Moves::ParkourEast => MovementParkour::cost(context, x, y, z, Direction::East, result),
            Moves::ParkourWest => MovementParkour::cost(context, x, y, z, Direction::West, result),
            _ => {
                if self.dynamic_xz() || self.dynamic_y() {
                    panic!("Movements with dynamic offset must override `apply`");
                }
                result.x = x + self.x_offset();
                result.y = y + self.y_offset();
                result.z = z + self.z_offset();
                result.cost = self.cost(context, x, y, z);
            }
        }
    }

    #[inline]
    pub fn cost(self, context: &CalculationContext, x: i32, y: i32, z: i32) -> f64 {
        match self {
            Moves::Downward => MovementDownward::cost(context, x, y, z),
            Moves::Pillar => MovementPillar::cost(context, x, y, z),
            Moves::TraverseNorth => MovementTraverse::cost(context, x, y, z, x, z - 1),
            Moves::TraverseSouth => MovementTraverse::cost(context, x, y, z, x, z + 1),
            Moves::TraverseEast => MovementTraverse::cost(context, x, y, z, x + 1, z),
            Moves::TraverseWest => MovementTraverse::cost(context, x, y, z, x - 1, z),
            Moves::AscendNorth => MovementAscend::cost(context, x, y, z, x, z - 1),
            Moves::AscendSouth => MovementAscend::cost(context, x, y, z, x, z + 1),
            Moves::AscendEast => MovementAscend::cost(context, x, y, z, x + 1, z),
            Moves::AscendWest => MovementAscend::cost(context, x, y, z, x - 1, z),
            _ => panic!("Movements must override `cost` or `apply`"),
        }
    }

    /// The upstream constant name (`TRAVERSE_NORTH`).
    pub fn name(self) -> &'static str {
        match self {
            Moves::Downward => "DOWNWARD",
            Moves::Pillar => "PILLAR",
            Moves::TraverseNorth => "TRAVERSE_NORTH",
            Moves::TraverseSouth => "TRAVERSE_SOUTH",
            Moves::TraverseEast => "TRAVERSE_EAST",
            Moves::TraverseWest => "TRAVERSE_WEST",
            Moves::AscendNorth => "ASCEND_NORTH",
            Moves::AscendSouth => "ASCEND_SOUTH",
            Moves::AscendEast => "ASCEND_EAST",
            Moves::AscendWest => "ASCEND_WEST",
            Moves::DescendEast => "DESCEND_EAST",
            Moves::DescendWest => "DESCEND_WEST",
            Moves::DescendNorth => "DESCEND_NORTH",
            Moves::DescendSouth => "DESCEND_SOUTH",
            Moves::DiagonalNortheast => "DIAGONAL_NORTHEAST",
            Moves::DiagonalNorthwest => "DIAGONAL_NORTHWEST",
            Moves::DiagonalSoutheast => "DIAGONAL_SOUTHEAST",
            Moves::DiagonalSouthwest => "DIAGONAL_SOUTHWEST",
            Moves::ParkourNorth => "PARKOUR_NORTH",
            Moves::ParkourSouth => "PARKOUR_SOUTH",
            Moves::ParkourEast => "PARKOUR_EAST",
            Moves::ParkourWest => "PARKOUR_WEST",
        }
    }
}

impl fmt::Display for Moves {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
