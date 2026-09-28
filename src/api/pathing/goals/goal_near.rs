// Ported from baritone src/api/java/baritone/api/pathing/goals/GoalNear.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::goals::{Goal, GoalBlock, goal_equals};
use crate::api::utils::BetterBlockPos;
use crate::api::utils::interfaces::IGoalRenderPos;
use crate::api::utils::settings_util::maybe_censor;
use crate::java;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoalNear {
    x: i32,
    y: i32,
    z: i32,
    range_sq: i32,
}

impl GoalNear {
    pub fn new(pos: BetterBlockPos, range: i32) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
            range_sq: range.wrapping_mul(range),
        }
    }
}

impl Goal for GoalNear {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        let x_diff = x.wrapping_sub(self.x);
        let y_diff = y.wrapping_sub(self.y);
        let z_diff = z.wrapping_sub(self.z);
        x_diff
            .wrapping_mul(x_diff)
            .wrapping_add(y_diff.wrapping_mul(y_diff))
            .wrapping_add(z_diff.wrapping_mul(z_diff))
            <= self.range_sq
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        let x_diff = x.wrapping_sub(self.x);
        let y_diff = y.wrapping_sub(self.y);
        let z_diff = z.wrapping_sub(self.z);
        GoalBlock::calculate(x_diff as f64, y_diff, z_diff as f64)
    }

    fn heuristic_at_goal(&self) -> f64 {
        // TODO less hacky solution
        let range = (self.range_sq as f64).sqrt().ceil() as i32;
        let mut maybe_always_inside = Vec::new(); // see pull request #1978
        let mut min_outside = f64::INFINITY;
        for dx in -range..=range {
            for dy in -range..=range {
                for dz in -range..=range {
                    let (x, y, z) = (
                        self.x.wrapping_add(dx),
                        self.y.wrapping_add(dy),
                        self.z.wrapping_add(dz),
                    );
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

    fn as_goal_render_pos(&self) -> Option<&dyn IGoalRenderPos> {
        Some(self)
    }
}

impl IGoalRenderPos for GoalNear {
    fn get_goal_pos(&self) -> BetterBlockPos {
        BetterBlockPos::new(self.x, self.y, self.z)
    }
}

impl fmt::Display for GoalNear {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GoalNear{{x={}, y={}, z={}, rangeSq={}}}",
            maybe_censor(self.x),
            maybe_censor(self.y),
            maybe_censor(self.z),
            self.range_sq
        )
    }
}
