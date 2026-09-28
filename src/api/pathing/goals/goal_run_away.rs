// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalRunAway.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, GoalXZ, GoalYLevel, goal_equals};
use crate::api::utils::BetterBlockPos;
use crate::api::utils::settings_util::maybe_censor;
use crate::java;

/// Useful for automated combat (retreating specifically)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalRunAway {
    from: Vec<BetterBlockPos>,
    distance_sq: i32,
    maintain_y: Option<i32>,
}

impl GoalRunAway {
    /// Both upstream constructors; `maintain_y` is the nullable `Integer`. Panics if `from` is
    /// empty, like upstream's `IllegalArgumentException`.
    pub fn new(distance: f64, maintain_y: Option<i32>, from: Vec<BetterBlockPos>) -> Self {
        if from.is_empty() {
            panic!("Positions to run away from must not be empty");
        }
        Self {
            from,
            distance_sq: (distance * distance) as i32,
            maintain_y,
        }
    }
}

impl Goal for GoalRunAway {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        if self.maintain_y.is_some_and(|maintain_y| maintain_y != y) {
            return false;
        }
        for p in &self.from {
            let diff_x = x.wrapping_sub(p.x);
            let diff_z = z.wrapping_sub(p.z);
            let dist_sq = diff_x
                .wrapping_mul(diff_x)
                .wrapping_add(diff_z.wrapping_mul(diff_z));
            if dist_sq < self.distance_sq {
                return false;
            }
        }
        true
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        // mostly copied from GoalBlock
        let mut min = f64::MAX;
        for p in &self.from {
            let h = GoalXZ::calculate(p.x.wrapping_sub(x) as f64, p.z.wrapping_sub(z) as f64);
            if h < min {
                min = h;
            }
        }
        min = -min;
        if let Some(maintain_y) = self.maintain_y {
            min = min * 0.6 + GoalYLevel::calculate(maintain_y, y) * 1.5;
        }
        min
    }

    fn heuristic_at_goal(&self) -> f64 {
        // TODO less hacky solution
        let distance = (self.distance_sq as f64).sqrt().ceil() as i32;
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut min_z = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        let mut max_z = i32::MIN;
        for p in &self.from {
            min_x = min_x.min(p.x.wrapping_sub(distance));
            min_y = min_y.min(p.y.wrapping_sub(distance));
            min_z = min_z.min(p.z.wrapping_sub(distance));
            // upstream compares against min*, not max*; kept as is
            max_x = min_x.max(p.x.wrapping_add(distance));
            max_y = min_y.max(p.y.wrapping_add(distance));
            max_z = min_z.max(p.z.wrapping_add(distance));
        }
        let mut maybe_always_inside = Vec::new(); // see pull request #1978
        let mut min_outside = f64::INFINITY;
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                for z in min_z..=max_z {
                    let h = self.heuristic(x, y, z);
                    if h < min_outside && self.is_in_goal(x, y, z) {
                        maybe_always_inside.push(h);
                    } else {
                        min_outside = java::min_f64(min_outside, h);
                    }
                }
            }
        }
        let mut max_inside = f64::NEG_INFINITY;
        for inside in maybe_always_inside {
            if inside < min_outside {
                max_inside = java::max_f64(max_inside, inside);
            }
        }
        max_inside
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for GoalRunAway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let from = self
            .from
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        match self.maintain_y {
            Some(maintain_y) => write!(
                f,
                "GoalRunAwayFromMaintainY y={}, [{from}]",
                maybe_censor(maintain_y)
            ),
            None => write!(f, "GoalRunAwayFrom[{from}]"),
        }
    }
}
