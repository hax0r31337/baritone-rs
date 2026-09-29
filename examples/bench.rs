//! The port's side of `tools/bench/run.sh`: replays the queries `refgen.Bench` ran on upstream's
//! `AStarPathFinder`, on the same worlds (chunks the vanilla server generated), with the same
//! settings, player, node budget and timing, checks that every calculation ends exactly like
//! upstream's (result, nodes, path, cost, best path so far), and prints the comparison, also to
//! `report.md` in the bench directory.
//!
//! Usage: `bench <bench dir>` (what `refgen.Bench` wrote: `<world>.json` and `<world>.world.gz`).

use std::fmt::Write as _;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use baritone::api::pathing::calc::IPathFinder;
use baritone::api::pathing::goals::{
    Goal, GoalBlock, GoalGetToBlock, GoalNear, GoalXZ, GoalYLevel,
};
use baritone::api::utils::BetterBlockPos;
use baritone::api::utils::path_calculation_result::Type;
use baritone::host::paletted::{self, PalettedStorage, SECTION_VOLUME};
use baritone::host::{
    BlockStateTable, Chunk, DimensionType, Inventory, MobEffectInstance, Player, SubChunk, World,
};
use baritone::pathing::calc::AStarPathFinder;
use baritone::pathing::movement::CalculationContext;
use baritone::utils::pathing::Favoring;
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct BenchWorld {
    name: String,
    nether: bool,
    world: String,
    budget: u32,
    timeout: i64,
    warmup: usize,
    reps: usize,
    java: String,
    jit_warmup: JitWarmup,
    /// JIT compilation during this world's timed passes.
    jit_ms_timed: i64,
    inventory: Inventory,
    player: PlayerSpec,
    queries: Vec<Query>,
}

/// `Bench.warmUpJit`: the untimed passes over every world's queries before anything is timed.
#[derive(Deserialize)]
struct JitWarmup {
    passes: Vec<JitPass>,
    /// The JIT stopped compiling and pass times settled before the last allowed pass.
    settled: bool,
}

#[derive(Deserialize)]
struct JitPass {
    ns: u64,
    jit_ms: i64,
}

#[derive(Deserialize)]
struct PlayerSpec {
    food: i32,
    frost_walker: i32,
    /// float bits
    water_efficiency: Option<u32>,
    effects: Vec<(String, i32)>,
    throwaway: bool,
}

#[derive(Deserialize)]
struct Query {
    kind: String,
    start: [i32; 3],
    goal: Value,
    result: Outcome,
    times_ns: Vec<u64>,
}

/// How a calculation ended; see `Bench.result`.
#[derive(Deserialize, Debug, PartialEq)]
struct Outcome {
    #[serde(rename = "type")]
    type_: String,
    /// `is_in_goal` calls: nodes considered.
    calls: u32,
    map_size: usize,
    positions: Option<Vec<i32>>,
    /// Sum of the movement costs, bits.
    cost: Option<i64>,
    num_nodes: Option<i32>,
    best_so_far: Option<Vec<i32>>,
}

/// `PathRefGen.CancellingGoal`: cancels the search on the `after`th `is_in_goal` call.
#[derive(Debug)]
struct BudgetGoal {
    goal: Arc<dyn Goal>,
    after: u32,
    calls: AtomicU32,
    cancel: OnceLock<Arc<AtomicBool>>,
}

impl std::fmt::Display for BudgetGoal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BudgetGoal{{{}, after {}}}", self.goal, self.after)
    }
}

impl Goal for BudgetGoal {
    fn is_in_goal(&self, x: i32, y: i32, z: i32) -> bool {
        if self.calls.fetch_add(1, Ordering::Relaxed) + 1 == self.after {
            self.cancel.get().unwrap().store(true, Ordering::Relaxed);
        }
        self.goal.is_in_goal(x, y, z)
    }

    fn heuristic(&self, x: i32, y: i32, z: i32) -> f64 {
        self.goal.heuristic(x, y, z)
    }

    fn heuristic_at_goal(&self) -> f64 {
        self.goal.heuristic_at_goal()
    }

    fn equals(&self, _other: &dyn Goal) -> bool {
        false
    }
}

