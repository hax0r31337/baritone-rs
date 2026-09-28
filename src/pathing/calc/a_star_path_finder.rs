// Ported from baritone src/main/java/baritone/pathing/calc/AStarPathFinder.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// The superclass is the `search` field. Timeouts use a monotonic clock
// (`java::current_time_millis`), so only their durations matter, as upstream intends.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::api::pathing::calc::{IPath, IPathFinder};
use crate::api::pathing::goals::Goal;
use crate::api::pathing::movement::COST_INF;
use crate::api::utils::helper::{log_debug, println};
use crate::api::utils::settings_util::maybe_censor;
use crate::api::utils::{BetterBlockPos, PathCalculationResult};
use crate::java::current_time_millis;
use crate::pathing::calc::abstract_node_cost_search::{
    COEFFICIENTS, MIN_DIST_PATH, MIN_IMPROVEMENT, dist_from_start_sq, node_at_position,
};
use crate::pathing::calc::openset::{BinaryHeapOpenSet, IOpenSet};
use crate::pathing::calc::{AbstractNodeCostSearch, Path};
use crate::pathing::movement::{CalculationContext, Moves};
use crate::settings::settings;
use crate::utils::pathing::{BetterWorldBorder, Favoring, MutableMoveResult};

/// The actual A* pathfinding
pub struct AStarPathFinder {
    search: AbstractNodeCostSearch,
    favoring: Favoring,
}

impl AStarPathFinder {
    pub fn new(
        real_start: BetterBlockPos,
        start_x: i32,
        start_y: i32,
        start_z: i32,
        goal: Arc<dyn Goal>,
        favoring: Favoring,
        context: CalculationContext,
    ) -> Self {
        Self {
            search: AbstractNodeCostSearch::new(
                real_start, start_x, start_y, start_z, goal, context,
            ),
            favoring,
        }
    }

    /// The `AbstractNodeCostSearch` part (what `PathingBehavior.getInProgress` hands out).
    pub fn search(&self) -> &AbstractNodeCostSearch {
        &self.search
    }

