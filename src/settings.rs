// Ported from baritone src/api/java/baritone/api/Settings.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Only the settings read by kept code (plans/port.md), with upstream defaults. Block and item
// lists hold Java registry ids ("minecraft:dirt"); they are resolved against the host block
// table, never against Minecraft classes.

//! Baritone's settings.
//!
//! Upstream reads `Baritone.settings().x.value` / `BaritoneAPI.getSettings().x.value` where it
//! needs a value; the port reads `settings().x`. The active settings are process-global like
//! upstream, and are replaced atomically, so a reader always sees one consistent snapshot.
//!
//! Serialized names are upstream's (`allowBreak`, `primaryTimeoutMS`, ...); missing fields
//! deserialize to their defaults.

use std::sync::{Arc, LazyLock};

use arc_swap::{ArcSwap, Guard};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Allow Baritone to break blocks
    pub allow_break: bool,

    /// Blocks that baritone will be allowed to break even with allowBreak set to false
    pub allow_break_anyway: Vec<String>,

    /// Allow Baritone to sprint
    pub allow_sprint: bool,

    /// Allow Baritone to place blocks
    pub allow_place: bool,

    /// Allow Baritone to place blocks in fluid source blocks
    pub allow_place_in_fluids_source: bool,

    /// Allow Baritone to place blocks in flowing fluid
    pub allow_place_in_fluids_flow: bool,

    /// Allow Baritone to move items in your inventory to your hotbar
    pub allow_inventory: bool,

    /// Wait this many ticks between InventoryBehavior moving inventory items
    pub ticks_between_inventory_moves: i32,

    /// Come to a halt before doing any inventory moves. Intended for anticheat such as 2b2t
    pub inventory_move_only_if_stationary: bool,

    /// Disable baritone's auto-tool at runtime, but still assume that another mod will provide auto tool functionality
    ///
    /// Specifically, path calculation will still assume that an auto tool will run at execution time, even though
    /// Baritone itself will not do that.
    pub assume_external_auto_tool: bool,

    /// Automatically select the best available tool
    pub auto_tool: bool,

    /// It doesn't actually take twenty ticks to place a block, this cost is so high
    /// because we want to generally conserve blocks which might be limited.
    ///
    /// Decrease to make Baritone more often consider paths that would require placing blocks
    pub block_placement_penalty: f64,

    /// This is just a tiebreaker to make it less likely to break blocks if it can avoid it.
    /// For example, fire has a break cost of 0, this makes it nonzero, so all else being equal
    /// it will take an otherwise equivalent route that doesn't require it to put out fire.
    pub block_break_additional_penalty: f64,

    /// Additional penalty for hitting the space bar (ascend, pillar, or parkour) because it uses hunger
    pub jump_penalty: f64,

    /// Walking on water uses up hunger really quick, so penalize it
    pub walk_on_water_one_penalty: f64,

    /// Don't allow breaking blocks next to liquids.
    ///
    /// Enable if you have mods adding custom fluid physics.
    pub strict_liquid_check: bool,

    /// Allow Baritone to fall arbitrary distances and place a water bucket beneath it.
    /// Reliability: questionable.
    pub allow_water_bucket_fall: bool,

    /// Allow Baritone to assume it can walk on still water just like any other block.
    /// This functionality is assumed to be provided by a separate library that might have imported Baritone.
    ///
    /// Note: This will prevent some usage of the frostwalker enchantment, like pillaring up from water.
    pub assume_walk_on_water: bool,

    /// If you have Fire Resistance and Jesus then I guess you could turn this on lol
    pub assume_walk_on_lava: bool,

    /// Assume step functionality; don't jump on an Ascend.
    pub assume_step: bool,

    /// Assume safe walk functionality; don't sneak on a backplace traverse.
    ///
    /// Warning: if you do something janky like sneak-backplace from an ender chest, if this is true
    /// it won't sneak right click, it'll just right click, which means it'll open the chest instead of placing
    /// against it. That's why this defaults to off.
    pub assume_safe_walk: bool,

    /// If true, parkour is allowed to make jumps when standing on blocks at the maximum height, so player feet is y=256
    ///
    /// Defaults to false because this fails on constantiam. Please let me know if this is ever disabled. Please.
    pub allow_jump_at_build_limit: bool,

    /// This should be monetized it's so good
    ///
    /// Defaults to true, but only actually takes effect if allowParkour is also true
    pub allow_parkour_ascend: bool,

    /// Allow descending diagonally
    ///
    /// Safer than allowParkour yet still slightly unsafe, can make contact with unchecked adjacent blocks, so it's unsafe in the nether.
    ///
    /// For a generic "take some risks" mode I'd turn on this one, parkour, and parkour place.
    pub allow_diagonal_descend: bool,

    /// Allow diagonal ascending
    ///
    /// Actually pretty safe, much safer than diagonal descend tbh
    pub allow_diagonal_ascend: bool,

    /// Allow mining the block directly beneath its feet
    ///
    /// Turn this off to force it to make more staircases and less shafts
    pub allow_downward: bool,

    /// Blocks that Baritone is allowed to place (as throwaway, for sneak bridging, pillaring, etc.)
    pub acceptable_throwaway_items: Vec<String>,

    /// Blocks that Baritone will attempt to avoid (Used in avoidance)
    pub blocks_to_avoid: Vec<String>,

    /// Blocks that Baritone is not allowed to break
    pub blocks_to_disallow_breaking: Vec<String>,

    /// blocks that baritone shouldn't break, but can if it needs to.
    pub blocks_to_avoid_breaking: Vec<String>,

    /// this multiplies the break speed, if set above 1 it's "encourage breaking" instead
    pub avoid_breaking_multiplier: f64,

    /// If this setting is true, Baritone will never break a block that is adjacent to an unsupported falling block.
    ///
    /// I.E. it will never trigger cascading sand / gravel falls
    pub avoid_updating_falling_blocks: bool,

    /// Enables some more advanced vine features. They're honestly just gimmicks and won't ever be needed in real
    /// pathing scenarios. And they can cause Baritone to get trapped indefinitely in a strange scenario.
    ///
    /// Almost never turn this on lol
    pub allow_vines: bool,

    /// Slab behavior is complicated, disable this for higher path reliability. Leave enabled if you have bottom slabs
    /// everywhere in your base.
    pub allow_walk_on_bottom_slab: bool,

    /// You know what it is
    ///
    /// But it's very unreliable and falls off when cornering like all the time so.
    ///
    /// It also overshoots the landing pretty much always (making contact with the next block over), so be careful
    pub allow_parkour: bool,

    /// Actually pretty reliable.
    ///
    /// Doesn't make it any more dangerous compared to just normal allowParkour th
    pub allow_parkour_place: bool,

    /// For example, if you have Mining Fatigue or Haste, adjust the costs of breaking blocks accordingly.
    pub consider_potion_effects: bool,

    /// Sprint and jump a block early on ascends wherever possible
    pub sprint_ascends: bool,

    /// If we overshoot a traverse and end up one block beyond the destination, mark it as successful anyway.
    ///
    /// This helps with speed exceeding 20m/s
    pub overshoot_traverse: bool,

    /// When breaking blocks for a movement, wait until all falling blocks have settled before continuing
    pub pause_mining_for_falling_blocks: bool,

    /// How many ticks between right clicks are allowed. Default in game is 4
    pub right_click_speed: i32,

    /// How many degrees to randomize the yaw every tick. Set to 0 to disable
    pub random_looking113: f64,

    /// How many ticks between breaking a block and starting to break the next block. Default in game is 6 ticks.
    /// Values under 1 will be clamped. The delay only applies to non-instant (1-tick) breaks.
    pub block_break_speed: i32,

    /// How many degrees to randomize the pitch and yaw every tick. Set to 0 to disable
    pub random_looking: f64,

    /// This is the big A* setting.
    /// As long as your cost heuristic is an *underestimate*, it's guaranteed to find you the best path.
    /// 3.5 is always an underestimate, even if you are sprinting.
    /// If you're walking only (with allowSprint off) 4.6 is safe.
    /// Any value below 3.5 is never worth it. It's just more computation to find the same path, guaranteed.
    /// (specifically, it needs to be strictly slightly less than ActionCosts.WALK_ONE_BLOCK_COST, which is about 3.56)
    ///
    /// Setting it at 3.57 or above with sprinting, or to 4.64 or above without sprinting, will result in
    /// faster computation, at the cost of a suboptimal path. Any value above the walk / sprint cost will result
    /// in it going straight at its goal, and not investigating alternatives, because the combined cost / heuristic
    /// metric gets better and better with each block, instead of slightly worse.
    ///
    /// Finding the optimal path is worth it, so it's the default.
    pub cost_heuristic: f64,

    /// The maximum number of times it will fetch outside loaded or cached chunks before assuming that
    /// pathing has reached the end of the known area, and should therefore stop.
    pub pathing_max_chunk_border_fetch: i32,

    /// Set to 1.0 to effectively disable this feature
    ///
    /// See [Issue #18](https://github.com/cabaletta/baritone/issues/18)
    pub backtrack_cost_favoring_coefficient: f64,

    /// Toggle the following 4 settings
    ///
    /// They have a noticeable performance impact, so they default off
    ///
    /// Specifically, building up the avoidance map on the main thread before pathing starts actually takes a noticeable
    /// amount of time, especially when there are a lot of mobs around, and your game jitters for like 200ms while doing so
    pub avoidance: bool,

    /// Set to 1.0 to effectively disable this feature
    ///
    /// Set below 1.0 to go out of your way to walk near mob spawners
    pub mob_spawner_avoidance_coefficient: f64,

    /// Distance to avoid mob spawners.
    pub mob_spawner_avoidance_radius: i32,

    /// Set to 1.0 to effectively disable this feature
    ///
    /// Set below 1.0 to go out of your way to walk near mobs
    pub mob_avoidance_coefficient: f64,

    /// Distance to avoid mobs.
    pub mob_avoidance_radius: i32,

    /// When running a goto towards a container block (chest, ender chest, furnace, etc),
    /// right click and open it once you arrive.
    pub right_click_container_on_arrival: bool,

    /// When running a goto towards a nether portal block, walk all the way into the portal
    /// instead of stopping one block before.
    pub enter_portal: bool,

    /// Don't repropagate cost improvements below 0.01 ticks. They're all just floating point inaccuracies,
    /// and there's no point.
    pub minimum_improvement_repropagation: bool,

    /// After calculating a path (potentially through cached chunks), artificially cut it off to just the part that is
    /// entirely within currently loaded chunks. Improves path safety because cached chunks are heavily simplified.
    ///
    /// This is much safer to leave off now, and makes pathing more efficient. More explanation in the issue.
    ///
    /// See [Issue #114](https://github.com/cabaletta/baritone/issues/114)
    pub cutoff_at_load_boundary: bool,

    /// If a movement's cost increases by more than this amount between calculation and execution (due to changes
    /// in the environment / world), cancel and recalculate
    pub max_cost_increase: f64,

    /// Stop 5 movements before anything that made the path COST_INF.
    /// For example, if lava has spread across the path, don't walk right up to it then recalculate, it might
    /// still be spreading lol
    pub cost_verification_lookahead: i32,

    /// Static cutoff factor. 0.9 means cut off the last 10% of all paths, regardless of chunk load state
    pub path_cutoff_factor: f64,

    /// Only apply static cutoff for paths of at least this length (in terms of number of movements)
    pub path_cutoff_minimum_length: i32,

    /// Start planning the next path once the remaining movements tick estimates sum up to less than this value
    pub planning_tick_lookahead: i32,

    /// Default size of the Long2ObjectOpenHashMap used in pathing
    pub pathing_map_default_size: i32,

    /// Load factor coefficient for the Long2ObjectOpenHashMap used in pathing
    ///
    /// Decrease for faster map operations, but higher memory usage
    pub pathing_map_load_factor: f32,

    /// How far are you allowed to fall onto solid ground (without a water bucket)?
    /// 3 won't deal any damage. But if you just want to get down the mountain quickly and you have
    /// Feather Falling IV, you might set it a bit higher, like 4 or 5.
    pub max_fall_height_no_water: i32,

    /// How far are you allowed to fall onto solid ground (with a water bucket)?
    /// It's not that reliable, so I've set it below what would kill an unarmored player (23)
    pub max_fall_height_bucket: i32,

    /// Is it okay to sprint through a descend followed by a diagonal?
    /// The player overshoots the landing, but not enough to fall off. And the diagonal ensures that there isn't
    /// lava or anything that's !canWalkInto in that space, so it's technically safe, just a little sketchy.
    ///
    /// Note: this is *not* related to the allowDiagonalDescend setting, that is a completely different thing.
    pub allow_overshoot_diagonal_descend: bool,

    /// If your goal is a GoalBlock in an unloaded chunk, assume it's far enough away that the Y coord
    /// doesn't matter yet, and replace it with a GoalXZ to the same place before calculating a path.
    /// Once a segment ends within chunk load range of the GoalBlock, it will go back to normal behavior
    /// of considering the Y coord. The reasoning is that if your X and Z are 10,000 blocks away,
    /// your Y coordinate's accuracy doesn't matter at all until you get much much closer.
    pub simplify_unloaded_y_coord: bool,

    /// If a movement takes this many ticks more than its initial cost estimate, cancel it
    pub movement_timeout_ticks: i32,

    /// Pathing ends after this amount of time, but only if a path has been found
    ///
    /// If no valid path (length above the minimum) has been found, pathing continues up until the failure timeout
    #[serde(rename = "primaryTimeoutMS")]
    pub primary_timeout_ms: i64,

    /// Pathing can never take longer than this, even if that means failing to find any path at all
    #[serde(rename = "failureTimeoutMS")]
    pub failure_timeout_ms: i64,

    /// Planning ahead while executing a segment ends after this amount of time, but only if a path has been found
    ///
    /// If no valid path (length above the minimum) has been found, pathing continues up until the failure timeout
    #[serde(rename = "planAheadPrimaryTimeoutMS")]
    pub plan_ahead_primary_timeout_ms: i64,

    /// Planning ahead while executing a segment can never take longer than this, even if that means failing to find any path at all
    #[serde(rename = "planAheadFailureTimeoutMS")]
    pub plan_ahead_failure_timeout_ms: i64,

    /// For debugging, consider nodes much much slower
    pub slow_path: bool,

    /// Milliseconds between each node
    #[serde(rename = "slowPathTimeDelayMS")]
    pub slow_path_time_delay_ms: i64,

    /// The alternative timeout number when slowPath is on
    #[serde(rename = "slowPathTimeoutMS")]
    pub slow_path_timeout_ms: i64,

    /// Move without having to force the client-sided rotations
    pub free_look: bool,

    /// Break and place blocks without having to force the client-sided rotations. Requires `#freeLook`.
    pub block_free_look: bool,

    /// Automatically elytra fly without having to force the client-sided rotations.
    pub elytra_free_look: bool,

    /// Forces the client-sided yaw rotation to an average of the last `#smoothLookTicks` of server-sided rotations.
    pub smooth_look: bool,

    /// Same as `#smoothLook` but for elytra flying.
    pub elytra_smooth_look: bool,

    /// The number of ticks to average across for `#smoothLook`;
    pub smooth_look_ticks: i32,

    /// When true, the player will remain with its existing look direction as often as possible.
    /// Although, in some cases this can get it stuck, hence this setting to disable that behavior.
    pub remain_with_existing_look_direction: bool,

    /// Will cause some minor behavioral differences to ensure that Baritone works on anticheats.
    ///
    /// At the moment this will silently set the player's rotations when using freeLook so you're not sprinting in
    /// directions other than forward, which is picken up by more "advanced" anticheats like AAC, but not NCP.
    pub anti_cheat_compatibility: bool,

    /// Exclusively use cached chunks for pathing
    ///
    /// Never turn this on
    pub path_through_cached_only: bool,

    /// Continue sprinting while in water
    pub sprint_in_water: bool,

    /// When GetToBlockProcess or MineProcess fails to calculate a path, instead of just giving up, mark the closest instance
    /// of that block as "unreachable" and go towards the next closest. GetToBlock expands this search to the whole "vein"; MineProcess does not.
    /// This is because MineProcess finds individual impossible blocks (like one block in a vein that has gravel on top then lava, so it can't break)
    /// Whereas GetToBlock should blacklist the whole "vein" if it can't get to any of them.
    pub blacklist_closest_on_failure: bool,

    /// Censor coordinates in goals and block positions
    pub censor_coordinates: bool,

    /// Stop using tools just before they are going to break.
    pub item_saver: bool,

    /// Durability to leave on the tool when using itemSaver
    pub item_saver_threshold: i32,

    /// Always prefer silk touch tools over regular tools. This will not sacrifice speed, but it will always prefer silk
    /// touch tools over other tools of the same speed. This includes always choosing ANY silk touch tool over your hand.
    pub prefer_silk_touch: bool,

    /// Don't stop walking forward when you need to break blocks in your way
    pub walk_while_breaking: bool,

    /// When a new segment is calculated that doesn't overlap with the current one, but simply begins where the current segment ends,
    /// splice it on and make a longer combined path. If this setting is off, any planned segment will not be spliced and will instead
    /// be the "next path" in PathingBehavior, and will only start after this one ends. Turning this off hurts planning ahead,
    /// because the next segment will exist even if it's very short.
    ///
    /// See `planning_tick_lookahead`
    pub splice_path: bool,

    /// If we are more than 300 movements into the current path, discard the oldest segments, as they are no longer useful
    pub max_path_history_length: i32,

    /// If the current path is too long, cut off this many movements from the beginning.
    pub path_history_cutoff_amount: i32,

    /// Rescan for the goal once every 5 ticks.
    /// Set to 0 to disable.
    pub mine_goal_update_interval: i32,

    /// After finding this many instances of the target block in the cache, it will stop expanding outward the chunk search.
    pub max_cached_world_scan_count: i32,

    /// Mine will not scan for or remember more than this many target locations.
    /// Note that the number of locations retrieved from cache is additionaly
    /// limited by `#maxCachedWorldScanCount`.
    pub mine_max_ore_locations_count: i32,

    /// Sets the minimum y level whilst mining - set to 0 to turn off.
    /// if world has negative y values, subtract the min world height to get the value to put here
    pub min_y_level_while_mining: i32,

    /// Sets the maximum y level to mine ores at.
    pub max_y_level_while_mining: i32,

    /// This will only allow baritone to mine exposed ores, can be used to stop ore obfuscators on servers that use them.
    pub allow_only_exposed_ores: bool,

    /// When allowOnlyExposedOres is enabled this is the distance around to search.
    ///
    /// It is recommended to keep this value low, as it dramatically increases calculation times.
    pub allow_only_exposed_ores_distance: i32,

    /// When GetToBlock or non-legit Mine doesn't know any locations for the desired block, explore randomly instead of giving up.
    pub explore_for_blocks: bool,

    /// While exploring the world, offset the closest unloaded chunk by this much in both axes.
    ///
    /// This can result in more efficient loading, if you set this to the render distance.
    pub world_exploring_chunk_offset: i32,

    /// Take the 10 closest chunks, even if they aren't strictly tied for distance metric from origin.
    pub explore_chunk_set_minimum_size: i32,

    /// Attempt to maintain Y coordinate while exploring
    ///
    /// -1 to disable
    pub explore_maintain_y: i32,

    /// Replant normal Crops while farming and leave cactus and sugarcane to regrow
    pub replant_crops: bool,

    /// Replant nether wart while farming. This setting only has an effect when replantCrops is also enabled
    pub replant_nether_wart: bool,

    /// When enabled, farming will be restricted to the current selection.
    pub farm_using_selection: bool,

    /// Farming will scan for at most this many blocks.
    pub farm_max_scan_size: i32,

    /// When the cache scan gives less blocks than the maximum threshold (but still above zero), scan the main world too.
    ///
    /// Only if you have a beefy CPU and automatically mine blocks that are in cache
    pub extend_cache_on_threshold: bool,

    /// While mining, should it also consider dropped items of the correct type as a pathing destination (as well as ore blocks)?
    pub mine_scan_dropped_items: bool,

    /// While mining, wait this number of milliseconds after mining an ore to see if it will drop an item
    /// instead of immediately going onto the next one
    ///
    /// Thanks Louca
    #[serde(rename = "mineDropLoiterDurationMSThanksLouca")]
    pub mine_drop_loiter_duration_ms_thanks_louca: i64,

    /// Cancel the current path if the goal has changed, and the path originally ended in the goal but doesn't anymore.
    ///
    /// Currently only runs when either MineBehavior or FollowBehavior is active.
    ///
    /// For example, if Baritone is doing "mine iron_ore", the instant it breaks the ore (and it becomes air), that location
    /// is no longer a goal. This means that if this setting is true, it will stop there. If this setting were off, it would
    /// continue with its path, and walk into that location. The tradeoff is if this setting is true, it mines ores much faster
    /// since it doesn't waste any time getting into locations that no longer contain ores, but on the other hand, it misses
    /// some drops, and continues on without ever picking them up.
    ///
    /// Also on cosmic prisons this should be set to true since you don't actually mine the ore it just gets replaced with stone.
    pub cancel_on_goal_invalidation: bool,

    /// The "axis" command (aka GoalAxis) will go to a axis, or diagonal axis, at this Y level.
    pub axis_height: i32,

    /// Disconnect from the server upon arriving at your goal
    pub disconnect_on_arrival: bool,

    /// Disallow MineBehavior from using X-Ray to see where the ores are. Turn this option on to force it to mine "legit"
    /// where it will only mine an ore once it can actually see it, so it won't do or know anything that a normal player
    /// couldn't. If you don't want it to look like you're X-Raying, turn this on
    /// This will always explore, regardless of exploreForBlocks
    pub legit_mine: bool,

    /// What Y level to go to for legit strip mining
    pub legit_mine_y_level: i32,

    /// Magically see ores that are separated diagonally from existing ores. Basically like mining around the ores that it finds
    /// in case there's one there touching it diagonally, except it checks it un-legit-ly without having the mine blocks to see it.
    /// You can decide whether this looks plausible or not.
    ///
    /// This is disabled because it results in some weird behavior. For example, it can """see""" the top block of a vein of iron_ore
    /// through a lava lake. This isn't an issue normally since it won't consider anything touching lava, so it just ignores it.
    /// However, this setting expands that and allows it to see the entire vein so it'll mine under the lava lake to get the iron that
    /// it can reach without mining blocks adjacent to lava. This really defeats the purpose of legitMine since a player could never
    /// do that lol, so thats one reason why its disabled
    pub legit_mine_include_diagonals: bool,

    /// When mining block of a certain type, try to mine two at once instead of one.
    /// If the block above is also a goal block, set GoalBlock instead of GoalTwoBlocks
    /// If the block below is also a goal block, set GoalBlock to the position one down instead of GoalTwoBlocks
    pub force_internal_mining: bool,

    /// Modification to the previous setting, only has effect if forceInternalMining is true
    /// If true, only apply the previous setting if the block adjacent to the goal isn't air.
    pub internal_mining_air_exception: bool,

    /// The actual GoalNear is set this distance away from the entity you're following
    ///
    /// For example, set followOffsetDistance to 5 and followRadius to 0 to always stay precisely 5 blocks north of your follow target.
    pub follow_offset_distance: f64,

    /// The actual GoalNear is set in this direction from the entity you're following. This value is in degrees.
    pub follow_offset_direction: f32,

    /// The radius (for the GoalNear) of how close to your target position you actually have to be
    pub follow_radius: i32,

    /// The maximum distance to the entity you're following
    pub follow_target_max_distance: i32,

    /// Turn this on if your exploration filter is enormous, you don't want it to check if it's done,
    /// and you are just fine with it just hanging on completion
    pub disable_completion_check: bool,

    /// Use sword to mine.
    pub use_sword_to_mine: bool,

    /// Desktop notification on path complete
    pub notification_on_path_complete: bool,

    /// Desktop notification on farm fail
    pub notification_on_farm_fail: bool,

    /// Desktop notification on explore finished
    pub notification_on_explore_finished: bool,

    /// Desktop notification on mine fail
    pub notification_on_mine_fail: bool,

    /// Sneak when magma blocks are under feet
    pub allow_walk_on_magma_blocks: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_break: true,
            allow_break_anyway: vec![],
            allow_sprint: true,
            allow_place: true,
            allow_place_in_fluids_source: true,
            allow_place_in_fluids_flow: true,
            allow_inventory: false,
            ticks_between_inventory_moves: 1,
            inventory_move_only_if_stationary: false,
            assume_external_auto_tool: false,
            auto_tool: true,
            block_placement_penalty: 20.0,
            block_break_additional_penalty: 2.0,
            jump_penalty: 2.0,
            walk_on_water_one_penalty: 3.0,
            strict_liquid_check: false,
            allow_water_bucket_fall: true,
            assume_walk_on_water: false,
            assume_walk_on_lava: false,
            assume_step: false,
            assume_safe_walk: false,
            allow_jump_at_build_limit: false,
            allow_parkour_ascend: true,
            allow_diagonal_descend: false,
            allow_diagonal_ascend: false,
            allow_downward: true,
            acceptable_throwaway_items: vec![
                "minecraft:dirt".into(),
                "minecraft:cobblestone".into(),
                "minecraft:netherrack".into(),
                "minecraft:stone".into(),
            ],
            blocks_to_avoid: vec!["minecraft:tripwire".into()],
            blocks_to_disallow_breaking: vec![],
            blocks_to_avoid_breaking: vec![
                "minecraft:crafting_table".into(),
                "minecraft:furnace".into(),
                "minecraft:chest".into(),
                "minecraft:trapped_chest".into(),
            ],
            avoid_breaking_multiplier: 0.1,
            avoid_updating_falling_blocks: true,
            allow_vines: false,
            allow_walk_on_bottom_slab: true,
            allow_parkour: false,
            allow_parkour_place: false,
            consider_potion_effects: true,
            sprint_ascends: true,
            overshoot_traverse: true,
            pause_mining_for_falling_blocks: true,
            right_click_speed: 4,
            random_looking113: 2.0,
            block_break_speed: 6,
            random_looking: 0.01,
            cost_heuristic: 3.563,
            pathing_max_chunk_border_fetch: 50,
            backtrack_cost_favoring_coefficient: 0.5,
            avoidance: false,
            mob_spawner_avoidance_coefficient: 2.0,
            mob_spawner_avoidance_radius: 16,
            mob_avoidance_coefficient: 1.5,
            mob_avoidance_radius: 8,
            right_click_container_on_arrival: true,
            enter_portal: true,
            minimum_improvement_repropagation: true,
            cutoff_at_load_boundary: false,
            max_cost_increase: 10.0,
            cost_verification_lookahead: 5,
            path_cutoff_factor: 0.9,
            path_cutoff_minimum_length: 30,
            planning_tick_lookahead: 150,
            pathing_map_default_size: 1024,
            pathing_map_load_factor: 0.75,
            max_fall_height_no_water: 3,
            max_fall_height_bucket: 20,
            allow_overshoot_diagonal_descend: true,
            simplify_unloaded_y_coord: true,
            movement_timeout_ticks: 100,
            primary_timeout_ms: 500,
            failure_timeout_ms: 2000,
            plan_ahead_primary_timeout_ms: 4000,
            plan_ahead_failure_timeout_ms: 5000,
            slow_path: false,
            slow_path_time_delay_ms: 100,
            slow_path_timeout_ms: 40000,
            free_look: true,
            block_free_look: false,
            elytra_free_look: true,
            smooth_look: false,
            elytra_smooth_look: false,
            smooth_look_ticks: 5,
            remain_with_existing_look_direction: true,
            anti_cheat_compatibility: true,
            path_through_cached_only: false,
            sprint_in_water: true,
            blacklist_closest_on_failure: true,
            censor_coordinates: false,
            item_saver: false,
            item_saver_threshold: 10,
            prefer_silk_touch: false,
            walk_while_breaking: true,
            splice_path: true,
            max_path_history_length: 300,
            path_history_cutoff_amount: 50,
            mine_goal_update_interval: 5,
            max_cached_world_scan_count: 10,
            mine_max_ore_locations_count: 64,
            min_y_level_while_mining: 0,
            max_y_level_while_mining: 2031,
            allow_only_exposed_ores: false,
            allow_only_exposed_ores_distance: 1,
            explore_for_blocks: true,
            world_exploring_chunk_offset: 0,
            explore_chunk_set_minimum_size: 10,
            explore_maintain_y: 64,
            replant_crops: true,
            replant_nether_wart: false,
            farm_using_selection: false,
            farm_max_scan_size: 256,
            extend_cache_on_threshold: false,
            mine_scan_dropped_items: true,
            mine_drop_loiter_duration_ms_thanks_louca: 250,
            cancel_on_goal_invalidation: true,
            axis_height: 120,
            disconnect_on_arrival: false,
            legit_mine: false,
            legit_mine_y_level: -59,
            legit_mine_include_diagonals: false,
            force_internal_mining: true,
            internal_mining_air_exception: true,
            follow_offset_distance: 0.0,
            follow_offset_direction: 0.0,
            follow_radius: 3,
            follow_target_max_distance: 0,
            disable_completion_check: false,
            use_sword_to_mine: true,
            notification_on_path_complete: true,
            notification_on_farm_fail: true,
            notification_on_explore_finished: true,
            notification_on_mine_fail: true,
            allow_walk_on_magma_blocks: false,
        }
    }
}

