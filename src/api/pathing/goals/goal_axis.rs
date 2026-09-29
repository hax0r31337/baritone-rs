// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalAxis.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, GoalYLevel, goal_equals};
use crate::java;
use crate::settings::with_settings;

const SQRT_2_OVER_2: f64 = std::f64::consts::SQRT_2 / 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GoalAxis;

impl GoalAxis {
    pub fn new() -> Self {
        Self
    }
}

impl Goal for GoalAxis {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        y == with_settings(|settings| settings.axis_height)
            && (x == 0 || z == 0 || x.wrapping_abs() == z.wrapping_abs())
    }

    fn heuristic(&self, x0: i32, y: i32, z0: i32) -> f64 {
        let x = x0.wrapping_abs();
        let z = z0.wrapping_abs();

        let shrt = x.min(z);
        let lng = x.max(z);
        let diff = lng.wrapping_sub(shrt);

        let flat_axis_distance = java::min_f64(
            x as f64,
            java::min_f64(z as f64, diff as f64 * SQRT_2_OVER_2),
        );

        let (cost_heuristic, axis_height) =
            with_settings(|settings| (settings.cost_heuristic, settings.axis_height));
        flat_axis_distance * cost_heuristic + GoalYLevel::calculate(axis_height, y)
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for GoalAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GoalAxis")
    }
}