    fn calculate0(
        search: &mut AbstractNodeCostSearch,
        favoring: &Favoring,
        primary_timeout: i64,
        failure_timeout: i64,
    ) -> Option<Box<dyn IPath>> {
        let AbstractNodeCostSearch {
            real_start,
            start_x,
            start_y,
            start_z,
            goal,
            context: calc_context,
            map,
            nodes,
            start_node: start_node_field,
            most_recent_considered,
            best_so_far,
            cancel_requested,
            ..
        } = search;
        let (start_x, start_y, start_z) = (*start_x, *start_y, *start_z);
        let dimension = calc_context.world.dimension();
        let min_y = dimension.min_y;
        let height = dimension.height;
        let start_node = node_at_position(
            map,
            nodes,
            &**goal,
            start_x,
            start_y,
            start_z,
            BetterBlockPos::long_hash(start_x, start_y, start_z),
        );
        *start_node_field = Some(start_node);
        {
            let start = &mut nodes[start_node as usize];
            start.cost = 0.0;
            start.combined_cost = start.estimated_cost_to_goal;
        }
        let mut open_set = BinaryHeapOpenSet::new();
        open_set.insert(nodes, start_node);
        let mut best_heuristic_so_far = [0.0; COEFFICIENTS.len()]; //keep track of the best node by the metric of (estimatedCostToGoal + cost / COEFFICIENTS[i])
        for i in 0..best_heuristic_so_far.len() {
            best_heuristic_so_far[i] = nodes[start_node as usize].estimated_cost_to_goal;
            best_so_far[i] = Some(start_node);
        }
        let mut res = MutableMoveResult::new();
        let world_border = BetterWorldBorder::new(&calc_context.world.border());
        let start_time = current_time_millis();
        let settings_now = settings();
        let slow_path = settings_now.slow_path;
        if slow_path {
            log_debug(&format!(
                "slowPath is on, path timeout will be {}ms instead of {}ms",
                settings_now.slow_path_timeout_ms, primary_timeout
            ));
        }
        let primary_timeout_time = start_time.wrapping_add(if slow_path {
            settings_now.slow_path_timeout_ms
        } else {
            primary_timeout
        });
        let failure_timeout_time = start_time.wrapping_add(if slow_path {
            settings_now.slow_path_timeout_ms
        } else {
            failure_timeout
        });
        let mut failing = true;
        let mut num_nodes: i32 = 0;
        let mut num_movements_considered: i32 = 0;
        let mut num_empty_chunk: i32 = 0;
        let is_favoring = !favoring.is_empty();
        let time_check_interval = 1 << 6;
        let pathing_max_chunk_border_fetch = settings_now.pathing_max_chunk_border_fetch; // grab all settings beforehand so that changing settings during pathing doesn't cause a crash or unpredictable behavior
        let minimum_improvement = if settings_now.minimum_improvement_repropagation {
            MIN_IMPROVEMENT
        } else {
            0.0
        };
        drop(settings_now);
        let all_moves = Moves::VALUES;
        while !open_set.is_empty()
            && num_empty_chunk < pathing_max_chunk_border_fetch
            && !cancel_requested.load(Ordering::Relaxed)
        {
            if (num_nodes & (time_check_interval - 1)) == 0 {
                // only call this once every 64 nodes (about half a millisecond)
                let now = current_time_millis(); // since nanoTime is slow on windows (takes many microseconds)
                if now.wrapping_sub(failure_timeout_time) >= 0
                    || (!failing && now.wrapping_sub(primary_timeout_time) >= 0)
                {
                    break;
                }
            }
            if slow_path {
                let delay = settings().slow_path_time_delay_ms;
                std::thread::sleep(Duration::from_millis(u64::try_from(delay).unwrap_or(0)));
            }
            let current_node = open_set.remove_lowest(nodes);
            *most_recent_considered = Some(current_node);
            num_nodes += 1;
            let (cx, cy, cz) = {
                let n = &nodes[current_node as usize];
                (n.x, n.y, n.z)
            };
            if goal.is_in_goal(cx, cy, cz) {
                log_debug(&format!(
                    "Took {}ms, {} movements considered",
                    current_time_millis() - start_time,
                    num_movements_considered
                ));
                return Some(Box::new(Path::new(
                    *real_start,
                    start_node,
                    current_node,
                    num_nodes,
                    Arc::clone(goal),
                    nodes,
                )));
            }
            for moves in all_moves {
                let new_x = cx + moves.x_offset();
                let new_z = cz + moves.z_offset();
                if ((new_x >> 4) != (cx >> 4) || (new_z >> 4) != (cz >> 4))
                    && !calc_context.is_loaded(new_x, new_z)
                {
                    // only need to check if the destination is a loaded chunk if it's in a different chunk than the start of the movement
                    if !moves.dynamic_xz() {
                        // only increment the counter if the movement would have gone out of bounds guaranteed
                        num_empty_chunk += 1;
                    }
                    continue;
                }
                if !moves.dynamic_xz() && !world_border.entirely_contains(new_x, new_z) {
                    continue;
                }
                if cy + moves.y_offset() > height || cy + moves.y_offset() < min_y {
                    continue;
                }
                res.reset();
                moves.apply(calc_context, cx, cy, cz, &mut res);
                num_movements_considered += 1;
                let mut action_cost = res.cost;
                if action_cost >= COST_INF {
                    continue;
                }
                if action_cost <= 0.0 || action_cost.is_nan() {
                    panic!(
                        "{} from {} {} {} calculated implausible cost {}",
                        moves,
                        maybe_censor(cx),
                        maybe_censor(cy),
                        maybe_censor(cz),
                        action_cost
                    );
                }
                // check destination after verifying it's not COST_INF -- some movements return COST_INF without adjusting the destination
                if moves.dynamic_xz() && !world_border.entirely_contains(res.x, res.z) {
                    // see issue #218
                    continue;
                }
                if !moves.dynamic_xz() && (res.x != new_x || res.z != new_z) {
                    panic!(
                        "{} from {} {} {} ended at x z {} {} instead of {} {}",
                        moves,
                        maybe_censor(cx),
                        maybe_censor(cy),
                        maybe_censor(cz),
                        maybe_censor(res.x),
                        maybe_censor(res.z),
                        maybe_censor(new_x),
                        maybe_censor(new_z)
                    );
                }
                if !moves.dynamic_y() && res.y != cy + moves.y_offset() {
                    panic!(
                        "{} from {} {} {} ended at y {} instead of {}",
                        moves,
                        maybe_censor(cx),
                        maybe_censor(cy),
                        maybe_censor(cz),
                        maybe_censor(res.y),
                        maybe_censor(cy + moves.y_offset())
                    );
                }
                let hash_code = BetterBlockPos::long_hash(res.x, res.y, res.z);
                if is_favoring {
                    // see issue #18
                    action_cost *= favoring.calculate(hash_code);
                }
                let neighbor =
                    node_at_position(map, nodes, &**goal, res.x, res.y, res.z, hash_code);
                let tentative_cost = nodes[current_node as usize].cost + action_cost;
                let neighbor_node = &mut nodes[neighbor as usize];
                if neighbor_node.cost - tentative_cost > minimum_improvement {
                    neighbor_node.previous = Some(current_node);
                    neighbor_node.cost = tentative_cost;
                    neighbor_node.combined_cost =
                        tentative_cost + neighbor_node.estimated_cost_to_goal;
                    let is_open = neighbor_node.is_open();
                    let estimated_cost_to_goal = neighbor_node.estimated_cost_to_goal;
                    if is_open {
                        open_set.update(nodes, neighbor);
                    } else {
                        open_set.insert(nodes, neighbor); //dont double count, dont insert into open set if it's already there
                    }
                    for i in 0..COEFFICIENTS.len() {
                        let heuristic = estimated_cost_to_goal + tentative_cost / COEFFICIENTS[i];
                        if best_heuristic_so_far[i] - heuristic > minimum_improvement {
                            best_heuristic_so_far[i] = heuristic;
                            best_so_far[i] = Some(neighbor);
                            if failing
                                && dist_from_start_sq(
                                    start_x,
                                    start_y,
                                    start_z,
                                    &nodes[neighbor as usize],
                                ) > MIN_DIST_PATH * MIN_DIST_PATH
                            {
                                failing = false;
                            }
                        }
                    }
                }
            }
        }
        if cancel_requested.load(Ordering::Relaxed) {
            return None;
        }
        println(&format!("{num_movements_considered} movements considered"));
        println(&format!("Open set size: {}", open_set.size()));
        println(&format!("PathNode map size: {}", map.len()));
        let elapsed = (current_time_millis() - start_time) as f32 / 1000.0;
        println(&format!(
            "{} nodes per second",
            (num_nodes as f64 * 1.0 / elapsed as f64) as i32
        ));
        let result = search.best_so_far(true, num_nodes);
        if result.is_some() {
            log_debug(&format!(
                "Took {}ms, {} movements considered",
                current_time_millis() - start_time,
                num_movements_considered
            ));
        }
        result
    }
}