static SETTINGS: LazyLock<ArcSwap<Settings>> =
    LazyLock::new(|| ArcSwap::from_pointee(Settings::default()));

/// The active settings (`BaritoneAPI.getSettings()` / `Baritone.settings()`).
pub fn settings() -> Guard<Arc<Settings>> {
    SETTINGS.load()
}

/// Replaces the active settings.
pub fn set_settings(settings: Settings) {
    SETTINGS.store(Arc::new(settings));
}

/// Applies `f` to a copy of the active settings and publishes the result. Concurrent updates
/// are retried, so `f` may run more than once.
pub fn update_settings(mut f: impl FnMut(&mut Settings)) {
    SETTINGS.rcu(|current| {
        let mut next = Settings::clone(current);
        f(&mut next);
        next
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upstream names of every ported setting.
    const UPSTREAM_NAMES: &[&str] = &[
        "acceptableThrowawayItems",
        "allowBreak",
        "allowBreakAnyway",
        "allowDiagonalAscend",
        "allowDiagonalDescend",
        "allowDownward",
        "allowInventory",
        "allowJumpAtBuildLimit",
        "allowOnlyExposedOres",
        "allowOnlyExposedOresDistance",
        "allowOvershootDiagonalDescend",
        "allowParkour",
        "allowParkourAscend",
        "allowParkourPlace",
        "allowPlace",
        "allowPlaceInFluidsFlow",
        "allowPlaceInFluidsSource",
        "allowSprint",
        "allowVines",
        "allowWalkOnBottomSlab",
        "allowWalkOnMagmaBlocks",
        "allowWaterBucketFall",
        "antiCheatCompatibility",
        "assumeExternalAutoTool",
        "assumeSafeWalk",
        "assumeStep",
        "assumeWalkOnLava",
        "assumeWalkOnWater",
        "autoTool",
        "avoidance",
        "avoidBreakingMultiplier",
        "avoidUpdatingFallingBlocks",
        "axisHeight",
        "backtrackCostFavoringCoefficient",
        "blacklistClosestOnFailure",
        "blockBreakAdditionalPenalty",
        "blockBreakSpeed",
        "blockFreeLook",
        "blockPlacementPenalty",
        "blocksToAvoid",
        "blocksToAvoidBreaking",
        "blocksToDisallowBreaking",
        "cancelOnGoalInvalidation",
        "censorCoordinates",
        "considerPotionEffects",
        "costHeuristic",
        "costVerificationLookahead",
        "cutoffAtLoadBoundary",
        "disableCompletionCheck",
        "disconnectOnArrival",
        "elytraFreeLook",
        "elytraSmoothLook",
        "enterPortal",
        "exploreChunkSetMinimumSize",
        "exploreForBlocks",
        "exploreMaintainY",
        "extendCacheOnThreshold",
        "failureTimeoutMS",
        "farmMaxScanSize",
        "farmUsingSelection",
        "followOffsetDirection",
        "followOffsetDistance",
        "followRadius",
        "followTargetMaxDistance",
        "forceInternalMining",
        "freeLook",
        "internalMiningAirException",
        "inventoryMoveOnlyIfStationary",
        "itemSaver",
        "itemSaverThreshold",
        "jumpPenalty",
        "legitMine",
        "legitMineIncludeDiagonals",
        "legitMineYLevel",
        "maxCachedWorldScanCount",
        "maxCostIncrease",
        "maxFallHeightBucket",
        "maxFallHeightNoWater",
        "maxPathHistoryLength",
        "maxYLevelWhileMining",
        "mineDropLoiterDurationMSThanksLouca",
        "mineGoalUpdateInterval",
        "mineMaxOreLocationsCount",
        "mineScanDroppedItems",
        "minimumImprovementRepropagation",
        "minYLevelWhileMining",
        "mobAvoidanceCoefficient",
        "mobAvoidanceRadius",
        "mobSpawnerAvoidanceCoefficient",
        "mobSpawnerAvoidanceRadius",
        "movementTimeoutTicks",
        "notificationOnExploreFinished",
        "notificationOnFarmFail",
        "notificationOnMineFail",
        "notificationOnPathComplete",
        "overshootTraverse",
        "pathCutoffFactor",
        "pathCutoffMinimumLength",
        "pathHistoryCutoffAmount",
        "pathingMapDefaultSize",
        "pathingMapLoadFactor",
        "pathingMaxChunkBorderFetch",
        "pathThroughCachedOnly",
        "pauseMiningForFallingBlocks",
        "planAheadFailureTimeoutMS",
        "planAheadPrimaryTimeoutMS",
        "planningTickLookahead",
        "preferSilkTouch",
        "primaryTimeoutMS",
        "randomLooking",
        "randomLooking113",
        "remainWithExistingLookDirection",
        "replantCrops",
        "replantNetherWart",
        "rightClickContainerOnArrival",
        "rightClickSpeed",
        "simplifyUnloadedYCoord",
        "slowPath",
        "slowPathTimeDelayMS",
        "slowPathTimeoutMS",
        "smoothLook",
        "smoothLookTicks",
        "splicePath",
        "sprintAscends",
        "sprintInWater",
        "strictLiquidCheck",
        "ticksBetweenInventoryMoves",
        "useSwordToMine",
        "walkOnWaterOnePenalty",
        "walkWhileBreaking",
        "worldExploringChunkOffset",
    ];

    #[test]
    fn serialized_names_are_upstream_names() {
        let value = serde_json::to_value(Settings::default()).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut expected = UPSTREAM_NAMES.to_vec();
        expected.sort_unstable();
        assert_eq!(keys, expected);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let parsed: Settings =
            serde_json::from_str(r#"{"allowParkour": true, "primaryTimeoutMS": 1234}"#).unwrap();
        assert_eq!(
            parsed,
            Settings {
                allow_parkour: true,
                primary_timeout_ms: 1234,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn round_trip() {
        let settings = Settings {
            cost_heuristic: 4.0,
            blocks_to_avoid: vec![],
            ..Settings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
    }
}
