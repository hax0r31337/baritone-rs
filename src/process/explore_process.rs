// Ported from baritone src/main/java/baritone/process/ExploreProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Also `IExploreProcess`. The chunk filters take the cached world (`crate::cache::CachedWorld`,
// the chunks Baritone has seen loaded) where upstream's `BaritoneChunkCache` fetches it. Its
// regions are always loaded, so no filter waits for a region from disk. The JSON filter is
// read with serde_json, which is strict where Gson is lenient (numbers must be integers).

use std::any::Any;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::Baritone;
use crate::api::pathing::goals::{Goal, GoalComposite, GoalXZ, GoalYLevel, goal_equals};
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::helper::{log_debug, log_direct, log_notification};
use crate::api::utils::{BetterBlockPos, MyChunkPos};
use crate::cache::CachedWorld;
use crate::host::world::chunk_key;
use crate::settings::settings;

#[derive(Debug, Default)]
pub struct ExploreProcess {
    exploration_origin: Option<BetterBlockPos>,

    filter: Option<Arc<JsonChunkFilter>>,

    distance_completed: i32,
}

impl ExploreProcess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn explore(&mut self, center_x: i32, center_z: i32) {
        self.exploration_origin = Some(BetterBlockPos::new(center_x, 0, center_z));
        self.distance_completed = 0;
    }

    /// `applyJsonFilter(Path, boolean)`: a JSON array of `{"x": .., "z": ..}` chunk positions.
    /// If `invert` is true, the list is interpreted as a list of chunks that are NOT explored,
    /// if false, the list is interpreted as a list of chunks that ARE explored.
    pub fn apply_json_filter(
        &mut self,
        path: &Path,
        invert: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.filter = Some(Arc::new(JsonChunkFilter::new(path, invert)?));
        Ok(())
    }

    pub fn calc_filter<'a>(&self, cache: &'a CachedWorld) -> Box<dyn IChunkFilter + 'a> {
        let filter: Box<dyn IChunkFilter + 'a> = match &self.filter {
            Some(filter) => Box::new(EitherChunk {
                a: Box::new(JsonFilterWithCache {
                    filter: Arc::clone(filter),
                    cache,
                }),
                b: Box::new(BaritoneChunkCache { cache }),
            }),
            None => Box::new(BaritoneChunkCache { cache }),
        };
        filter
    }

    fn closest_uncached_chunks(
        &mut self,
        center: BetterBlockPos,
        filter: &dyn IChunkFilter,
    ) -> Option<Vec<Arc<dyn Goal>>> {
        let chunk_x = center.x >> 4;
        let chunk_z = center.z >> 4;
        let settings = settings();
        let mut count = filter
            .count_remain()
            .min(settings.explore_chunk_set_minimum_size);
        let mut centers: Vec<BetterBlockPos> = Vec::new();
        let render_distance = settings.world_exploring_chunk_offset;
        let mut dist = self.distance_completed;
        loop {
            for dx in -dist..=dist {
                let zval = dist.wrapping_sub(dx.wrapping_abs());
                for mult in 0..2 {
                    let dz = (mult * 2 - 1) * zval; // dz can be either -zval or zval
                    let true_dist = dx.wrapping_abs().wrapping_add(dz.wrapping_abs());
                    if true_dist != dist {
                        panic!("Offset {dx} {dz} has distance {true_dist}, expected {dist}");
                    }
                    match filter.is_already_explored(chunk_x + dx, chunk_z + dz) {
                        Status::Unknown => return None, // awaiting load
                        Status::NotExplored => {}
                        Status::Explored => continue,
                    }
                    let mut center_x = ((chunk_x + dx) << 4) + 8;
                    let mut center_z = ((chunk_z + dz) << 4) + 8;
                    let offset = render_distance << 4;
                    if dx < 0 {
                        center_x -= offset;
                    } else {
                        center_x += offset;
                    }
                    if dz < 0 {
                        center_z -= offset;
                    } else {
                        center_z += offset;
                    }
                    centers.push(BetterBlockPos::new(center_x, 0, center_z));
                }
            }
            if dist % 10 == 0 {
                count = filter
                    .count_remain()
                    .min(settings.explore_chunk_set_minimum_size);
            }
            if centers.len() as i64 >= count as i64 {
                return Some(
                    centers
                        .iter()
                        .map(|pos| create_goal(pos.x, pos.z))
                        .collect(),
                );
            }
            if centers.is_empty() {
                // we have explored everything from 0 to dist inclusive
                // next time we should start our check at dist+1
                self.distance_completed = dist + 1;
            }
            dist += 1;
        }
    }
}