impl IPathFinder for AStarPathFinder {
    fn get_goal(&self) -> &Arc<dyn Goal> {
        self.search.get_goal()
    }

    fn calculate(&mut self, primary_timeout: i64, failure_timeout: i64) -> PathCalculationResult {
        let favoring = &self.favoring;
        self.search.calculate(
            primary_timeout,
            failure_timeout,
            |search, primary, failure| Self::calculate0(search, favoring, primary, failure),
        )
    }

    fn is_finished(&self) -> bool {
        self.search.is_finished()
    }

    fn path_to_most_recent_node_considered(&self) -> Option<Box<dyn IPath>> {
        self.search.path_to_most_recent_node_considered()
    }

    fn best_path_so_far(&self) -> Option<Box<dyn IPath>> {
        self.search.best_path_so_far()
    }
}

#[cfg(test)]
mod tests {
    // Path results are checked against upstream by tests/reference_paths.rs; these cover what
    // the fixtures cannot: timeouts, cancellation, reuse and exceptions.

    use std::sync::atomic::Ordering;
    use std::time::Instant;

    use super::*;
    use crate::api::pathing::goals::{GoalBlock, GoalXZ};
    use crate::api::utils::path_calculation_result::Type;
    use crate::host::{BlockState, BlockStateTable, Chunk, DimensionType, Player, SubChunk, World};
    use crate::pathing::precompute::Ternary;

    const HALF: i32 = 12; // chunks each way from the origin

    /// Air above an unbreakable floor at y = 0 (section 1), `2 * HALF` chunks wide.
    fn flat_world() -> Arc<World> {
        let air = BlockState {
            name: "minecraft:air".into(),
            air: true,
            can_walk_on: Ternary::No,
            can_walk_through: Ternary::Yes,
            fully_passable: Ternary::Yes,
            pathfindable_land: true,
            ..BlockState::default()
        };
        let floor = BlockState {
            name: "minecraft:bedrock".into(),
            can_walk_on: Ternary::Yes,
            can_walk_through: Ternary::No,
            fully_passable: Ternary::No,
            normal_cube: true,
            can_place_against: true,
            hardness: -1.0,
            ..BlockState::default()
        };
        let table = Arc::new(BlockStateTable::new(vec![air, floor], 0).unwrap());
        let dimension = DimensionType {
            min_y: -16,
            height: 48,
            water_evaporates: false,
        };
        let mut world = World::new(table, dimension).unwrap();
        for cx in -HALF..HALF {
            for cz in -HALF..HALF {
                let mut chunk = Chunk::new(dimension.section_count());
                chunk.set_section(0, Some(SubChunk::filled(1)));
                let mut ground = SubChunk::filled(0);
                for x in 0..16 {
                    for z in 0..16 {
                        ground.set(x, 0, z, 1);
                    }
                }
                chunk.set_section(1, Some(ground));
                world.load_chunk(cx, cz, chunk).unwrap();
            }
        }
        Arc::new(world)
    }

