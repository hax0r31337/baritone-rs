package refgen;

import baritone.api.BaritoneAPI;
import baritone.api.IBaritone;
import baritone.api.Settings;
import baritone.api.pathing.calc.IPath;
import baritone.api.pathing.goals.Goal;
import baritone.api.pathing.goals.GoalGetToBlock;
import baritone.api.pathing.goals.GoalNear;
import baritone.api.pathing.goals.GoalStrictDirection;
import baritone.api.pathing.goals.GoalTwoBlocks;
import baritone.api.pathing.goals.GoalXZ;
import baritone.api.pathing.movement.ActionCosts;
import baritone.api.pathing.movement.IMovement;
import baritone.api.utils.BetterBlockPos;
import baritone.api.utils.IPlayerContext;
import baritone.api.utils.PathCalculationResult;
import baritone.pathing.calc.AStarPathFinder;
import baritone.pathing.movement.CalculationContext;
import baritone.pathing.movement.MovementHelper;
import baritone.pathing.movement.Moves;
import baritone.pathing.precompute.PrecomputedData;
import baritone.utils.BlockStateInterface;
import baritone.utils.ToolSet;
import baritone.utils.pathing.Avoidance;
import baritone.utils.pathing.BetterWorldBorder;
import baritone.utils.pathing.Favoring;
import baritone.utils.pathing.MutableMoveResult;
import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import it.unimi.dsi.fastutil.longs.Long2DoubleOpenHashMap;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderSet;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.tags.TagKey;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.EntityEquipment;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.Tool;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.DoorBlock;
import net.minecraft.world.level.block.DoublePlantBlock;
import net.minecraft.world.level.block.FenceGateBlock;
import net.minecraft.world.level.block.LadderBlock;
import net.minecraft.world.level.block.SlabBlock;
import net.minecraft.world.level.block.SnowLayerBlock;
import net.minecraft.world.level.block.StairBlock;
import net.minecraft.world.level.block.TrapDoorBlock;
import net.minecraft.world.level.block.VineBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;
import net.minecraft.world.level.block.state.properties.Half;
import net.minecraft.world.level.block.state.properties.SlabType;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.dimension.DimensionType;

import java.io.OutputStream;
import java.io.OutputStreamWriter;
import java.io.PrintStream;
import java.io.Writer;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Random;
import java.util.Set;
import java.util.zip.GZIPOutputStream;

import static refgen.RefGen.GoalCase;
import static refgen.RefGen.d;
import static refgen.RefGen.f;
import static refgen.RefGen.pos;
import static refgen.RefGen.row;

/**
 * Generates reference path calculations: the real upstream {@link AStarPathFinder} (with the
 * real CalculationContext methods, MovementHelper, movements, Path, ToolSet, Favoring) running
 * on generated worlds, plus the raw results of every {@link Moves} at sampled positions and
 * {@link ToolSet} break speeds for every block.
 * <p>
 * The client objects the code under test reaches for are stand-ins: a {@code ClientLevel} and a
 * {@code LocalPlayer} allocated without a constructor, with only the fields it reads set, an
 * {@code IBaritone} proxy, and an array-backed {@code BlockStateInterface}. The
 * {@code CalculationContext} is allocated the same way and its fields are set with the
 * expressions of the upstream constructor. Needs the block and item tags bound
 * ({@link BlockRefGen#write} does that).
 */
final class PathRefGen {

    private static final Random R = new Random(0x3c6ef372L);

    static final int MIN_Y = -32;
    static final int HEIGHT = 96;
    /** Loaded chunks are CHUNK_MIN..CHUNK_MAX on both axes. */
    static final int CHUNK_MIN = -2;
    static final int CHUNK_MAX = 1;
    static final int X0 = CHUNK_MIN * 16;
    static final int Z0 = CHUNK_MIN * 16;
    static final int SX = (CHUNK_MAX - CHUNK_MIN + 1) * 16;
    static final int SZ = SX;

    /** Large enough that no calculation times out, so results do not depend on speed. */
    static final long TIMEOUT = 600_000L;

    private static final BlockState AIR = Blocks.AIR.defaultBlockState();

    private PathRefGen() {}