fn create_goal(x: i32, z: i32) -> Arc<dyn Goal> {
    if settings().explore_maintain_y == -1 {
        return Arc::new(GoalXZ::new(x, z));
    }
    // don't use a goalblock because we still want isInGoal to return true if X and Z are correct
    // we just want to try and maintain Y on the way there, not necessarily end at that specific Y
    Arc::new(MaintainYGoalXZ(GoalXZ::new(x, z)))
}

impl IBaritoneProcess for ExploreProcess {
    fn is_active(&mut self, _baritone: &mut Baritone) -> bool {
        self.exploration_origin.is_some()
    }

    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        calc_failed: bool,
        _is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        if calc_failed {
            log_direct("Failed");
            if settings().notification_on_explore_finished {
                log_notification("Exploration failed", true);
            }
            self.on_lost_control(baritone);
            return None;
        }
        let filter = self.calc_filter(&baritone.cached_world);
        if !settings().disable_completion_check && filter.count_remain() == 0 {
            log_direct("Explored all chunks");
            if settings().notification_on_explore_finished {
                log_notification("Explored all chunks", false);
            }
            drop(filter);
            self.on_lost_control(baritone);
            return None;
        }
        let origin = self
            .exploration_origin
            .expect("NullPointerException: explorationOrigin");
        let Some(closest_uncached) = self.closest_uncached_chunks(origin, &*filter) else {
            log_debug("awaiting region load from disk");
            return Some(PathingCommand::new(None, PathingCommandType::RequestPause));
        };
        Some(PathingCommand::new(
            Some(Arc::new(GoalComposite::new(closest_uncached))),
            PathingCommandType::ForceRevalidateGoalAndPath,
        ))
    }

    fn is_temporary(&self) -> bool {
        false
    }

    fn on_lost_control(&mut self, _baritone: &mut Baritone) {
        self.exploration_origin = None;
    }

    fn display_name0(&mut self, baritone: &mut Baritone) -> String {
        let origin = self
            .exploration_origin
            .expect("NullPointerException: explorationOrigin");
        let filter = self.calc_filter(&baritone.cached_world);
        let going_to = match self.closest_uncached_chunks(origin, &*filter) {
            Some(goals) => GoalComposite::new(goals).to_string(),
            None => "GoalCompositenull".to_owned(),
        };
        format!(
            "Exploring around BlockPos{{x={}, y={}, z={}}}, distance completed {}, currently going to {going_to}",
            origin.x, origin.y, origin.z, self.distance_completed
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// `new GoalXZ(x, z) { ... }`: also tries to keep `exploreMaintainY` on the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MaintainYGoalXZ(GoalXZ);

impl Goal for MaintainYGoalXZ {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        self.0.is_in_goal(x, y, z)
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        self.0.heuristic(x, y, z) + GoalYLevel::calculate(settings().explore_maintain_y, y)
    }

    fn equals(&self, other: &dyn Goal) -> bool {
        goal_equals(self, other)
    }
}

impl fmt::Display for MaintainYGoalXZ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Explored,
    NotExplored,
    Unknown,
}

pub trait IChunkFilter {
    fn is_already_explored(&self, chunk_x: i32, chunk_z: i32) -> Status;

    fn count_remain(&self) -> i32;
}

struct BaritoneChunkCache<'a> {
    cache: &'a CachedWorld,
}

impl IChunkFilter for BaritoneChunkCache<'_> {
    fn is_already_explored(&self, chunk_x: i32, chunk_z: i32) -> Status {
        let center_x = chunk_x << 4;
        let center_z = chunk_z << 4;
        if self.cache.is_cached(center_x, center_z) {
            return Status::Explored;
        }
        if !self.cache.region_loaded(center_x, center_z) {
            return Status::Unknown; // we still need to load regions from disk in order to decide properly
        }
        Status::NotExplored
    }

    fn count_remain(&self) -> i32 {
        i32::MAX
    }
}

#[derive(Debug)]
pub struct JsonChunkFilter {
    /// if true, the list is interpreted as a list of chunks that are NOT explored, if false, the list is interpreted as a list of chunks that ARE explored
    invert: bool,
    in_filter: FxHashSet<i64>,
    positions: Vec<MyChunkPos>,
}

impl JsonChunkFilter {
    /// ioexception, json exception, etc
    fn new(path: &Path, invert: bool) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let positions: Vec<MyChunkPos> =
            serde_json::from_reader(BufReader::new(File::open(path)?))?;
        log_direct(&format!("Loaded {} positions", positions.len()));
        let in_filter = positions
            .iter()
            .map(|mcp| chunk_key(mcp.x, mcp.z))
            .collect();
        Ok(Self {
            invert,
            in_filter,
            positions,
        })
    }
}