    fn finder(world: &Arc<World>, goal: Arc<dyn Goal>) -> AStarPathFinder {
        let context =
            CalculationContext::new(Arc::clone(world), Arc::new(Player::default()), false, true);
        let favoring = Favoring::new(None, &context);
        let start = BetterBlockPos::new(0, 1, 0);
        AStarPathFinder::new(start, 0, 1, 0, goal, favoring, context)
    }

    /// Sealed in the floor: the search explores the whole world.
    fn unreachable() -> Arc<dyn Goal> {
        Arc::new(GoalBlock::new(3, -5, 3))
    }

    #[test]
    fn walks_to_goal() {
        let world = flat_world();
        let mut finder = finder(&world, Arc::new(GoalXZ::new(20, -7)));
        let result = finder.calculate(10_000, 10_000);
        assert_eq!(result.get_type(), Type::SuccessToGoal);
        let path = result.get_path().unwrap();
        assert_eq!(path.get_dest(), BetterBlockPos::new(20, 1, -7));
        assert!(finder.is_finished());
    }

    /// Explores well past the timeouts below, even in an optimized build.
    fn assert_stopped_early(finder: &AStarPathFinder) {
        assert!(finder.search().map_size() < (2 * HALF as usize * 16).pow(2) / 2);
    }

    #[test]
    fn failure_timeout() {
        let world = flat_world();
        // the best nodes are next to the start, never MIN_DIST_PATH away, so the search is
        // failing and runs until the failure timeout
        let mut finder = finder(&world, unreachable());
        let start = Instant::now();
        let result = finder.calculate(2, 10);
        let elapsed = start.elapsed().as_millis();
        // the search reads whole milliseconds (like currentTimeMillis), so a timeout can end up
        // to 1ms short of real time
        assert!((9..1000).contains(&elapsed), "took {elapsed}ms");
        assert_eq!(result.get_type(), Type::Failure);
        assert_stopped_early(&finder);
    }

    #[test]
    fn primary_timeout() {
        let world = flat_world();
        // far away: once the best node is MIN_DIST_PATH from the start, the primary timeout applies
        let mut finder = finder(&world, Arc::new(GoalBlock::new(150, -5, 150)));
        let start = Instant::now();
        let result = finder.calculate(5, 5_000);
        let elapsed = start.elapsed().as_millis();
        assert!((4..1000).contains(&elapsed), "took {elapsed}ms"); // see failure_timeout
        assert_eq!(result.get_type(), Type::SuccessSegment);
        assert_stopped_early(&finder);
    }

    #[test]
    fn huge_timeouts_never_expire() {
        let world = flat_world();
        // upstream's `startTime + timeout` and `now - timeoutTime >= 0` wrap, so a timeout of
        // Long.MAX_VALUE never fires (and must not overflow in a debug build). The clock starts
        // at 0 on first use, where `startTime + Long.MAX_VALUE` would not overflow yet.
        while current_time_millis() < 1 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let mut finder = finder(&world, Arc::new(GoalXZ::new(20, -7)));
        let result = finder.calculate(i64::MAX, i64::MAX);
        assert_eq!(result.get_type(), Type::SuccessToGoal);
    }

    #[test]
    fn cancel_from_another_thread() {
        let world = flat_world();
        let mut finder = finder(&world, unreachable());
        let cancel = finder.search().cancel_handle();
        let finished = finder.search().finished_handle();
        let calculation = std::thread::spawn(move || finder.calculate(60_000, 60_000).get_type());
        // calculate clears the flag when it starts (like upstream), so keep setting it; the
        // whole search takes longer than this (see failure_timeout)
        while !finished.load(Ordering::Relaxed) {
            cancel.store(true, Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(calculation.join().unwrap(), Type::Cancellation);
    }

    #[test]
    #[should_panic(expected = "Path finder cannot be reused!")]
    fn cannot_be_reused() {
        let world = flat_world();
        let mut finder = finder(&world, Arc::new(GoalXZ::new(2, 2)));
        finder.calculate(1_000, 1_000);
        finder.calculate(1_000, 1_000);
    }

    #[derive(Debug)]
    struct NanGoal;

    impl std::fmt::Display for NanGoal {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("NanGoal")
        }
    }

    impl Goal for NanGoal {
        fn is_in_goal(&self, _x: i32, _y: i32, _z: i32) -> bool {
            false
        }

        fn heuristic(&self, _x: i32, _y: i32, _z: i32) -> f64 {
            f64::NAN
        }

        fn equals(&self, _other: &dyn Goal) -> bool {
            false
        }
    }

    #[test]
    fn exception() {
        let world = flat_world();
        let mut finder = finder(&world, Arc::new(NanGoal));
        // "NanGoal calculated implausible heuristic NaN at 0 1 0" is caught
        assert_eq!(finder.calculate(1_000, 1_000).get_type(), Type::Exception);
        assert!(finder.is_finished());
    }
}