    static void write(String upstream, String minecraft, Path out) throws Exception {
        JsonObject configs = configs();
        Map<String, Inventory0> inventories = inventories();
        JsonObject inventoriesJson = new JsonObject();
        inventories.forEach((name, inv) -> inventoriesJson.add(name, inv.json()));

        JsonObject root = new JsonObject();
        root.addProperty("upstream", upstream);
        root.addProperty("minecraft", minecraft);
        root.addProperty("min_y", MIN_Y);
        root.addProperty("height", HEIGHT);
        root.add("chunk_range", row(CHUNK_MIN, CHUNK_MAX));
        root.addProperty("timeout", TIMEOUT);
        root.add("configs", configs);
        root.add("inventories", inventoriesJson);
        root.add("tool_set", toolSet(configs, inventories));
        root.add("worlds", worlds(configs, inventories));
        BlockRefGen.apply(new JsonObject());

        try (Writer w = new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(out)), StandardCharsets.UTF_8)) {
            w.write(new Gson().toJson(root));
            w.write('\n');
        }
    }

    // region settings

    /**
     * Settings configurations. Every key must be a setting the Rust port has (the replay test
     * checks).
     */
    private static JsonObject configs() {
        JsonObject configs = new JsonObject();
        configs.add("a", new JsonObject());

        JsonObject b = new JsonObject();
        b.addProperty("allowParkour", true);
        b.addProperty("allowParkourPlace", true);
        b.addProperty("allowDiagonalDescend", true);
        b.addProperty("allowDiagonalAscend", true);
        b.addProperty("allowWalkOnMagmaBlocks", true);
        b.addProperty("allowVines", true);
        b.addProperty("maxFallHeightNoWater", 6);
        b.addProperty("maxFallHeightBucket", 12);
        b.addProperty("jumpPenalty", 1.25);
        b.addProperty("blockPlacementPenalty", 4.5);
        b.addProperty("walkOnWaterOnePenalty", 0.75);
        b.addProperty("blockBreakAdditionalPenalty", 0.5);
        b.addProperty("backtrackCostFavoringCoefficient", 0.25);
        configs.add("b", b);

        JsonObject c = new JsonObject();
        c.addProperty("allowBreak", false);
        c.add("allowBreakAnyway", BlockRefGen.names(Blocks.DIRT, Blocks.GRASS_BLOCK, Blocks.OAK_LEAVES, Blocks.SAND));
        c.addProperty("allowDownward", false);
        c.addProperty("assumeWalkOnWater", true);
        c.addProperty("allowPlaceInFluidsSource", false);
        c.addProperty("allowPlaceInFluidsFlow", false);
        c.addProperty("allowWaterBucketFall", false);
        c.addProperty("allowSprint", false);
        c.addProperty("strictLiquidCheck", true);
        c.addProperty("avoidUpdatingFallingBlocks", false);
        c.addProperty("considerPotionEffects", false);
        c.add("blocksToAvoid", BlockRefGen.names(Blocks.TRIPWIRE, Blocks.OAK_DOOR, Blocks.COBWEB, Blocks.TORCH));
        c.add("blocksToDisallowBreaking", BlockRefGen.names(Blocks.OAK_LOG, Blocks.GLASS));
        c.addProperty("allowParkour", true);
        c.addProperty("allowParkourAscend", false);
        c.addProperty("allowWalkOnBottomSlab", false);
        configs.add("c", c);

        JsonObject d = new JsonObject();
        d.addProperty("cutoffAtLoadBoundary", true);
        d.addProperty("pathCutoffMinimumLength", 8);
        d.addProperty("pathCutoffFactor", 0.5);
        d.addProperty("minimumImprovementRepropagation", false);
        d.addProperty("autoTool", false);
        d.addProperty("useSwordToMine", false);
        d.addProperty("itemSaver", true);
        d.addProperty("itemSaverThreshold", 20);
        d.add("blocksToAvoidBreaking", BlockRefGen.names(Blocks.STONE, Blocks.DIRT));
        d.addProperty("avoidBreakingMultiplier", 3.0);
        d.addProperty("assumeWalkOnLava", true);
        d.addProperty("allowJumpAtBuildLimit", true);
        d.addProperty("allowParkour", true);
        d.addProperty("allowDiagonalDescend", true);
        d.addProperty("pathingMaxChunkBorderFetch", 200);
        d.addProperty("allowPlace", false);
        d.addProperty("backtrackCostFavoringCoefficient", 2.0);
        configs.add("d", d);
        return configs;
    }

    // endregion

    // region inventories and the player

    /**
     * A named inventory: 36 main slots, the selected slot.
     */
    record Inventory0(ItemStack[] items, int selected) {
        JsonObject json() {
            JsonObject o = new JsonObject();
            JsonArray a = new JsonArray();
            int last = items.length;
            while (last > 0 && items[last - 1].isEmpty()) {
                last--;
            }
            for (int i = 0; i < last; i++) {
                a.add(itemJson(items[i]));
            }
            o.add("items", a);
            o.addProperty("selected", selected);
            return o;
        }
    }

    private static ItemStack stack(Item item, int count) {
        return new ItemStack(item, count);
    }

    private static ItemStack damaged(Item item, int damageFromMax) {
        ItemStack s = new ItemStack(item);
        s.setDamageValue(s.getMaxDamage() - damageFromMax);
        return s;
    }

    private static Map<String, Inventory0> inventories() {
        Map<String, Inventory0> m = new LinkedHashMap<>();
        m.put("empty", inventory(0));

        Inventory0 tools = inventory(3);
        tools.items[0] = stack(Items.WOODEN_PICKAXE, 1);
        tools.items[1] = stack(Items.STONE_AXE, 1);
        tools.items[2] = stack(Items.IRON_SHOVEL, 1);
        tools.items[3] = damaged(Items.DIAMOND_PICKAXE, 40);
        tools.items[4] = stack(Items.GOLDEN_HOE, 1);
        tools.items[5] = stack(Items.SHEARS, 1);
        tools.items[6] = stack(Items.IRON_SWORD, 1);
        tools.items[7] = stack(Items.WATER_BUCKET, 1);
        tools.items[8] = stack(Items.DIRT, 64);
        tools.items[20] = stack(Items.NETHERITE_PICKAXE, 1);
        m.put("tools", tools);

        Inventory0 basic = inventory(5);
        basic.items[0] = stack(Items.OAK_PLANKS, 32);
        basic.items[2] = stack(Items.STONE_PICKAXE, 1);
        basic.items[4] = stack(Items.COBBLESTONE, 64);
        basic.items[15] = stack(Items.WATER_BUCKET, 1);
        m.put("basic", basic);

        Inventory0 swords = inventory(1);
        swords.items[0] = stack(Items.DIAMOND_SWORD, 1);
        swords.items[1] = stack(Items.NETHERITE_SWORD, 1);
        swords.items[2] = stack(Items.STICK, 3);
        swords.items[3] = stack(Items.SHEARS, 1);
        swords.items[4] = stack(Items.STONE_SHOVEL, 1);
        swords.items[6] = stack(Items.WATER_BUCKET, 1);
        m.put("swords", swords);

        Inventory0 worn = inventory(8);
        worn.items[1] = damaged(Items.DIAMOND_PICKAXE, 5);
        worn.items[2] = damaged(Items.IRON_PICKAXE, 200);
        worn.items[3] = damaged(Items.GOLDEN_PICKAXE, 1);
        worn.items[5] = damaged(Items.DIAMOND_AXE, 1000);
        worn.items[7] = stack(Items.COPPER_PICKAXE, 1);
        m.put("worn", worn);
        return m;
    }

    private static Inventory0 inventory(int selected) {
        ItemStack[] items = new ItemStack[36];
        java.util.Arrays.fill(items, ItemStack.EMPTY);
        return new Inventory0(items, selected);
    }

    /**
     * An item as the host sends it (crate::host::ItemStack).
     */
    static JsonObject itemJson(ItemStack s) {
        JsonObject o = new JsonObject();
        o.addProperty("name", BuiltInRegistries.ITEM.getKey(s.getItem()).toString());
        o.addProperty("count", s.getCount());
        int damage = s.getOrDefault(DataComponents.DAMAGE, 0);
        if (damage != 0) {
            o.addProperty("damage", damage);
        }
        if (s.getMaxDamage() != 0) {
            o.addProperty("max_damage", s.getMaxDamage());
        }
        Tool tool = s.get(DataComponents.TOOL);
        if (tool != null) {
            JsonObject t = new JsonObject();
            JsonArray rules = new JsonArray();
            for (Tool.Rule rule : tool.rules()) {
                JsonObject r = new JsonObject();
                r.add("blocks", blockSet(rule.blocks()));
                rule.speed().ifPresent(speed -> r.addProperty("speed", speed));
                rule.correctForDrops().ifPresent(correct -> r.addProperty("correct_for_drops", correct));
                rules.add(r);
            }
            t.add("rules", rules);
            t.addProperty("default_mining_speed", tool.defaultMiningSpeed());
            o.add("tool", t);
        }
        List<String> tags = new ArrayList<>();
        s.typeHolder().tags().forEach(tag -> tags.add(tag.location().toString()));
        tags.sort(null);
        if (!tags.isEmpty()) {
            JsonArray a = new JsonArray();
            tags.forEach(a::add);
            o.add("tags", a);
        }
        return o;
    }

    private static JsonObject blockSet(HolderSet<Block> set) {
        JsonObject o = new JsonObject();
        Optional<TagKey<Block>> tag = set.unwrapKey();
        if (tag.isPresent()) {
            o.addProperty("tag", tag.get().location().toString());
        } else {
            JsonArray blocks = new JsonArray();
            for (Holder<Block> holder : set) {
                blocks.add(BuiltInRegistries.BLOCK.getKey(holder.value()).toString());
            }
            o.add("blocks", blocks);
        }
        return o;
    }

    /**
     * The player-level inputs of one calculation.
     */
    record PlayerSpec(String inventory, int food, int frostWalker, Float waterEfficiency,
                      List<MobEffectInstance> effects, boolean throwaway) {
        JsonObject json() {
            JsonObject o = new JsonObject();
            o.addProperty("inventory", inventory);
            o.addProperty("food", food);
            o.addProperty("frost_walker", frostWalker);
            o.add("water_efficiency", waterEfficiency == null ? JsonNull.INSTANCE : f(waterEfficiency));
            JsonArray e = new JsonArray();
            for (MobEffectInstance effect : effects) {
                e.add(row(BuiltInRegistries.MOB_EFFECT.getKey(effect.getEffect().value()).toString(), effect.getAmplifier()));
            }
            o.add("effects", e);
            o.addProperty("throwaway", throwaway);
            return o;
        }
    }

    private static PlayerSpec randomPlayer(List<String> inventories) {
        String inventory = inventories.get(R.nextInt(inventories.size()));
        int food = R.nextInt(4) == 0 ? R.nextInt(7) : 6 + R.nextInt(15);
        int frostWalker = R.nextInt(6) == 0 ? 1 + R.nextInt(2) : 0;
        Float water = switch (R.nextInt(6)) {
            case 0 -> 1.0f / 3.0f;
            case 1 -> 1.0f;
            case 2 -> 0.0f;
            default -> null;
        };
        List<MobEffectInstance> effects = new ArrayList<>();
        if (R.nextInt(4) == 0) {
            effects.add(new MobEffectInstance(MobEffects.HASTE, 100, R.nextInt(3)));
        }
        if (R.nextInt(5) == 0) {
            effects.add(new MobEffectInstance(MobEffects.MINING_FATIGUE, 100, R.nextInt(5)));
        }
        return new PlayerSpec(inventory, food, frostWalker, water, effects, R.nextInt(5) != 0);
    }

    /**
     * A LocalPlayer with only the inventory and effects set.
     */
    static LocalPlayer fakePlayer(Inventory0 inv, List<MobEffectInstance> effects) throws Exception {
        LocalPlayer player = BlockRefGen.allocate(LocalPlayer.class);
        Inventory inventory = new Inventory(player, new EntityEquipment());
        for (int i = 0; i < inv.items.length; i++) {
            inventory.setItem(i, inv.items[i].copy());
        }
        inventory.setSelectedSlot(inv.selected);
        BlockRefGen.setField(Player.class, player, "inventory", inventory);
        Map<Holder<MobEffect>, MobEffectInstance> active = new HashMap<>();
        for (MobEffectInstance effect : effects) {
            active.put(effect.getEffect(), effect);
        }
        BlockRefGen.setField(LivingEntity.class, player, "activeEffects", active);
        return player;
    }

    // endregion

    // region fake client

    /**
     * An IBaritone whose player context is a proxy that answers nothing: movements store it
     * but path calculation never uses it.
     */
    static IBaritone fakeBaritone() {
        ClassLoader loader = PathRefGen.class.getClassLoader();
        IPlayerContext ctx = (IPlayerContext) Proxy.newProxyInstance(loader, new Class<?>[]{IPlayerContext.class},
                (proxy, method, args) -> {
                    throw new UnsupportedOperationException("IPlayerContext." + method.getName());
                });
        return (IBaritone) Proxy.newProxyInstance(loader, new Class<?>[]{IBaritone.class},
                (proxy, method, args) -> {
                    if (method.getName().equals("getPlayerContext")) {
                        return ctx;
                    }
                    throw new UnsupportedOperationException("IBaritone." + method.getName());
                });
    }

    static ClientLevel fakeLevel(WorldBorder border) throws Exception {
        ClientLevel level = BlockRefGen.allocate(ClientLevel.class);
        DimensionType type = new DimensionType(false, true, false, false, 1.0, MIN_Y, HEIGHT, HEIGHT,
                null, 0f, null, null, null, null, null, Optional.empty());
        BlockRefGen.setField(Level.class, level, "dimensionTypeRegistration", Holder.direct(type));
        BlockRefGen.setField(ClientLevel.class, level, "worldBorder", border);
        return level;
    }

    /**
     * A BlockStateInterface over a generated world, read like the real one: air above and
     * below the world and in unloaded chunks.
     */
    static final class ArrayBsi extends BlockStateInterface {

        private GenWorld world;

        @SuppressWarnings("unused")
        private ArrayBsi() {
            super(null);
        }

        static ArrayBsi create(GenWorld world) throws Exception {
            ArrayBsi bsi = BlockRefGen.allocate(ArrayBsi.class);
            bsi.world = world;
            BlockRefGen.setField(BlockStateInterface.class, bsi, "worldBorder", new BetterWorldBorder(world.border));
            return bsi;
        }

        @Override
        public boolean worldContainsLoadedChunk(int blockX, int blockZ) {
            return world.loaded.contains(ChunkPos.pack(blockX >> 4, blockZ >> 4));
        }

        @Override
        public boolean isLoaded(int x, int z) {
            return worldContainsLoadedChunk(x, z);
        }

        @Override
        public BlockState get0(int x, int y, int z) {
            if (y < MIN_Y || y >= MIN_Y + HEIGHT) {
                return AIR;
            }
            if (!worldContainsLoadedChunk(x, z)) {
                return AIR;
            }
            return world.get(x, y, z);
        }
    }

    /**
     * The CalculationContext upstream's constructor would build for this player and the
     * current settings.
     */
    static CalculationContext context(IBaritone baritone, ClientLevel level, ArrayBsi bsi, Inventory0 inv, PlayerSpec p)
            throws Exception {
        LocalPlayer player = fakePlayer(inv, p.effects);
        Settings s = BaritoneAPI.getSettings();
        CalculationContext c = BlockRefGen.allocate(CalculationContext.class);
        Set<String> set = new HashSet<>();
        FieldSetter put = (name, value) -> {
            BlockRefGen.setField(CalculationContext.class, c, name, value);
            set.add(name);
        };
        put.set("precomputedData", new PrecomputedData());
        put.set("safeForThreadedUse", true);
        put.set("baritone", baritone);
        put.set("world", level);
        put.set("worldData", null);
        put.set("bsi", bsi);
        put.set("toolSet", new ToolSet(player));
        put.set("hasThrowaway", s.allowPlace.value && p.throwaway);
        put.set("hasWaterBucket", s.allowWaterBucketFall.value
                && Inventory.isHotbarSlot(player.getInventory().findSlotMatchingItem(new ItemStack(Items.WATER_BUCKET)))
                && level.dimension() != Level.NETHER);
        put.set("canSprint", s.allowSprint.value && p.food > 6);
        put.set("placeBlockCost", s.blockPlacementPenalty.value);
        put.set("allowBreak", s.allowBreak.value);
        put.set("allowBreakAnyway", new ArrayList<>(s.allowBreakAnyway.value));
        put.set("allowParkour", s.allowParkour.value);
        put.set("allowParkourPlace", s.allowParkourPlace.value);
        put.set("allowJumpAtBuildLimit", s.allowJumpAtBuildLimit.value);
        put.set("allowParkourAscend", s.allowParkourAscend.value);
        put.set("assumeWalkOnWater", s.assumeWalkOnWater.value);
        put.set("allowFallIntoLava", false);
        put.set("frostWalker", p.frostWalker);
        put.set("allowDiagonalDescend", s.allowDiagonalDescend.value);
        put.set("allowDiagonalAscend", s.allowDiagonalAscend.value);
        put.set("allowDownward", s.allowDownward.value);
        put.set("minFallHeight", 3);
        put.set("maxFallHeightNoWater", s.maxFallHeightNoWater.value);
        put.set("maxFallHeightBucket", s.maxFallHeightBucket.value);
        float waterSpeedMultiplier = p.waterEfficiency == null ? 1.0f : p.waterEfficiency;
        put.set("waterWalkSpeed", ActionCosts.WALK_ONE_IN_WATER_COST * (1 - waterSpeedMultiplier) + ActionCosts.WALK_ONE_BLOCK_COST * waterSpeedMultiplier);
        put.set("breakBlockAdditionalCost", s.blockBreakAdditionalPenalty.value);
        put.set("backtrackCostFavoringCoefficient", s.backtrackCostFavoringCoefficient.value);
        put.set("jumpPenalty", s.jumpPenalty.value);
        put.set("walkOnWaterOnePenalty", s.walkOnWaterOnePenalty.value);
        put.set("allowWalkOnMagmaBlocks", s.allowWalkOnMagmaBlocks.value);
        put.set("worldBorder", new BetterWorldBorder(level.getWorldBorder()));
        for (Field field : CalculationContext.class.getDeclaredFields()) {
            if (!Modifier.isStatic(field.getModifiers()) && !set.contains(field.getName())) {
                throw new IllegalStateException("CalculationContext." + field.getName() + " is not set");
            }
        }
        return c;
    }

    @FunctionalInterface
    interface FieldSetter {
        void set(String name, Object value) throws ReflectiveOperationException;
    }

    // endregion

    // region tool set

    private static JsonArray toolSet(JsonObject configs, Map<String, Inventory0> inventories) throws Exception {
        List<Block> blocks = new ArrayList<>();
        BuiltInRegistries.BLOCK.forEach(blocks::add);
        JsonArray out = new JsonArray();
        for (String config : new String[]{"a", "d"}) {
            for (Map.Entry<String, Inventory0> inv : inventories.entrySet()) {
                for (int e = 0; e < 2; e++) {
                    BlockRefGen.apply(configs.getAsJsonObject(config));
                    List<MobEffectInstance> effects = e == 0 ? List.of()
                            : List.of(new MobEffectInstance(MobEffects.HASTE, 100, 1), new MobEffectInstance(MobEffects.MINING_FATIGUE, 100, 0));
                    ToolSet ts = new ToolSet(fakePlayer(inv.getValue(), effects));
                    JsonObject o = new JsonObject();
                    o.addProperty("config", config);
                    o.add("player", new PlayerSpec(inv.getKey(), 20, 0, null, effects, false).json());
                    // [default state id, getStrVsBlock bits, getBestSlot(b, false), getBestSlot(b, true)]
                    JsonArray values = new JsonArray();
                    for (Block block : blocks) {
                        BlockState state = block.defaultBlockState();
                        values.add(row(Block.BLOCK_STATE_REGISTRY.getId(state), d(ts.getStrVsBlock(state)),
                                ts.getBestSlot(block, false), ts.getBestSlot(block, true)));
                    }
                    o.add("values", values);
                    out.add(o);
                }
            }
        }
        return out;
    }

    // endregion

    // region worlds

    /**
     * A generated world: blocks for every loaded chunk position (x outermost, then z, then y),
     * the loaded chunks and the border.
     */
    static final class GenWorld {
        final String kind;
        final BlockState[] blocks = new BlockState[SX * SZ * HEIGHT];
        final Set<Long> loaded = new HashSet<>();
        WorldBorder border = new WorldBorder();

        GenWorld(String kind) {
            this.kind = kind;
            java.util.Arrays.fill(blocks, AIR);
            for (int cx = CHUNK_MIN; cx <= CHUNK_MAX; cx++) {
                for (int cz = CHUNK_MIN; cz <= CHUNK_MAX; cz++) {
                    loaded.add(ChunkPos.pack(cx, cz));
                }
            }
        }

        static boolean inside(int x, int y, int z) {
            return x >= X0 && x < X0 + SX && z >= Z0 && z < Z0 + SZ && y >= MIN_Y && y < MIN_Y + HEIGHT;
        }

        BlockState get(int x, int y, int z) {
            if (!inside(x, y, z)) {
                return AIR;
            }
            return blocks[((x - X0) * SZ + (z - Z0)) * HEIGHT + (y - MIN_Y)];
        }

        void set(int x, int y, int z, BlockState state) {
            if (inside(x, y, z)) {
                blocks[((x - X0) * SZ + (z - Z0)) * HEIGHT + (y - MIN_Y)] = state;
            }
        }

        /**
         * First y from the top where the block below is not air-like (the surface to stand on).
         */
        int surface(int x, int z) {
            for (int y = MIN_Y + HEIGHT - 1; y > MIN_Y; y--) {
                if (!get(x, y - 1, z).isAir()) {
                    return y;
                }
            }
            return MIN_Y;
        }

        /**
         * [id, count, id, count, ...] over the block order.
         */
        JsonArray rle() {
            JsonArray a = new JsonArray();
            int i = 0;
            while (i < blocks.length) {
                int j = i;
                while (j < blocks.length && blocks[j] == blocks[i]) {
                    j++;
                }
                a.add(Block.BLOCK_STATE_REGISTRY.getId(blocks[i]));
                a.add(j - i);
                i = j;
            }
            return a;
        }
    }

    private static JsonArray worlds(JsonObject configs, Map<String, Inventory0> inventories) throws Exception {
        List<String> configNames = new ArrayList<>(configs.keySet());
        List<String> inventoryNames = new ArrayList<>(inventories.keySet());
        List<BlockState> allStates = new ArrayList<>();
        Block.BLOCK_STATE_REGISTRY.forEach(allStates::add);
        IBaritone baritone = fakeBaritone();

        List<GenWorld> worlds = new ArrayList<>();
        for (int i = 0; i < 14; i++) {
            worlds.add(terrain(allStates));
        }
        for (int i = 0; i < 4; i++) {
            worlds.add(noise(allStates));
        }
        worlds.add(flat());
        worlds.add(flat());

        JsonArray out = new JsonArray();
        PrintStream stdout = System.out;
        for (int index = 0; index < worlds.size(); index++) {
            GenWorld world = worlds.get(index);
            if (index % 3 == 1) {
                // a hole in the loaded area
                int cx = CHUNK_MIN + R.nextInt(CHUNK_MAX - CHUNK_MIN + 1);
                int cz = CHUNK_MIN + R.nextInt(CHUNK_MAX - CHUNK_MIN + 1);
                world.loaded.remove(ChunkPos.pack(cx, cz));
            }
            if (index % 4 == 2) {
                world.border.setCenter(R.nextInt(17) - 8 + R.nextDouble(), R.nextInt(17) - 8 + R.nextDouble());
                world.border.setSize(24 + R.nextInt(40) + R.nextDouble());
            }
            ClientLevel level = fakeLevel(world.border);
            ArrayBsi bsi = ArrayBsi.create(world);

            JsonObject w = new JsonObject();
            w.addProperty("kind", world.kind);
            w.add("blocks", world.rle());
            JsonArray loaded = new JsonArray();
            for (int cx = CHUNK_MIN; cx <= CHUNK_MAX; cx++) {
                for (int cz = CHUNK_MIN; cz <= CHUNK_MAX; cz++) {
                    if (world.loaded.contains(ChunkPos.pack(cx, cz))) {
                        loaded.add(row(cx, cz));
                    }
                }
            }
            w.add("loaded_chunks", loaded);
            w.add("border", row(world.border.getMinX(), world.border.getMaxX(), world.border.getMinZ(), world.border.getMaxZ()));

            System.setOut(new PrintStream(OutputStream.nullOutputStream()));
            try {
                w.add("move_samples", moveSamples(world, level, bsi, baritone, configs, inventories, configNames, inventoryNames));
                w.add("queries", queries(world, level, bsi, baritone, configs, inventories, configNames, inventoryNames));
            } finally {
                System.setOut(stdout);
            }
            out.add(w);
        }
        return out;
    }

    // endregion

    // region calculations

    private static BetterBlockPos standable(GenWorld world, ArrayBsi bsi) {
        for (int tries = 0; tries < 200; tries++) {
            int x = X0 + R.nextInt(SX);
            int z = Z0 + R.nextInt(SZ);
            if (!bsi.worldContainsLoadedChunk(x, z)) {
                continue;
            }
            List<Integer> ys = new ArrayList<>();
            for (int y = MIN_Y + 1; y < MIN_Y + HEIGHT - 1; y++) {
                if (MovementHelper.canWalkOn(bsi, x, y - 1, z)
                        && MovementHelper.canWalkThrough(bsi, x, y, z)
                        && MovementHelper.canWalkThrough(bsi, x, y + 1, z)) {
                    ys.add(y);
                }
            }
            if (!ys.isEmpty()) {
                // mostly the top one
                int y = R.nextInt(3) == 0 ? ys.get(R.nextInt(ys.size())) : ys.get(ys.size() - 1);
                return new BetterBlockPos(x, y, z);
            }
        }
        return new BetterBlockPos(X0 + R.nextInt(SX), R.nextInt(20), Z0 + R.nextInt(SZ));
    }

    private static GoalCase randomGoal(GenWorld world, ArrayBsi bsi, BetterBlockPos start) {
        int r = R.nextInt(100);
        if (r < 30) {
            BetterBlockPos p = standable(world, bsi);
            return RefGen.block(p.x, p.y, p.z);
        } else if (r < 40) {
            return RefGen.xz(X0 - 8 + R.nextInt(SX + 16), Z0 - 8 + R.nextInt(SZ + 16));
        } else if (r < 47) {
            return RefGen.yLevel(MIN_Y + 4 + R.nextInt(50));
        } else if (r < 56) {
            BetterBlockPos p = standable(world, bsi);
            int range = R.nextInt(5);
            return new GoalCase(RefGen.spec("GoalNear", "x", p.x, "y", p.y, "z", p.z, "range", range), new GoalNear(p, range), p);
        } else if (r < 63) {
            BetterBlockPos p = standable(world, bsi).below();
            return new GoalCase(RefGen.spec("GoalGetToBlock", "x", p.x, "y", p.y, "z", p.z), new GoalGetToBlock(p), p);
        } else if (r < 69) {
            BetterBlockPos p = standable(world, bsi);
            return new GoalCase(RefGen.spec("GoalTwoBlocks", "x", p.x, "y", p.y, "z", p.z), new GoalTwoBlocks(p), p);
        } else if (r < 75) {
            BetterBlockPos a = standable(world, bsi);
            BetterBlockPos b = standable(world, bsi);
            return RefGen.composite(RefGen.block(a.x, a.y, a.z), RefGen.block(b.x, b.y, b.z));
        } else if (r < 83) {
            Integer maintainY = R.nextBoolean() ? null : start.y;
            return RefGen.runAway(6 + R.nextInt(20) + R.nextDouble(), maintainY, start);
        } else if (r < 88) {
            Direction dir = horizontal();
            return new GoalCase(RefGen.spec("GoalStrictDirection", "x", start.x, "y", start.y, "z", start.z, "direction", dir.getName()),
                    new GoalStrictDirection(start, dir), start);
        } else if (r < 91) {
            return RefGen.inverted(RefGen.block(start.x, start.y, start.z));
        } else {
            int x = (R.nextBoolean() ? 1 : -1) * (200 + R.nextInt(2000));
            int z = (R.nextBoolean() ? 1 : -1) * (200 + R.nextInt(2000));
            return RefGen.xz(x, z);
        }
    }

    private static JsonArray moveSamples(GenWorld world, ClientLevel level, ArrayBsi bsi, IBaritone baritone, JsonObject configs,
                                         Map<String, Inventory0> inventories, List<String> configNames, List<String> inventoryNames)
            throws Exception {
        JsonArray out = new JsonArray();
        for (int k = 0; k < 2; k++) {
            String config = configNames.get(R.nextInt(configNames.size()));
            BlockRefGen.apply(configs.getAsJsonObject(config));
            PlayerSpec player = randomPlayer(inventoryNames);
            CalculationContext context = context(baritone, level, bsi, inventories.get(player.inventory), player);
            JsonObject o = new JsonObject();
            o.addProperty("config", config);
            o.add("player", player.json());
            // [x, y, z, [move ordinal, x, y, z, cost bits]... for each finite cost]
            JsonArray positions = new JsonArray();
            MutableMoveResult res = new MutableMoveResult();
            for (int i = 0; i < 150; i++) {
                BetterBlockPos p = i % 3 == 0
                        ? new BetterBlockPos(X0 + R.nextInt(SX), MIN_Y + R.nextInt(HEIGHT), Z0 + R.nextInt(SZ))
                        : standable(world, bsi);
                JsonArray results = new JsonArray();
                for (Moves moves : Moves.values()) {
                    res.reset();
                    moves.apply(context, p.x, p.y, p.z, res);
                    if (res.cost < ActionCosts.COST_INF) {
                        results.add(row(moves.ordinal(), res.x, res.y, res.z, d(res.cost)));
                    }
                }
                positions.add(row(p.x, p.y, p.z, results));
            }
            o.add("positions", positions);
            out.add(o);
        }
        return out;
    }

    private static JsonArray queries(GenWorld world, ClientLevel level, ArrayBsi bsi, IBaritone baritone, JsonObject configs,
                                     Map<String, Inventory0> inventories, List<String> configNames, List<String> inventoryNames)
            throws Exception {
        JsonArray out = new JsonArray();
        List<BetterBlockPos> previous = null;
        int count = world.kind.equals("noise") ? 8 : 12;
        for (int q = 0; q < count; q++) {
            String config = configNames.get(R.nextInt(configNames.size()));
            BlockRefGen.apply(configs.getAsJsonObject(config));
            PlayerSpec player = randomPlayer(inventoryNames);
            BetterBlockPos start = R.nextInt(8) == 0
                    ? new BetterBlockPos(X0 + R.nextInt(SX), MIN_Y + R.nextInt(HEIGHT), Z0 + R.nextInt(SZ))
                    : standable(world, bsi);
            BetterBlockPos realStart = R.nextInt(8) == 0 ? start.relative(Direction.from3DDataValue(R.nextInt(6))) : start;
            GoalCase goal = randomGoal(world, bsi, start);

            CalculationContext context = context(baritone, level, bsi, inventories.get(player.inventory), player);

            List<BetterBlockPos> favored = previous != null && R.nextInt(3) == 0 ? previous : null;
            Favoring favoring = new Favoring(favored == null ? null : positionsPath(favored), context);
            JsonArray avoidances = new JsonArray();
            if (R.nextInt(5) == 0) {
                Field field = Favoring.class.getDeclaredField("favorings");
                field.setAccessible(true);
                Long2DoubleOpenHashMap map = (Long2DoubleOpenHashMap) field.get(favoring);
                for (int i = 1 + R.nextInt(3); i > 0; i--) {
                    int x = start.x + R.nextInt(21) - 10;
                    int y = start.y + R.nextInt(7) - 3;
                    int z = start.z + R.nextInt(21) - 10;
                    double coefficient = R.nextBoolean() ? 1.5 + R.nextInt(10) : 0.5;
                    int radius = 1 + R.nextInt(6);
                    new Avoidance(x, y, z, coefficient, radius).applySpherical(map);
                    avoidances.add(row(x, y, z, d(coefficient), radius));
                }
            }

            JsonObject o = new JsonObject();
            o.addProperty("config", config);
            o.add("player", player.json());
            o.add("real_start", pos(realStart));
            o.add("start", pos(start));
            o.add("goal", goal.spec());
            if (favored != null) {
                JsonArray f = new JsonArray();
                for (BetterBlockPos p : favored) {
                    f.add(p.x);
                    f.add(p.y);
                    f.add(p.z);
                }
                o.add("favoring", f);
            }
            o.add("avoidances", avoidances);
            JsonObject ctx = new JsonObject();
            ctx.addProperty("has_throwaway", context.hasThrowaway);
            ctx.addProperty("has_water_bucket", context.hasWaterBucket);
            ctx.addProperty("can_sprint", context.canSprint);
            ctx.add("water_walk_speed", d(context.waterWalkSpeed));
            o.add("context", ctx);

            AStarPathFinder finder = new AStarPathFinder(realStart, start.x, start.y, start.z, goal.goal(), favoring, context);
            PathCalculationResult result = finder.calculate(TIMEOUT, TIMEOUT);
            JsonObject r = new JsonObject();
            r.addProperty("type", result.getType().name());
            if (result.getPath().isPresent()) {
                IPath path = result.getPath().get();
                JsonArray positions = new JsonArray();
                for (BetterBlockPos p : path.positions()) {
                    positions.add(p.x);
                    positions.add(p.y);
                    positions.add(p.z);
                }
                r.add("positions", positions);
                // [class, cost bits, calculatedWhileLoaded]
                JsonArray movements = new JsonArray();
                for (IMovement m : path.movements()) {
                    movements.add(row(m.getClass().getSimpleName(), d(m.getCost()), m.calculatedWhileLoaded()));
                }
                r.add("movements", movements);
                r.addProperty("num_nodes", path.getNumNodesConsidered());
                previous = path.positions();
            }
            o.add("result", r);
            out.add(o);
        }
        return out;
    }

    /**
     * A previous path, as far as Favoring reads it.
     */
    private static IPath positionsPath(List<BetterBlockPos> positions) {
        return new IPath() {
            @Override
            public List<IMovement> movements() {
                throw new UnsupportedOperationException();
            }

            @Override
            public List<BetterBlockPos> positions() {
                return positions;
            }

            @Override
            public Goal getGoal() {
                throw new UnsupportedOperationException();
            }

            @Override
            public int getNumNodesConsidered() {
                throw new UnsupportedOperationException();
            }
        };
    }

    // endregion

    // region world generation

    private static BlockState pick(Block... blocks) {
        return blocks[R.nextInt(blocks.length)].defaultBlockState();
    }

    private static Direction horizontal() {
        return Direction.from2DDataValue(R.nextInt(4));
    }

    private static GenWorld flat() {
        GenWorld w = new GenWorld("flat");
        for (int x = X0; x < X0 + SX; x++) {
            for (int z = Z0; z < Z0 + SZ; z++) {
                w.set(x, MIN_Y, z, Blocks.BEDROCK.defaultBlockState());
                for (int y = MIN_Y + 1; y < 0; y++) {
                    w.set(x, y, z, Blocks.STONE.defaultBlockState());
                }
                w.set(x, 0, z, Blocks.GRASS_BLOCK.defaultBlockState());
            }
        }
        for (int i = 0; i < 30; i++) {
            int x = X0 + R.nextInt(SX);
            int z = Z0 + R.nextInt(SZ);
            int h = 1 + R.nextInt(3);
            BlockState s = pick(Blocks.COBBLESTONE, Blocks.OAK_LOG, Blocks.GLASS, Blocks.DIRT, Blocks.SAND);
            for (int y = 1; y <= h; y++) {
                w.set(x, y, z, s);
            }
        }
        return w;
    }

    private static GenWorld noise(List<BlockState> allStates) {
        GenWorld w = new GenWorld("noise");
        int wAir = 20 + R.nextInt(40);
        int wStone = 10 + R.nextInt(30);
        int wWater = R.nextInt(15);
        int wInteresting = 5 + R.nextInt(25);
        int wAny = 2 + R.nextInt(10);
        int total = wAir + wStone + wWater + wInteresting + wAny;
        for (int x = X0; x < X0 + SX; x++) {
            for (int z = Z0; z < Z0 + SZ; z++) {
                for (int y = MIN_Y; y < -4; y++) {
                    w.set(x, y, z, Blocks.STONE.defaultBlockState());
                }
                w.set(x, -4, z, Blocks.GRASS_BLOCK.defaultBlockState());
                for (int y = -3; y < 8; y++) {
                    BlockState s;
                    int r = R.nextInt(total);
                    if ((r -= wAir) < 0) {
                        s = AIR;
                    } else if ((r -= wStone) < 0) {
                        s = Blocks.STONE.defaultBlockState();
                    } else if ((r -= wWater) < 0) {
                        s = R.nextInt(4) == 0 ? BlockRefGen.anyStateOf(Blocks.WATER) : Blocks.WATER.defaultBlockState();
                    } else if ((r -= wInteresting) < 0) {
                        s = BlockRefGen.anyStateOf(BlockRefGen.INTERESTING.get(R.nextInt(BlockRefGen.INTERESTING.size())));
                    } else {
                        s = allStates.get(R.nextInt(allStates.size()));
                    }
                    w.set(x, y, z, s);
                }
            }
        }
        return w;
    }

    private static GenWorld terrain(List<BlockState> allStates) {
        GenWorld w = new GenWorld("terrain");
        double a1 = R.nextDouble() * 6.28, a2 = R.nextDouble() * 6.28, a3 = R.nextDouble() * 6.28;
        double f1 = 4 + R.nextDouble() * 8, f2 = 4 + R.nextDouble() * 8, f3 = 6 + R.nextDouble() * 10;
        int amp1 = R.nextInt(5), amp2 = R.nextInt(4), amp3 = 1 + R.nextInt(4);
        int base = 4 + R.nextInt(8);
        boolean bedrock = R.nextInt(3) != 0;
        int waterLevel = base + R.nextInt(3) - 1;
        int[][] h = new int[SX][SZ];
        for (int x = X0; x < X0 + SX; x++) {
            for (int z = Z0; z < Z0 + SZ; z++) {
                int top = base + (int) Math.round(amp1 * Math.sin(x / f1 + a1) + amp2 * Math.cos(z / f2 + a2) + amp3 * Math.sin((x + z) / f3 + a3));
                h[x - X0][z - Z0] = top;
                for (int y = MIN_Y; y < top; y++) {
                    BlockState s;
                    if (y == MIN_Y && bedrock) {
                        s = Blocks.BEDROCK.defaultBlockState();
                    } else if (y < top - 4) {
                        int r = R.nextInt(100);
                        s = r < 3 ? pick(Blocks.IRON_ORE, Blocks.COAL_ORE, Blocks.DIAMOND_ORE, Blocks.GRAVEL, Blocks.ANDESITE, Blocks.INFESTED_STONE)
                                : Blocks.STONE.defaultBlockState();
                    } else if (y < top - 1) {
                        s = Blocks.DIRT.defaultBlockState();
                    } else {
                        s = top - 1 < waterLevel ? pick(Blocks.SAND, Blocks.GRAVEL, Blocks.DIRT, Blocks.CLAY) : Blocks.GRASS_BLOCK.defaultBlockState();
                    }
                    w.set(x, y, z, s);
                }
                for (int y = top; y <= waterLevel; y++) {
                    w.set(x, y, z, Blocks.WATER.defaultBlockState());
                }
            }
        }
        // shores: some flowing water next to the edges
        for (int i = 0; i < 40; i++) {
            int x = X0 + R.nextInt(SX), z = Z0 + R.nextInt(SZ);
            int y = h[x - X0][z - Z0];
            if (y > waterLevel && y <= waterLevel + 2) {
                w.set(x, y, z, BlockRefGen.anyStateOf(Blocks.WATER));
            }
        }
        // caves
        for (int i = R.nextInt(7); i > 0; i--) {
            int cx = X0 + R.nextInt(SX), cz = Z0 + R.nextInt(SZ);
            int top = h[cx - X0][cz - Z0];
            int cy = MIN_Y + 3 + R.nextInt(Math.max(1, top - MIN_Y - 6));
            int r = 2 + R.nextInt(3);
            BlockState floor = switch (R.nextInt(5)) {
                case 0 -> Blocks.WATER.defaultBlockState();
                case 1 -> Blocks.LAVA.defaultBlockState();
                default -> AIR;
            };
            for (int x = cx - r; x <= cx + r; x++) {
                for (int y = cy - r; y <= cy + r; y++) {
                    for (int z = cz - r; z <= cz + r; z++) {
                        int dx = x - cx, dy = y - cy, dz = z - cz;
                        if (dx * dx + dy * dy + dz * dz <= r * r && y > MIN_Y && !w.get(x, y, z).is(Blocks.BEDROCK)) {
                            w.set(x, y, z, y == cy - r + 1 ? floor : AIR);
                        }
                    }
                }
            }
        }
        // a shaft into the void in worlds without bedrock
        if (!bedrock) {
            int x = X0 + R.nextInt(SX), z = Z0 + R.nextInt(SZ);
            for (int y = MIN_Y; y < h[x - X0][z - Z0]; y++) {
                w.set(x, y, z, AIR);
            }
        }
        for (int i = 40 + R.nextInt(60); i > 0; i--) {
            int x = X0 + R.nextInt(SX), z = Z0 + R.nextInt(SZ);
            feature(w, x, w.surface(x, z), z, allStates);
        }
        return w;
    }

    private static void feature(GenWorld w, int x, int y, int z, List<BlockState> allStates) {
        Direction dir = horizontal();
        int dx = dir.getStepX(), dz = dir.getStepZ();
        switch (R.nextInt(17)) {
            case 0 -> { // tree
                int t = 4 + R.nextInt(3);
                for (int i = 0; i < t; i++) {
                    w.set(x, y + i, z, Blocks.OAK_LOG.defaultBlockState());
                }
                for (int lx = -2; lx <= 2; lx++) {
                    for (int ly = -1; ly <= 1; ly++) {
                        for (int lz = -2; lz <= 2; lz++) {
                            if (Math.abs(lx) + Math.abs(ly) + Math.abs(lz) <= 3 && w.get(x + lx, y + t - 1 + ly, z + lz).isAir()) {
                                w.set(x + lx, y + t - 1 + ly, z + lz, Blocks.OAK_LEAVES.defaultBlockState());
                            }
                        }
                    }
                }
            }
            case 1 -> { // pillar
                BlockState s = pick(Blocks.COBBLESTONE, Blocks.GLASS, Blocks.OBSIDIAN, Blocks.STONE_BRICKS, Blocks.OAK_PLANKS, Blocks.IRON_BLOCK, Blocks.STAINED_GLASS.red());
                for (int i = 1 + R.nextInt(4); i > 0; i--) {
                    w.set(x, y + i - 1, z, s);
                }
            }
            case 2 -> { // wall with something in a gap
                int len = 3 + R.nextInt(5);
                int height = 2 + R.nextInt(2);
                int gap = len / 2;
                for (int i = 0; i < len; i++) {
                    int px = x + dz * i, pz = z + dx * i;
                    for (int j = 0; j < height; j++) {
                        w.set(px, y + j, pz, i == gap ? AIR : Blocks.COBBLESTONE.defaultBlockState());
                    }
                }
                int px = x + dz * gap, pz = z + dx * gap;
                switch (R.nextInt(5)) {
                    case 0, 1 -> {
                        Block door = R.nextBoolean() ? Blocks.OAK_DOOR : Blocks.IRON_DOOR;
                        BlockState lower = door.defaultBlockState()
                                .setValue(DoorBlock.FACING, horizontal())
                                .setValue(DoorBlock.OPEN, R.nextBoolean());
                        w.set(px, y, pz, lower);
                        w.set(px, y + 1, pz, lower.setValue(DoorBlock.HALF, DoubleBlockHalf.UPPER));
                    }
                    case 2 -> w.set(px, y, pz, Blocks.OAK_FENCE_GATE.defaultBlockState()
                            .setValue(FenceGateBlock.FACING, horizontal()).setValue(FenceGateBlock.OPEN, R.nextBoolean()));
                    case 3 -> w.set(px, y, pz, Blocks.OAK_TRAPDOOR.defaultBlockState()
                            .setValue(TrapDoorBlock.OPEN, R.nextBoolean()).setValue(TrapDoorBlock.HALF, R.nextBoolean() ? Half.TOP : Half.BOTTOM));
                    default -> w.set(px, y, pz, Blocks.OAK_FENCE.defaultBlockState());
                }
            }
            case 3 -> { // ladder up a pillar
                int t = 3 + R.nextInt(5);
                for (int i = 0; i < t; i++) {
                    w.set(x, y + i, z, Blocks.COBBLESTONE.defaultBlockState());
                    w.set(x + dx, y + i, z + dz, Blocks.LADDER.defaultBlockState().setValue(LadderBlock.FACING, dir));
                }
            }
            case 4 -> { // vines hanging off a pillar
                int t = 3 + R.nextInt(5);
                for (int i = 0; i < t; i++) {
                    w.set(x, y + i, z, Blocks.MOSSY_COBBLESTONE.defaultBlockState());
                }
                for (int i = 1 + R.nextInt(t); i > 0; i--) {
                    w.set(x + dx, y + t - i, z + dz, Blocks.VINE.defaultBlockState().setValue(VineBlock.getPropertyForFace(dir.getOpposite()), true));
                }
            }
            case 5 -> { // steps of slabs, stairs and full blocks
                int yy = y;
                for (int i = 0; i < 3 + R.nextInt(4); i++) {
                    int px = x + dx * i, pz = z + dz * i;
                    BlockState s = switch (R.nextInt(4)) {
                        case 0 -> Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE, R.nextBoolean() ? SlabType.BOTTOM : SlabType.TOP);
                        case 1 -> Blocks.STONE_STAIRS.defaultBlockState().setValue(StairBlock.FACING, dir.getOpposite())
                                .setValue(StairBlock.HALF, R.nextInt(4) == 0 ? Half.TOP : Half.BOTTOM);
                        case 2 -> Blocks.OAK_PLANKS.defaultBlockState();
                        default -> Blocks.SMOOTH_STONE_SLAB.defaultBlockState();
                    };
                    w.set(px, yy, pz, s);
                    if (R.nextBoolean()) {
                        yy++;
                    }
                }
            }
            case 6 -> { // trench to jump over
                int len = 4 + R.nextInt(6);
                int width = 1 + R.nextInt(3);
                int depth = 2 + R.nextInt(5);
                for (int i = 0; i < len; i++) {
                    for (int j = 0; j < width; j++) {
                        int px = x + dz * i + dx * j, pz = z + dx * i + dz * j;
                        for (int k = 1; k <= depth; k++) {
                            w.set(px, y - k, pz, AIR);
                        }
                        if (R.nextInt(6) == 0) {
                            w.set(px, y - depth, pz, pick(Blocks.WATER, Blocks.LAVA, Blocks.MAGMA_BLOCK, Blocks.FARMLAND));
                        }
                    }
                }
            }
            case 7 -> { // falling blocks
                BlockState s = pick(Blocks.SAND, Blocks.GRAVEL, Blocks.RED_SAND, Blocks.ANVIL);
                if (R.nextBoolean()) {
                    for (int i = 1 + R.nextInt(3); i > 0; i--) {
                        w.set(x, y + i - 1, z, s);
                    }
                } else {
                    // unsupported, above a gap or a walkway
                    w.set(x, y + 2 + R.nextInt(2), z, s);
                    w.set(x + dx, y + 2, z + dz, s);
                }
            }
            case 8 -> { // surface patch
                BlockState s = pick(Blocks.SOUL_SAND, Blocks.MAGMA_BLOCK, Blocks.FARMLAND, Blocks.DIRT_PATH, Blocks.ICE,
                        Blocks.PACKED_ICE, Blocks.HONEY_BLOCK, Blocks.SLIME_BLOCK, Blocks.MUD, Blocks.SOUL_SOIL);
                boolean snow = R.nextInt(4) == 0;
                for (int px = x - 1; px <= x + 1; px++) {
                    for (int pz = z - 1; pz <= z + 1; pz++) {
                        int top = w.surface(px, pz);
                        w.set(px, top - 1, pz, s);
                        if (snow) {
                            w.set(px, top, pz, Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS, 1 + R.nextInt(8)));
                        }
                    }
                }
            }
            case 9 -> w.set(x, y, z, pick(Blocks.COBWEB, Blocks.SWEET_BERRY_BUSH, Blocks.CACTUS, Blocks.FIRE, Blocks.POWDER_SNOW,
                    Blocks.LAVA, Blocks.CAMPFIRE, Blocks.SOUL_FIRE, Blocks.END_PORTAL, Blocks.BUBBLE_COLUMN));
            case 10 -> {
                BlockState s = pick(Blocks.TORCH, Blocks.SHORT_GRASS, Blocks.POPPY, Blocks.CARPET.white(), Blocks.OAK_SIGN,
                        Blocks.FLOWER_POT, Blocks.SKELETON_SKULL, Blocks.CHEST, Blocks.ENDER_CHEST, Blocks.CRAFTING_TABLE,
                        Blocks.SCAFFOLDING, Blocks.BELL, Blocks.LANTERN, Blocks.END_ROD, Blocks.POINTED_DRIPSTONE,
                        Blocks.AMETHYST_CLUSTER, Blocks.BAMBOO, Blocks.AZALEA, Blocks.TALL_GRASS, Blocks.MOSS_CARPET,
                        Blocks.SHULKER_BOX, Blocks.CAULDRON, Blocks.HOPPER, Blocks.OAK_PRESSURE_PLATE, Blocks.RAIL, Blocks.TRIPWIRE);
                w.set(x, y, z, s);
                if (s.getBlock() instanceof DoublePlantBlock) {
                    w.set(x, y + 1, z, s.setValue(DoublePlantBlock.HALF, DoubleBlockHalf.UPPER));
                }
                if (s.is(Blocks.SCAFFOLDING)) {
                    for (int i = 1 + R.nextInt(4); i > 0; i--) {
                        w.set(x, y + i, z, s);
                    }
                }
            }
            case 11 -> { // ceiling to bonk heads on
                int ceiling = y + 2 + R.nextInt(2);
                BlockState s = pick(Blocks.STONE, Blocks.OAK_SLAB, Blocks.GLASS, Blocks.OAK_LEAVES);
                for (int i = -1; i <= 3; i++) {
                    w.set(x + dx * i, ceiling, z + dz * i, s);
                }
            }
            case 12 -> { // pool
                boolean lily = R.nextBoolean();
                for (int px = x; px <= x + 1; px++) {
                    for (int pz = z; pz <= z + 1; pz++) {
                        for (int k = 1; k <= 3; k++) {
                            w.set(px, y - k, pz, Blocks.WATER.defaultBlockState());
                        }
                        if (lily && R.nextBoolean()) {
                            w.set(px, y, pz, Blocks.LILY_PAD.defaultBlockState());
                        }
                    }
                }
            }
            case 13 -> { // shaft
                int depth = 3 + R.nextInt(10);
                BlockState bottom = pick(Blocks.WATER, Blocks.STONE, Blocks.HAY_BLOCK, Blocks.OAK_SLAB, Blocks.LAVA, Blocks.COBWEB);
                boolean ladder = R.nextInt(3) == 0;
                for (int k = 1; k <= depth; k++) {
                    w.set(x, y - k, z, AIR);
                    if (ladder) {
                        w.set(x, y - k, z, Blocks.LADDER.defaultBlockState().setValue(LadderBlock.FACING, dir));
                    }
                }
                w.set(x, y - depth - 1, z, bottom);
            }
            case 14 -> { // things not to break
                BlockState s = pick(Blocks.BEDROCK, Blocks.INFESTED_STONE, Blocks.ICE, Blocks.OBSIDIAN, Blocks.BARRIER, Blocks.GRAVEL);
                w.set(x, y - 1, z, s);
                w.set(x + dx, y, z + dz, s);
            }
            case 15 -> { // waterlogged and liquid-adjacent blocks
                BlockState s = BlockRefGen.anyStateOf(switch (R.nextInt(6)) {
                    case 0 -> Blocks.OAK_SLAB;
                    case 1 -> Blocks.OAK_STAIRS;
                    case 2 -> Blocks.OAK_TRAPDOOR;
                    case 3 -> Blocks.SEAGRASS;
                    case 4 -> Blocks.KELP;
                    default -> Blocks.OAK_LEAVES;
                });
                w.set(x, y - 1, z, s);
                if (R.nextBoolean()) {
                    w.set(x + dx, y - 1, z + dz, Blocks.WATER.defaultBlockState());
                }
            }
            default -> { // anything at all
                for (int i = R.nextInt(4); i >= 0; i--) {
                    w.set(x + R.nextInt(3) - 1, y + R.nextInt(3) - 1, z + R.nextInt(3) - 1, R.nextBoolean()
                            ? allStates.get(R.nextInt(allStates.size()))
                            : BlockRefGen.anyStateOf(BlockRefGen.INTERESTING.get(R.nextInt(BlockRefGen.INTERESTING.size()))));
                }
            }
        }
    }

    // endregion
}
