// Ported from baritone src/main/java/baritone/process/FollowProcess.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Also `IFollowProcess`; `cancel` reaches the `Baritone`, so it is an associated function
// taking it. Entities are the host's (`crate::host::Entity`); two are the same entity when
// their ids are (Java's identity `equals`). The followed entities are copies from the tick
// they were found in.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use crate::Baritone;
use crate::api::pathing::goals::{Goal, GoalBlock, GoalComposite, GoalNear, GoalXZ};
use crate::api::process::{IBaritoneProcess, PathingCommand, PathingCommandType};
use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::host::{Entity, ItemStack};
use crate::settings::settings;

/// `Predicate<Entity>`
pub type EntityFilter = Arc<dyn Fn(&Entity) -> bool + Send + Sync>;

/// Follow an entity
#[derive(Default)]
pub struct FollowProcess {
    filter: Option<EntityFilter>,
    cache: Option<Vec<Entity>>,
    into: bool, // walk straight into the target, regardless of settings
}

impl fmt::Debug for FollowProcess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FollowProcess")
            .field("filter", &self.filter.as_ref().map(|_| "<predicate>"))
            .field("cache", &self.cache)
            .field("into", &self.into)
            .finish()
    }
}

impl FollowProcess {
    pub fn new() -> Self {
        Self::default()
    }

    fn towards(&self, following: &Entity) -> Arc<dyn Goal> {
        let settings = settings();
        let pos = if settings.follow_offset_distance == 0.0 || self.into {
            following.block_position()
        } else {
            let g = GoalXZ::from_direction(
                following.position,
                settings.follow_offset_direction,
                settings.follow_offset_distance,
            );
            BetterBlockPos::from_f64(g.get_x() as f64, following.position.y, g.get_z() as f64)
        };
        if self.into {
            return Arc::new(GoalBlock::from_pos(pos));
        }
        Arc::new(GoalNear::new(pos, settings.follow_radius))
    }

    fn followable(&self, ctx: &dyn IPlayerContext, entity: &Entity) -> bool {
        if !entity.alive {
            return false;
        }
        // entity.equals(ctx.player()): the entities never hold the local player
        let max_dist = settings().follow_target_max_distance;
        if max_dist != 0
            && entity.distance_to_sqr(ctx.player().position)
                > max_dist.wrapping_mul(max_dist) as f64
        {
            return false;
        }
        ctx.entities().iter().any(|e| e.id == entity.id)
    }

    fn scan_world(&mut self, ctx: &dyn IPlayerContext) {
        let filter = self.filter.clone().expect("NullPointerException: filter");
        let mut cache: Vec<Entity> = Vec::new();
        for entity in ctx.entities() {
            if self.followable(ctx, entity)
                && filter(entity)
                && !cache.iter().any(|e| e.id == entity.id)
            {
                cache.push(entity.clone());
            }
        }
        self.cache = Some(cache);
    }

    /// Set the follow target to any entities matching this predicate
    pub fn follow(&mut self, filter: EntityFilter) {
        self.filter = Some(filter);
        self.into = false;
    }

    /// Try to pick up any items matching this predicate
    pub fn pickup(&mut self, filter: impl Fn(&ItemStack) -> bool + Send + Sync + 'static) {
        self.filter = Some(Arc::new(move |e: &Entity| {
            e.as_item_entity().is_some_and(&filter)
        }));
        self.into = true;
    }

    /// The entities that are currently being followed. `None` if not currently following,
    /// empty if nothing matches the predicate
    pub fn following(&self) -> Option<&[Entity]> {
        self.cache.as_deref()
    }

    pub fn current_filter(&self) -> Option<&EntityFilter> {
        self.filter.as_ref()
    }

    /// Cancels the follow behavior, this will clear the current follow target.
    pub fn cancel(baritone: &mut Baritone) {
        baritone
            .with_process_of(|this: &mut FollowProcess, baritone| this.on_lost_control(baritone));
    }
}

impl IBaritoneProcess for FollowProcess {
    fn on_tick(
        &mut self,
        baritone: &mut Baritone,
        _calc_failed: bool,
        _is_safe_to_cancel: bool,
    ) -> Option<PathingCommand> {
        self.scan_world(&baritone.player_context);
        let goal = GoalComposite::new(
            self.cache
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|entity| self.towards(entity))
                .collect(),
        );
        Some(PathingCommand::new(
            Some(Arc::new(goal)),
            PathingCommandType::RevalidateGoalAndPath,
        ))
    }

    fn is_active(&mut self, baritone: &mut Baritone) -> bool {
        if self.filter.is_none() {
            return false;
        }
        self.scan_world(&baritone.player_context);
        self.cache.as_ref().is_some_and(|cache| !cache.is_empty())
    }

    fn is_temporary(&self) -> bool {
        false
    }

    fn on_lost_control(&mut self, _baritone: &mut Baritone) {
        self.filter = None;
        self.cache = None;
    }

    fn display_name0(&mut self, _baritone: &mut Baritone) -> String {
        match &self.cache {
            Some(cache) => format!(
                "Following [{}]",
                cache
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => "Following null".to_owned(),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
