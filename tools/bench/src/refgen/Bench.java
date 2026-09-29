package refgen;

import baritone.api.IBaritone;
import baritone.api.pathing.calc.IPath;
import baritone.api.pathing.goals.GoalGetToBlock;
import baritone.api.pathing.goals.GoalNear;
import baritone.api.pathing.movement.IMovement;
import baritone.api.utils.BetterBlockPos;
import baritone.api.utils.PathCalculationResult;
import baritone.pathing.calc.AStarPathFinder;
import baritone.pathing.movement.CalculationContext;
import baritone.pathing.movement.MovementHelper;
import baritone.utils.BlockStateInterface;
import baritone.utils.pathing.Favoring;
import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.state.BlockState;

import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.management.CompilationMXBean;
import java.lang.management.ManagementFactory;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;

import static refgen.RefGen.GoalCase;
import static refgen.RefGen.d;
import static refgen.RefGen.pos;

/**
 * Benchmarks upstream's {@link AStarPathFinder} on worlds the vanilla server generated (see
 * {@link ServerWorldGen}), and writes the worlds, the queries and the results for the port's
 * side of the benchmark ({@code examples/bench.rs}), which checks that it finds the same paths.
 * <p>
 * Every calculation runs with upstream's default settings, a player with tools, blocks and a
 * water bucket, and timeouts large enough never to end it. It ends at the goal, when the
 * search runs out of loaded chunks or nodes, or after a node budget: the goal cancels the
 * search on its {@code budget}th {@code isInGoal} call (one per node the search considers), so
 * both sides do exactly the same work. The timed part is the path finder's construction and
 * {@code calculate}; the {@link CalculationContext} (with its {@code BlockStateInterface} and
 * {@code PrecomputedData}, filled lazily during the search as upstream does) is built before.
 * Nothing is timed until the JIT has compiled what every world's queries run
 * ({@link #warmUpJit}), and the JIT compilation during the timed passes is recorded.
 * <p>
 * Usage: {@code Bench <server jar> <cache dir> <out dir> <seeds> <queries per world> <warmup
 * passes per world> <timed passes> <budget> <min JIT warm-up passes> <max JIT warm-up passes>},
 * seeds comma separated.
 */
public final class Bench {

    /** Loaded chunks are CMIN..CMAX on both axes: a client with render distance 12. */
    static final int CMIN = -12;
    static final int CMAX = 11;
    /** Queries start within this distance of 0, 0 (on both axes). */
    static final int START_RANGE = 96;
    /** Goals stay this far inside the loaded area. */
    static final int MARGIN = 16;

    static final long TIMEOUT = 600_000L;

    /** The player of every calculation. */
    static final String INVENTORY = "tools";
    static final PathRefGen.PlayerSpec PLAYER = new PathRefGen.PlayerSpec(INVENTORY, 20, 0, null, List.of(), true);

    private static PrintStream out;

    private Bench() {}

    public static void main(String[] args) throws Exception {
        // Bootstrap redirects System.out/err into log4j, which has no provider here
        out = System.out;
        RefGen.err = System.err;
        try {
            run(args);
        } catch (Throwable t) {
            t.printStackTrace(RefGen.err);
            System.exit(1);
        }
        System.exit(0);
    }