fn gunzip(path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    GzDecoder::new(BufReader::new(fs::File::open(path).unwrap()))
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

fn table() -> Arc<BlockStateTable> {
    #[derive(Deserialize)]
    struct Blocks {
        table: BlockStateTable,
    }
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reference/blocks.json.gz");
    let blocks: Blocks = serde_json::from_slice(&gunzip(&path)).unwrap();
    Arc::new(blocks.table)
}

/// Big-endian values, as `DataOutputStream` writes them.
struct Reader {
    bytes: Vec<u8>,
    at: usize,
}

impl Reader {
    fn u16(&mut self) -> u16 {
        let v = u16::from_be_bytes([self.bytes[self.at], self.bytes[self.at + 1]]);
        self.at += 2;
        v
    }

    fn i32(&mut self) -> i32 {
        let v = i32::from_be_bytes(self.bytes[self.at..self.at + 4].try_into().unwrap());
        self.at += 4;
        v
    }
}

/// Reads `RegionWorld.export`.
fn load_world(path: &Path, table: &Arc<BlockStateTable>, nether: bool) -> Arc<World> {
    let mut r = Reader {
        bytes: gunzip(path),
        at: 0,
    };
    let min_y = r.i32();
    let height = r.i32();
    let cmin = r.i32();
    let cmax = r.i32();
    let dimension = DimensionType {
        min_y,
        height,
        water_evaporates: nether,
    };
    let mut world = World::new(Arc::clone(table), dimension).unwrap();
    let air = table.air().id;
    let mut palette = Vec::new();
    let mut ids = [0u32; SECTION_VOLUME];
    for cx in cmin..=cmax {
        for cz in cmin..=cmax {
            let mut chunk = Chunk::new(dimension.section_count());
            for section in 0..dimension.section_count() {
                palette.clear();
                for _ in 0..r.u16() {
                    palette.push(r.i32() as u32);
                }
                if palette.len() == 1 {
                    if palette[0] != air {
                        chunk.set_section(section, Some(SubChunk::filled(palette[0])));
                    }
                    continue;
                }
                // the game's order is (y << 8) | (z << 4) | x
                for i in 0..SECTION_VOLUME {
                    let (x, y, z) = (i & 15, i >> 8, (i >> 4) & 15);
                    ids[paletted::index(x, y, z)] = palette[r.u16() as usize];
                }
                chunk.set_section(
                    section,
                    Some(SubChunk::from_storage(PalettedStorage::from_ids(&ids))),
                );
            }
            world.load_chunk(cx, cz, chunk).unwrap();
        }
    }
    assert_eq!(r.at, r.bytes.len(), "trailing bytes in {}", path.display());
    Arc::new(world)
}

fn int(v: &Value) -> i32 {
    v.as_i64().unwrap() as i32
}

fn build_goal(spec: &Value) -> Arc<dyn Goal> {
    let i = |key: &str| int(&spec[key]);
    let p = || BetterBlockPos::new(i("x"), i("y"), i("z"));
    match spec["kind"].as_str().unwrap() {
        "GoalBlock" => Arc::new(GoalBlock::new(i("x"), i("y"), i("z"))),
        "GoalGetToBlock" => Arc::new(GoalGetToBlock::new(p())),
        "GoalNear" => Arc::new(GoalNear::new(p(), i("range"))),
        "GoalXZ" => Arc::new(GoalXZ::new(i("x"), i("z"))),
        "GoalYLevel" => Arc::new(GoalYLevel::new(i("level"))),
        kind => panic!("unknown goal kind {kind}"),
    }
}

fn flat(positions: &[BetterBlockPos]) -> Vec<i32> {
    positions.iter().flat_map(|p| [p.x, p.y, p.z]).collect()
}

fn type_name(t: Type) -> &'static str {
    match t {
        Type::SuccessToGoal => "SUCCESS_TO_GOAL",
        Type::SuccessSegment => "SUCCESS_SEGMENT",
        Type::Failure => "FAILURE",
        Type::Cancellation => "CANCELLATION",
        Type::Exception => "EXCEPTION",
    }
}

