// Ported from baritone src/api/java/baritone/api/pathing/calc/IPathFinder.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::sync::Arc;

use crate::api::pathing::calc::IPath;
use crate::api::pathing::goals::Goal;
use crate::api::utils::PathCalculationResult;

/// Generic path finder interface
pub trait IPathFinder {
    fn get_goal(&self) -> &Arc<dyn Goal>;

    /// Calculate the path in full. Will take several seconds.
    ///
    /// `primary_timeout`: If a path is found, the path finder will stop after this amount of time
    /// `failure_timeout`: If a path isn't found, the path finder will continue for this amount of time
    /// Returns the final path
    fn calculate(&mut self, primary_timeout: i64, failure_timeout: i64) -> PathCalculationResult;

    /// Intended to be called concurrently with calculatePath from a different thread to tell if it's finished yet
    ///
    /// Returns whether or not this finder is finished
    fn is_finished(&self) -> bool;

    /// Called for path rendering. Returns a path to the most recent node popped from the open set and considered.
    fn path_to_most_recent_node_considered(&self) -> Option<Box<dyn IPath>>;

    /// The best path so far, according to the most forgiving coefficient heuristic (the reason being that that path is
    /// most likely to represent the true shape of the path to the goal, assuming it's within a possible cost heuristic.
    /// That's almost always a safe assumption, but in the case of a nearly impossible path, it still works by providing
    /// a theoretically plausible but practically unlikely path)
    fn best_path_so_far(&self) -> Option<Box<dyn IPath>>;
}
