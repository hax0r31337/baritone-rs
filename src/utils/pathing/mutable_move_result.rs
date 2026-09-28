// Ported from baritone src/main/java/baritone/utils/pathing/MutableMoveResult.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use crate::api::pathing::movement::COST_INF;

/// The result of a calculated movement, with destination x, y, z, and the cost of performing the movement
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MutableMoveResult {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub cost: f64,
}

impl Default for MutableMoveResult {
    fn default() -> Self {
        Self::new()
    }
}

impl MutableMoveResult {
    pub fn new() -> Self {
        Self {
            x: 0,
            y: 0,
            z: 0,
            cost: COST_INF,
        }
    }

    pub fn reset(&mut self) {
        self.x = 0;
        self.y = 0;
        self.z = 0;
        self.cost = COST_INF;
    }
}