    private static void run(String[] args) throws Exception {
        Path serverJar = Path.of(args[0]);
        Path cacheDir = Path.of(args[1]);
        Path outDir = Path.of(args[2]);
        List<Long> seeds = new ArrayList<>();
        for (String seed : args[3].split(",")) {
            seeds.add(Long.parseLong(seed.trim()));
        }
        int queryCount = Integer.parseInt(args[4]);
        int warmup = Integer.parseInt(args[5]);
        int reps = Integer.parseInt(args[6]);
        int budget = Integer.parseInt(args[7]);
        int jitMinPasses = Integer.parseInt(args[8]);
        int jitMaxPasses = Integer.parseInt(args[9]);

        // generate first: the server must not run while anything is timed
        List<Path> dirs = new ArrayList<>();
        for (long seed : seeds) {
            out.println("generating seed " + seed);
            dirs.add(ServerWorldGen.generate(serverJar, cacheDir.resolve("seed-" + seed), seed, CMIN, CMAX));
        }

        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        BlockRefGen.bindRegistries();
        BlockRefGen.apply(new JsonObject());
        baritone = PathRefGen.fakeBaritone();
        PathRefGen.Inventory0 inventory = PathRefGen.inventories().get(INVENTORY);
        Files.createDirectories(outDir);

        List<Case> cases = new ArrayList<>();
        for (int s = 0; s < seeds.size(); s++) {
            long seed = seeds.get(s);
            for (String dimension : ServerWorldGen.DIMENSIONS) {
                String name = dimension + "-" + seed;
                RegionWorld world = RegionWorld.load(ServerWorldGen.regionDir(dirs.get(s), dimension), dimension, CMIN, CMAX);
                world.export(outDir.resolve(name + ".world.gz"));
                List<Query> queries = queries(world, new Random(seed * 31 + dimension.hashCode()), queryCount);
                cases.add(new Case(name, seed, world, queries));
            }
        }

        JsonObject jit = warmUpJit(cases, inventory, budget, jitMinPasses, jitMaxPasses);

        for (Case c : cases) {
            JsonObject o = new JsonObject();
            o.addProperty("name", c.name);
            o.addProperty("seed", c.seed);
            o.addProperty("dimension", c.world.dimension);
            o.addProperty("nether", c.world.nether);
            o.addProperty("world", c.name + ".world.gz");
            o.addProperty("budget", budget);
            o.addProperty("timeout", TIMEOUT);
            o.addProperty("warmup", warmup);
            o.addProperty("reps", reps);
            o.addProperty("java", System.getProperty("java.vm.name") + " " + System.getProperty("java.runtime.version"));
            o.add("jit_warmup", jit);
            o.add("inventory", inventory.json());
            o.add("player", PLAYER.json());
            bench(c, inventory, warmup, reps, budget, o);
            Files.writeString(outDir.resolve(c.name + ".json"), new Gson().toJson(o) + "\n");
        }
    }

    /** A world and its queries. */
    record Case(String name, long seed, RegionWorld world, List<Query> queries) {}

    // region queries

    record Query(String kind, BetterBlockPos start, GoalCase goal) {}

    private static List<Query> queries(RegionWorld world, Random r, int count) throws Exception {
        BlockStateInterface bsi = world.bsi();
        String[] kinds = world.nether
                ? new String[]{"block", "xz", "near", "ore"}
                : new String[]{"block", "xz", "near", "ore", "y_level"};
        List<Query> queries = new ArrayList<>();
        for (int i = 0; i < count; i++) {
            String kind = kinds[i % kinds.length];
            for (int tries = 0; ; tries++) {
                if (tries == 1000) {
                    throw new IllegalStateException("no " + kind + " query in " + world.dimension);
                }
                Query q = query(world, bsi, r, kind);
                if (q != null) {
                    queries.add(q);
                    break;
                }
            }
        }
        return queries;
    }

    private static Query query(RegionWorld world, BlockStateInterface bsi, Random r, String kind) {
        int sx = r.nextInt(2 * START_RANGE) - START_RANGE;
        int sz = r.nextInt(2 * START_RANGE) - START_RANGE;
        Integer sy = standable(world, bsi, sx, sz, r, true);
        if (sy == null) {
            return null;
        }
        BetterBlockPos start = new BetterBlockPos(sx, sy, sz);
        GoalCase goal = switch (kind) {
            case "block" -> {
                int[] xz = around(r, sx, sz, 32, 160);
                Integer y = xz == null ? null : standable(world, bsi, xz[0], xz[1], r, true);
                yield y == null ? null : RefGen.block(xz[0], y, xz[1]);
            }
            case "xz" -> {
                double angle = r.nextDouble() * 2 * Math.PI;
                yield RefGen.xz(sx + (int) (10000 * Math.cos(angle)), sz + (int) (10000 * Math.sin(angle)));
            }
            case "near" -> {
                int[] xz = around(r, sx, sz, 16, 96);
                Integer y = xz == null ? null : standable(world, bsi, xz[0], xz[1], r, false);
                if (y == null) {
                    yield null;
                }
                BlockPos p = new BlockPos(xz[0], y, xz[1]);
                yield new GoalCase(RefGen.spec("GoalNear", "x", p.getX(), "y", p.getY(), "z", p.getZ(), "range", 2), new GoalNear(p, 2), p);
            }
            case "ore" -> {
                BlockPos p = ore(world, r, start, 32);
                yield p == null ? null : new GoalCase(RefGen.spec("GoalGetToBlock", "x", p.getX(), "y", p.getY(), "z", p.getZ()),
                        new GoalGetToBlock(p), p);
            }
            case "y_level" -> RefGen.yLevel(-58);
            default -> throw new IllegalArgumentException(kind);
        };
        return goal == null ? null : new Query(kind, start, goal);
    }

