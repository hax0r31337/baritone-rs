// Ported from Minecraft 26.3 net/minecraft/core/Direction.java (client jar bytecode)

use std::fmt;

use serde::{Deserialize, Serialize};

/// `net.minecraft.core.Direction`. The discriminant is the Java ordinal, which is also the 3D
/// data value. Serialized by its Minecraft name (`"north"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Direction {
    Down = 0,
    Up = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AxisDirection {
    Positive,
    Negative,
}

impl AxisDirection {
    pub fn get_step(self) -> i32 {
        match self {
            AxisDirection::Positive => 1,
            AxisDirection::Negative => -1,
        }
    }
}

impl Direction {
    /// `Direction.values()`, in ordinal order.
    pub const VALUES: [Direction; 6] = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];

    /// `Direction.Plane.HORIZONTAL`
    pub const HORIZONTAL: [Direction; 4] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];

    /// `Direction.Plane.VERTICAL`
    pub const VERTICAL: [Direction; 2] = [Direction::Up, Direction::Down];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    pub fn get_3d_data_value(self) -> i32 {
        self as i32
    }

    pub fn get_2d_data_value(self) -> i32 {
        match self {
            Direction::Down | Direction::Up => -1,
            Direction::South => 0,
            Direction::West => 1,
            Direction::North => 2,
            Direction::East => 3,
        }
    }

    pub fn from_3d_data_value(value: i32) -> Direction {
        Self::VALUES[(value % 6).unsigned_abs() as usize]
    }

    pub fn from_2d_data_value(value: i32) -> Direction {
        const BY_2D: [Direction; 4] = [
            Direction::South,
            Direction::West,
            Direction::North,
            Direction::East,
        ];
        BY_2D[(value % 4).unsigned_abs() as usize]
    }

    pub fn get_opposite(self) -> Direction {
        match self {
            Direction::Down => Direction::Up,
            Direction::Up => Direction::Down,
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
        }
    }

    /// Rotation around the Y axis. Panics for vertical directions, like Java's
    /// `IllegalStateException`.
    pub fn get_clock_wise(self) -> Direction {
        match self {
            Direction::North => Direction::East,
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            Direction::East => Direction::South,
            _ => panic!("Unable to get Y-rotated facing of {self}"),
        }
    }

    /// Rotation around the Y axis. Panics for vertical directions, like Java's
    /// `IllegalStateException`.
    pub fn get_counter_clock_wise(self) -> Direction {
        match self {
            Direction::North => Direction::West,
            Direction::South => Direction::East,
            Direction::West => Direction::South,
            Direction::East => Direction::North,
            _ => panic!("Unable to get CCW facing of {self}"),
        }
    }

    pub fn get_step_x(self) -> i32 {
        match self {
            Direction::West => -1,
            Direction::East => 1,
            _ => 0,
        }
    }

    pub fn get_step_y(self) -> i32 {
        match self {
            Direction::Down => -1,
            Direction::Up => 1,
            _ => 0,
        }
    }

    pub fn get_step_z(self) -> i32 {
        match self {
            Direction::North => -1,
            Direction::South => 1,
            _ => 0,
        }
    }

    /// `getUnitVec3i()` as `(x, y, z)`.
    pub fn get_unit_vec3i(self) -> (i32, i32, i32) {
        (self.get_step_x(), self.get_step_y(), self.get_step_z())
    }

    pub fn get_axis(self) -> Axis {
        match self {
            Direction::Down | Direction::Up => Axis::Y,
            Direction::North | Direction::South => Axis::Z,
            Direction::West | Direction::East => Axis::X,
        }
    }

    pub fn get_axis_direction(self) -> AxisDirection {
        match self {
            Direction::Up | Direction::South | Direction::East => AxisDirection::Positive,
            Direction::Down | Direction::North | Direction::West => AxisDirection::Negative,
        }
    }

    pub fn get_name(self) -> &'static str {
        match self {
            Direction::Down => "down",
            Direction::Up => "up",
            Direction::North => "north",
            Direction::South => "south",
            Direction::West => "west",
            Direction::East => "east",
        }
    }

    pub fn by_name(name: &str) -> Option<Direction> {
        Self::VALUES.into_iter().find(|d| d.get_name() == name)
    }

    /// `getApproximateNearest(double, double, double)`: computed in `float`.
    pub fn get_approximate_nearest(dx: f64, dy: f64, dz: f64) -> Direction {
        Self::get_approximate_nearest_f32(dx as f32, dy as f32, dz as f32)
    }

    /// `getApproximateNearest(float, float, float)`
    pub fn get_approximate_nearest_f32(dx: f32, dy: f32, dz: f32) -> Direction {
        let mut result = Direction::North;
        // Float.MIN_VALUE, the smallest positive float
        let mut highest_dot = f32::from_bits(1);
        for direction in Self::VALUES {
            let dot = dx * direction.get_step_x() as f32
                + dy * direction.get_step_y() as f32
                + dz * direction.get_step_z() as f32;
            if dot > highest_dot {
                highest_dot = dot;
                result = direction;
            }
        }
        result
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_is_involution_and_negates_step() {
        for d in Direction::VALUES {
            let o = d.get_opposite();
            assert_eq!(o.get_opposite(), d);
            assert_eq!(
                o.get_unit_vec3i(),
                (-d.get_step_x(), -d.get_step_y(), -d.get_step_z())
            );
            assert_eq!(o.get_axis(), d.get_axis());
        }
    }

    #[test]
    fn data_values_round_trip() {
        for d in Direction::VALUES {
            assert_eq!(Direction::from_3d_data_value(d.get_3d_data_value()), d);
            assert_eq!(Direction::by_name(d.get_name()), Some(d));
        }
        for d in Direction::HORIZONTAL {
            assert_eq!(Direction::from_2d_data_value(d.get_2d_data_value()), d);
            assert_eq!(d.get_clock_wise().get_counter_clock_wise(), d);
        }
        assert_eq!(Direction::from_3d_data_value(-7), Direction::Up);
    }

    #[test]
    fn clockwise_order_matches_horizontal_plane() {
        for (i, d) in Direction::HORIZONTAL.into_iter().enumerate() {
            assert_eq!(d.get_clock_wise(), Direction::HORIZONTAL[(i + 1) % 4]);
        }
    }
}