/// The JSON filter with the cached world its `countRemain` asks.
struct JsonFilterWithCache<'a> {
    filter: Arc<JsonChunkFilter>,
    cache: &'a CachedWorld,
}

impl IChunkFilter for JsonFilterWithCache<'_> {
    fn is_already_explored(&self, chunk_x: i32, chunk_z: i32) -> Status {
        if self.filter.in_filter.contains(&chunk_key(chunk_x, chunk_z)) ^ self.filter.invert {
            // either it's on the list of explored chunks, or it's not on the list of unexplored chunks
            // either way, we have it
            Status::Explored
        } else {
            // either it's not on the list of explored chunks, or it's on the list of unexplored chunks
            // either way, it depends on if baritone has cached it so defer to that
            Status::Unknown
        }
    }

    fn count_remain(&self) -> i32 {
        if !self.filter.invert {
            // if invert is false, anything not on the list is uncached
            return i32::MAX;
        }
        // but if invert is true, anything not on the list IS assumed cached
        // so we are done if everything on our list is cached!
        let mut count_remain = 0;
        let bcc = BaritoneChunkCache { cache: self.cache };
        let minimum = settings().explore_chunk_set_minimum_size;
        for pos in &self.filter.positions {
            if bcc.is_already_explored(pos.x, pos.z) != Status::Explored {
                // either waiting for it or dont have it at all
                count_remain += 1;
                if count_remain >= minimum {
                    return count_remain;
                }
            }
        }
        count_remain
    }
}

struct EitherChunk<'a> {
    a: Box<dyn IChunkFilter + 'a>,
    b: Box<dyn IChunkFilter + 'a>,
}

impl IChunkFilter for EitherChunk<'_> {
    fn is_already_explored(&self, chunk_x: i32, chunk_z: i32) -> Status {
        if self.a.is_already_explored(chunk_x, chunk_z) == Status::Explored {
            return Status::Explored;
        }
        self.b.is_already_explored(chunk_x, chunk_z)
    }

    fn count_remain(&self) -> i32 {
        self.a.count_remain().min(self.b.count_remain())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goals(process: &mut ExploreProcess, cache: &CachedWorld) -> Vec<String> {
        let origin = process.exploration_origin.unwrap();
        let filter = process.calc_filter(cache);
        process
            .closest_uncached_chunks(origin, &*filter)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn nearest_uncached_chunks_first() {
        let mut process = ExploreProcess::new();
        process.explore(0, 0);
        let mut cache = CachedWorld::new();
        // nothing cached: distance 0 is the origin's chunk twice (for both signs of dz = 0),
        // distance 1 makes 8, not yet 10, and distance 2 adds 10 more
        let first = goals(&mut process, &cache);
        assert_eq!(first.len(), 18);
        assert_eq!(first[0], "GoalXZ{x=8,z=8}");
        assert_eq!(first[1], "GoalXZ{x=8,z=8}");
        assert_eq!(process.distance_completed, 0);

        for x in -3..=3 {
            for z in -3..=3 {
                cache.queue_for_packing(x, z);
            }
        }
        let next = goals(&mut process, &cache);
        // distances 0 to 3 are all cached; 4 has the corners of the 7x7 square cached
        assert_eq!(process.distance_completed, 4);
        assert_eq!(next[0], "GoalXZ{x=-56,z=8}");
        assert!(next.len() >= 10);
    }

    #[test]
    fn json_filter_explores_only_the_listed_chunks() {
        let dir = std::env::temp_dir().join(format!("baritone-explore-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("filter.json");
        std::fs::write(&path, r#"[{"x": 2, "z": 0}, {"x": 5, "z": 5, "extra": 1}]"#).unwrap();
        let mut process = ExploreProcess::new();
        process.apply_json_filter(&path, true).unwrap();
        process.explore(0, 0);
        let mut cache = CachedWorld::new();
        assert_eq!(process.calc_filter(&cache).count_remain(), 2);
        let first = goals(&mut process, &cache);
        // the listed chunks are the only unexplored ones; the first one, at dz = 0, counts
        // twice and makes the two it takes
        assert_eq!(first, ["GoalXZ{x=40,z=8}", "GoalXZ{x=40,z=8}"]);
        cache.queue_for_packing(2, 0);
        cache.queue_for_packing(5, 5);
        assert_eq!(process.calc_filter(&cache).count_remain(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(process.apply_json_filter(&path, false).is_err());
    }
}