    /** A random x, z between {@code min} and {@code max} blocks from x, z, inside the margin. */
    private static int[] around(Random r, int x, int z, int min, int max) {
        double angle = r.nextDouble() * 2 * Math.PI;
        double dist = min + r.nextDouble() * (max - min);
        int gx = x + (int) (dist * Math.cos(angle));
        int gz = z + (int) (dist * Math.sin(angle));
        int lo = (CMIN << 4) + MARGIN;
        int hi = (CMAX << 4) + 15 - MARGIN;
        if (gx < lo || gx > hi || gz < lo || gz > hi) {
            return null;
        }
        return new int[]{gx, gz};
    }

    /**
     * A y to stand at in the column, out of liquids: the top one, or any. In the Nether, below
     * the bedrock roof.
     */
    private static Integer standable(RegionWorld world, BlockStateInterface bsi, int x, int z, Random r, boolean top) {
        List<Integer> ys = new ArrayList<>();
        int maxY = world.nether ? 120 : world.minY + world.height - 2;
        for (int y = maxY; y > world.minY; y--) {
            if (MovementHelper.canWalkOn(bsi, x, y - 1, z)
                    && MovementHelper.canWalkThrough(bsi, x, y, z)
                    && MovementHelper.canWalkThrough(bsi, x, y + 1, z)
                    && world.get(x, y, z).getFluidState().isEmpty()
                    && world.get(x, y + 1, z).getFluidState().isEmpty()) {
                if (top) {
                    return y;
                }
                ys.add(y);
            }
        }
        return ys.isEmpty() ? null : ys.get(r.nextInt(ys.size()));
    }

    /** A random ore block within {@code range} of {@code p} (on every axis), what mining goes to. */
    private static BlockPos ore(RegionWorld world, Random r, BlockPos p, int range) {
        List<BlockPos> ores = new ArrayList<>();
        for (int x = p.getX() - range; x <= p.getX() + range; x++) {
            for (int z = p.getZ() - range; z <= p.getZ() + range; z++) {
                for (int y = Math.max(world.minY, p.getY() - range); y <= p.getY() + range && y < world.minY + world.height; y++) {
                    BlockState state = world.get(x, y, z);
                    if (BuiltInRegistries.BLOCK.getKey(state.getBlock()).getPath().endsWith("_ore")) {
                        ores.add(new BlockPos(x, y, z));
                    }
                }
            }
        }
        return ores.isEmpty() ? null : ores.get(r.nextInt(ores.size()));
    }

    // endregion

    // region timing

    /**
     * JIT compilation in a warm-up pass at most this share of the pass's time counts as the
     * compiler being done. It never quite stops: a few hundred ms per 100 s pass keep trickling
     * in, on the compiler threads, long after pass times stopped changing.
     */
    static final double JIT_QUIET = 0.005;
    /** ... and pass times this close to the previous pass's count as settled. */
    static final double SETTLED = 0.03;

    /** Set once the registries are bootstrapped. */
    private static IBaritone baritone;

    /** One calculation: its time, and how it ended (if {@code record}). */
    record Run(long nanos, JsonObject result) {}

    private static Run runOnce(RegionWorld world, PathRefGen.Inventory0 inventory, Query q, int budget, boolean record)
            throws Exception {
        CalculationContext context = PathRefGen.context(baritone, world.level, world.bsi(), inventory, PLAYER);
        Favoring favoring = new Favoring((IPath) null, context);
        PathRefGen.CancellingGoal goal = new PathRefGen.CancellingGoal(q.goal.goal(), budget);
        System.gc();

        long t0 = System.nanoTime();
        AStarPathFinder finder = new AStarPathFinder(q.start, q.start.x, q.start.y, q.start.z, goal, favoring, context);
        goal.finder = finder;
        PathCalculationResult result = finder.calculate(TIMEOUT, TIMEOUT);
        long t = System.nanoTime() - t0;

        return new Run(t, record ? result(finder, result, goal.calls) : null);
    }

