// Ported from baritone src/main/java/baritone/pathing/calc/AbstractNodeCostSearch.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Nodes live in an arena (`nodes`), the map holds their indices. `calculate0` is passed to
// `calculate` by the subclass. The search runs on the thread that owns it: `cancel` and
// `is_finished` go through shared atomic flags (`cancel_handle`, `finished_handle`) so another
// thread can use them. Upstream's catch of any `Exception` in `calculate` catches panics.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rustc_hash::FxHashMap;

use crate::api::pathing::calc::IPath;
use crate::api::pathing::goals::Goal;
use crate::api::utils::BetterBlockPos;
use crate::api::utils::helper::{log_debug, log_direct, log_notification, println};
use crate::api::utils::path_calculation_result::{PathCalculationResult, Type};
use crate::pathing::calc::{Path, PathNode};
use crate::pathing::movement::CalculationContext;
use crate::settings::settings;

/// This is really complicated and hard to explain. I wrote a comment in the old version of MineBot but it was so
/// long it was easier as a Google Doc (because I could insert charts).
///
/// See <https://docs.google.com/document/d/1WVHHXKXFdCR1Oz__KtK8sFqyvSwJN_H4lftkHFgmzlc/edit>
pub const COEFFICIENTS: [f64; 7] = [1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 10.0];

/// If a path goes less than 5 blocks and doesn't make it to its goal, it's not worth considering.
pub const MIN_DIST_PATH: f64 = 5.0;

/// there are floating point errors caused by random combinations of traverse and diagonal over a flat area
/// that means that sometimes there's a cost improvement of like 10 ^ -16
/// it's not worth the time to update the costs, decrease-key the heap, potentially repropagate, etc
///
/// who cares about a hundredth of a tick? that's half a millisecond for crying out loud!
pub const MIN_IMPROVEMENT: f64 = 0.01;

/// Any pathfinding algorithm that keeps track of nodes recursively by their cost (e.g. A*, dijkstra)
pub struct AbstractNodeCostSearch {
    pub(crate) real_start: BetterBlockPos,
    pub(crate) start_x: i32,
    pub(crate) start_y: i32,
    pub(crate) start_z: i32,

    pub(crate) goal: Arc<dyn Goal>,

    pub(crate) context: CalculationContext,

    /// See <https://github.com/cabaletta/baritone/issues/107>
    ///
    /// `longHash` → index in `nodes`.
    pub(crate) map: FxHashMap<i64, u32>,

    /// The node arena.
    pub(crate) nodes: Vec<PathNode>,

    pub(crate) start_node: Option<u32>,

    pub(crate) most_recent_considered: Option<u32>,

    pub(crate) best_so_far: [Option<u32>; COEFFICIENTS.len()],

    is_finished: Arc<AtomicBool>,

    pub(crate) cancel_requested: Arc<AtomicBool>,
}

