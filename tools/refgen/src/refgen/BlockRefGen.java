package refgen;

import baritone.api.BaritoneAPI;
import baritone.api.Settings;
import baritone.pathing.movement.MovementHelper;
import baritone.pathing.precompute.PrecomputedData;
import baritone.pathing.precompute.Ternary;
import baritone.utils.BlockStateInterface;
import baritone.utils.pathing.BetterWorldBorder;
import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.tags.TagKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.AirBlock;
import net.minecraft.world.level.block.BaseFireBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.CarpetBlock;
import net.minecraft.world.level.block.DoorBlock;
import net.minecraft.world.level.block.FallingBlock;
import net.minecraft.world.level.block.FenceGateBlock;
import net.minecraft.world.level.block.InfestedBlock;
import net.minecraft.world.level.block.LeavesBlock;
import net.minecraft.world.level.block.LilyPadBlock;
import net.minecraft.world.level.block.LiquidBlock;
import net.minecraft.world.level.block.SlabBlock;
import net.minecraft.world.level.block.SnowLayerBlock;
import net.minecraft.world.level.block.StainedGlassBlock;
import net.minecraft.world.level.block.StairBlock;
import net.minecraft.world.level.block.TrapDoorBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockSetType;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.block.state.properties.StairsShape;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.phys.AABB;

import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Enumeration;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.zip.GZIPOutputStream;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;

/**
 * Generates the Java-semantics block state table (the host trait table as a 26.3 client would
 * send it) and reference results of the real upstream MovementHelper / PrecomputedData /
 * BetterWorldBorder for every state and for positions in random small worlds.
 * <p>
 * Needs a bootstrapped registry. Block tags are bound from the client jar's data pack, so
 * tag checks such as {@code FallingBlock.isFree} behave as in game.
 */
final class BlockRefGen {

    private static final Random R = new Random(0xbb67ae85L);

    private static final int MIN_Y = -16;
    private static final int HEIGHT = 32;

    private BlockRefGen() {}

