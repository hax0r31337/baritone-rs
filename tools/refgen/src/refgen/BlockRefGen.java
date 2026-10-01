package refgen;

import baritone.api.BaritoneAPI;
import baritone.api.Settings;
import baritone.api.utils.BlockOptionalMeta;
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
import net.minecraft.core.HolderLookup;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.core.component.DataComponents;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.ItemTags;
import net.minecraft.tags.TagKey;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.Tool;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.AirBlock;
import net.minecraft.world.level.block.BaseFireBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.BonemealSource;
import net.minecraft.world.level.block.BonemealableBlock;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.LootTable;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
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
import net.minecraft.world.level.block.state.BlockBehaviour;
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
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.ArrayVoxelShape;
import net.minecraft.world.phys.shapes.DiscreteVoxelShape;
import net.minecraft.world.phys.shapes.OffsetDoubleList;
import net.minecraft.world.phys.shapes.VoxelShape;
import it.unimi.dsi.fastutil.doubles.DoubleList;

import java.io.InputStreamReader;
import java.lang.reflect.Constructor;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.ParameterizedType;
import java.lang.reflect.Type;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.Enumeration;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Random;
import java.util.Set;
import java.util.function.Function;
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

    /**
     * Datagen's vanilla registries (enchantments, ...), set by {@link #bindRegistries}.
     */
    static HolderLookup.Provider LOOKUP;

    private BlockRefGen() {}

    /**
     * Binds the block and item tags and the item components, and sets {@link #LOOKUP}.
     */
    static void bindRegistries() throws Exception {
        bindTags(BuiltInRegistries.BLOCK, "block");
        if (!Blocks.SOUL_FIRE.defaultBlockState().is(TagKey.create(Registries.BLOCK, Identifier.parse("minecraft:fire")))) {
            throw new IllegalStateException("block tags not bound");
        }
        bindTags(BuiltInRegistries.ITEM, "item");
        // item components are bound when a world's registries load (like the client does in
        // RegistryDataCollector.updateComponents); datagen's vanilla registries stand in
        HolderLookup.Provider lookup = VanillaRegistries.createReloadableLookup(VanillaRegistries.createWorldLookup());
        LOOKUP = lookup;
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(DataComponentInitializers.PendingComponents::apply);
        if (!Items.IRON_SWORD.getDefaultInstance().is(ItemTags.SWORDS)) {
            throw new IllegalStateException("item tags not bound");
        }
    }

    static void write(String upstream, String minecraft, Path out) throws Exception {
        bindRegistries();

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
        if (dropsFailures > 0) {
            RefGen.err.println("note: BlockOptionalMeta.drops fails for " + dropsFailures
                    + " blocks, as upstream's does, so they drop nothing: " + firstDropsFailure);
        }
        if (!BONEMEAL_UNKNOWN.isEmpty()) {
            RefGen.err.println("note: bone meal checks need a real world for " + BONEMEAL_UNKNOWN
                    + "; exported as not bonemealable");
        }
        root.add("offsets", offsets(states));
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
        root.add("block_optional_meta", blockOptionalMeta());

        try (Writer w = new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(out)), StandardCharsets.UTF_8)) {
            w.write(new Gson().toJson(root));
            w.write('\n');
        }
    }

    /**
     * The real {@code BlockOptionalMeta(String)} on assorted selectors: what it prints and
     * which states it matches, or the exception it throws. Its item drops are always empty here
     * (see {@link #drops}), so its item matching is not sampled.
     */
    private static JsonArray blockOptionalMeta() {
        String[] selectors = {
                "stone", "minecraft:stone", ":stone", "stone[]", "STONE", "stone\n", "stone\r\n", "st\none",
                "furnace[lit=true]", "furnace[lit=true,facing=north]", "furnace[lit=true,]",
                "furnace[,lit=true]", "furnace[lit]", "furnace[lit=]", "furnace[=true]", "furnace[lit=maybe]",
                "furnace[color=red]", "furnace[lit=true,lit=false]", "furnace[lit=true=false]",
                "oak_stairs[half=top,shape=inner_left]", "wheat[age=7]", "wheat[age=+7]", "wheat[age=07]",
                "wheat[age=8]", "wheat[age=-1]", "redstone_wire[power=15,east=side]", "water[level=0]",
                "not_a_block", "", "[", "a[b", "stone[lit=true]", "chest[type=single]", "minecraft:oak_log[axis=y]",
        };
        JsonArray out = new JsonArray();
        for (String selector : selectors) {
            JsonObject o = new JsonObject();
            o.addProperty("selector", selector);
            try {
                BlockOptionalMeta bom = new BlockOptionalMeta(selector);
                o.addProperty("string", bom.toString());
                JsonArray ids = new JsonArray();
                bom.getAllBlockStates().stream().map(Block.BLOCK_STATE_REGISTRY::getId).sorted().forEach(ids::add);
                o.add("states", ids);
            } catch (Exception e) {
                o.addProperty("error", e.getClass().getSimpleName() + ": " + e.getMessage());
            }
            out.add(o);
        }
        return out;
    }

    // region tags

    /**
     * Binds every tag of {@code registry} from the vanilla data pack in the client jar
     * ({@code data/minecraft/tags/<folder>/}). Without a server, nothing else loads tags, and
     * {@code Holder.is(TagKey)} would throw.
     */
    @SuppressWarnings("unchecked")
    private static <T> void bindTags(Registry<T> registry, String folder) throws Exception {
        Path jar = Path.of(Block.class.getProtectionDomain().getCodeSource().getLocation().toURI());
        String prefix = "data/minecraft/tags/" + folder + "/";
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
        Map<String, Set<T>> resolved = new HashMap<>();
        for (String tag : raw.keySet()) {
            resolveTag(registry, tag, raw, resolved);
        }
        Map<T, List<TagKey<T>>> byValue = new HashMap<>();
        resolved.forEach((tag, values) -> {
            TagKey<T> key = TagKey.create(registry.key(), Identifier.parse(tag));
            for (T value : values) {
                byValue.computeIfAbsent(value, b -> new ArrayList<>()).add(key);
            }
        });
        Method bindTags = Holder.Reference.class.getDeclaredMethod("bindTags", Collection.class);
        bindTags.setAccessible(true);
        for (Holder.Reference<T> holder : registry.listElements().toList()) {
            bindTags.invoke(holder, byValue.getOrDefault(holder.value(), List.of()));
        }
    }

    private static <T> Set<T> resolveTag(Registry<T> registry, String tag, Map<String, JsonArray> raw, Map<String, Set<T>> resolved) {
        Set<T> done = resolved.get(tag);
        if (done != null) {
            return done;
        }
        Set<T> blocks = new HashSet<>();
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
                    blocks.addAll(resolveTag(registry, ref, raw, resolved));
                } else if (required) {
                    throw new IllegalStateException("unknown tag " + ref + " in " + tag);
                }
            } else {
                var block = registry.getOptional(Identifier.parse(id));
                if (block.isPresent()) {
                    blocks.add(block.get());
                } else if (required) {
                    throw new IllegalStateException("unknown entry " + id + " in " + tag);
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

    static JsonArray names(Block... blocks) {
        JsonArray a = new JsonArray();
        for (Block b : blocks) {
            a.add(BuiltInRegistries.BLOCK.getKey(b).toString());
        }
        return a;
    }

    /**
     * Resets every setting, then applies {@code config}. Lists hold registry ids (blocks or
     * items, from the setting's type); numbers take the type of the setting.
     */
    @SuppressWarnings("unchecked")
    static void apply(JsonObject config) throws ReflectiveOperationException {
        Settings settings = BaritoneAPI.getSettings();
        for (Settings.Setting<?> setting : settings.allSettings) {
            setting.reset();
        }
        for (String key : config.keySet()) {
            Settings.Setting<Object> setting = (Settings.Setting<Object>) Settings.class.getField(key).get(settings);
            JsonElement value = config.get(key);
            Object old = setting.defaultValue;
            if (value.isJsonArray()) {
                Type element = ((ParameterizedType) setting.getType()).getActualTypeArguments()[0];
                Registry<?> registry = element == Block.class ? BuiltInRegistries.BLOCK
                        : element == Item.class ? BuiltInRegistries.ITEM : null;
                if (registry == null) {
                    throw new IllegalArgumentException(key + " is a list of " + element);
                }
                List<Object> values = new ArrayList<>();
                for (JsonElement e : value.getAsJsonArray()) {
                    values.add(registry.getValue(Identifier.parse(e.getAsString())));
                }
                setting.value = values;
            } else if (old instanceof Boolean) {
                setting.value = value.getAsBoolean();
            } else if (old instanceof Integer) {
                setting.value = value.getAsInt();
            } else if (old instanceof Long) {
                setting.value = value.getAsLong();
            } else if (old instanceof Double) {
                setting.value = value.getAsDouble();
            } else if (old instanceof Float) {
                setting.value = value.getAsFloat();
            } else {
                throw new IllegalArgumentException(key + " has type " + old.getClass());
            }
        }
    }

    // endregion

    // region table

    /**
     * The trait table, computed with the settings the host assumes: upstream defaults, except
     * that {@code blocksToAvoid} is empty (Rust applies it).
     */
    private static JsonObject table(List<BlockState> states) throws Exception {
        JsonObject base = new JsonObject();
        base.add("blocksToAvoid", new JsonArray());
        apply(base);

        List<TagKey<Block>> toolTags = toolTags();
        JsonArray out = new JsonArray();
        for (BlockState state : states) {
            JsonObject t = traits(state);
            flag(t, "default", state == state.getBlock().defaultBlockState());
            JsonArray tags = new JsonArray();
            for (TagKey<Block> tag : toolTags) {
                if (state.is(tag)) {
                    tags.add(tag.location().toString());
                }
            }
            if (!tags.isEmpty()) {
                t.add("tags", tags);
            }
            out.add(t);
        }
        JsonObject table = new JsonObject();
        table.addProperty("version", 5);
        table.addProperty("air", Block.BLOCK_STATE_REGISTRY.getId(Blocks.AIR.defaultBlockState()));
        table.add("states", out);
        return table;
    }

    /**
     * The block tags that item tool rules refer to, sorted: the tags the host must send.
     */
    static List<TagKey<Block>> toolTags() {
        Set<TagKey<Block>> tags = new HashSet<>();
        for (Item item : BuiltInRegistries.ITEM) {
            Tool tool = item.components().get(DataComponents.TOOL);
            if (tool == null) {
                continue;
            }
            for (Tool.Rule rule : tool.rules()) {
                rule.blocks().unwrapKey().ifPresent(tags::add);
            }
        }
        List<TagKey<Block>> sorted = new ArrayList<>(tags);
        sorted.sort(Comparator.comparing(t -> t.location().toString()));
        return sorted;
    }

    private static JsonObject traits(BlockState state) throws Exception {
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
        flag(t, "bonemealable", bonemealable(state));
        if (state == block.defaultBlockState()) {
            JsonArray drops = new JsonArray();
            drops(block).forEach(drops::add);
            if (!drops.isEmpty()) {
                t.add("drops", drops);
            }
        }

        float hardness;
        try {
            hardness = state.getDestroySpeed(null, null);
        } catch (NullPointerException npe) {
            hardness = -1;
        }
        t.addProperty("hardness", (double) hardness);
        flag(t, "requires_tool", state.requiresCorrectToolForDrops());

        // shapes that move with the block's offset are stored unmoved; the host moves them
        Function<BlockPos, VoxelShape> collision = pos -> state.getCollisionShape(EmptyBlockGetter.INSTANCE, pos);
        Function<BlockPos, VoxelShape> outline = pos -> state.getShape(EmptyBlockGetter.INSTANCE, pos);
        boolean collisionMoves = moves(state, collision);
        boolean outlineMoves = moves(state, outline);
        t.add("collision_shape", shape(unmoved(state, collision.apply(BlockPos.ZERO), collisionMoves).toAabbs()));
        t.add("outline_shape", shape(unmoved(state, outline.apply(BlockPos.ZERO), outlineMoves).toAabbs()));
        t.add("interaction_shape", shape(state.getInteractionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).toAabbs()));
        if (state.hasOffsetFunction()) {
            JsonObject offset = new JsonObject();
            offset.addProperty("type", offsetType(state));
            // protected; invoked virtually, so blocks that override them answer
            Method maxHorizontal = BlockBehaviour.class.getDeclaredMethod("getMaxHorizontalOffset");
            maxHorizontal.setAccessible(true);
            Method maxVertical = BlockBehaviour.class.getDeclaredMethod("getMaxVerticalOffset");
            maxVertical.setAccessible(true);
            offset.addProperty("max_horizontal", (double) (float) maxHorizontal.invoke(block));
            offset.addProperty("max_vertical", (double) (float) maxVertical.invoke(block));
            flag(offset, "outline", outlineMoves);
            flag(offset, "collision", collisionMoves);
            t.add("offset", offset);
        }
        return t;
    }

    /**
     * Whether {@code shape} follows the state's offset: it differs between two positions whose
     * offsets differ. Blocks may have an offset they only render with (short grass).
     */
    private static boolean moves(BlockState state, Function<BlockPos, VoxelShape> shape) {
        if (!state.hasOffsetFunction()) {
            return false;
        }
        BlockPos other = null;
        for (int x = 1; other == null; x++) {
            if (!state.getOffset(new BlockPos(x, 0, 0)).equals(state.getOffset(BlockPos.ZERO))) {
                other = new BlockPos(x, 0, 0);
            }
        }
        List<AABB> atZero = shape.apply(BlockPos.ZERO).toAabbs();
        return !atZero.equals(shape.apply(other).toAabbs());
    }

    /**
     * {@code XYZ} when the offset moves the block vertically anywhere, else {@code XZ}: the
     * offset function does not say which {@code OffsetType} made it.
     */
    private static String offsetType(BlockState state) {
        for (int x = -8; x < 8; x++) {
            for (int z = -8; z < 8; z++) {
                if (state.getOffset(new BlockPos(x, 0, z)).y != 0.0) {
                    return "xyz";
                }
            }
        }
        return "xz";
    }

    /**
     * The shape before {@code state.getOffset(pos)} moved it. A moved shape is an
     * {@code ArrayVoxelShape} whose coordinate lists are {@code OffsetDoubleList}s over the
     * unmoved ones; this rebuilds the shape from those.
     */
    static VoxelShape unmoved(BlockState state, VoxelShape shape, boolean moves) throws ReflectiveOperationException {
        if (!moves) {
            return shape;
        }
        if (!(shape instanceof ArrayVoxelShape)) {
            throw new IllegalStateException(state + " has an offset shape of " + shape.getClass());
        }
        Vec3 offset = state.getOffset(BlockPos.ZERO);
        double[] expected = {offset.x, offset.y, offset.z};
        String[] names = {"xs", "ys", "zs"};
        DoubleList[] coords = new DoubleList[3];
        for (int i = 0; i < 3; i++) {
            Object list = getField(ArrayVoxelShape.class, shape, names[i]);
            if (!(list instanceof OffsetDoubleList)) {
                throw new IllegalStateException(state + " has an offset shape that was not moved");
            }
            coords[i] = (DoubleList) getField(OffsetDoubleList.class, list, "delegate");
            double moved = (double) getField(OffsetDoubleList.class, list, "offset");
            if (Double.doubleToRawLongBits(moved) != Double.doubleToRawLongBits(expected[i])) {
                throw new IllegalStateException(state + " was moved by " + moved + ", not its offset " + expected[i]);
            }
        }
        Constructor<ArrayVoxelShape> constructor = ArrayVoxelShape.class.getDeclaredConstructor(
                DiscreteVoxelShape.class, DoubleList.class, DoubleList.class, DoubleList.class);
        constructor.setAccessible(true);
        return constructor.newInstance(getField(VoxelShape.class, shape, "shape"), coords[0], coords[1], coords[2]);
    }

    static Object getField(Class<?> owner, Object target, String name) throws ReflectiveOperationException {
        Field field = owner.getDeclaredField(name);
        field.setAccessible(true);
        return field.get(target);
    }

    /**
     * For every state with an offset: the offset and the moved shapes at a few positions, which
     * the host computes from the table's offset and unmoved shapes.
     */
    private static JsonArray offsets(List<BlockState> states) {
        // its own random, so the worlds below stay what they were
        Random r = new Random(0x510e527fL);
        List<BlockPos> positions = new ArrayList<>();
        positions.add(BlockPos.ZERO);
        positions.add(new BlockPos(-1, 0, -1));
        positions.add(new BlockPos(29999999, 64, -29999999));
        for (int i = 0; i < 12; i++) {
            positions.add(new BlockPos(r.nextInt(2001) - 1000, r.nextInt(384) - 64, r.nextInt(2001) - 1000));
        }
        JsonArray out = new JsonArray();
        for (int id = 0; id < states.size(); id++) {
            BlockState state = states.get(id);
            if (!state.hasOffsetFunction()) {
                continue;
            }
            for (BlockPos pos : positions) {
                JsonObject o = new JsonObject();
                o.addProperty("state", id);
                o.add("pos", pos(pos));
                Vec3 offset = state.getOffset(pos);
                JsonArray bits = new JsonArray();
                bits.add(Double.doubleToRawLongBits(offset.x));
                bits.add(Double.doubleToRawLongBits(offset.y));
                bits.add(Double.doubleToRawLongBits(offset.z));
                o.add("offset", bits);
                o.add("outline", shapeBits(state.getShape(EmptyBlockGetter.INSTANCE, pos).toAabbs()));
                o.add("collision", shapeBits(state.getCollisionShape(EmptyBlockGetter.INSTANCE, pos).toAabbs()));
                out.add(o);
            }
        }
        return out;
    }

    static JsonArray pos(BlockPos pos) {
        JsonArray a = new JsonArray();
        a.add(pos.getX());
        a.add(pos.getY());
        a.add(pos.getZ());
        return a;
    }

    static JsonArray shapeBits(List<AABB> boxes) {
        JsonArray a = new JsonArray();
        for (AABB b : boxes) {
            for (double d : new double[]{b.minX, b.minY, b.minZ, b.maxX, b.maxY, b.maxZ}) {
                a.add(Double.doubleToRawLongBits(d));
            }
        }
        return a;
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

    /**
     * A random source whose every roll is the lowest possible: 0, false, itself.
     */
    static RandomSource luckiest() {
        return (RandomSource) java.lang.reflect.Proxy.newProxyInstance(BlockRefGen.class.getClassLoader(),
                new Class<?>[]{RandomSource.class}, (proxy, method, args) -> {
                    Class<?> r = method.getReturnType();
                    if (r == int.class) {
                        return 0;
                    } else if (r == long.class) {
                        return 0L;
                    } else if (r == float.class) {
                        return 0F;
                    } else if (r == double.class) {
                        return 0D;
                    } else if (r == boolean.class) {
                        return false;
                    } else if (RandomSource.class.isAssignableFrom(r)) {
                        return proxy;
                    } else if (r == void.class) {
                        return null;
                    }
                    throw new UnsupportedOperationException("RandomSource." + method.getName());
                });
    }

    private static ExecRefGen.FakeLevel bonemealLevel;

    /**
     * Blocks whose bone meal checks need more of a world than the stand-in level has (they
     * scan chunk sections for neighbours to spread from); exported as not bonemealable. None
     * of them may be a block FarmProcess scans.
     */
    static final Set<String> BONEMEAL_UNKNOWN = new java.util.TreeSet<>();

    /**
     * {@code BonemealableBlock.isValidBonemealTarget && isBonemealSuccess} for the state alone
     * in an otherwise empty world, with the luckiest roll: FarmProcess's check without the world.
     */
    private static boolean bonemealable(BlockState state) throws Exception {
        if (!(state.getBlock() instanceof BonemealableBlock b)) {
            return false;
        }
        if (bonemealLevel == null) {
            ExecRefGen.ExecWorld world = new ExecRefGen.ExecWorld(-16, -16, 32, 32, -16, 48);
            bonemealLevel = ExecRefGen.fakeLevel(world);
            setField(Level.class, bonemealLevel, "random", luckiest());
        }
        BlockPos pos = new BlockPos(0, 0, 0);
        bonemealLevel.world.set(0, 0, 0, state);
        try {
            return b.isValidBonemealTarget(bonemealLevel, pos, state, BonemealSource.INTERACTION)
                    && b.isBonemealSuccess(bonemealLevel, luckiest(), pos, state, BonemealSource.INTERACTION);
        } catch (RuntimeException e) {
            String name = BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString();
            for (Block farmed : FARM_SCANNED) {
                if (state.getBlock() == farmed) {
                    throw new IllegalStateException("bone meal on " + state, e);
                }
            }
            BONEMEAL_UNKNOWN.add(name);
            return false;
        } finally {
            bonemealLevel.world.set(0, 0, 0, Blocks.AIR.defaultBlockState());
        }
    }

    /**
     * The blocks FarmProcess scans for (its {@code Harvest} blocks and the replanting ones).
     */
    private static final Block[] FARM_SCANNED = {
            Blocks.WHEAT, Blocks.CARROTS, Blocks.POTATOES, Blocks.BEETROOTS, Blocks.PUMPKIN, Blocks.MELON,
            Blocks.NETHER_WART, Blocks.COCOA, Blocks.SUGAR_CANE, Blocks.BAMBOO, Blocks.CACTUS,
            Blocks.FARMLAND, Blocks.JUNGLE_LOG, Blocks.SOUL_SAND,
    };

    private static final Map<Block, List<String>> DROPS = new HashMap<>();

    /**
     * The blocks whose loot roll failed in {@link #drops}, and the first failure.
     */
    static int dropsFailures;
    static Throwable firstDropsFailure;

    /**
     * {@code BlockOptionalMeta.drops(Block)}, with its mixin accessor
     * ({@code ILootTable.invokeGetRandomItems}) replaced by reflection on the private
     * {@code LootTable.getRandomItems(LootContext)}. Item ids, in the loot table's order.
     * <p>
     * Like upstream's, a failure means no drops. In 26.3 every roll fails:
     * {@code LootContext.Builder.create} asks the level for its server, and the
     * {@code ServerLevelStub} upstream allocates without a constructor has none.
     */
    @SuppressWarnings("unchecked")
    static List<String> drops(Block block) {
        List<String> cached = DROPS.get(block);
        if (cached != null) {
            return cached;
        }
        List<String> items = new ArrayList<>();
        Optional<ResourceKey<LootTable>> key = block.getLootTable();
        if (key.isPresent()) {
            try {
                ServerLevel level = BlockOptionalMeta.ServerLevelStub.fastCreate();
                LootParams.Builder builder = new LootParams.Builder(level)
                        .withParameter(LootContextParams.ORIGIN, Vec3.ZERO)
                        .withParameter(LootContextParams.BLOCK_STATE, block.defaultBlockState())
                        .withParameter(LootContextParams.TOOL, new ItemStack(Items.NETHERITE_PICKAXE, 1));
                // getDrops(Block, LootParams.Builder)
                LootParams params = builder.withParameter(LootContextParams.BLOCK_STATE, block.defaultBlockState())
                        .create(LootContextParamSets.BLOCK);
                BlockOptionalMeta.ServerLevelStub stub = (BlockOptionalMeta.ServerLevelStub) params.getLevel();
                LootTable table = stub.holder().getLootTable(key.get());
                Method getRandomItems = LootTable.class.getDeclaredMethod("getRandomItems", LootContext.class);
                getRandomItems.setAccessible(true);
                List<ItemStack> stacks = (List<ItemStack>) getRandomItems.invoke(table,
                        new LootContext.Builder(params).withOptionalRandomSeed(1).create(null));
                for (ItemStack stack : stacks) {
                    items.add(BuiltInRegistries.ITEM.getKey(stack.getItem()).toString());
                }
            } catch (Throwable e) {
                items.clear();
                dropsFailures++;
                if (firstDropsFailure == null) {
                    firstDropsFailure = e;
                }
            }
        }
        DROPS.put(block, items);
        return items;
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
            FakeBsi bsi = allocate(FakeBsi.class);
            bsi.blocks = blocks;
            bsi.unloadedChunks = unloadedChunks;
            setField(BlockStateInterface.class, bsi, "worldBorder", new BetterWorldBorder(border));
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
     * An instance of {@code clazz} created without running a constructor.
     */
    @SuppressWarnings("unchecked")
    static <T> T allocate(Class<T> clazz) throws Exception {
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        return (T) ((sun.misc.Unsafe) theUnsafe.get(null)).allocateInstance(clazz);
    }

    /**
     * Sets a (possibly private or final) field declared by {@code owner}.
     */
    static void setField(Class<?> owner, Object target, String name, Object value) throws ReflectiveOperationException {
        Field field = owner.getDeclaredField(name);
        field.setAccessible(true);
        field.set(target, value);
    }

    /**
     * Blocks that the position checks treat specially, drawn more often than a uniform pick.
     */
    static final List<Block> INTERESTING = List.of(
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

    static BlockState anyStateOf(Block block) {
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