impl AbstractNodeCostSearch {
    pub(crate) fn new(
        real_start: BetterBlockPos,
        start_x: i32,
        start_y: i32,
        start_z: i32,
        goal: Arc<dyn Goal>,
        context: CalculationContext,
    ) -> Self {
        // pathingMapLoadFactor tunes the Java hash map; hashbrown's is fixed
        let capacity = usize::try_from(settings().pathing_map_default_size).unwrap_or(0);
        Self {
            real_start,
            start_x,
            start_y,
            start_z,
            goal,
            context,
            map: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            nodes: Vec::with_capacity(capacity),
            start_node: None,
            most_recent_considered: None,
            best_so_far: [None; COEFFICIENTS.len()],
            is_finished: Arc::new(AtomicBool::new(false)),
            cancel_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel_requested.store(true, Ordering::Relaxed);
    }

    /// The flag `cancel` sets, for cancelling from another thread while this one calculates.
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_requested)
    }

    /// The flag `is_finished` reads, for polling from another thread.
    pub fn finished_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_finished)
    }

    /// The calculation context the search reads the world through.
    pub fn context(&self) -> &CalculationContext {
        &self.context
    }

    /// `calculate(long, long)`, with the subclass's `calculate0`.
    pub(crate) fn calculate(
        &mut self,
        primary_timeout: i64,
        failure_timeout: i64,
        calculate0: impl FnOnce(&mut Self, i64, i64) -> Option<Box<dyn IPath>>,
    ) -> PathCalculationResult {
        if self.is_finished.load(Ordering::Relaxed) {
            panic!("Path finder cannot be reused!");
        }
        self.cancel_requested.store(false, Ordering::Relaxed);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let path = calculate0(self, primary_timeout, failure_timeout)
                .map(|path| path.post_process(&self.context));
            if self.cancel_requested.load(Ordering::Relaxed) {
                return PathCalculationResult::new(Type::Cancellation);
            }
            let Some(mut path) = path else {
                return PathCalculationResult::new(Type::Failure);
            };
            let mut previous_length = path.length();
            path = path.cutoff_at_loaded_chunks(&self.context.bsi);
            if path.length() < previous_length {
                log_debug("Cutting off path at edge of loaded chunks");
                log_debug(&format!(
                    "Length decreased by {}",
                    previous_length - path.length()
                ));
            } else {
                log_debug("Path ends within loaded chunks");
            }
            previous_length = path.length();
            path = path.static_cutoff(Some(&*self.goal));
            if path.length() < previous_length {
                log_debug(&format!(
                    "Static cutoff {} to {}",
                    previous_length,
                    path.length()
                ));
            }
            if self.goal.is_in_goal_pos(path.get_dest()) {
                PathCalculationResult::with_path(Type::SuccessToGoal, path)
            } else {
                PathCalculationResult::with_path(Type::SuccessSegment, path)
            }
        }));
        // this is run regardless of what exception may or may not be raised by calculate0
        self.is_finished.store(true, Ordering::Relaxed);
        result.unwrap_or_else(|e| {
            let message = e
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| e.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            log_direct(&format!("Pathing exception: {message}"));
            PathCalculationResult::new(Type::Exception)
        })
    }

    /// Determines the distance squared from the specified node to the start
    /// node. Intended for use in distance comparison, rather than anything that
    /// considers the real distance value, hence the "sq".
    pub(crate) fn get_dist_from_start_sq(&self, n: &PathNode) -> f64 {
        dist_from_start_sq(self.start_x, self.start_y, self.start_z, n)
    }

    /// Attempts to search the block position hashCode long to [`PathNode`] map
    /// for the node mapped to the specified pos. If no node is found,
    /// a new node is created.
    ///
    /// `hash_code` is the hash code of the node, provided by [`BetterBlockPos::long_hash`].
    /// Returns the node's index in the arena. See <https://github.com/cabaletta/baritone/issues/107>
    pub fn get_node_at_position(&mut self, x: i32, y: i32, z: i32, hash_code: i64) -> u32 {
        node_at_position(
            &mut self.map,
            &mut self.nodes,
            &*self.goal,
            x,
            y,
            z,
            hash_code,
        )
    }

    pub fn path_to_most_recent_node_considered(&self) -> Option<Box<dyn IPath>> {
        self.most_recent_considered.map(|node| {
            Box::new(Path::new(
                self.real_start,
                self.start_node.expect("start node"),
                node,
                0,
                Arc::clone(&self.goal),
                &self.nodes,
            )) as Box<dyn IPath>
        })
    }

    pub fn best_path_so_far(&self) -> Option<Box<dyn IPath>> {
        self.best_so_far(false, 0)
    }

    pub(crate) fn best_so_far(&self, log_info: bool, num_nodes: i32) -> Option<Box<dyn IPath>> {
        let start_node = self.start_node?;
        let mut best_dist = 0.0;
        for (i, &best) in self.best_so_far.iter().enumerate() {
            let Some(best) = best else {
                continue;
            };
            let dist = self.get_dist_from_start_sq(&self.nodes[best as usize]);
            if dist > best_dist {
                best_dist = dist;
            }
            if dist > MIN_DIST_PATH * MIN_DIST_PATH {
                // square the comparison since distFromStartSq is squared
                if log_info {
                    if COEFFICIENTS[i] >= 3.0 {
                        println(
                            "Warning: cost coefficient is greater than three! Probably means that",
                        );
                        println(
                            "the path I found is pretty terrible (like sneak-bridging for dozens of blocks)",
                        );
                        println("But I'm going to do it anyway, because yolo");
                    }
                    println(&format!("Path goes for {} blocks", dist.sqrt()));
                    log_debug(&format!("A* cost coefficient {}", COEFFICIENTS[i]));
                }
                return Some(Box::new(Path::new(
                    self.real_start,
                    start_node,
                    best,
                    num_nodes,
                    Arc::clone(&self.goal),
                    &self.nodes,
                )));
            }
        }
        // instead of returning bestSoFar[0], be less misleading
        // if it actually won't find any path, don't make them think it will by rendering a dark blue that will never actually happen
        if log_info {
            log_debug(&format!(
                "Even with a cost coefficient of {}, I couldn't get more than {} blocks",
                COEFFICIENTS[COEFFICIENTS.len() - 1],
                best_dist.sqrt()
            ));
            log_debug("No path found =(");
            log_notification("No path found =(", true);
        }
        None
    }

    pub fn is_finished(&self) -> bool {
        self.is_finished.load(Ordering::Relaxed)
    }

    pub fn get_goal(&self) -> &Arc<dyn Goal> {
        &self.goal
    }

    pub fn get_start(&self) -> BetterBlockPos {
        BetterBlockPos::new(self.start_x, self.start_y, self.start_z)
    }

    pub fn map_size(&self) -> usize {
        self.map.len()
    }
}

/// `getDistFromStartSq` over the start coordinates, for callers that borrow the search's
/// fields separately.
#[inline]
pub(crate) fn dist_from_start_sq(start_x: i32, start_y: i32, start_z: i32, n: &PathNode) -> f64 {
    let x_diff = n.x.wrapping_sub(start_x);
    let y_diff = n.y.wrapping_sub(start_y);
    let z_diff = n.z.wrapping_sub(start_z);
    x_diff
        .wrapping_mul(x_diff)
        .wrapping_add(y_diff.wrapping_mul(y_diff))
        .wrapping_add(z_diff.wrapping_mul(z_diff)) as f64
}

/// `getNodeAtPosition` over the search's map and arena, for callers that borrow the search's
/// fields separately.
#[inline]
pub(crate) fn node_at_position(
    map: &mut FxHashMap<i64, u32>,
    nodes: &mut Vec<PathNode>,
    goal: &dyn Goal,
    x: i32,
    y: i32,
    z: i32,
    hash_code: i64,
) -> u32 {
    *map.entry(hash_code).or_insert_with(|| {
        let index = u32::try_from(nodes.len()).expect("too many path nodes");
        nodes.push(PathNode::new(x, y, z, goal));
        index
    })
}