/// One timed calculation, and how it ended.
fn run(
    world: &Arc<World>,
    player: &Arc<Player>,
    bench: &BenchWorld,
    query: &Query,
    goal: &Arc<dyn Goal>,
) -> (u64, Outcome) {
    let context = CalculationContext::new(
        Arc::clone(world),
        Arc::clone(player),
        bench.player.throwaway,
        true,
    );
    let favoring = Favoring::new(None, &context);
    let budget = Arc::new(BudgetGoal {
        goal: Arc::clone(goal),
        after: bench.budget,
        calls: AtomicU32::new(0),
        cancel: OnceLock::new(),
    });
    let [x, y, z] = query.start;

    let t0 = Instant::now();
    let mut finder = AStarPathFinder::new(
        BetterBlockPos::new(x, y, z),
        x,
        y,
        z,
        Arc::clone(&budget) as Arc<dyn Goal>,
        favoring,
        context,
    );
    budget.cancel.set(finder.search().cancel_handle()).unwrap();
    let result = finder.calculate(bench.timeout, bench.timeout);
    let time = t0.elapsed().as_nanos() as u64;

    let path = result.get_path();
    let outcome = Outcome {
        type_: type_name(result.get_type()).to_owned(),
        calls: budget.calls.load(Ordering::Relaxed),
        map_size: finder.search().map_size(),
        positions: path.map(|p| flat(p.positions())),
        cost: path.map(|p| {
            let mut cost = 0.0;
            for m in p.movements() {
                cost += m.get_cost();
            }
            cost.to_bits() as i64
        }),
        num_nodes: path.map(|p| p.get_num_nodes_considered()),
        best_so_far: finder.best_path_so_far().map(|p| flat(p.positions())),
    };
    (time, outcome)
}

fn median(times: &[u64]) -> f64 {
    let mut t = times.to_vec();
    t.sort_unstable();
    let n = t.len();
    if n % 2 == 1 {
        t[n / 2] as f64
    } else {
        (t[n / 2 - 1] + t[n / 2]) as f64 / 2.0
    }
}