    static void write(String upstream, String minecraft, Path out) throws Exception {
        bindBlockTags();

        List<BlockState> states = new ArrayList<>();
        Block.BLOCK_STATE_REGISTRY.forEach(states::add);
        for (int i = 0; i < states.size(); i++) {
            if (Block.BLOCK_STATE_REGISTRY.getId(states.get(i)) != i) {
                throw new IllegalStateException("state ids are not dense");
            }
        }

        JsonObject configs = new JsonObject();
        configs.add("a", new JsonObject());
        configs.add("b", configB());

        JsonObject root = new JsonObject();
        root.addProperty("upstream", upstream);
        root.addProperty("minecraft", minecraft);
        root.add("table", table(states));
        root.add("configs", configs);
        JsonObject stateRefs = new JsonObject();
        for (String config : configs.keySet()) {
            apply(configs.getAsJsonObject(config));
            JsonArray refs = new JsonArray();
            for (BlockState state : states) {
                refs.add(stateRef(state));
            }
            stateRefs.add(config, refs);
        }
        root.add("state_refs", stateRefs);
        root.add("worlds", worlds(states, configs));
        apply(new JsonObject());

        try (Writer w = new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(out)), StandardCharsets.UTF_8)) {
            w.write(new Gson().toJson(root));
            w.write('\n');
        }
    }

    // region tags

    /**
     * Binds every block tag from the vanilla data pack in the client jar. Without a server,
     * nothing else loads tags, and {@code Holder.is(TagKey)} would throw.
     */
    @SuppressWarnings("unchecked")
    private static void bindBlockTags() throws Exception {
        Path jar = Path.of(Block.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        String prefix = "data/minecraft/tags/block/";
        Map<String, JsonArray> raw = new HashMap<>();
        try (ZipFile zip = new ZipFile(jar.toFile())) {
            Enumeration<? extends ZipEntry> entries = zip.entries();
            while (entries.hasMoreElements()) {
                ZipEntry entry = entries.nextElement();
                String name = entry.getName();
                if (!name.startsWith(prefix) || !name.endsWith(".json")) {
                    continue;
                }
                String tag = "minecraft:" + name.substring(prefix.length(), name.length() - ".json".length());
                try (InputStreamReader reader = new InputStreamReader(zip.getInputStream(entry), StandardCharsets.UTF_8)) {
                    raw.put(tag, JsonParser.parseReader(reader).getAsJsonObject().getAsJsonArray("values"));
                }
            }
        }
        Map<String, Set<Block>> resolved = new HashMap<>();
        for (String tag : raw.keySet()) {
            resolveTag(tag, raw, resolved);
        }
        Map<Block, List<TagKey<Block>>> byBlock = new HashMap<>();
        resolved.forEach((tag, blocks) -> {
            TagKey<Block> key = TagKey.create(Registries.BLOCK, Identifier.parse(tag));
            for (Block block : blocks) {
                byBlock.computeIfAbsent(block, b -> new ArrayList<>()).add(key);
            }
        });
        Method bindTags = Holder.Reference.class.getDeclaredMethod("bindTags", Collection.class);
        bindTags.setAccessible(true);
        for (Holder.Reference<Block> holder : BuiltInRegistries.BLOCK.listElements().toList()) {
            bindTags.invoke(holder, byBlock.getOrDefault(holder.value(), List.of()));
        }
        if (!Blocks.SOUL_FIRE.defaultBlockState().is(TagKey.create(Registries.BLOCK, Identifier.parse("minecraft:fire")))) {
            throw new IllegalStateException("tags not bound");
        }
    }

    private static Set<Block> resolveTag(String tag, Map<String, JsonArray> raw, Map<String, Set<Block>> resolved) {
        Set<Block> done = resolved.get(tag);
        if (done != null) {
            return done;
        }
        Set<Block> blocks = new HashSet<>();
        for (JsonElement value : raw.get(tag)) {
            String id;
            boolean required = true;
            if (value.isJsonObject()) {
                id = value.getAsJsonObject().get("id").getAsString();
                if (value.getAsJsonObject().has("required")) {
                    required = value.getAsJsonObject().get("required").getAsBoolean();
                }
            } else {
                id = value.getAsString();
            }
            if (id.startsWith("#")) {
                String ref = id.substring(1);
                if (raw.containsKey(ref)) {
                    blocks.addAll(resolveTag(ref, raw, resolved));
                } else if (required) {
                    throw new IllegalStateException("unknown tag " + ref + " in " + tag);
                }
            } else {
                var block = BuiltInRegistries.BLOCK.getOptional(Identifier.parse(id));
                if (block.isPresent()) {
                    blocks.add(block.get());
                } else if (required) {
                    throw new IllegalStateException("unknown block " + id + " in " + tag);
                }
            }
        }
        resolved.put(tag, blocks);
        return blocks;
    }

    // endregion

    // region settings

    /**
     * Every setting read by the code under test, set away from its default. Block lists hold
     * registry ids, like the Rust settings.
     */
    private static JsonObject configB() {
        JsonObject c = new JsonObject();
        c.addProperty("allowWalkOnMagmaBlocks", true);
        c.addProperty("allowVines", true);
        c.addProperty("assumeWalkOnLava", true);
        c.addProperty("allowWalkOnBottomSlab", false);
        c.addProperty("assumeWalkOnWater", true);
        c.addProperty("strictLiquidCheck", true);
        c.addProperty("avoidUpdatingFallingBlocks", false);
        c.add("blocksToAvoid", names(Blocks.TRIPWIRE, Blocks.STONE, Blocks.OAK_DOOR, Blocks.WATER, Blocks.AIR,
                Blocks.OAK_SLAB, Blocks.SNOW, Blocks.CARPET.white(), Blocks.LADDER));
        c.add("blocksToDisallowBreaking", names(Blocks.DIRT, Blocks.SAND, Blocks.GRAVEL, Blocks.STONE));
        return c;
    }

    private static JsonArray names(Block... blocks) {
        JsonArray a = new JsonArray();
        for (Block b : blocks) {
            a.add(BuiltInRegistries.BLOCK.getKey(b).toString());
        }
        return a;
    }

    /**
     * Resets every setting, then applies {@code config}.
     */
    @SuppressWarnings("unchecked")
    private static void apply(JsonObject config) throws ReflectiveOperationException {
        Settings settings = BaritoneAPI.getSettings();
        for (Settings.Setting<?> setting : settings.allSettings) {
            setting.reset();
        }
        for (String key : config.keySet()) {
            Settings.Setting<Object> setting = (Settings.Setting<Object>) Settings.class.getField(key).get(settings);
            JsonElement value = config.get(key);
            if (value.isJsonArray()) {
                List<Block> blocks = new ArrayList<>();
                for (JsonElement e : value.getAsJsonArray()) {
                    blocks.add(BuiltInRegistries.BLOCK.getValue(Identifier.parse(e.getAsString())));
                }
                setting.value = blocks;
            } else {
                setting.value = value.getAsBoolean();
            }
        }
    }

    // endregion

    // region table

    /**
     * The trait table, computed with the settings the host assumes: upstream defaults, except
     * that {@code blocksToAvoid} is empty (Rust applies it).
     */
    private static JsonObject table(List<BlockState> states) throws ReflectiveOperationException {
        JsonObject base = new JsonObject();
        base.add("blocksToAvoid", new JsonArray());
        apply(base);

        JsonArray out = new JsonArray();
        for (BlockState state : states) {
            out.add(traits(state));
        }
        JsonObject table = new JsonObject();
        table.addProperty("version", 1);
        table.addProperty("air", Block.BLOCK_STATE_REGISTRY.getId(Blocks.AIR.defaultBlockState()));
        table.add("states", out);
        return table;
    }

    private static JsonObject traits(BlockState state) throws ReflectiveOperationException {
        Block block = state.getBlock();
        JsonObject t = new JsonObject();
        t.addProperty("name", BuiltInRegistries.BLOCK.getKey(block).toString());
        if (!state.getProperties().isEmpty()) {
            JsonObject props = new JsonObject();
            for (Property<?> p : state.getProperties()) {
                props.addProperty(p.getName(), valueName(state, p));
            }
            t.add("properties", props);
        }
        flag(t, "air", block instanceof AirBlock);
        t.addProperty("can_walk_on", ternary(MovementHelper.canWalkOnBlockState(state)));
        t.addProperty("can_walk_through", ternary(MovementHelper.canWalkThroughBlockState(state)));
        t.addProperty("fully_passable", ternary(MovementHelper.fullyPassableBlockState(state)));

        FluidState fluid = state.getFluidState();
        if (!fluid.isEmpty()) {
            if (fluid.is(Fluids.WATER) || fluid.is(Fluids.FLOWING_WATER)) {
                t.addProperty("fluid", "water");
            } else if (fluid.is(Fluids.LAVA) || fluid.is(Fluids.FLOWING_LAVA)) {
                t.addProperty("fluid", "lava");
            } else {
                throw new IllegalStateException("unknown fluid in " + state);
            }
            flag(t, "fluid_source", fluid.isSource());
            t.addProperty("fluid_amount", fluid.getType().getAmount(fluid));
        }
        flag(t, "liquid_block", block instanceof LiquidBlock);

        if (block == Blocks.LADDER) {
            t.addProperty("climbable", "ladder");
        } else if (block == Blocks.VINE) {
            t.addProperty("climbable", "vine");
        } else if (block == Blocks.WEEPING_VINES || block == Blocks.WEEPING_VINES_PLANT
                || block == Blocks.TWISTING_VINES || block == Blocks.TWISTING_VINES_PLANT) {
            t.addProperty("climbable", "nether_vine");
        } else if (block == Blocks.SCAFFOLDING) {
            t.addProperty("climbable", "scaffolding");
        }
        flag(t, "falls", block instanceof FallingBlock);
        flag(t, "avoid_walking_into", block == Blocks.CACTUS
                || block == Blocks.SWEET_BERRY_BUSH
                || block instanceof BaseFireBlock
                || block == Blocks.END_PORTAL
                || block == Blocks.COBWEB
                || block == Blocks.BUBBLE_COLUMN);
        flag(t, "hot_floor", block == Blocks.MAGMA_BLOCK);
        flag(t, "fire", block instanceof BaseFireBlock);
        flag(t, "avoid_breaking", block == Blocks.ICE || block instanceof InfestedBlock);
        // MovementHelper.isReplaceable forces LARGE_FERN and TALL_GRASS to true; the port relies
        // on canBeReplaced() already covering them
        if ((block == Blocks.LARGE_FERN || block == Blocks.TALL_GRASS) && !state.canBeReplaced()) {
            throw new IllegalStateException(state + " is not replaceable");
        }
        flag(t, "replaceable", state.canBeReplaced());
        // BlockBehaviour.liquid(): water, lava and bubble columns (FallingBlock.isFree)
        flag(t, "liquid", state.liquid());
        boolean normalCube = MovementHelper.isBlockNormalCube(state);
        flag(t, "can_place_against", normalCube || block == Blocks.GLASS || block instanceof StainedGlassBlock);
        flag(t, "normal_cube", normalCube);
        flag(t, "pathfindable_land", state.isPathfindable(PathComputationType.LAND));

        if (block instanceof SlabBlock) {
            t.addProperty("slab", state.getValue(SlabBlock.TYPE).getSerializedName());
        }
        if (block instanceof StairBlock) {
            JsonObject stairs = new JsonObject();
            stairs.addProperty("half", state.getValue(StairBlock.HALF).getSerializedName());
            StairsShape shape = state.getValue(StairBlock.SHAPE);
            stairs.addProperty("inner_corner", shape == StairsShape.INNER_LEFT || shape == StairsShape.INNER_RIGHT);
            t.add("stairs", stairs);
        }
        if (block instanceof DoorBlock door) {
            t.addProperty("openable", "door");
            flag(t, "open", state.getValue(DoorBlock.OPEN));
            t.addProperty("half", state.getValue(DoorBlock.HALF) == DoubleBlockHalf.UPPER ? "top" : "bottom");
            flag(t, "hand_openable", door.type().canOpenByHand());
        } else if (block instanceof FenceGateBlock) {
            t.addProperty("openable", "fence_gate");
            flag(t, "open", state.getValue(FenceGateBlock.OPEN));
            flag(t, "hand_openable", true);
        } else if (block instanceof TrapDoorBlock) {
            t.addProperty("openable", "trap_door");
            flag(t, "open", state.getValue(TrapDoorBlock.OPEN));
            t.addProperty("half", state.getValue(TrapDoorBlock.HALF).getSerializedName());
            Method getType = TrapDoorBlock.class.getDeclaredMethod("getType");
            getType.setAccessible(true);
            flag(t, "hand_openable", ((BlockSetType) getType.invoke(block)).canOpenByHand());
        }
        if (state.hasProperty(BlockStateProperties.HORIZONTAL_FACING)) {
            t.addProperty("facing", state.getValue(BlockStateProperties.HORIZONTAL_FACING).getSerializedName());
        }
        if (block instanceof SnowLayerBlock) {
            t.addProperty("snow_layers", state.getValue(SnowLayerBlock.LAYERS));
        }
        flag(t, "farmland", block == Blocks.FARMLAND);
        if (block == Blocks.SOUL_SAND) {
            t.addProperty("speed_kind", "soul_sand");
        } else if (block == Blocks.HONEY_BLOCK) {
            t.addProperty("speed_kind", "honey");
        } else if (block == Blocks.COBWEB) {
            t.addProperty("speed_kind", "cobweb");
        }
        flag(t, "lily_pad", block instanceof LilyPadBlock);
        flag(t, "carpet", block instanceof CarpetBlock);
        flag(t, "leaves", block instanceof LeavesBlock);
        flag(t, "chest_like", block == Blocks.CHEST || block == Blocks.TRAPPED_CHEST || block == Blocks.ENDER_CHEST);

        float hardness;
        try {
            hardness = state.getDestroySpeed(null, null);
        } catch (NullPointerException npe) {
            hardness = -1;
        }
        t.addProperty("hardness", (double) hardness);
        flag(t, "requires_tool", state.requiresCorrectToolForDrops());

        t.add("collision_shape", shape(state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).toAabbs()));
        t.add("outline_shape", shape(state.getShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).toAabbs()));
        return t;
    }

    @SuppressWarnings("unchecked")
    private static <T extends Comparable<T>> String valueName(BlockState state, Property<T> p) {
        return p.getName(state.getValue(p));
    }

    private static void flag(JsonObject t, String name, boolean value) {
        if (value) {
            t.addProperty(name, true);
        }
    }

    private static String ternary(Ternary t) {
        return switch (t) {
            case YES -> "yes";
            case MAYBE -> "maybe";
            case NO -> "no";
        };
    }

    private static JsonArray shape(List<AABB> boxes) {
        JsonArray a = new JsonArray();
        for (AABB b : boxes) {
            JsonArray box = new JsonArray();
            box.add(b.minX);
            box.add(b.minY);
            box.add(b.minZ);
            box.add(b.maxX);
            box.add(b.maxY);
            box.add(b.maxZ);
            a.add(box);
        }
        return a;
    }

    // endregion

    // region per-state reference

    private static char t(Ternary t) {
        return switch (t) {
            case YES -> 'Y';
            case MAYBE -> 'M';
            case NO -> 'N';
        };
    }

    private static char b(boolean b) {
        return b ? '1' : '0';
    }

    /**
     * Keep in sync with {@code STATE_CHECKS} in tests/reference_block_states.rs.
     */
    private static String stateRef(BlockState s) {
        Block block = s.getBlock();
        return new String(new char[]{
                t(MovementHelper.canWalkOnBlockState(s)),
                t(MovementHelper.canWalkThroughBlockState(s)),
                t(MovementHelper.fullyPassableBlockState(s)),
                b(MovementHelper.isBlockNormalCube(s)),
                b(MovementHelper.isWater(s)),
                b(MovementHelper.isLava(s)),
                b(MovementHelper.isLiquid(s)),
                b(MovementHelper.possiblyFlowing(s)),
                b(MovementHelper.isClimbable(block)),
                b(MovementHelper.avoidWalkingInto(s)),
                b(MovementHelper.isBottomSlab(s)),
                b(MovementHelper.isTransparent(block)),
                b(FallingBlock.isFree(s)),
        });
    }

    // endregion

    // region worlds

    /**
     * A BlockStateInterface over a map of blocks: the real one needs a client world. Allocated
     * without running a constructor; every method the code under test calls is overridden or
     * reads a field set here.
     */
    static final class FakeBsi extends BlockStateInterface {

        private Map<Long, BlockState> blocks;
        private Set<Long> unloadedChunks;

        @SuppressWarnings("unused")
        private FakeBsi() {
            super(null);
        }

        static FakeBsi create(Map<Long, BlockState> blocks, Set<Long> unloadedChunks, WorldBorder border) throws Exception {
            Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
            theUnsafe.setAccessible(true);
            FakeBsi bsi = (FakeBsi) ((sun.misc.Unsafe) theUnsafe.get(null)).allocateInstance(FakeBsi.class);
            bsi.blocks = blocks;
            bsi.unloadedChunks = unloadedChunks;
            Field worldBorder = BlockStateInterface.class.getDeclaredField("worldBorder");
            worldBorder.setAccessible(true);
            worldBorder.set(bsi, new BetterWorldBorder(border));
            return bsi;
        }

        @Override
        public boolean worldContainsLoadedChunk(int blockX, int blockZ) {
            return !unloadedChunks.contains(ChunkPos.pack(blockX >> 4, blockZ >> 4));
        }

        @Override
        public boolean isLoaded(int x, int z) {
            return worldContainsLoadedChunk(x, z);
        }

        @Override
        public BlockState get0(int x, int y, int z) {
            // same order as the real get0: height check, then loaded chunk, then the section
            if (y < MIN_Y || y >= MIN_Y + HEIGHT) {
                return Blocks.AIR.defaultBlockState();
            }
            if (!worldContainsLoadedChunk(x, z)) {
                return Blocks.AIR.defaultBlockState();
            }
            BlockState state = blocks.get(BlockPos.asLong(x, y, z));
            return state == null ? Blocks.AIR.defaultBlockState() : state;
        }
    }

    /**
     * Blocks that the position checks treat specially, drawn more often than a uniform pick.
     */
    private static final List<Block> INTERESTING = List.of(
                Blocks.WATER, Blocks.LAVA, Blocks.BUBBLE_COLUMN, Blocks.KELP, Blocks.KELP_PLANT, Blocks.SEAGRASS,
                Blocks.SNOW, Blocks.CARPET.white(), Blocks.MOSS_CARPET, Blocks.PALE_MOSS_CARPET, Blocks.LILY_PAD,
                Blocks.SAND, Blocks.GRAVEL, Blocks.RED_SAND, Blocks.ANVIL,
                Blocks.MAGMA_BLOCK, Blocks.SOUL_SAND, Blocks.ICE, Blocks.INFESTED_STONE,
                Blocks.LADDER, Blocks.VINE, Blocks.WEEPING_VINES, Blocks.SCAFFOLDING,
                Blocks.OAK_DOOR, Blocks.IRON_DOOR, Blocks.OAK_TRAPDOOR, Blocks.OAK_FENCE_GATE,
                Blocks.OAK_SLAB, Blocks.OAK_STAIRS, Blocks.OAK_LEAVES, Blocks.GLASS, Blocks.STAINED_GLASS.red(),
                Blocks.TALL_GRASS, Blocks.SHORT_GRASS, Blocks.LARGE_FERN, Blocks.FIRE, Blocks.SOUL_FIRE, Blocks.COBWEB,
                Blocks.FARMLAND, Blocks.DIRT_PATH, Blocks.CHEST, Blocks.CACTUS, Blocks.POWDER_SNOW, Blocks.TRIPWIRE,
                Blocks.DIRT, Blocks.OAK_SIGN, Blocks.TORCH, Blocks.CAULDRON, Blocks.WATER_CAULDRON, Blocks.AZALEA);

    private static BlockState anyStateOf(Block block) {
        List<BlockState> states = block.getStateDefinition().getPossibleStates();
        return states.get(R.nextInt(states.size()));
    }

    private static JsonArray worlds(List<BlockState> states, JsonObject configs) throws Exception {
        BlockState water = Blocks.WATER.defaultBlockState();
        BlockState stone = Blocks.STONE.defaultBlockState();
        BlockState air = Blocks.AIR.defaultBlockState();

        JsonArray worlds = new JsonArray();
        for (int w = 0; w < 64; w++) {
            int sx = 10;
            int sy = 8;
            int sz = 10;
            int x0 = -5;
            int y0 = R.nextBoolean() ? MIN_Y : MIN_Y + HEIGHT - sy;
            int z0 = -5;
            // per world mix, so some worlds are mostly fluid and others mostly solid
            int wAir = R.nextInt(40);
            int wStone = R.nextInt(30);
            int wWater = R.nextInt(30);
            int wInteresting = 10 + R.nextInt(40);
            int wAny = 5 + R.nextInt(20);
            int total = wAir + wStone + wWater + wInteresting + wAny;
            // chance that a block repeats the one below it, which builds fluid bodies and
            // stacks with something else on top
            double repeat = R.nextDouble() * 0.7;

            BlockState[][][] region = new BlockState[sx][sy][sz];
            for (int x = 0; x < sx; x++) {
                for (int z = 0; z < sz; z++) {
                    for (int y = 0; y < sy; y++) {
                        BlockState s;
                        int r = R.nextInt(total);
                        if (y > 0 && R.nextDouble() < repeat) {
                            s = region[x][y - 1][z];
                        } else if ((r -= wAir) < 0) {
                            s = air;
                        } else if ((r -= wStone) < 0) {
                            s = stone;
                        } else if ((r -= wWater) < 0) {
                            s = R.nextInt(4) == 0 ? anyStateOf(Blocks.WATER) : water;
                        } else if ((r -= wInteresting) < 0) {
                            s = anyStateOf(INTERESTING.get(R.nextInt(INTERESTING.size())));
                        } else {
                            s = states.get(R.nextInt(states.size()));
                        }
                        region[x][y][z] = s;
                    }
                }
            }
            Map<Long, BlockState> blocks = new HashMap<>();
            JsonArray ids = new JsonArray();
            for (int x = 0; x < sx; x++) {
                for (int y = 0; y < sy; y++) {
                    for (int z = 0; z < sz; z++) {
                        blocks.put(BlockPos.asLong(x0 + x, y0 + y, z0 + z), region[x][y][z]);
                        ids.add(Block.BLOCK_STATE_REGISTRY.getId(region[x][y][z]));
                    }
                }
            }

            Set<Long> unloaded = new HashSet<>();
            JsonArray unloadedJson = new JsonArray();
            if (R.nextInt(3) == 0) {
                int cx = R.nextInt(2) - 1;
                int cz = R.nextInt(2) - 1;
                unloaded.add(ChunkPos.pack(cx, cz));
                unloadedJson.add(row(cx, cz));
            }

            WorldBorder border = new WorldBorder();
            if (R.nextInt(3) == 0) {
                border.setCenter(R.nextInt(9) - 4 + R.nextDouble(), R.nextInt(9) - 4 + R.nextDouble());
                border.setSize(2 + R.nextInt(8) + R.nextDouble());
            }

            JsonObject world = new JsonObject();
            world.addProperty("min_y", MIN_Y);
            world.addProperty("height", HEIGHT);
            world.add("origin", row(x0, y0, z0));
            world.add("size", row(sx, sy, sz));
            world.add("blocks", ids);
            world.add("unloaded_chunks", unloadedJson);
            JsonArray borderJson = new JsonArray();
            borderJson.add(border.getMinX());
            borderJson.add(border.getMaxX());
            borderJson.add(border.getMinZ());
            borderJson.add(border.getMaxZ());
            world.add("border", borderJson);

            JsonObject results = new JsonObject();
            for (String config : configs.keySet()) {
                apply(configs.getAsJsonObject(config));
                FakeBsi bsi = FakeBsi.create(blocks, unloaded, border);
                PrecomputedData pd = new PrecomputedData();
                StringBuilder sb = new StringBuilder();
                for (int x = x0 - 1; x <= x0 + sx; x++) {
                    for (int y = y0 - 1; y <= y0 + sy; y++) {
                        for (int z = z0 - 1; z <= z0 + sz; z++) {
                            sb.append(positionRef(bsi, pd, x, y, z));
                        }
                    }
                }
                results.addProperty(config, sb.toString());
            }
            world.add("results", results);
            worlds.add(world);
        }
        return worlds;
    }

    /**
     * Keep in sync with {@code POSITION_CHECKS} in tests/reference_block_states.rs.
     */
    private static String positionRef(FakeBsi bsi, PrecomputedData pd, int x, int y, int z) {
        BlockState s = bsi.get0(x, y, z);
        return new String(new char[]{
                b(pd.canWalkOn(bsi, x, y, z, s)),
                b(pd.canWalkThrough(bsi, x, y, z, s)),
                b(pd.fullyPassable(bsi, x, y, z, s)),
                b(MovementHelper.canWalkOn(bsi, x, y, z)),
                b(MovementHelper.canWalkThrough(bsi, x, y, z)),
                b(MovementHelper.isFlowing(x, y, z, s, bsi)),
                b(MovementHelper.isReplaceable(x, y, z, s, bsi)),
                b(MovementHelper.avoidBreaking(bsi, x, y, z, s)),
                b(MovementHelper.canPlaceAgainst(bsi, x, y, z)),
        });
    }

    private static JsonArray row(int... values) {
        JsonArray a = new JsonArray();
        for (int v : values) {
            a.add(v);
        }
        return a;
    }

    // endregion
}