    /**
     * Runs every query of every world, untimed, until the JIT is done with them: at least
     * {@code minPasses} passes, then until a pass compiles for at most {@link #JIT_QUIET} of its time
     * and takes within {@link #SETTLED} of the previous one (or {@code maxPasses}). Every world
     * is in it, so no world's timed passes meet code the compiler has not seen (a branch the
     * profile never took is an uncommon trap, which deoptimizes and recompiles).
     */
    private static JsonObject warmUpJit(List<Case> cases, PathRefGen.Inventory0 inventory, int budget, int minPasses, int maxPasses)
            throws Exception {
        CompilationMXBean jit = ManagementFactory.getCompilationMXBean();
        JsonArray passes = new JsonArray();
        long previous = -1;
        boolean settled = false;
        PrintStream stdout = System.out;
        // upstream prints progress
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        try {
            for (int pass = 0; pass < maxPasses && !settled; pass++) {
                long compiling0 = jit.getTotalCompilationTime();
                long total = 0;
                for (Case c : cases) {
                    for (Query q : c.queries) {
                        total += runOnce(c.world, inventory, q, budget, false).nanos;
                    }
                }
                long compiling = jit.getTotalCompilationTime() - compiling0;
                double change = previous < 0 ? Double.NaN : Math.abs(total - previous) / (double) previous;
                settled = pass + 1 >= minPasses && compiling * 1e6 <= JIT_QUIET * total && change <= SETTLED;
                previous = total;
                JsonObject p = new JsonObject();
                p.addProperty("ns", total);
                p.addProperty("jit_ms", compiling);
                passes.add(p);
                out.printf("JIT warm-up pass %d: %.1f s, %d ms compiling, %s from the previous pass%n",
                        pass + 1, total / 1e9, compiling, Double.isNaN(change) ? "-" : String.format("%.1f%%", change * 100));
            }
        } finally {
            System.setOut(stdout);
        }
        if (!settled) {
            out.println("warning: the JIT did not settle in " + maxPasses + " warm-up passes");
        }
        JsonObject o = new JsonObject();
        o.add("passes", passes);
        o.addProperty("settled", settled);
        return o;
    }

    /**
     * {@code warmup} untimed passes over the world's queries (the JIT is warm by now; these warm
     * the world's data for the caches, as the port's side does too), then {@code reps} timed
     * ones. Adds the queries (with their results and times) and the JIT compilation during the
     * timed passes to {@code o}.
     */
    private static void bench(Case c, PathRefGen.Inventory0 inventory, int warmup, int reps, int budget, JsonObject o)
            throws Exception {
        List<Query> queries = c.queries;
        long[][] times = new long[queries.size()][reps];
        JsonObject[] results = new JsonObject[queries.size()];
        CompilationMXBean jit = ManagementFactory.getCompilationMXBean();
        long compiling = 0;
        PrintStream stdout = System.out;
        // upstream prints progress
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        try {
            for (int pass = 0; pass < warmup + reps; pass++) {
                long compiling0 = jit.getTotalCompilationTime();
                for (int i = 0; i < queries.size(); i++) {
                    Run run = runOnce(c.world, inventory, queries.get(i), budget, pass == 0);
                    if (pass >= warmup) {
                        times[i][pass - warmup] = run.nanos;
                    }
                    if (pass == 0) {
                        results[i] = run.result;
                    }
                }
                if (pass >= warmup) {
                    compiling += jit.getTotalCompilationTime() - compiling0;
                }
                out.printf("%s: pass %d of %d%n", c.name, pass + 1, warmup + reps);
            }
        } finally {
            System.setOut(stdout);
        }
        JsonArray a = new JsonArray();
        for (int i = 0; i < queries.size(); i++) {
            Query q = queries.get(i);
            JsonObject qo = new JsonObject();
            qo.addProperty("kind", q.kind);
            qo.add("start", pos(q.start));
            qo.add("goal", q.goal.spec());
            qo.add("result", results[i]);
            JsonArray t = new JsonArray();
            for (long v : times[i]) {
                t.add(v);
            }
            qo.add("times_ns", t);
            a.add(qo);
        }
        o.add("queries", a);
        o.addProperty("jit_ms_timed", compiling);
    }

    /**
     * What the port must reproduce: the result type, the search's size, the path (positions,
     * total cost bits, nodes considered) and the best path so far.
     */
    private static JsonObject result(AStarPathFinder finder, PathCalculationResult result, int calls) throws Exception {
        JsonObject r = new JsonObject();
        r.addProperty("type", result.getType().name());
        r.addProperty("calls", calls);
        r.addProperty("map_size", PathRefGen.mapSize(finder));
        if (result.getPath().isPresent()) {
            IPath path = result.getPath().get();
            r.add("positions", flat(path.positions()));
            double cost = 0;
            for (IMovement m : path.movements()) {
                cost += m.getCost();
            }
            r.add("cost", d(cost));
            r.addProperty("num_nodes", path.getNumNodesConsidered());
        }
        r.add("best_so_far", finder.bestPathSoFar().map(p -> (JsonElement) flat(p.positions())).orElse(JsonNull.INSTANCE));
        return r;
    }

    private static JsonArray flat(List<BetterBlockPos> positions) {
        JsonArray a = new JsonArray();
        for (BetterBlockPos p : positions) {
            a.add(p.x);
            a.add(p.y);
            a.add(p.z);
        }
        return a;
    }

    // endregion
}
