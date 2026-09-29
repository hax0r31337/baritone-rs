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
import baritone.pathing.calc.AbstractNodeCostSearch;
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
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.tags.TagKey;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.EnchantmentEffectComponents;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.item.enchantment.ItemEnchantments;
import net.minecraft.world.item.enchantment.effects.EnchantmentAttributeEffect;
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
import net.minecraft.world.level.block.LiquidBlock;
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

    /**
     * A world's dimension: its min Y and height, and whether it is the Nether (by dimension key,
     * which is what upstream checks).
     */
    record Shape(int minY, int height, boolean nether) {}

    static final Shape OVERWORLD = new Shape(-32, 96, false);
    /** Low and shallow, so the build limit is close to the surface. */
    static final Shape NETHER = new Shape(0, 64, true);

    /** Loaded chunks are CHUNK_MIN..CHUNK_MAX on both axes. */
    static final int CHUNK_MIN = -2;
    static final int CHUNK_MAX = 1;
    static final int X0 = CHUNK_MIN * 16;
    static final int Z0 = CHUNK_MIN * 16;
    static final int SX = (CHUNK_MAX - CHUNK_MIN + 1) * 16;
    static final int SZ = SX;

    /** Targeted queries per world after the random ones (see {@link #queries}). */
    static final int EDGE_QUERIES = 8;

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

        // ladders and vines are no longer walked through, so one above the player is a block to
        // break, which MovementPillar then skips because it climbs it
        JsonObject e = new JsonObject();
        e.add("blocksToAvoid", BlockRefGen.names(Blocks.LADDER, Blocks.VINE));
        configs.add("e", e);

        // slowPath, without the delay; the second also makes its timeouts fire at once
        JsonObject slow = new JsonObject();
        slow.addProperty("slowPath", true);
        slow.addProperty("slowPathTimeDelayMS", 0L);
        slow.addProperty("slowPathTimeoutMS", TIMEOUT);
        configs.add(SLOW_CONFIG, slow);
        JsonObject slowTimeout = slow.deepCopy();
        slowTimeout.addProperty("slowPathTimeoutMS", 0L);
        configs.add(SLOW_TIMEOUT_CONFIG, slowTimeout);

        // random mixes, so that settings also meet in combinations the bundles above never make
        for (int i = 0; i < RANDOM_CONFIGS; i++) {
            configs.add("r" + i, randomConfig());
        }
        return configs;
    }

    static final int RANDOM_CONFIGS = 8;

    /** A config with cutoffAtLoadBoundary on. */
    static final String LOAD_BOUNDARY_CONFIG = "d";
    /** A config with assumeWalkOnWater on. */
    static final String WALK_ON_WATER_CONFIG = "c";
    /** A config that avoids ladders and vines. */
    static final String AVOID_CLIMBABLE_CONFIG = "e";
    /** Only for the queries that use them, not picked at random. */
    static final String SLOW_CONFIG = "slow";
    static final String SLOW_TIMEOUT_CONFIG = "slow_timeout";

    private static final String[] RANDOM_BOOLEANS = {
            "allowBreak", "allowPlace", "allowSprint", "allowParkour", "allowParkourPlace", "allowParkourAscend",
            "allowDiagonalDescend", "allowDiagonalAscend", "allowDownward", "allowWalkOnBottomSlab",
            "allowWalkOnMagmaBlocks", "allowVines", "assumeWalkOnWater", "assumeWalkOnLava", "allowWaterBucketFall",
            "allowJumpAtBuildLimit", "allowPlaceInFluidsSource", "allowPlaceInFluidsFlow", "strictLiquidCheck",
            "avoidUpdatingFallingBlocks", "considerPotionEffects", "autoTool", "useSwordToMine", "itemSaver",
            "cutoffAtLoadBoundary", "minimumImprovementRepropagation",
    };

    /** Setting → values to pick from (exact in both Java's and Rust's number parsing). */
    private static final Map<String, Number[]> RANDOM_NUMBERS = new LinkedHashMap<>();

    static {
        RANDOM_NUMBERS.put("maxFallHeightNoWater", new Number[]{2, 3, 4, 6, 10});
        RANDOM_NUMBERS.put("maxFallHeightBucket", new Number[]{5, 12, 20, 40});
        RANDOM_NUMBERS.put("jumpPenalty", new Number[]{0.0, 1.25, 2.0, 5.0});
        RANDOM_NUMBERS.put("blockPlacementPenalty", new Number[]{0.0, 4.5, 20.0, 50.0});
        RANDOM_NUMBERS.put("walkOnWaterOnePenalty", new Number[]{0.0, 0.75, 3.0});
        RANDOM_NUMBERS.put("blockBreakAdditionalPenalty", new Number[]{0.0, 0.5, 2.0, 8.0});
        RANDOM_NUMBERS.put("backtrackCostFavoringCoefficient", new Number[]{0.25, 0.5, 1.0, 2.0});
        RANDOM_NUMBERS.put("pathCutoffMinimumLength", new Number[]{4, 8, 30});
        RANDOM_NUMBERS.put("pathCutoffFactor", new Number[]{0.5, 0.75, 0.9});
        RANDOM_NUMBERS.put("avoidBreakingMultiplier", new Number[]{0.5, 3.0, 10.0});
        RANDOM_NUMBERS.put("itemSaverThreshold", new Number[]{5, 20, 500});
        RANDOM_NUMBERS.put("costHeuristic", new Number[]{2.5, 3.563, 4.0, 5.0});
        RANDOM_NUMBERS.put("pathingMaxChunkBorderFetch", new Number[]{5, 50, 200});
    }

    private static final String[] RANDOM_LISTS = {"blocksToAvoid", "blocksToDisallowBreaking", "blocksToAvoidBreaking", "allowBreakAnyway"};

    private static final Block[] RANDOM_LIST_BLOCKS = {
            Blocks.STONE, Blocks.DIRT, Blocks.GRASS_BLOCK, Blocks.OAK_LOG, Blocks.OAK_LEAVES, Blocks.GLASS, Blocks.SAND,
            Blocks.GRAVEL, Blocks.COBBLESTONE, Blocks.NETHERRACK, Blocks.SOUL_SAND, Blocks.TORCH, Blocks.COBWEB,
            Blocks.OAK_DOOR, Blocks.WATER, Blocks.LADDER, Blocks.OAK_SLAB,
    };

    /**
     * Each setting is left at its default or set to a random value, independently.
     */
    private static JsonObject randomConfig() {
        JsonObject c = new JsonObject();
        for (String key : RANDOM_BOOLEANS) {
            if (R.nextBoolean()) {
                c.addProperty(key, R.nextBoolean());
            }
        }
        for (Map.Entry<String, Number[]> e : RANDOM_NUMBERS.entrySet()) {
            if (R.nextInt(3) == 0) {
                c.addProperty(e.getKey(), e.getValue()[R.nextInt(e.getValue().length)]);
            }
        }
        for (String key : RANDOM_LISTS) {
            if (R.nextInt(3) == 0) {
                List<Block> blocks = new ArrayList<>();
                for (int i = 1 + R.nextInt(4); i > 0; i--) {
                    Block b = RANDOM_LIST_BLOCKS[R.nextInt(RANDOM_LIST_BLOCKS.length)];
                    if (!blocks.contains(b)) {
                        blocks.add(b);
                    }
                }
                c.add(key, BlockRefGen.names(blocks.toArray(new Block[0])));
            }
        }
        return c;
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

    static Map<String, Inventory0> inventories() {
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

        // equal speeds with and without Silk Touch (the tie break), Efficiency on tools, shears
        // and a sword, and enchantments that do nothing for mining
        Inventory0 enchanted = inventory(0);
        enchanted.items[0] = stack(Items.IRON_PICKAXE, 1);
        enchanted.items[1] = enchant(stack(Items.IRON_PICKAXE, 1), Enchantments.SILK_TOUCH, 1);
        enchanted.items[2] = enchant(stack(Items.DIAMOND_SHOVEL, 1), Enchantments.EFFICIENCY, 3);
        enchanted.items[3] = enchant(enchant(stack(Items.STONE_AXE, 1), Enchantments.EFFICIENCY, 5), Enchantments.SILK_TOUCH, 1);
        enchanted.items[4] = enchant(stack(Items.SHEARS, 1), Enchantments.EFFICIENCY, 2);
        enchanted.items[5] = enchant(stack(Items.GOLDEN_PICKAXE, 1), Enchantments.UNBREAKING, 3);
        enchanted.items[6] = enchant(stack(Items.WOODEN_SWORD, 1), Enchantments.EFFICIENCY, 4);
        enchanted.items[7] = enchant(enchant(damaged(Items.DIAMOND_PICKAXE, 10), Enchantments.SILK_TOUCH, 1), Enchantments.EFFICIENCY, 1);
        enchanted.items[8] = enchant(stack(Items.STICK, 1), Enchantments.SILK_TOUCH, 1);
        m.put("enchanted", enchanted);
        return m;
    }

    private static ItemStack enchant(ItemStack s, ResourceKey<Enchantment> key, int level) {
        s.enchant(BlockRefGen.LOOKUP.lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(key), level);
        return s;
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
        // what the host evaluates from the enchantments, as ToolSet does
        ItemEnchantments enchantments = s.getEnchantments();
        OUTER:
        for (Holder<Enchantment> enchant : enchantments.keySet()) {
            for (EnchantmentAttributeEffect e : enchant.value().getEffects(EnchantmentEffectComponents.ATTRIBUTES)) {
                if (e.attribute().is(Attributes.MINING_EFFICIENCY.unwrapKey().get())) {
                    o.addProperty("mining_efficiency", e.amount().calculate(enchantments.getLevel(enchant)));
                    break OUTER;
                }
            }
        }
        for (Holder<Enchantment> enchant : enchantments.keySet()) {
            if (enchant.is(Enchantments.SILK_TOUCH) && enchantments.getLevel(enchant) > 0) {
                o.addProperty("silk_touch", true);
            }
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

    static ClientLevel fakeLevel(GenWorld world) throws Exception {
        ClientLevel level = BlockRefGen.allocate(ClientLevel.class);
        DimensionType type = new DimensionType(false, true, false, false, 1.0, world.minY, world.height, world.height,
                null, 0f, null, null, null, null, null, Optional.empty());
        BlockRefGen.setField(Level.class, level, "dimensionTypeRegistration", Holder.direct(type));
        BlockRefGen.setField(Level.class, level, "dimension", world.nether ? Level.NETHER : Level.OVERWORLD);
        BlockRefGen.setField(ClientLevel.class, level, "worldBorder", world.border);
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
            if (y < world.minY || y >= world.minY + world.height) {
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
    static CalculationContext context(IBaritone baritone, ClientLevel level, BlockStateInterface bsi, Inventory0 inv, PlayerSpec p)
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
        // c turns considerPotionEffects off, d changes the tool settings
        for (String config : new String[]{"a", "c", "d"}) {
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
        final int minY;
        final int height;
        final boolean nether;
        final BlockState[] blocks;
        final Set<Long> loaded = new HashSet<>();
        WorldBorder border = new WorldBorder();
        /**
         * Positions next to features that random samples rarely hit (ledges above pools, the
         * inside of ladder and vine columns), where the extra move samples go.
         */
        final List<BetterBlockPos> hotspots = new ArrayList<>();

        GenWorld(String kind, Shape shape) {
            this.kind = kind;
            this.minY = shape.minY;
            this.height = shape.height;
            this.nether = shape.nether;
            this.blocks = new BlockState[SX * SZ * height];
            java.util.Arrays.fill(blocks, AIR);
            for (int cx = CHUNK_MIN; cx <= CHUNK_MAX; cx++) {
                for (int cz = CHUNK_MIN; cz <= CHUNK_MAX; cz++) {
                    loaded.add(ChunkPos.pack(cx, cz));
                }
            }
        }

        int maxY() {
            return minY + height - 1;
        }

        boolean inside(int x, int y, int z) {
            return x >= X0 && x < X0 + SX && z >= Z0 && z < Z0 + SZ && y >= minY && y < minY + height;
        }

        BlockState get(int x, int y, int z) {
            if (!inside(x, y, z)) {
                return AIR;
            }
            return blocks[((x - X0) * SZ + (z - Z0)) * height + (y - minY)];
        }

        void set(int x, int y, int z, BlockState state) {
            if (inside(x, y, z)) {
                blocks[((x - X0) * SZ + (z - Z0)) * height + (y - minY)] = state;
            }
        }

        /**
         * First y from the top where the block below is not air-like (the surface to stand on).
         */
        int surface(int x, int z) {
            for (int y = maxY(); y > minY; y--) {
                if (!get(x, y - 1, z).isAir()) {
                    return y;
                }
            }
            return minY;
        }

        boolean hasHole() {
            return loaded.size() < (CHUNK_MAX - CHUNK_MIN + 1) * (CHUNK_MAX - CHUNK_MIN + 1);
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
        configNames.remove(SLOW_CONFIG);
        configNames.remove(SLOW_TIMEOUT_CONFIG);
        List<String> inventoryNames = new ArrayList<>(inventories.keySet());
        List<BlockState> allStates = new ArrayList<>();
        Block.BLOCK_STATE_REGISTRY.forEach(allStates::add);
        IBaritone baritone = fakeBaritone();

        List<GenWorld> worlds = new ArrayList<>();
        for (int i = 0; i < 14; i++) {
            worlds.add(terrain(allStates, OVERWORLD));
        }
        for (int i = 0; i < 4; i++) {
            worlds.add(noise(allStates));
        }
        worlds.add(flat());
        worlds.add(flat());
        for (int i = 0; i < 4; i++) {
            worlds.add(terrain(allStates, NETHER));
        }

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
            ClientLevel level = fakeLevel(world);
            ArrayBsi bsi = ArrayBsi.create(world);

            JsonObject w = new JsonObject();
            w.addProperty("kind", world.kind);
            w.addProperty("min_y", world.minY);
            w.addProperty("height", world.height);
            w.addProperty("nether", world.nether);
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
            for (int y = world.minY + 1; y < world.maxY(); y++) {
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
        return new BetterBlockPos(X0 + R.nextInt(SX), world.minY + 32 + R.nextInt(20), Z0 + R.nextInt(SZ));
    }

    private static BetterBlockPos anywhere(GenWorld world) {
        return new BetterBlockPos(X0 + R.nextInt(SX), world.minY + R.nextInt(world.height), Z0 + R.nextInt(SZ));
    }

    private static GoalCase randomGoal(GenWorld world, ArrayBsi bsi, BetterBlockPos start) {
        int r = R.nextInt(100);
        if (r < 30) {
            BetterBlockPos p = standable(world, bsi);
            return RefGen.block(p.x, p.y, p.z);
        } else if (r < 40) {
            return RefGen.xz(X0 - 8 + R.nextInt(SX + 16), Z0 - 8 + R.nextInt(SZ + 16));
        } else if (r < 47) {
            return RefGen.yLevel(world.minY + 4 + R.nextInt(world.height - 8));
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
        // 0, 1, 2: random positions. At the hotspots: 3: assumeWalkOnWater, 4:
        // WalkOffCalculationContext's fields, 5: ladders and vines avoided. The settings stay as
        // the context was built with: upstream reads some of them live, the port reads all of them
        // from the context.
        for (int k = 0; k < 6; k++) {
            boolean atHotspots = k >= 3;
            if (atHotspots && world.hotspots.isEmpty()) {
                break;
            }
            String config = switch (k) {
                case 3 -> WALK_ON_WATER_CONFIG;
                case 5 -> AVOID_CLIMBABLE_CONFIG;
                default -> configNames.get(R.nextInt(configNames.size()));
            };
            boolean walkOff = k == 4;
            BlockRefGen.apply(configs.getAsJsonObject(config));
            PlayerSpec player = randomPlayer(inventoryNames);
            CalculationContext context = context(baritone, level, bsi, inventories.get(player.inventory), player);
            if (walkOff) {
                // the fields ElytraProcess.WalkOffCalculationContext sets (not its overrides)
                context.allowFallIntoLava = true;
                context.minFallHeight = 8;
                context.maxFallHeightNoWater = 10000;
            }
            JsonObject o = new JsonObject();
            o.addProperty("config", config);
            o.addProperty("walk_off", walkOff);
            o.add("player", player.json());
            // [x, y, z, [move ordinal, x, y, z, cost bits]... for each finite cost]
            JsonArray positions = new JsonArray();
            MutableMoveResult res = new MutableMoveResult();
            int count = atHotspots ? world.hotspots.size() : 150;
            for (int i = 0; i < count; i++) {
                BetterBlockPos p = atHotspots ? world.hotspots.get(i) : i % 3 == 0 ? anywhere(world) : standable(world, bsi);
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
        for (int q = 0; q < count + EDGE_QUERIES; q++) {
            String config = configNames.get(R.nextInt(configNames.size()));
            BetterBlockPos start;
            BetterBlockPos realStart;
            GoalCase goal;
            long primaryTimeout = TIMEOUT;
            long failureTimeout = TIMEOUT;
            int cancelAfter = 0;
            if (q < count) {
                start = R.nextInt(8) == 0 ? anywhere(world) : standable(world, bsi);
                realStart = R.nextInt(8) == 0 ? start.relative(Direction.from3DDataValue(R.nextInt(6))) : start;
                goal = randomGoal(world, bsi, start);
            } else if (q - count >= 3) {
                // Ending the search early, in ways that do not depend on speed. A timeout of 0
                // expires at the first check (every 64 nodes); the primary one only once the
                // search is no longer failing.
                start = standable(world, bsi);
                realStart = start;
                goal = randomGoal(world, bsi, start);
                switch (q - count) {
                    // cancelled from inside the search, by the goal (see CancellingGoal)
                    case 3 -> cancelAfter = 1 + R.nextInt(500);
                    case 4 -> primaryTimeout = 0;
                    case 5 -> failureTimeout = 0;
                    // slowPath (without the delay): its timeouts replace the given ones
                    case 6 -> config = SLOW_CONFIG;
                    default -> config = SLOW_TIMEOUT_CONFIG;
                }
            } else {
                // Edge cases of Path and PathBase that random queries rarely reach. All start in
                // the goal, so the path is the start alone (plus the fake node for realStart).
                start = standable(world, bsi);
                switch (q - count) {
                    // the fake node: realStart next to the start, the movement from it is found
                    case 0 -> realStart = start.relative(Direction.from3DDataValue(R.nextInt(6)));
                    // the fake node too far for any movement: "Movement became impossible"
                    case 1 -> realStart = start.relative(horizontal(), 3 + R.nextInt(3));
                    // starting in an unloaded chunk: cutoffAtLoadedChunks cuts at the start
                    default -> {
                        if (world.hasHole()) {
                            config = LOAD_BOUNDARY_CONFIG;
                            do {
                                start = anywhere(world);
                            } while (bsi.worldContainsLoadedChunk(start.x, start.z));
                        }
                        realStart = R.nextBoolean() ? start : start.relative(Direction.from3DDataValue(R.nextInt(6)));
                    }
                }
                goal = R.nextBoolean() ? RefGen.block(start.x, start.y, start.z) : RefGen.yLevel(start.y);
            }
            BlockRefGen.apply(configs.getAsJsonObject(config));
            PlayerSpec player = randomPlayer(inventoryNames);

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
            o.add("timeouts", row(primaryTimeout, failureTimeout));
            if (cancelAfter != 0) {
                o.addProperty("cancel_after", cancelAfter);
            }
            if (favored != null) {
                o.add("favoring", flatPositions(favored));
            }
            o.add("avoidances", avoidances);
            JsonObject ctx = new JsonObject();
            ctx.addProperty("has_throwaway", context.hasThrowaway);
            ctx.addProperty("has_water_bucket", context.hasWaterBucket);
            ctx.addProperty("can_sprint", context.canSprint);
            ctx.add("water_walk_speed", d(context.waterWalkSpeed));
            o.add("context", ctx);

            CancellingGoal cancelling = cancelAfter != 0 ? new CancellingGoal(goal.goal(), cancelAfter) : null;
            AStarPathFinder finder = new AStarPathFinder(realStart, start.x, start.y, start.z,
                    cancelling != null ? cancelling : goal.goal(), favoring, context);
            if (cancelling != null) {
                cancelling.finder = finder;
            }
            PathCalculationResult result = finder.calculate(primaryTimeout, failureTimeout);
            JsonObject r = new JsonObject();
            r.addProperty("type", result.getType().name());
            if (result.getPath().isPresent()) {
                IPath path = result.getPath().get();
                r.add("positions", flatPositions(path.positions()));
                // [class, cost bits, calculatedWhileLoaded]
                JsonArray movements = new JsonArray();
                for (IMovement m : path.movements()) {
                    movements.add(row(m.getClass().getSimpleName(), d(m.getCost()), m.calculatedWhileLoaded()));
                }
                r.add("movements", movements);
                r.addProperty("num_nodes", path.getNumNodesConsidered());
                previous = path.positions();
            }
            // the search's state after it ended, for every result (failures included): how many
            // nodes it created, the chain to the node it considered last, and the best partial path
            r.addProperty("map_size", mapSize(finder));
            r.add("most_recent", finder.pathToMostRecentNodeConsidered()
                    .map(p -> (JsonElement) flatPositions(p.positions())).orElse(JsonNull.INSTANCE));
            r.add("best_so_far", finder.bestPathSoFar()
                    .map(p -> (JsonElement) flatPositions(p.positions())).orElse(JsonNull.INSTANCE));
            o.add("result", r);
            out.add(o);
        }
        return out;
    }

    /**
     * A goal that cancels its path finder on the {@code after}th {@code isInGoal} call (the
     * search makes one per node it considers), so cancellation happens at the same point
     * wherever it runs.
     */
    static final class CancellingGoal implements Goal {
        final Goal goal;
        final int after;
        int calls;
        AStarPathFinder finder;

        CancellingGoal(Goal goal, int after) {
            this.goal = goal;
            this.after = after;
        }

        @Override
        public boolean isInGoal(int x, int y, int z) {
            if (++calls == after) {
                finder.cancel();
            }
            return goal.isInGoal(x, y, z);
        }

        @Override
        public double heuristic(int x, int y, int z) {
            return goal.heuristic(x, y, z);
        }

        @Override
        public double heuristic() {
            return goal.heuristic();
        }
    }

    private static JsonArray flatPositions(List<BetterBlockPos> positions) {
        JsonArray a = new JsonArray();
        for (BetterBlockPos p : positions) {
            a.add(p.x);
            a.add(p.y);
            a.add(p.z);
        }
        return a;
    }

    /**
     * {@code AbstractNodeCostSearch.mapSize()}, which is protected.
     */
    static int mapSize(AStarPathFinder finder) throws ReflectiveOperationException {
        java.lang.reflect.Method method = AbstractNodeCostSearch.class.getDeclaredMethod("mapSize");
        method.setAccessible(true);
        return (int) method.invoke(finder);
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
        GenWorld w = new GenWorld("flat", OVERWORLD);
        for (int x = X0; x < X0 + SX; x++) {
            for (int z = Z0; z < Z0 + SZ; z++) {
                w.set(x, w.minY, z, Blocks.BEDROCK.defaultBlockState());
                for (int y = w.minY + 1; y < 0; y++) {
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
        GenWorld w = new GenWorld("noise", OVERWORLD);
        int wAir = 20 + R.nextInt(40);
        int wStone = 10 + R.nextInt(30);
        int wWater = R.nextInt(15);
        int wInteresting = 5 + R.nextInt(25);
        int wAny = 2 + R.nextInt(10);
        int total = wAir + wStone + wWater + wInteresting + wAny;
        for (int x = X0; x < X0 + SX; x++) {
            for (int z = Z0; z < Z0 + SZ; z++) {
                for (int y = w.minY; y < -4; y++) {
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

    /**
     * Rolling terrain with a sea, caves and features. In the Nether shape the sea is lava and the
     * ground is Nether blocks; its surface is close to the build limit.
     */
    private static GenWorld terrain(List<BlockState> allStates, Shape shape) {
        GenWorld w = new GenWorld("terrain", shape);
        boolean nether = shape.nether();
        Block fluid = nether ? Blocks.LAVA : Blocks.WATER;
        double a1 = R.nextDouble() * 6.28, a2 = R.nextDouble() * 6.28, a3 = R.nextDouble() * 6.28;
        double f1 = 4 + R.nextDouble() * 8, f2 = 4 + R.nextDouble() * 8, f3 = 6 + R.nextDouble() * 10;
        int amp1 = R.nextInt(5), amp2 = R.nextInt(4), amp3 = 1 + R.nextInt(4);
        int base = w.minY + 36 + R.nextInt(8);
        boolean bedrock = R.nextInt(3) != 0;
        int waterLevel = base + R.nextInt(3) - 1;
        int[][] h = new int[SX][SZ];
        for (int x = X0; x < X0 + SX; x++) {
            for (int z = Z0; z < Z0 + SZ; z++) {
                int top = base + (int) Math.round(amp1 * Math.sin(x / f1 + a1) + amp2 * Math.cos(z / f2 + a2) + amp3 * Math.sin((x + z) / f3 + a3));
                h[x - X0][z - Z0] = top;
                for (int y = w.minY; y < top; y++) {
                    BlockState s;
                    if (y == w.minY && bedrock) {
                        s = Blocks.BEDROCK.defaultBlockState();
                    } else if (y < top - 4) {
                        int r = R.nextInt(100);
                        if (nether) {
                            s = r < 5 ? pick(Blocks.NETHER_QUARTZ_ORE, Blocks.NETHER_GOLD_ORE, Blocks.GLOWSTONE, Blocks.MAGMA_BLOCK, Blocks.GRAVEL, Blocks.BLACKSTONE, Blocks.ANCIENT_DEBRIS)
                                    : Blocks.NETHERRACK.defaultBlockState();
                        } else {
                            s = r < 3 ? pick(Blocks.IRON_ORE, Blocks.COAL_ORE, Blocks.DIAMOND_ORE, Blocks.GRAVEL, Blocks.ANDESITE, Blocks.INFESTED_STONE)
                                    : Blocks.STONE.defaultBlockState();
                        }
                    } else if (y < top - 1) {
                        s = nether ? pick(Blocks.NETHERRACK, Blocks.SOUL_SOIL) : Blocks.DIRT.defaultBlockState();
                    } else if (nether) {
                        s = top - 1 < waterLevel ? pick(Blocks.SOUL_SAND, Blocks.GRAVEL, Blocks.MAGMA_BLOCK)
                                : pick(Blocks.NETHERRACK, Blocks.NETHERRACK, Blocks.SOUL_SAND, Blocks.SOUL_SOIL, Blocks.CRIMSON_NYLIUM, Blocks.BASALT);
                    } else {
                        s = top - 1 < waterLevel ? pick(Blocks.SAND, Blocks.GRAVEL, Blocks.DIRT, Blocks.CLAY) : Blocks.GRASS_BLOCK.defaultBlockState();
                    }
                    w.set(x, y, z, s);
                }
                for (int y = top; y <= waterLevel; y++) {
                    w.set(x, y, z, fluid.defaultBlockState());
                }
            }
        }
        // shores: some flowing fluid next to the edges
        for (int i = 0; i < 40; i++) {
            int x = X0 + R.nextInt(SX), z = Z0 + R.nextInt(SZ);
            int y = h[x - X0][z - Z0];
            if (y > waterLevel && y <= waterLevel + 2) {
                w.set(x, y, z, BlockRefGen.anyStateOf(fluid));
            }
        }
        // caves
        for (int i = R.nextInt(7); i > 0; i--) {
            int cx = X0 + R.nextInt(SX), cz = Z0 + R.nextInt(SZ);
            int top = h[cx - X0][cz - Z0];
            int cy = w.minY + 3 + R.nextInt(Math.max(1, top - w.minY - 6));
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
                        if (dx * dx + dy * dy + dz * dz <= r * r && y > w.minY && !w.get(x, y, z).is(Blocks.BEDROCK)) {
                            w.set(x, y, z, y == cy - r + 1 ? floor : AIR);
                        }
                    }
                }
            }
        }
        // a shaft into the void in worlds without bedrock
        if (!bedrock) {
            int x = X0 + R.nextInt(SX), z = Z0 + R.nextInt(SZ);
            for (int y = w.minY; y < h[x - X0][z - Z0]; y++) {
                w.set(x, y, z, AIR);
            }
        }
        for (int i = 40 + R.nextInt(60); i > 0; i--) {
            int x = X0 + R.nextInt(SX), z = Z0 + R.nextInt(SZ);
            feature(w, x, w.surface(x, z), z, allStates);
        }
        hotspotFeatures(w);
        return w;
    }

    /**
     * A ladder up a pillar. Its two lowest cells are hotspots: pillaring up inside the ladder.
     */
    private static void ladderColumn(GenWorld w, int x, int y, int z, Direction dir) {
        int dx = dir.getStepX(), dz = dir.getStepZ();
        int t = 3 + R.nextInt(5);
        for (int i = 0; i < t; i++) {
            w.set(x, y + i, z, Blocks.COBBLESTONE.defaultBlockState());
            w.set(x + dx, y + i, z + dz, Blocks.LADDER.defaultBlockState().setValue(LadderBlock.FACING, dir));
        }
        w.hotspots.add(new BetterBlockPos(x + dx, y, z + dz));
        w.hotspots.add(new BetterBlockPos(x + dx, y + 1, z + dz));
    }

    /**
     * Vines hanging off a pillar. The lowest vine and the cell below it are hotspots.
     */
    private static void vineColumn(GenWorld w, int x, int y, int z, Direction dir) {
        int dx = dir.getStepX(), dz = dir.getStepZ();
        int t = 3 + R.nextInt(5);
        for (int i = 0; i < t; i++) {
            w.set(x, y + i, z, Blocks.MOSSY_COBBLESTONE.defaultBlockState());
        }
        int vines = 1 + R.nextInt(t);
        for (int i = vines; i > 0; i--) {
            w.set(x + dx, y + t - i, z + dz, Blocks.VINE.defaultBlockState().setValue(VineBlock.getPropertyForFace(dir.getOpposite()), true));
        }
        w.hotspots.add(new BetterBlockPos(x + dx, y + t - vines, z + dz));
        w.hotspots.add(new BetterBlockPos(x + dx, y + t - vines - 1, z + dz));
    }

    /**
     * A ledge 3 to 12 blocks above a one block pool of {@code fluid} (with flowing water next to
     * it if {@code flowingNeighbor}), for descends that fall into it. The top of the ledge is a
     * hotspot.
     */
    private static void ledge(GenWorld w, int x, int z, BlockState fluid, boolean flowingNeighbor) {
        Direction dir = horizontal();
        int dx = dir.getStepX(), dz = dir.getStepZ();
        int y = w.surface(x, z);
        int h = Math.min(3 + R.nextInt(10), w.maxY() - 2 - y);
        if (h < 1) {
            return;
        }
        for (int i = 0; i < h; i++) {
            w.set(x, y + i, z, Blocks.COBBLESTONE.defaultBlockState());
        }
        w.set(x, y + h, z, AIR);
        w.set(x, y + h + 1, z, AIR);
        int px = x + dx, pz = z + dz;
        for (int yy = y; yy <= y + h + 1; yy++) {
            w.set(px, yy, pz, AIR);
        }
        w.set(px, y - 1, pz, fluid);
        w.set(px, y - 2, pz, Blocks.STONE.defaultBlockState());
        if (flowingNeighbor) {
            w.set(px + dx, y - 1, pz + dz, Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL, 1 + R.nextInt(7)));
        }
        w.hotspots.add(new BetterBlockPos(x, y + h, z));
    }

    /**
     * The features the extra move samples need, placed last so that nothing covers them.
     */
    private static void hotspotFeatures(GenWorld w) {
        BlockState water = Blocks.WATER.defaultBlockState();
        BlockState lava = Blocks.LAVA.defaultBlockState();
        for (int i = 0; i < 8; i++) {
            int x = X0 + 2 + R.nextInt(SX - 4), z = Z0 + 2 + R.nextInt(SZ - 4);
            switch (i % 4) {
                case 0 -> ledge(w, x, z, water, false);
                case 1 -> ledge(w, x, z, lava, false);
                case 2 -> ledge(w, x, z, water, true);
                default -> ledge(w, x, z, BlockRefGen.anyStateOf(R.nextBoolean() ? Blocks.WATER : Blocks.LAVA), R.nextBoolean());
            }
        }
        for (int i = 0; i < 2; i++) {
            int x = X0 + 2 + R.nextInt(SX - 4), z = Z0 + 2 + R.nextInt(SZ - 4);
            ladderColumn(w, x, w.surface(x, z), z, horizontal());
            x = X0 + 2 + R.nextInt(SX - 4);
            z = Z0 + 2 + R.nextInt(SZ - 4);
            vineColumn(w, x, w.surface(x, z), z, horizontal());
        }
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
            case 3 -> ladderColumn(w, x, y, z, dir);
            case 4 -> vineColumn(w, x, y, z, dir);
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