struct Row {
    world: String,
    index: usize,
    kind: String,
    outcome: String,
    calls: u32,
    java_ns: f64,
    rust_ns: f64,
    same: bool,
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("usage: bench <bench dir>"));
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no worlds in {}", dir.display());
    let table = table();

    let mut rows = Vec::new();
    let mut mismatches = Vec::new();
    let mut header = None;
    let mut jit_timed = Vec::new();
    for file in &files {
        let bench: BenchWorld = serde_json::from_slice(&fs::read(file).unwrap()).unwrap();
        let world = load_world(&dir.join(&bench.world), &table, bench.nether);
        let player = Arc::new(Player {
            inventory: bench.inventory.clone(),
            food_level: bench.player.food,
            effects: bench
                .player
                .effects
                .iter()
                .map(|(effect, amplifier)| MobEffectInstance {
                    effect: effect.clone(),
                    amplifier: *amplifier,
                })
                .collect(),
            frost_walker: bench.player.frost_walker,
            water_movement_efficiency: bench.player.water_efficiency.map(f32::from_bits),
            ..Player::default()
        });
        let goals: Vec<Arc<dyn Goal>> = bench.queries.iter().map(|q| build_goal(&q.goal)).collect();
        let mut times = vec![Vec::new(); bench.queries.len()];
        let mut outcomes = Vec::new();
        for pass in 0..bench.warmup + bench.reps {
            for (i, query) in bench.queries.iter().enumerate() {
                let (time, outcome) = run(&world, &player, &bench, query, &goals[i]);
                if pass >= bench.warmup {
                    times[i].push(time);
                }
                if pass == 0 {
                    outcomes.push(outcome);
                }
            }
            eprintln!(
                "{}: pass {} of {}",
                bench.name,
                pass + 1,
                bench.warmup + bench.reps
            );
        }
        for (i, query) in bench.queries.iter().enumerate() {
            let same = outcomes[i] == query.result;
            if !same {
                mismatches.push(format!(
                    "{} query {i} ({}, from {:?} to {}):\n  port     {:?}\n  upstream {:?}",
                    bench.name, query.kind, query.start, query.goal, outcomes[i], query.result
                ));
            }
            rows.push(Row {
                world: bench.name.clone(),
                index: i,
                kind: query.kind.clone(),
                outcome: query.result.type_.clone(),
                calls: query.result.calls,
                java_ns: median(&query.times_ns),
                rust_ns: median(&times[i]),
                same,
            });
        }
        jit_timed.push((bench.name.clone(), bench.jit_ms_timed));
        header.get_or_insert((
            bench.java,
            bench.budget,
            bench.warmup,
            bench.reps,
            bench.jit_warmup,
        ));
    }

    let (java, budget, warmup, reps, jit) = header.unwrap();
    let jit_passes: Vec<String> = jit
        .passes
        .iter()
        .map(|p| format!("{:.1} s ({} ms compiling)", p.ns as f64 / 1e9, p.jit_ms))
        .collect();
    let mut report = String::new();
    writeln!(
        report,
        "# Path calculation: upstream Baritone vs baritone-rs\n"
    )
    .unwrap();
    writeln!(
        report,
        "Upstream on {java}; the port built with `--release`. Worlds generated by the vanilla 26.3 \
         server, 24x24 chunks each. Upstream's default settings; a calculation is cancelled after \
         {budget} nodes. Times are the median of {reps} timed passes, after {warmup} untimed \
         pass(es) over the same world on both sides.\n"
    )
    .unwrap();
    writeln!(
        report,
        "Before anything is timed, upstream runs every query of every world until the JIT is done \
         ({}): {}. The table shows the JIT compilation during each world's timed passes.\n",
        if jit.settled {
            "it settled"
        } else {
            "**it did not settle**"
        },
        jit_passes.join(", ")
    )
    .unwrap();
    writeln!(
        report,
        "| world | queries | same result | nodes | upstream ms | port ms | speedup (total) | speedup (geomean) | upstream JIT ms while timed |"
    )
    .unwrap();
    writeln!(report, "|---|---:|---:|---:|---:|---:|---:|---:|---:|").unwrap();
    let mut summarize = |name: &str, rows: &[&Row], jit_ms: i64| {
        let java: f64 = rows.iter().map(|r| r.java_ns).sum();
        let rust: f64 = rows.iter().map(|r| r.rust_ns).sum();
        let nodes: u64 = rows.iter().map(|r| r.calls as u64).sum();
        let geomean = (rows
            .iter()
            .map(|r| (r.java_ns / r.rust_ns).ln())
            .sum::<f64>()
            / rows.len() as f64)
            .exp();
        let same = rows.iter().filter(|r| r.same).count();
        writeln!(
            report,
            "| {name} | {} | {same} | {nodes} | {:.1} | {:.1} | {:.2}x | {:.2}x | {jit_ms} |",
            rows.len(),
            java / 1e6,
            rust / 1e6,
            java / rust,
            geomean
        )
        .unwrap();
    };
    let mut worlds: Vec<&str> = rows.iter().map(|r| r.world.as_str()).collect();
    worlds.dedup();
    for world in &worlds {
        let of: Vec<&Row> = rows.iter().filter(|r| r.world == *world).collect();
        let jit_ms = jit_timed.iter().find(|(w, _)| w == world).unwrap().1;
        summarize(world, &of, jit_ms);
    }
    let jit_ms = jit_timed.iter().map(|(_, ms)| ms).sum();
    summarize("**all**", &rows.iter().collect::<Vec<_>>(), jit_ms);

    writeln!(report, "\n## Per query\n").unwrap();
    writeln!(
        report,
        "| world | # | goal | result | nodes | upstream ms | port ms | speedup | upstream Mnodes/s | port Mnodes/s | same |"
    )
    .unwrap();
    writeln!(
        report,
        "|---|---:|---|---|---:|---:|---:|---:|---:|---:|---|"
    )
    .unwrap();
    for r in &rows {
        writeln!(
            report,
            "| {} | {} | {} | {} | {} | {:.2} | {:.2} | {:.2}x | {:.2} | {:.2} | {} |",
            r.world,
            r.index,
            r.kind,
            r.outcome,
            r.calls,
            r.java_ns / 1e6,
            r.rust_ns / 1e6,
            r.java_ns / r.rust_ns,
            r.calls as f64 / r.java_ns * 1e3,
            r.calls as f64 / r.rust_ns * 1e3,
            if r.same { "yes" } else { "**no**" }
        )
        .unwrap();
    }
    if !mismatches.is_empty() {
        writeln!(report, "\n## Different results\n\n```").unwrap();
        for m in &mismatches {
            writeln!(report, "{m}").unwrap();
        }
        writeln!(report, "```").unwrap();
    }
    print!("{report}");
    fs::write(dir.join("report.md"), &report).unwrap();
    if !mismatches.is_empty() {
        eprintln!("{} calculations differ from upstream", mismatches.len());
        std::process::exit(1);
    }
}
