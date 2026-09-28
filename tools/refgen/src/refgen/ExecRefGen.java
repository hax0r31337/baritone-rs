package refgen;

import baritone.Baritone;
import baritone.api.BaritoneAPI;
import baritone.api.IBaritone;
import baritone.api.IBaritoneProvider;
import baritone.api.event.events.PacketEvent;
import baritone.api.event.events.PathEvent;
import baritone.api.event.events.PlayerUpdateEvent;
import baritone.api.event.events.RotationMoveEvent;
import baritone.api.event.events.SprintStateEvent;
import baritone.api.event.events.TickEvent;
import baritone.api.event.events.type.EventState;
import baritone.api.cache.IWorldScanner;
import baritone.api.event.listener.AbstractGameEventListener;
import baritone.api.pathing.goals.GoalBlock;
import baritone.api.process.PathingCommand;
import baritone.api.utils.BetterBlockPos;
import baritone.api.utils.BlockOptionalMeta;
import baritone.api.utils.BlockOptionalMetaLookup;
import baritone.api.utils.IPlayerContext;
import baritone.api.utils.IPlayerController;
import baritone.api.utils.RayTraceUtils;
import baritone.api.utils.Rotation;
import baritone.api.utils.RotationUtils;
import baritone.api.utils.VecUtils;
import baritone.api.utils.input.Input;
import baritone.behavior.InventoryBehavior;
import baritone.behavior.LookBehavior;
import baritone.behavior.PathingBehavior;
import baritone.behavior.look.ForkableRandom;
import baritone.cache.CachedChunk;
import baritone.cache.CachedRegion;
import baritone.cache.CachedWorld;
import baritone.cache.FasterWorldScanner;
import baritone.cache.WorldData;
import baritone.cache.WorldProvider;
import baritone.event.GameEventHandler;
import baritone.pathing.path.PathExecutor;
import baritone.process.BackfillProcess;
import baritone.process.BuilderProcess;
import baritone.process.CustomGoalProcess;
import baritone.process.ExploreProcess;
import baritone.process.FarmProcess;
import baritone.process.FollowProcess;
import baritone.process.GetToBlockProcess;
import baritone.process.InventoryPauserProcess;
import baritone.process.MineProcess;
import baritone.process.elytra.NullElytraProcess;
import baritone.utils.BaritoneProcessHelper;
import baritone.utils.InputOverrideHandler;
import baritone.utils.PathingControlManager;
import baritone.utils.PlayerMovementInput;
import baritone.utils.ToolSet;
import baritone.utils.accessor.IChunkArray;
import baritone.utils.accessor.IClientChunkProvider;
import baritone.utils.accessor.IPlayerControllerMP;
import baritone.utils.player.BaritonePlayerContext;
import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.minecraft.client.Minecraft;
import net.minecraft.client.OptionInstance;
import net.minecraft.client.Options;
import net.minecraft.client.multiplayer.ClientChunkCache;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.client.player.ClientInput;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.ai.attributes.AttributeMap;
import net.minecraft.world.entity.player.Abilities;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.player.PlayerEquipment;
import net.minecraft.world.food.FoodData;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.CraftingMenu;
import net.minecraft.world.inventory.InventoryMenu;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.SwingAnimation;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.BambooStalkBlock;
import net.minecraft.world.level.block.BeetrootBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.CropBlock;
import net.minecraft.world.level.block.DoorBlock;
import net.minecraft.world.level.block.FenceGateBlock;
import net.minecraft.world.level.block.LadderBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.IntegerProperty;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.ChunkSource;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import net.minecraft.world.phys.Vec2;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.VoxelShape;
import net.minecraft.util.Mth;

import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;

import java.io.OutputStreamWriter;
import java.io.Writer;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Random;
import java.util.Set;
import java.util.concurrent.SynchronousQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.function.Predicate;
import java.util.stream.Collectors;
import java.util.stream.Stream;
import java.util.zip.GZIPOutputStream;

/**
 * Runs the real upstream execution code (PathingBehavior, PathExecutor, the movements'
 * updateState, LookBehavior, InventoryBehavior, InputOverrideHandler with its break and place
 * helpers, PathingControlManager, the processes) tick by tick around a simulated client, and
 * records what happens every tick. The Rust side (tests/common/sim.rs) runs the same
 * simulation around the port; keep the two in sync operation for operation.
 * <p>
 * The client objects are stand-ins allocated without a constructor (Minecraft, ClientLevel and
 * its chunk cache and entities, LocalPlayer, MultiPlayerGameMode, Options, the world provider's
 * cached world), over a world held here. The Baritone is allocated the same way and wired with
 * the real behaviors, the real control manager and the processes the port has.
 * <p>
 * A tick holds {@code pathPlanLock} while Baritone ticks, the background work its processes
 * started runs ({@link GatedExecutor}) and the player ticks, then waits for a running
 * calculation to finish, then applies the blocks broken and placed during the tick and loads
 * the chunks around the player (if the scenario loads chunks): a calculation reads the live
 * world, so the world may not change while it runs.
 * <p>
 * Also writes focused samples of the geometry execution relies on: raytraces
 * ({@code Level.clip}), block centers, {@code RotationUtils.reachable} and
 * {@code playerFeet} / {@code pathStart}.
 */
final class ExecRefGen {

    private static final Random R = new Random(0x9b05688cL);

    private static final BlockState AIR = Blocks.AIR.defaultBlockState();

    private ExecRefGen() {}

    static void write(String upstream, String minecraft, Path out) throws Exception {
        installGate();
        JsonObject root = new JsonObject();
        root.addProperty("upstream", upstream);
        root.addProperty("minecraft", minecraft);
        root.add("scenarios", scenarios());
        root.add("geometry", geometry());
        try (Writer w = new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(out)), StandardCharsets.UTF_8)) {
            w.write(new Gson().toJson(root));
            w.write('\n');
        }
        BlockRefGen.apply(new JsonObject());
    }

    // region world

    /**
     * A box of blocks; everything outside is air. Chunks are loaded in {@code loaded}.
     */
    static final class ExecWorld {
        final int x0, z0, sx, sz, minY, height;
        final BlockState[] blocks;
        final Set<Long> loaded = new HashSet<>();
        boolean nether;

        ExecWorld(int x0, int z0, int sx, int sz, int minY, int height) {
            this.x0 = x0;
            this.z0 = z0;
            this.sx = sx;
            this.sz = sz;
            this.minY = minY;
            this.height = height;
            this.blocks = new BlockState[sx * sz * height];
            java.util.Arrays.fill(blocks, AIR);
            for (int cx = x0 >> 4; cx <= (x0 + sx - 1) >> 4; cx++) {
                for (int cz = z0 >> 4; cz <= (z0 + sz - 1) >> 4; cz++) {
                    loaded.add(ChunkPos.pack(cx, cz));
                }
            }
        }

        ExecWorld copy() {
            ExecWorld w = new ExecWorld(x0, z0, sx, sz, minY, height);
            System.arraycopy(blocks, 0, w.blocks, 0, blocks.length);
            w.loaded.clear();
            w.loaded.addAll(loaded);
            w.nether = nether;
            return w;
        }

        boolean inside(int x, int y, int z) {
            return x >= x0 && x < x0 + sx && z >= z0 && z < z0 + sz && y >= minY && y < minY + height;
        }

        BlockState get(int x, int y, int z) {
            if (!inside(x, y, z)) {
                return AIR;
            }
            return blocks[((x - x0) * sz + (z - z0)) * height + (y - minY)];
        }

        void set(int x, int y, int z, BlockState state) {
            if (!inside(x, y, z)) {
                throw new IllegalArgumentException(x + " " + y + " " + z + " is outside");
            }
            blocks[((x - x0) * sz + (z - z0)) * height + (y - minY)] = state;
        }

        void fill(int xa, int ya, int za, int xb, int yb, int zb, BlockState state) {
            for (int x = Math.min(xa, xb); x <= Math.max(xa, xb); x++) {
                for (int y = Math.min(ya, yb); y <= Math.max(ya, yb); y++) {
                    for (int z = Math.min(za, zb); z <= Math.max(za, zb); z++) {
                        set(x, y, z, state);
                    }
                }
            }
        }

        boolean isLoaded(int cx, int cz) {
            return loaded.contains(ChunkPos.pack(cx, cz));
        }

        JsonObject json() {
            JsonObject o = new JsonObject();
            o.addProperty("x0", x0);
            o.addProperty("z0", z0);
            o.addProperty("sx", sx);
            o.addProperty("sz", sz);
            o.addProperty("min_y", minY);
            o.addProperty("height", height);
            o.addProperty("nether", nether);
            JsonArray rle = new JsonArray();
            int i = 0;
            while (i < blocks.length) {
                int j = i;
                while (j < blocks.length && blocks[j] == blocks[i]) {
                    j++;
                }
                rle.add(Block.BLOCK_STATE_REGISTRY.getId(blocks[i]));
                rle.add(j - i);
                i = j;
            }
            o.add("blocks", rle);
            JsonArray chunks = new JsonArray();
            loaded.stream().sorted().forEach(key -> {
                JsonArray c = new JsonArray();
                c.add(ChunkPos.getX(key));
                c.add(ChunkPos.getZ(key));
                chunks.add(c);
            });
            o.add("loaded_chunks", chunks);
            return o;
        }
    }

    // endregion

    // region fake client

    static final class FakeLevel extends ClientLevel {
        ExecWorld world;
        FakeChunkCache chunks;
        WorldBorder border;

        @SuppressWarnings("unused")
        private FakeLevel() {
            super(null, null, null, null, 0, 0, null, false, 0L, 0);
        }

        @Override
        public BlockState getBlockState(BlockPos pos) {
            if (pos.getY() < world.minY || pos.getY() >= world.minY + world.height) {
                return Blocks.VOID_AIR.defaultBlockState();
            }
            if (!world.isLoaded(pos.getX() >> 4, pos.getZ() >> 4)) {
                return Blocks.VOID_AIR.defaultBlockState();
            }
            return world.get(pos.getX(), pos.getY(), pos.getZ());
        }

        @Override
        public FluidState getFluidState(BlockPos pos) {
            return getBlockState(pos).getFluidState();
        }

        @Override
        public ClientChunkCache getChunkSource() {
            return chunks;
        }

        @Override
        public WorldBorder getWorldBorder() {
            return border;
        }

        @Override
        public <T extends Entity> List<T> getEntitiesOfClass(Class<T> baseClass, AABB bb, Predicate<? super T> selector) {
            return List.of();
        }

        /**
         * The local player and the other entities, like the client's entity storage.
         */
        FakePlayer player;
        List<Entity> entities = new ArrayList<>();

        List<Entity> all() {
            List<Entity> all = new ArrayList<>();
            if (player != null) {
                all.add(player);
            }
            all.addAll(entities);
            return all;
        }

        @Override
        public Iterable<Entity> entitiesForRendering() {
            return all();
        }

        @Override
        public List<Entity> getEntities(Entity except, AABB bb, Predicate<? super Entity> selector) {
            List<Entity> out = new ArrayList<>();
            for (Entity e : all()) {
                if (e != except && e.getBoundingBox().intersects(bb) && selector.test(e)) {
                    out.add(e);
                }
            }
            return out;
        }

        GameMode gameMode;

        @Override
        public void disconnect(Component message) {
            gameMode.actions.add("disconnect " + message.getString());
        }
    }

    static FakeLevel fakeLevel(ExecWorld world) throws Exception {
        FakeLevel level = BlockRefGen.allocate(FakeLevel.class);
        level.world = world;
        level.border = new WorldBorder();
        DimensionType type = new DimensionType(false, true, false, false, 1.0, world.minY, world.height, world.height,
                null, 0f, null, null, null, null, null, Optional.empty());
        BlockRefGen.setField(Level.class, level, "dimensionTypeRegistration", Holder.direct(type));
        BlockRefGen.setField(Level.class, level, "dimension", world.nether ? Level.NETHER : Level.OVERWORLD);
        FakeChunkCache chunks = BlockRefGen.allocate(FakeChunkCache.class);
        chunks.level = level;
        // the world scanner reads chunks from a parallel stream
        chunks.cache = new java.util.concurrent.ConcurrentHashMap<>();
        level.chunks = chunks;
        return level;
    }

    /**
     * The chunk cache as the port keeps it: every region within 4096 blocks of the origin is in
     * memory (so none waits for the disk), and a chunk is cached once it is marked by
     * {@link Sim#cacheLoadedChunks}. A cached chunk's contents are never read: chunks stay
     * loaded once they are.
     */
    static CachedWorld fakeCachedWorld(FakeLevel level) throws Exception {
        CachedWorld cache = BlockRefGen.allocate(CachedWorld.class);
        Long2ObjectOpenHashMap<CachedRegion> regions = new Long2ObjectOpenHashMap<>();
        Method regionId = CachedWorld.class.getDeclaredMethod("getRegionID", int.class, int.class);
        regionId.setAccessible(true);
        for (int x = -8; x <= 8; x++) {
            for (int z = -8; z <= 8; z++) {
                CachedRegion region = BlockRefGen.allocate(CachedRegion.class);
                BlockRefGen.setField(CachedRegion.class, region, "chunks", new CachedChunk[32][32]);
                BlockRefGen.setField(CachedRegion.class, region, "dimension", level.dimensionType());
                regions.put((long) regionId.invoke(cache, x, z), region);
            }
        }
        BlockRefGen.setField(CachedWorld.class, cache, "cachedRegions", regions);
        return cache;
    }

    /**
     * Stands in for {@code Baritone.getExecutor()}'s pool: background work that processes
     * start (rescans) waits in a queue until the tick that started it has run, then runs on
     * the tick's thread, so it sees the world and the player of that tick and its results land
     * before the next one. Path calculations run on the pool as usual.
     */
    static final class GatedExecutor extends ThreadPoolExecutor {
        private final List<Runnable> gated = new ArrayList<>();

        GatedExecutor() {
            super(4, Integer.MAX_VALUE, 60L, TimeUnit.SECONDS, new SynchronousQueue<>());
        }

        @Override
        public void execute(Runnable command) {
            boolean fromProcess = StackWalker.getInstance(StackWalker.Option.RETAIN_CLASS_REFERENCE)
                    .walk(frames -> frames.anyMatch(f -> f.getDeclaringClass().getPackageName().equals("baritone.process")));
            if (!fromProcess) {
                super.execute(command);
                return;
            }
            synchronized (gated) {
                gated.add(command);
            }
        }

        /**
         * Runs the queued work; a task that throws ends like it would on a pool thread.
         */
        void runGated() {
            List<Runnable> tasks;
            synchronized (gated) {
                tasks = new ArrayList<>(gated);
                gated.clear();
            }
            for (Runnable task : tasks) {
                try {
                    task.run();
                } catch (RuntimeException e) {
                    // an uncaught exception on a pool thread
                }
            }
        }
    }

    static final GatedExecutor GATE = new GatedExecutor();

    /**
     * Replaces the pool behind {@code Baritone.getExecutor()} (a static final field) with
     * {@link #GATE}.
     */
    @SuppressWarnings("removal")
    static void installGate() throws Exception {
        Baritone.getExecutor(); // initializes the class and its pool
        Field pool = Baritone.class.getDeclaredField("threadPool");
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) theUnsafe.get(null);
        unsafe.putObject(unsafe.staticFieldBase(pool), unsafe.staticFieldOffset(pool), GATE);
        if (Baritone.getExecutor() != GATE) {
            throw new IllegalStateException("the executor was not replaced");
        }
    }

    static final class FakeChunkCache extends ClientChunkCache implements IClientChunkProvider {
        FakeLevel level;
        Map<Long, FakeChunk> cache;

        @SuppressWarnings("unused")
        private FakeChunkCache() {
            super(null, 0);
        }

        @Override
        public LevelChunk getChunk(int x, int z, ChunkStatus targetStatus, boolean loadOrGenerate) {
            if (!level.world.isLoaded(x, z)) {
                return null;
            }
            return cache.computeIfAbsent(ChunkPos.pack(x, z), key -> {
                try {
                    return FakeChunk.create(level, x, z);
                } catch (Exception e) {
                    throw new RuntimeException(e);
                }
            });
        }

        @Override
        public boolean hasChunk(int x, int z) {
            return level.world.isLoaded(x, z);
        }

        @Override
        public ClientChunkCache createThreadSafeCopy() {
            // the world is only changed between calculations, see the class docs
            return this;
        }

        @Override
        public IChunkArray extractReferenceArray() {
            throw new UnsupportedOperationException();
        }
    }

    static final class FakeChunk extends LevelChunk {
        @SuppressWarnings("unused")
        private FakeChunk() {
            super(null, (ChunkPos) null);
        }

        static FakeChunk create(FakeLevel level, int cx, int cz) throws Exception {
            FakeChunk chunk = BlockRefGen.allocate(FakeChunk.class);
            BlockRefGen.setField(ChunkAccess.class, chunk, "chunkPos", new ChunkPos(cx, cz));
            LevelChunkSection[] sections = new LevelChunkSection[level.world.height >> 4];
            for (int i = 0; i < sections.length; i++) {
                FakeSection section = BlockRefGen.allocate(FakeSection.class);
                section.world = level.world;
                section.x = cx << 4;
                section.y = level.world.minY + (i << 4);
                section.z = cz << 4;
                sections[i] = section;
            }
            BlockRefGen.setField(ChunkAccess.class, chunk, "sections", sections);
            return chunk;
        }
    }

    static final class FakeSection extends LevelChunkSection {
        ExecWorld world;
        int x, y, z;

        @SuppressWarnings("unused")
        private FakeSection() {
            super((PalettedContainerFactory) null);
        }

        @Override
        public BlockState getBlockState(int sectionX, int sectionY, int sectionZ) {
            return world.get(x + sectionX, y + sectionY, z + sectionZ);
        }

        @Override
        public boolean hasOnlyAir() {
            return false;
        }
    }

    /**
     * The local player: the fields upstream reads are set directly, and what would reach the
     * entity data or the network is overridden.
     */
    static final class FakePlayer extends LocalPlayer {
        boolean sprinting;
        boolean inWall;
        GameMode gameMode;

        @SuppressWarnings("unused")
        private FakePlayer() {
            super(null, null, null, null, null, null, false, null, null);
        }

        @Override
        public boolean isSprinting() {
            return sprinting;
        }

        @Override
        public void setSprinting(boolean sprinting) {
            this.sprinting = sprinting;
        }

        @Override
        public boolean isInWall() {
            return inWall;
        }

        @Override
        public boolean isFallFlying() {
            return false;
        }

        @Override
        public boolean isSpectator() {
            return false;
        }

        @Override
        public boolean isAlive() {
            return true;
        }

        @Override
        public boolean swing(InteractionHand hand, SwingAnimation animation, boolean sendToSwingingEntity) {
            gameMode.actions.add("swing " + hand(hand));
            return true;
        }
    }

    static String hand(InteractionHand hand) {
        return hand == InteractionHand.MAIN_HAND ? "MainHand" : "OffHand";
    }

    static void setPosition(FakePlayer player, Vec3 position) throws ReflectiveOperationException {
        BlockRefGen.setField(Entity.class, player, "position", position);
        BlockPos block = BlockPos.containing(position);
        BlockRefGen.setField(Entity.class, player, "blockPosition", block);
        BlockRefGen.setField(Entity.class, player, "chunkPosition", ChunkPos.containing(block));
    }

    static FakePlayer fakePlayer(FakeLevel level, Vec3 position, ItemStack[] items, int selected, GameMode gameMode) throws Exception {
        FakePlayer player = BlockRefGen.allocate(FakePlayer.class);
        player.gameMode = gameMode;
        BlockRefGen.setField(Entity.class, player, "level", level);
        setPosition(player, position);
        player.xo = position.x;
        player.yo = position.y;
        player.zo = position.z;
        BlockRefGen.setField(Entity.class, player, "deltaMovement", Vec3.ZERO);
        BlockRefGen.setField(Entity.class, player, "onGround", true);
        BlockRefGen.setField(Entity.class, player, "eyeHeight", 1.62F);
        BlockRefGen.setField(Entity.class, player, "bb", Sim.playerBox(position, false));
        BlockRefGen.setField(LivingEntity.class, player, "attributes", new AttributeMap(Player.createAttributes().build()));
        BlockRefGen.setField(LivingEntity.class, player, "activeEffects", new HashMap<>());
        PlayerEquipment equipment = new PlayerEquipment(player);
        BlockRefGen.setField(LivingEntity.class, player, "equipment", equipment);
        Inventory inventory = new Inventory(player, equipment);
        for (int i = 0; i < items.length; i++) {
            inventory.setItem(i, items[i].copy());
        }
        inventory.setSelectedSlot(selected);
        BlockRefGen.setField(Player.class, player, "inventory", inventory);
        BlockRefGen.setField(Player.class, player, "abilities", new Abilities());
        BlockRefGen.setField(Player.class, player, "foodData", new FoodData());
        // no container open: the open menu is the inventory's (container id 0)
        InventoryMenu menu = BlockRefGen.allocate(InventoryMenu.class);
        BlockRefGen.setField(Player.class, player, "inventoryMenu", menu);
        player.containerMenu = menu;
        player.input = new ClientInput();
        // entities are equal when their ids are; the scenarios' entities count from 1
        player.setId(1000);
        // as LivingEntity's constructor sets it
        BlockRefGen.setField(Entity.class, player, "blocksBuilding", true);
        return player;
    }

    /**
     * The simplified game mode, the same as {@code GameMode} in tests/common/sim.rs.
     */
    static final class GameMode {
        boolean destroying;
        BlockPos destroyPos = BlockPos.ZERO;
        double destroyProgress;
        int destroyDelay;
        final List<Object[]> pending = new ArrayList<>();
        final List<String> actions = new ArrayList<>();

        static double speed(Player player, BlockState state) {
            return ToolSet.calculateSpeedVsBlock(player.getItemInHand(InteractionHand.MAIN_HAND), state);
        }

        void destroy(BlockPos pos) {
            pending.add(new Object[]{pos, AIR});
        }

        boolean startDestroyBlock(Player player, Level level, BlockPos pos) {
            if (!destroying || !pos.equals(destroyPos)) {
                BlockState state = level.getBlockState(pos);
                boolean notAir = !state.isAir();
                if (notAir && speed(player, state) >= 1.0) {
                    destroy(pos);
                } else {
                    destroying = true;
                    destroyPos = pos;
                    destroyProgress = 0.0;
                }
            }
            return true;
        }

        boolean continueDestroyBlock(Player player, Level level, BlockPos pos) {
            if (destroyDelay > 0) {
                destroyDelay--;
                return true;
            }
            if (destroying && pos.equals(destroyPos)) {
                BlockState state = level.getBlockState(pos);
                if (state.isAir()) {
                    destroying = false;
                    return false;
                }
                destroyProgress += speed(player, state);
                if (destroyProgress >= 1.0) {
                    destroying = false;
                    destroy(pos);
                    destroyProgress = 0.0;
                    destroyDelay = 5;
                }
                return true;
            }
            return startDestroyBlock(player, level, pos);
        }

        InteractionResult place(Player player, Level level, BlockHitResult hit) {
            Inventory inventory = player.getInventory();
            int selected = inventory.getSelectedSlot();
            ItemStack item = inventory.getItem(selected);
            // doors, gates and trapdoors open and close, unless sneaking with something in hand
            BlockState clicked = level.getBlockState(hit.getBlockPos());
            if (clicked.hasProperty(BlockStateProperties.OPEN) && !(player.isCrouching() && !item.isEmpty())) {
                BlockPos pos = hit.getBlockPos();
                pending.add(new Object[]{pos, clicked.cycle(BlockStateProperties.OPEN)});
                // a door's other half opens with it
                if (clicked.getBlock() instanceof DoorBlock) {
                    BlockPos other = clicked.getValue(DoorBlock.HALF) == DoubleBlockHalf.UPPER ? pos.below() : pos.above();
                    BlockState otherState = level.getBlockState(other);
                    if (otherState.getBlock() == clicked.getBlock()) {
                        pending.add(new Object[]{other, otherState.cycle(BlockStateProperties.OPEN)});
                    }
                }
                return InteractionResult.SUCCESS;
            }
            // containers open, unless sneaking with something in hand
            if (CONTAINERS.contains(clicked.getBlock()) && !(player.isCrouching() && !item.isEmpty())) {
                try {
                    // any menu but the inventory's
                    player.containerMenu = BlockRefGen.allocate(CraftingMenu.class);
                } catch (Exception e) {
                    throw new IllegalStateException(e);
                }
                return InteractionResult.SUCCESS;
            }
            // bone meal ages what has an age by 3, up to its maximum
            if (item.is(Items.BONE_MEAL)) {
                IntegerProperty age = age(clicked);
                if (age == null) {
                    return InteractionResult.PASS;
                }
                int max = Collections.max(age.getPossibleValues());
                int now = clicked.getValue(age);
                if (now >= max) {
                    return InteractionResult.PASS;
                }
                pending.add(new Object[]{hit.getBlockPos(), clicked.setValue(age, Math.min(now + 3, max))});
                shrink(inventory, selected, item);
                return InteractionResult.SUCCESS;
            }
            // seeds and the like plant their crop against the clicked face, even inside the player
            Block plant = PLANTS.get(item.getItem());
            if (plant != null) {
                BlockPos target = clicked.canBeReplaced() ? hit.getBlockPos() : hit.getBlockPos().relative(hit.getDirection());
                if (!level.getBlockState(target).canBeReplaced()) {
                    return InteractionResult.FAIL;
                }
                pending.add(new Object[]{target, plant.defaultBlockState()});
                shrink(inventory, selected, item);
                return InteractionResult.SUCCESS;
            }
            Identifier id = BuiltInRegistries.ITEM.getKey(item.getItem());
            Optional<Block> block = BuiltInRegistries.BLOCK.getOptional(id);
            if (item.isEmpty() || block.isEmpty()) {
                return InteractionResult.PASS;
            }
            BlockPos target = clicked.canBeReplaced() ? hit.getBlockPos() : hit.getBlockPos().relative(hit.getDirection());
            if (!level.getBlockState(target).canBeReplaced()) {
                return InteractionResult.FAIL;
            }
            if (player.getBoundingBox().intersects(new AABB(target))) {
                return InteractionResult.FAIL;
            }
            pending.add(new Object[]{target, block.get().defaultBlockState()});
            shrink(inventory, selected, item);
            return InteractionResult.SUCCESS;
        }

        static void shrink(Inventory inventory, int selected, ItemStack item) {
            item.shrink(1);
            if (item.isEmpty()) {
                inventory.setItem(selected, ItemStack.EMPTY);
            }
        }

        /**
         * The state's {@code age} property, if it has one.
         */
        static IntegerProperty age(BlockState state) {
            for (Property<?> p : state.getProperties()) {
                if (p.getName().equals("age") && p instanceof IntegerProperty age) {
                    return age;
                }
            }
            return null;
        }

        static final Set<Block> CONTAINERS = Set.of(Blocks.CRAFTING_TABLE, Blocks.FURNACE, Blocks.BLAST_FURNACE,
                Blocks.CHEST, Blocks.TRAPPED_CHEST, Blocks.ENDER_CHEST);

        static final Map<net.minecraft.world.item.Item, Block> PLANTS = Map.of(
                Items.WHEAT_SEEDS, Blocks.WHEAT,
                Items.CARROT, Blocks.CARROTS,
                Items.POTATO, Blocks.POTATOES,
                Items.BEETROOT_SEEDS, Blocks.BEETROOTS,
                Items.NETHER_WART, Blocks.NETHER_WART,
                Items.COCOA_BEANS, Blocks.COCOA,
                Items.MELON_SEEDS, Blocks.MELON_STEM,
                Items.PUMPKIN_SEEDS, Blocks.PUMPKIN_STEM);
    }

    static String result(InteractionResult r) {
        if (r == InteractionResult.SUCCESS) {
            return "Success";
        }
        if (r == InteractionResult.PASS) {
            return "Pass";
        }
        if (r == InteractionResult.FAIL) {
            return "Fail";
        }
        throw new IllegalStateException(String.valueOf(r));
    }

    static String pos(BlockPos p) {
        return p.getX() + "," + p.getY() + "," + p.getZ();
    }

    /**
     * {@code mc.gameMode}: only the accessor BlockBreakHelper casts it to is used.
     */
    static final class FakeGameMode extends MultiPlayerGameMode implements IPlayerControllerMP {
        GameMode state;

        @SuppressWarnings("unused")
        private FakeGameMode() {
            super(null, null);
        }

        @Override
        public void setIsHittingBlock(boolean isHittingBlock) {
            state.destroying = isHittingBlock;
        }

        @Override
        public boolean isHittingBlock() {
            return state.destroying;
        }

        @Override
        public BlockPos getCurrentBlock() {
            return state.destroyPos;
        }

        @Override
        public void callSyncCurrentPlayItem() {
            throw new UnsupportedOperationException();
        }

        @Override
        public void setDestroyDelay(int destroyDelay) {
            state.destroyDelay = destroyDelay;
        }
    }

    /**
     * The player controller, the same as {@code Controller} in tests/common/sim.rs.
     */
    static final class FakeController implements IPlayerController {
        final GameMode gm;
        final FakePlayer player;
        final FakeLevel level;

        FakeController(GameMode gm, FakePlayer player, FakeLevel level) {
            this.gm = gm;
            this.player = player;
            this.level = level;
        }

        @Override
        public void syncHeldItem() {
            gm.actions.add("sync " + player.getInventory().getSelectedSlot());
        }

        @Override
        public boolean hasBrokenBlock() {
            return !gm.destroying;
        }

        @Override
        public boolean onPlayerDamageBlock(BlockPos pos, Direction side) {
            boolean result = gm.continueDestroyBlock(player, level, pos);
            gm.actions.add("damage " + pos(pos) + " " + side.getName() + " " + result);
            return result;
        }

        @Override
        public void resetBlockRemoving() {
            gm.destroying = false;
            gm.destroyProgress = 0.0;
            gm.actions.add("reset");
        }

        @Override
        public void windowClick(int windowId, int slotId, int mouseButton, ContainerInput type, Player player) {
            if (windowId != 0 || type != ContainerInput.SWAP) {
                throw new IllegalStateException(windowId + " " + type);
            }
            int slot = slotId >= 36 ? slotId - 36 : slotId;
            Inventory inventory = player.getInventory();
            ItemStack a = inventory.getItem(slot);
            ItemStack b = inventory.getItem(mouseButton);
            inventory.setItem(slot, b);
            inventory.setItem(mouseButton, a);
            gm.actions.add("swap " + slotId + " " + mouseButton);
        }

        @Override
        public GameType getGameType() {
            return GameType.SURVIVAL;
        }

        @Override
        public InteractionResult processRightClickBlock(LocalPlayer player, Level world, InteractionHand hand, BlockHitResult result) {
            InteractionResult r = hand == InteractionHand.MAIN_HAND ? gm.place(player, world, result) : InteractionResult.PASS;
            gm.actions.add("use " + hand(hand) + " " + pos(result.getBlockPos()) + " " + result.getDirection().getName() + " " + result(r));
            return r;
        }

        @Override
        public InteractionResult processRightClick(LocalPlayer player, Level world, InteractionHand hand) {
            gm.actions.add("use item " + hand(hand));
            return InteractionResult.PASS;
        }

        @Override
        public boolean clickBlock(BlockPos loc, Direction face) {
            boolean result = gm.startDestroyBlock(player, level, loc);
            gm.actions.add("click " + pos(loc) + " " + face.getName());
            return result;
        }

        @Override
        public void setHittingBlock(boolean hittingBlock) {
            gm.destroying = hittingBlock;
        }
    }

    // endregion

    // region client

    /**
     * A Baritone and its simulated client.
     */
    static final class Sim {
        final ExecWorld world;
        final FakeLevel level;
        final FakePlayer player;
        final GameMode gm;
        final Baritone baritone;
        final GameEventHandler geh;
        final PathingBehavior pathing;
        final InputOverrideHandler input;
        final List<String> events = new ArrayList<>();
        final Object planLock;
        final PathingControlManager pcm;
        final CachedWorld cache;
        /**
         * Chunks of the world's box within this many chunks of the player load after every
         * tick; -1 for none.
         */
        int loadRadius = -1;
        boolean prevShift;
        int jumpDelay;
        float[] lastSent;
        int tick;

        Sim(ExecWorld world, Vec3 position, ItemStack[] items, int selected, long seed) throws Exception {
            this.world = world;
            this.level = fakeLevel(world);
            this.gm = new GameMode();
            level.gameMode = gm;
            this.player = fakePlayer(level, position, items, selected, gm);

            Minecraft mc = BlockRefGen.allocate(Minecraft.class);
            BlockRefGen.setField(Minecraft.class, mc, "player", player);
            BlockRefGen.setField(Minecraft.class, mc, "level", level);
            BlockRefGen.setField(Minecraft.class, mc, "gameThread", Thread.currentThread());
            // OptionInstance.set asks Minecraft.getInstance().isRunning() (false: just store it)
            BlockRefGen.setField(Minecraft.class, null, "instance", mc);
            Options options = BlockRefGen.allocate(Options.class);
            BlockRefGen.setField(Options.class, options, "sensitivity", option(0.5));
            // PathingBehavior sets it, which validates the value
            BlockRefGen.setField(Options.class, options, "autoJump", OptionInstance.createBoolean("options.autoJump", false));
            BlockRefGen.setField(Minecraft.class, mc, "options", options);
            FakeGameMode gameMode = BlockRefGen.allocate(FakeGameMode.class);
            gameMode.state = gm;
            BlockRefGen.setField(Minecraft.class, mc, "gameMode", gameMode);

            baritone = BlockRefGen.allocate(Baritone.class);
            BaritonePlayerContext ctx = BlockRefGen.allocate(BaritonePlayerContext.class);
            BlockRefGen.setField(BaritonePlayerContext.class, ctx, "baritone", baritone);
            BlockRefGen.setField(BaritonePlayerContext.class, ctx, "mc", mc);
            BlockRefGen.setField(BaritonePlayerContext.class, ctx, "playerController", new FakeController(gm, player, level));
            BlockRefGen.setField(Baritone.class, baritone, "mc", mc);
            BlockRefGen.setField(Baritone.class, baritone, "playerContext", ctx);
            // no world data (the chunk cache is not ported); mcWorld keeps it from loading one
            WorldProvider worldProvider = BlockRefGen.allocate(WorldProvider.class);
            BlockRefGen.setField(WorldProvider.class, worldProvider, "baritone", baritone);
            BlockRefGen.setField(WorldProvider.class, worldProvider, "ctx", ctx);
            BlockRefGen.setField(WorldProvider.class, worldProvider, "mcWorld", level);
            // the port's cache: which chunks were loaded, every region in memory
            cache = fakeCachedWorld(level);
            WorldData worldData = BlockRefGen.allocate(WorldData.class);
            BlockRefGen.setField(WorldData.class, worldData, "cache", cache);
            BlockRefGen.setField(WorldProvider.class, worldProvider, "currentWorld", worldData);
            BlockRefGen.setField(Baritone.class, baritone, "worldProvider", worldProvider);
            level.player = player;
            geh = new GameEventHandler(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "gameEventHandler", geh);
            // registration order: look, pathing, inventory, input override (, waypoints)
            LookBehavior look = new LookBehavior(baritone);
            Object processor = BlockRefGen.getField(LookBehavior.class, look, "processor");
            BlockRefGen.setField(processor.getClass().getSuperclass(), processor, "rand", new ForkableRandom(seed));
            BlockRefGen.setField(Baritone.class, baritone, "lookBehavior", look);
            geh.registerEventListener(look);
            pathing = new PathingBehavior(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "pathingBehavior", pathing);
            geh.registerEventListener(pathing);
            InventoryBehavior inventory = new InventoryBehavior(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "inventoryBehavior", inventory);
            geh.registerEventListener(inventory);
            input = new InputOverrideHandler(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "inputOverrideHandler", input);
            geh.registerEventListener(input);
            PathingControlManager pcm = new PathingControlManager(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "pathingControlManager", pcm);
            // the processes the port has, in upstream's registration order; the builder and
            // elytra processes are never active, and upstream asks the builder for
            // placementPlausible
            FollowProcess follow = new FollowProcess(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "followProcess", follow);
            pcm.registerProcess(follow);
            MineProcess mine = new MineProcess(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "mineProcess", mine);
            pcm.registerProcess(mine);
            CustomGoalProcess customGoal = new CustomGoalProcess(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "customGoalProcess", customGoal);
            pcm.registerProcess(customGoal);
            GetToBlockProcess getToBlock = new GetToBlockProcess(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "getToBlockProcess", getToBlock);
            pcm.registerProcess(getToBlock);
            BuilderProcess builder = BlockRefGen.allocate(BuilderProcess.class);
            BlockRefGen.setField(BaritoneProcessHelper.class, builder, "baritone", baritone);
            BlockRefGen.setField(BaritoneProcessHelper.class, builder, "ctx", ctx);
            BlockRefGen.setField(Baritone.class, baritone, "builderProcess", builder);
            ExploreProcess explore = new ExploreProcess(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "exploreProcess", explore);
            pcm.registerProcess(explore);
            FarmProcess farm = new FarmProcess(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "farmProcess", farm);
            pcm.registerProcess(farm);
            InventoryPauserProcess pauser = new InventoryPauserProcess(baritone);
            BlockRefGen.setField(Baritone.class, baritone, "inventoryPauserProcess", pauser);
            pcm.registerProcess(pauser);
            BlockRefGen.setField(Baritone.class, baritone, "elytraProcess", new NullElytraProcess(baritone));
            pcm.registerProcess(new BackfillProcess(baritone));
            this.pcm = pcm;
            geh.registerEventListener(new AbstractGameEventListener() {
                @Override
                public void onPathEvent(PathEvent event) {
                    events.add(event.name());
                }
            });
            planLock = BlockRefGen.getField(PathingBehavior.class, pathing, "pathPlanLock");

            BaritoneAPI.provider = (IBaritoneProvider) Proxy.newProxyInstance(ExecRefGen.class.getClassLoader(),
                    new Class<?>[]{IBaritoneProvider.class}, (proxy, method, args) -> switch (method.getName()) {
                        case "getPrimaryBaritone", "getBaritoneForPlayer" -> baritone;
                        case "getAllBaritones" -> List.<IBaritone>of(baritone);
                        case "getWorldScanner" -> RefWorldScanner.INSTANCE;
                        default -> throw new UnsupportedOperationException("IBaritoneProvider." + method.getName());
                    });
        }

        @SuppressWarnings("unchecked")
        static <T> OptionInstance<T> option(T value) throws Exception {
            OptionInstance<T> option = BlockRefGen.allocate(OptionInstance.class);
            BlockRefGen.setField(OptionInstance.class, option, "value", value);
            return option;
        }

        JsonObject tick() throws Exception {
            synchronized (planLock) {
                // ChunkEvent: every chunk loaded so far is cached
                cacheLoadedChunks();
                geh.onTick(new TickEvent(EventState.PRE, TickEvent.Type.IN, tick));
                // the background work processes started during the tick
                GATE.runGated();
                playerTick();
            }
            // let a calculation started this tick finish and take effect
            long started = System.nanoTime();
            while (pathing.getInProgress().isPresent()) {
                if (System.nanoTime() - started > 60_000_000_000L) {
                    throw new IllegalStateException("calculation did not finish");
                }
                Thread.sleep(0, 200_000);
            }
            for (Object[] change : gm.pending) {
                BlockPos pos = (BlockPos) change[0];
                world.set(pos.getX(), pos.getY(), pos.getZ(), (BlockState) change[1]);
            }
            gm.pending.clear();
            loadChunks();
            tick++;
            return record();
        }

        /**
         * Loads the chunks of the world's box within {@link #loadRadius} of the player's chunk.
         */
        void loadChunks() {
            if (loadRadius < 0) {
                return;
            }
            BlockPos feet = player.blockPosition();
            int cx = feet.getX() >> 4;
            int cz = feet.getZ() >> 4;
            for (int x = cx - loadRadius; x <= cx + loadRadius; x++) {
                for (int z = cz - loadRadius; z <= cz + loadRadius; z++) {
                    if (x >= world.x0 >> 4 && x <= (world.x0 + world.sx - 1) >> 4
                            && z >= world.z0 >> 4 && z <= (world.z0 + world.sz - 1) >> 4) {
                        world.loaded.add(ChunkPos.pack(x, z));
                    }
                }
            }
        }

        void cacheLoadedChunks() throws Exception {
            for (long key : world.loaded) {
                int x = ChunkPos.getX(key);
                int z = ChunkPos.getZ(key);
                CachedRegion region = cache.getRegion(x >> 5, z >> 5);
                CachedChunk[][] chunks = (CachedChunk[][]) BlockRefGen.getField(CachedRegion.class, region, "chunks");
                if (chunks[x & 31][z & 31] == null) {
                    chunks[x & 31][z & 31] = BlockRefGen.allocate(CachedChunk.class);
                }
            }
        }

        static final float WIDTH = 0.6F;
        static final float STANDING_HEIGHT = 1.8F;
        static final float CROUCHING_HEIGHT = 1.5F;

        static AABB playerBox(Vec3 pos, boolean crouching) {
            double w = (double) (WIDTH / 2.0F);
            double h = (double) (crouching ? CROUCHING_HEIGHT : STANDING_HEIGHT);
            return new AABB(pos.x - w, pos.y, pos.z - w, pos.x + w, pos.y + h, pos.z + w);
        }

        List<AABB> colliders(AABB area) {
            List<AABB> boxes = new ArrayList<>();
            for (int x = Mth.floor(area.minX) - 1; x <= Mth.floor(area.maxX) + 1; x++) {
                for (int y = Mth.floor(area.minY) - 1; y <= Mth.floor(area.maxY) + 1; y++) {
                    for (int z = Mth.floor(area.minZ) - 1; z <= Mth.floor(area.maxZ) + 1; z++) {
                        BlockPos pos = new BlockPos(x, y, z);
                        for (AABB b : level.getBlockState(pos).getCollisionShape(level, pos).toAabbs()) {
                            boxes.add(b.move(pos));
                        }
                    }
                }
            }
            return boxes;
        }

        static double min(AABB b, int axis) {
            return axis == 0 ? b.minX : axis == 1 ? b.minY : b.minZ;
        }

        static double max(AABB b, int axis) {
            return axis == 0 ? b.maxX : axis == 1 ? b.maxY : b.maxZ;
        }

        static boolean overlaps(AABB b, AABB bb, int axis) {
            return min(b, axis) < max(bb, axis) && max(b, axis) > min(bb, axis);
        }

        static double collideAxis(int axis, AABB bb, List<AABB> boxes, double d) {
            if (Math.abs(d) < 1.0E-7) {
                return 0.0;
            }
            for (AABB b : boxes) {
                boolean others = switch (axis) {
                    case 0 -> overlaps(b, bb, 1) && overlaps(b, bb, 2);
                    case 1 -> overlaps(b, bb, 0) && overlaps(b, bb, 2);
                    default -> overlaps(b, bb, 0) && overlaps(b, bb, 1);
                };
                if (!others) {
                    continue;
                }
                if (d > 0.0) {
                    if (min(b, axis) >= max(bb, axis) - 1.0E-7) {
                        d = Math.min(d, min(b, axis) - max(bb, axis));
                    }
                } else if (max(b, axis) <= min(bb, axis) + 1.0E-7) {
                    d = Math.max(d, max(b, axis) - min(bb, axis));
                }
            }
            return d;
        }

        static AABB moveBox(AABB bb, int axis, double d) {
            return switch (axis) {
                case 0 -> bb.move(d, 0.0, 0.0);
                case 1 -> bb.move(0.0, d, 0.0);
                default -> bb.move(0.0, 0.0, d);
            };
        }

        Vec3 collide(AABB bb, Vec3 delta) {
            AABB area = new AABB(
                    Math.min(bb.minX, bb.minX + delta.x),
                    Math.min(bb.minY, bb.minY + delta.y),
                    Math.min(bb.minZ, bb.minZ + delta.z),
                    Math.max(bb.maxX, bb.maxX + delta.x),
                    Math.max(bb.maxY, bb.maxY + delta.y),
                    Math.max(bb.maxZ, bb.maxZ + delta.z));
            List<AABB> boxes = colliders(area);
            int[] order = Math.abs(delta.x) < Math.abs(delta.z) ? new int[]{1, 2, 0} : new int[]{1, 0, 2};
            double[] d = {delta.x, delta.y, delta.z};
            double[] out = new double[3];
            for (int axis : order) {
                double m = collideAxis(axis, bb, boxes, d[axis]);
                bb = moveBox(bb, axis, m);
                out[axis] = m;
            }
            return new Vec3(out[0], out[1], out[2]);
        }

        /**
         * The player's step height ({@code Attributes.STEP_HEIGHT}).
         */
        static final double STEP = 0.6;

        /**
         * Entity.collide with stepping up: a move on the ground that runs into something at
         * most {@link #STEP} high goes up, across and back down instead, if that gets further.
         */
        Vec3 collideAndStep(AABB bb, Vec3 delta, boolean onGround) {
            Vec3 movement = collide(bb, delta);
            boolean horizontal = movement.x != delta.x || movement.z != delta.z;
            boolean onGroundAfter = onGround || (movement.y != delta.y && delta.y < 0.0);
            if (onGroundAfter && horizontal) {
                double up = collide(bb, new Vec3(0.0, STEP, 0.0)).y;
                AABB raised = bb.move(0.0, up, 0.0);
                Vec3 across = collide(raised, new Vec3(delta.x, 0.0, delta.z));
                Vec3 down = collide(raised.move(across.x, 0.0, across.z), new Vec3(0.0, movement.y - up, 0.0));
                Vec3 stepped = new Vec3(across.x, up + down.y, across.z);
                if (stepped.x * stepped.x + stepped.z * stepped.z > movement.x * movement.x + movement.z * movement.z) {
                    return stepped;
                }
            }
            return movement;
        }

        boolean canFallAtLeast(AABB bb, double dx, double dz, double minHeight) {
            AABB area = new AABB(
                    bb.minX + 1.0E-7 + dx,
                    bb.minY - minHeight - 1.0E-7,
                    bb.minZ + 1.0E-7 + dz,
                    bb.maxX - 1.0E-7 + dx,
                    bb.minY,
                    bb.maxZ - 1.0E-7 + dz);
            for (AABB b : colliders(area)) {
                if (b.intersects(area)) {
                    return false;
                }
            }
            return true;
        }

        Vec3 backOffFromEdge(AABB bb, Vec3 delta) {
            double maxDownStep = (double) 0.6F;
            double dx = delta.x;
            double dz = delta.z;
            double stepX = Math.signum(dx) * 0.05;
            double stepZ = Math.signum(dz) * 0.05;
            while (dx != 0.0 && canFallAtLeast(bb, dx, 0.0, maxDownStep)) {
                if (Math.abs(dx) <= 0.05) {
                    dx = 0.0;
                    break;
                }
                dx -= stepX;
            }
            while (dz != 0.0 && canFallAtLeast(bb, 0.0, dz, maxDownStep)) {
                if (Math.abs(dz) <= 0.05) {
                    dz = 0.0;
                    break;
                }
                dz -= stepZ;
            }
            while (dx != 0.0 && dz != 0.0 && canFallAtLeast(bb, dx, dz, maxDownStep)) {
                if (Math.abs(dx) <= 0.05) {
                    dx = 0.0;
                } else {
                    dx -= stepX;
                }
                if (Math.abs(dz) <= 0.05) {
                    dz = 0.0;
                } else {
                    dz -= stepZ;
                }
            }
            return new Vec3(dx, delta.y, dz);
        }

        /**
         * Blocks the simulation climbs: those the table marks climbable (ladders, vines,
         * weeping and twisting vines, scaffolding).
         */
        static boolean climbable(BlockState state) {
            Block b = state.getBlock();
            return b == Blocks.LADDER || b == Blocks.VINE || b == Blocks.WEEPING_VINES || b == Blocks.WEEPING_VINES_PLANT
                    || b == Blocks.TWISTING_VINES || b == Blocks.TWISTING_VINES_PLANT || b == Blocks.SCAFFOLDING;
        }

        static Vec3 inputVector(Vec3 input, float speed, float yRot) {
            double length = input.lengthSqr();
            if (length < 1.0E-7) {
                return Vec3.ZERO;
            }
            Vec3 movement = (length > 1.0 ? input.normalize() : input).scale(speed);
            float sin = Mth.sin(yRot * (float) (Math.PI / 180.0));
            float cos = Mth.cos(yRot * (float) (Math.PI / 180.0));
            return new Vec3(movement.x * cos - movement.z * sin, movement.y, movement.z * cos + movement.x * sin);
        }

        void playerTick() throws Exception {
            player.setOldPosAndRot();

            boolean crouching = prevShift;
            float left = 0.0F;
            float forward = 0.0F;
            net.minecraft.world.entity.player.Input keys = net.minecraft.world.entity.player.Input.EMPTY;
            if (player.input instanceof PlayerMovementInput) {
                player.input.tick();
                Vec2 move = player.input.getMoveVector();
                left = move.x;
                forward = move.y;
                keys = player.input.keyPresses;
            }
            prevShift = keys.shift();
            SprintStateEvent sprintState = new SprintStateEvent();
            geh.onPlayerSprintState(sprintState);
            boolean sprintKey = sprintState.getState() != null ? sprintState.getState() : keys.sprint();
            boolean sprinting = sprintKey && forward > 1.0E-5F && !crouching;
            float xxa = left * 0.98F;
            float zza = forward * 0.98F;

            RotationMoveEvent jumpEvent = new RotationMoveEvent(RotationMoveEvent.Type.JUMP, player.getYRot(), player.getXRot());
            geh.onPlayerRotationMove(jumpEvent);
            RotationMoveEvent motionEvent = new RotationMoveEvent(RotationMoveEvent.Type.MOTION_UPDATE, player.getYRot(), player.getXRot());
            geh.onPlayerRotationMove(motionEvent);

            BlockRefGen.setField(LocalPlayer.class, player, "crouching", crouching);
            player.sprinting = sprinting;
            BlockRefGen.setField(Entity.class, player, "eyeHeight", crouching ? 1.27F : 1.62F);
            AABB bb = playerBox(player.position(), crouching);
            Vec3 vel = player.getDeltaMovement();
            boolean onGround = player.onGround();

            // LivingEntity.aiStep: jumping
            if (jumpDelay > 0) {
                jumpDelay--;
            }
            if (keys.jump()) {
                if (onGround && jumpDelay == 0) {
                    vel = new Vec3(vel.x, Math.max((double) 0.42F, vel.y), vel.z);
                    if (sprinting) {
                        float angle = jumpEvent.getYaw() * (float) (Math.PI / 180.0);
                        vel = vel.add(-Mth.sin(angle) * 0.2, 0.0, Mth.cos(angle) * 0.2);
                    }
                    jumpDelay = 10;
                }
            } else {
                jumpDelay = 0;
            }

            // LivingEntity.travelInAir
            float blockFriction = onGround ? 0.6F : 1.0F;
            float speed;
            if (onGround) {
                float base = sprinting ? 0.13F : 0.1F;
                speed = base * (0.21600002F / (blockFriction * blockFriction * blockFriction));
            } else {
                speed = sprinting ? 0.025999999F : 0.02F;
            }
            vel = vel.add(inputVector(new Vec3(xxa, 0.0, zza), speed, motionEvent.getYaw()));
            // LivingEntity.handleOnClimbable
            BlockState inBlock = level.getBlockState(player.blockPosition());
            if (climbable(inBlock)) {
                double max = (double) 0.15F;
                double yd = Math.max(vel.y, -max);
                if (yd < 0.0 && !inBlock.is(Blocks.SCAFFOLDING) && keys.shift()) {
                    yd = 0.0;
                }
                vel = new Vec3(Mth.clamp(vel.x, -max, max), yd, Mth.clamp(vel.z, -max, max));
            }

            // Entity.move
            Vec3 delta = vel;
            if (crouching && onGround && delta.y <= 0.0) {
                delta = backOffFromEdge(bb, delta);
            }
            Vec3 movement = collideAndStep(bb, delta, onGround);
            boolean xCollision = movement.x != delta.x;
            boolean yCollision = movement.y != delta.y;
            boolean zCollision = movement.z != delta.z;
            setPosition(player, player.position().add(movement));
            bb = bb.move(movement);
            player.horizontalCollision = xCollision || zCollision;
            BlockRefGen.setField(Entity.class, player, "onGround", yCollision && delta.y < 0.0);
            vel = new Vec3(xCollision ? 0.0 : vel.x, yCollision ? 0.0 : vel.y, zCollision ? 0.0 : vel.z);
            // climbing: pushing against a wall or jumping while in a climbable block
            if ((player.horizontalCollision || keys.jump()) && climbable(level.getBlockState(player.blockPosition()))) {
                vel = new Vec3(vel.x, 0.2, vel.z);
            }
            float friction = blockFriction * 0.91F;
            vel = new Vec3(vel.x * (double) friction, (vel.y - 0.08) * (double) 0.98F, vel.z * (double) friction);
            BlockRefGen.setField(Entity.class, player, "deltaMovement", vel);
            BlockRefGen.setField(Entity.class, player, "bb", bb);

            // isInWall: the eyes are inside a block's collision box
            Vec3 eye = player.getEyePosition();
            BlockPos eyeBlock = BlockPos.containing(eye);
            boolean inWall = false;
            for (AABB b : level.getBlockState(eyeBlock).getCollisionShape(level, eyeBlock).toAabbs()) {
                if (b.move(eyeBlock).contains(eye)) {
                    inWall = true;
                }
            }
            player.inWall = inWall;

            // MixinClientPlayerEntity.onPreUpdate: after super.tick(), so after the move
            geh.onPlayerUpdate(new PlayerUpdateEvent(EventState.PRE));

            // Minecraft.tick: LocalPlayer.sendChanges -> sendPosition
            float yRot = player.getYRot();
            float xRot = player.getXRot();
            if (lastSent == null || lastSent[0] != yRot || lastSent[1] != xRot) {
                lastSent = new float[]{yRot, xRot};
                geh.onSendPacket(new PacketEvent(null, EventState.PRE,
                        new ServerboundMovePlayerPacket.Rot(yRot, xRot, player.onGround(), player.horizontalCollision)));
            }

            geh.onPlayerUpdate(new PlayerUpdateEvent(EventState.POST));
        }

        @SuppressWarnings("unchecked")
        JsonObject record() throws Exception {
            JsonObject r = new JsonObject();
            r.add("position", bits(player.position()));
            r.add("delta_movement", bits(player.getDeltaMovement()));
            JsonArray rotation = new JsonArray();
            rotation.add(Float.floatToRawIntBits(player.getYRot()));
            rotation.add(Float.floatToRawIntBits(player.getXRot()));
            r.add("rotation", rotation);
            r.addProperty("on_ground", player.onGround());
            r.addProperty("crouching", player.isCrouching());
            r.addProperty("sprinting", player.sprinting);
            r.addProperty("selected", player.getInventory().getSelectedSlot());
            Map<Input, Boolean> forced = (Map<Input, Boolean>) BlockRefGen.getField(InputOverrideHandler.class, input, "inputForceStateMap");
            JsonArray inputs = new JsonArray();
            forced.entrySet().stream().sorted(Map.Entry.comparingByKey(Comparator.comparingInt(Enum::ordinal))).forEach(e -> {
                JsonArray entry = new JsonArray();
                entry.add(e.getKey().name());
                entry.add(e.getValue());
                inputs.add(entry);
            });
            r.add("inputs", inputs);
            r.addProperty("baritone_input", player.input instanceof PlayerMovementInput);
            JsonArray actions = new JsonArray();
            gm.actions.forEach(actions::add);
            gm.actions.clear();
            r.add("actions", actions);
            JsonArray events = new JsonArray();
            this.events.forEach(events::add);
            this.events.clear();
            r.add("events", events);
            PathExecutor current = pathing.getCurrent();
            if (current != null) {
                JsonArray path = new JsonArray();
                path.add(current.getPosition());
                path.add(current.getPath().length());
                r.add("path", path);
                int position = current.getPosition();
                if (position >= 0 && position < current.getPath().movements().size()) {
                    r.addProperty("movement", current.getPath().movements().get(position).getClass().getSimpleName());
                }
            }
            PathingCommand command = pcm.mostRecentCommand().orElse(null);
            if (command != null) {
                r.addProperty("command", command.commandType.name() + " " + command.goal);
            }
            pcm.mostRecentInControl().ifPresent(p -> r.addProperty("in_control", p.getClass().getSimpleName()));
            return r;
        }

        /**
         * Changes a block between ticks.
         */
        void edit(BlockPos pos, BlockState state) {
            world.set(pos.getX(), pos.getY(), pos.getZ(), state);
        }

        /**
         * Moves the player between ticks, like a server correction.
         */
        void teleport(Vec3 position) throws ReflectiveOperationException {
            setPosition(player, position);
            BlockRefGen.setField(Entity.class, player, "bb", playerBox(position, player.isCrouching()));
            BlockRefGen.setField(Entity.class, player, "deltaMovement", Vec3.ZERO);
        }

        /**
         * Replaces the other entities between ticks.
         */
        void setEntities(List<EntitySpec> entities) throws ReflectiveOperationException {
            level.entities = new ArrayList<>();
            for (EntitySpec e : entities) {
                level.entities.add(e.create(level));
            }
        }
    }

    /**
     * {@code FasterWorldScanner} (what upstream's provider hands out), copied, because it reads
     * sections through mixin accessors ({@code IPalettedContainer}) that the stand-in sections
     * lack. The stand-in sections have no palette. They are never single-valued (and neither
     * are the sections of the worlds the port replays, which are filled block by block), so
     * every section with a block is scanned in storage order (y, z, x), and a section of only
     * air is skipped ({@code hasOnlyAir()}).
     */
    static final class RefWorldScanner implements IWorldScanner {
        static final RefWorldScanner INSTANCE = new RefWorldScanner();

        @Override
        public List<BlockPos> scanChunkRadius(IPlayerContext ctx, BlockOptionalMetaLookup filter, int max, int yLevelThreshold, int maxSearchRadius) {
            if (maxSearchRadius < 0) {
                throw new IllegalArgumentException("chunkRange must be >= 0");
            }
            return scanChunksInternal(ctx, filter, FasterWorldScanner.getChunkRange(ctx.playerFeet().x >> 4, ctx.playerFeet().z >> 4, maxSearchRadius), max);
        }

        @Override
        public List<BlockPos> scanChunk(IPlayerContext ctx, BlockOptionalMetaLookup filter, ChunkPos pos, int max, int yLevelThreshold) {
            Stream<BlockPos> stream = scanChunkInternal(ctx, filter, pos);
            if (max >= 0) {
                stream = stream.limit(max);
            }
            return stream.collect(Collectors.toList());
        }

        @Override
        public int repack(IPlayerContext ctx) {
            throw new UnsupportedOperationException();
        }

        @Override
        public int repack(IPlayerContext ctx, int range) {
            throw new UnsupportedOperationException();
        }

        private List<BlockPos> scanChunksInternal(IPlayerContext ctx, BlockOptionalMetaLookup lookup, List<ChunkPos> chunkPositions, int maxBlocks) {
            Stream<BlockPos> posStream = chunkPositions.parallelStream().flatMap(p -> scanChunkInternal(ctx, lookup, p));
            if (maxBlocks >= 0) {
                posStream = posStream.limit(maxBlocks);
            }
            return posStream.collect(Collectors.toList());
        }

        private Stream<BlockPos> scanChunkInternal(IPlayerContext ctx, BlockOptionalMetaLookup lookup, ChunkPos pos) {
            ChunkSource chunkProvider = ctx.world().getChunkSource();
            // if chunk is not loaded, return empty stream
            if (!chunkProvider.hasChunk(pos.x(), pos.z())) {
                return Stream.empty();
            }

            long chunkX = (long) pos.x() << 4;
            long chunkZ = (long) pos.z() << 4;

            int playerSectionY = (ctx.playerFeet().y - ctx.world().getMinY()) >> 4;

            // chunk.getMinY() asks the chunk's height accessor, which the stand-in chunks lack
            return collectChunkSections(lookup, chunkProvider.getChunk(pos.x(), pos.z(), false), ctx.world().getMinY(), chunkX, chunkZ, playerSectionY).stream();
        }

        private List<BlockPos> collectChunkSections(BlockOptionalMetaLookup lookup, LevelChunk chunk, int chunkY, long chunkX, long chunkZ, int playerSection) {
            // iterate over sections relative to player
            List<BlockPos> blocks = new ArrayList<>();
            LevelChunkSection[] sections = chunk.getSections();
            int l = sections.length;
            int i = playerSection - 1;
            int j = playerSection;
            for (; i >= 0 || j < l; ++j, --i) {
                if (j < l) {
                    visitSection(lookup, sections[j], blocks, chunkX, chunkY + j * 16, chunkZ);
                }
                if (i >= 0) {
                    visitSection(lookup, sections[i], blocks, chunkX, chunkY + i * 16, chunkZ);
                }
            }
            return blocks;
        }

        private void visitSection(BlockOptionalMetaLookup lookup, LevelChunkSection section, List<BlockPos> blocks, long chunkX, int sectionY, long chunkZ) {
            if (section == null || onlyAir(section)) {
                return;
            }
            for (int idx = 0; idx < 4096; idx++) {
                if (lookup.has(section.getBlockState(idx & 15, idx >> 8, (idx & 255) >> 4))) {
                    blocks.add(new BlockPos(
                            (int) chunkX + ((idx & 255) & 15),
                            sectionY + (idx >> 8),
                            (int) chunkZ + ((idx & 255) >> 4)
                    ));
                }
            }
        }

        private static boolean onlyAir(LevelChunkSection section) {
            for (int y = 0; y < 16; y++) {
                for (int z = 0; z < 16; z++) {
                    for (int x = 0; x < 16; x++) {
                        if (!section.getBlockState(x, y, z).isAir()) {
                            return false;
                        }
                    }
                }
            }
            return true;
        }
    }

    static JsonArray bits(Vec3 v) {
        JsonArray a = new JsonArray();
        a.add(Double.doubleToRawLongBits(v.x));
        a.add(Double.doubleToRawLongBits(v.y));
        a.add(Double.doubleToRawLongBits(v.z));
        return a;
    }

    // endregion

    // region scenarios

    /**
     * A scenario: the player starts at {@code start} with {@code items} in {@code world}, and
     * a process starts: the custom goal process to {@code goal}, unless {@link #process} says
     * which process to start how (see {@link #start}).
     */
    static final class Scenario {
        final String name;
        final ExecWorld world;
        final Vec3 start;
        final BlockPos goal;
        final ItemStack[] items;
        final int selected;
        final JsonObject config;
        final int ticks;
        final List<Event> events;
        JsonObject process;
        List<EntitySpec> entities = List.of();
        int loadRadius = -1;

        Scenario(String name, ExecWorld world, Vec3 start, BlockPos goal, ItemStack[] items, int selected,
                 JsonObject config, int ticks, List<Event> events) {
            this.name = name;
            this.world = world;
            this.start = start;
            this.goal = goal;
            this.items = items;
            this.selected = selected;
            this.config = config;
            this.ticks = ticks;
            this.events = events;
        }

        Scenario(String name, ExecWorld world, Vec3 start, BlockPos goal, ItemStack[] items, int selected,
                 JsonObject config, int ticks) {
            this(name, world, start, goal, items, selected, config, ticks, List.of());
        }

        Scenario process(Object... kv) {
            JsonObject p = new JsonObject();
            for (int i = 0; i < kv.length; i += 2) {
                Object v = kv[i + 1];
                if (v instanceof String s) {
                    p.addProperty((String) kv[i], s);
                } else if (v instanceof Number n) {
                    p.addProperty((String) kv[i], n);
                } else if (v instanceof String[] a) {
                    JsonArray array = new JsonArray();
                    for (String s : a) {
                        array.add(s);
                    }
                    p.add((String) kv[i], array);
                } else {
                    throw new IllegalArgumentException(String.valueOf(v));
                }
            }
            this.process = p;
            return this;
        }

        Scenario entities(EntitySpec... entities) {
            this.entities = List.of(entities);
            return this;
        }

        Scenario loadRadius(int loadRadius) {
            this.loadRadius = loadRadius;
            return this;
        }
    }

    /**
     * Starts the scenario's process. The process JSON has a {@code type}: {@code mine}
     * ({@code quantity}, {@code blocks}: selectors), {@code get_to_block} ({@code block}: a
     * selector), {@code follow} ({@code entity_type}), {@code pickup} ({@code item}),
     * {@code explore} ({@code x}, {@code z}) or {@code farm} ({@code range}).
     */
    static void start(Sim sim, Scenario s) {
        if (s.process == null) {
            sim.baritone.getCustomGoalProcess().setGoalAndPath(new GoalBlock(s.goal));
            return;
        }
        JsonObject p = s.process;
        switch (p.get("type").getAsString()) {
            case "mine" -> {
                List<String> blocks = new ArrayList<>();
                p.getAsJsonArray("blocks").forEach(b -> blocks.add(b.getAsString()));
                sim.baritone.getMineProcess().mineByName(p.get("quantity").getAsInt(), blocks.toArray(new String[0]));
            }
            case "get_to_block" -> sim.baritone.getGetToBlockProcess().getToBlock(new BlockOptionalMeta(p.get("block").getAsString()));
            case "follow" -> {
                String type = p.get("entity_type").getAsString();
                sim.baritone.getFollowProcess().follow(e -> BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).toString().equals(type));
            }
            case "pickup" -> {
                net.minecraft.world.item.Item item = BuiltInRegistries.ITEM.getValue(Identifier.parse(p.get("item").getAsString()));
                sim.baritone.getFollowProcess().pickup(stack -> stack.is(item));
            }
            case "explore" -> sim.baritone.getExploreProcess().explore(p.get("x").getAsInt(), p.get("z").getAsInt());
            case "farm" -> sim.baritone.getFarmProcess().farm(p.get("range").getAsInt(), null);
            default -> throw new IllegalArgumentException(p.toString());
        }
    }

    /**
     * An item entity: {@code item} lying at {@code position}.
     */
    record EntitySpec(int id, Vec3 position, boolean onGround, ItemStack item) {
        /**
         * As the port's {@code host::Entity}.
         */
        JsonObject json() {
            JsonObject o = new JsonObject();
            o.addProperty("id", id);
            o.addProperty("type_id", "minecraft:item");
            JsonArray pos = new JsonArray();
            pos.add(position.x);
            pos.add(position.y);
            pos.add(position.z);
            o.add("position", pos);
            AABB bb = EntityTypes.ITEM.getDimensions().makeBoundingBox(position);
            JsonArray box = new JsonArray();
            for (double d : new double[]{bb.minX, bb.minY, bb.minZ, bb.maxX, bb.maxY, bb.maxZ}) {
                box.add(d);
            }
            o.add("bounding_box", box);
            o.addProperty("on_ground", onGround);
            o.addProperty("alive", true);
            o.addProperty("blocks_building", false);
            o.add("item", PathRefGen.itemJson(item));
            return o;
        }

        ItemEntity create(FakeLevel level) throws ReflectiveOperationException {
            ItemEntity e = new ItemEntity(EntityTypes.ITEM, level);
            e.setId(id);
            e.setPos(position.x, position.y, position.z);
            e.setItem(item.copy());
            // setOnGround looks for the supporting block through the chunks
            BlockRefGen.setField(Entity.class, e, "onGround", onGround);
            return e;
        }
    }

    static JsonArray entitiesJson(List<EntitySpec> entities) {
        JsonArray a = new JsonArray();
        entities.forEach(e -> a.add(e.json()));
        return a;
    }

    /**
     * Something that happens after tick {@code tick}: a box of blocks changes ({@code state}
     * from {@code a} to {@code b}), the player is moved to {@code teleport}, or the other
     * entities become {@code entities}.
     */
    record Event(int tick, BlockPos a, BlockPos b, BlockState state, Vec3 teleport, List<EntitySpec> entities) {
        static Event fill(int tick, BlockPos a, BlockPos b, BlockState state) {
            return new Event(tick, a, b, state, null, null);
        }

        static Event teleport(int tick, Vec3 to) {
            return new Event(tick, null, null, null, to, null);
        }

        static Event entities(int tick, EntitySpec... entities) {
            return new Event(tick, null, null, null, null, List.of(entities));
        }

        JsonObject json() {
            JsonObject o = new JsonObject();
            o.addProperty("tick", tick);
            if (teleport != null) {
                o.add("teleport", bits(teleport));
            } else if (entities != null) {
                o.add("entities", entitiesJson(entities));
            } else {
                o.add("from", blockPos(a));
                o.add("to", blockPos(b));
                o.addProperty("state", Block.BLOCK_STATE_REGISTRY.getId(state));
            }
            return o;
        }

        void apply(Sim sim) throws ReflectiveOperationException {
            if (teleport != null) {
                sim.teleport(teleport);
                return;
            }
            if (entities != null) {
                sim.setEntities(entities);
                return;
            }
            for (int x = Math.min(a.getX(), b.getX()); x <= Math.max(a.getX(), b.getX()); x++) {
                for (int y = Math.min(a.getY(), b.getY()); y <= Math.max(a.getY(), b.getY()); y++) {
                    for (int z = Math.min(a.getZ(), b.getZ()); z <= Math.max(a.getZ(), b.getZ()); z++) {
                        sim.edit(new BlockPos(x, y, z), state);
                    }
                }
            }
        }
    }

    static ItemStack stack(net.minecraft.world.item.Item item, int count) {
        return new ItemStack(item, count);
    }

    /**
     * A stone pickaxe on slot 0 and cobblestone on slot 1.
     */
    static ItemStack[] basicItems() {
        ItemStack[] items = new ItemStack[36];
        java.util.Arrays.fill(items, ItemStack.EMPTY);
        items[0] = stack(Items.STONE_PICKAXE, 1);
        items[1] = stack(Items.COBBLESTONE, 64);
        return items;
    }

    /**
     * Stone below y = 0, grass at y = 0, over x and z in [-48, 48).
     */
    static ExecWorld flat() {
        ExecWorld w = new ExecWorld(-48, -48, 96, 96, -16, 64);
        w.fill(-48, -16, -48, 47, -1, 47, Blocks.STONE.defaultBlockState());
        w.fill(-48, 0, -48, 47, 0, 47, Blocks.GRASS_BLOCK.defaultBlockState());
        return w;
    }

    static JsonObject config(Object... kv) {
        JsonObject c = new JsonObject();
        for (int i = 0; i < kv.length; i += 2) {
            Object v = kv[i + 1];
            if (v instanceof Boolean b) {
                c.addProperty((String) kv[i], b);
            } else {
                c.addProperty((String) kv[i], (Number) v);
            }
        }
        return c;
    }

    /**
     * Random hills: heights from a few smooth bumps, with stone, dirt and grass, gravel and
     * sand patches, some trees, flowers and grass.
     */
    static ExecWorld terrain() {
        ExecWorld w = new ExecWorld(-40, -40, 80, 80, -16, 64);
        int bumps = 30 + R.nextInt(20);
        double[][] b = new double[bumps][4];
        for (double[] bump : b) {
            bump[0] = R.nextInt(80) - 40;
            bump[1] = R.nextInt(80) - 40;
            bump[2] = 3 + R.nextInt(10);
            bump[3] = R.nextInt(11) - 4;
        }
        for (int x = -40; x < 40; x++) {
            for (int z = -40; z < 40; z++) {
                double h = 0;
                for (double[] bump : b) {
                    double d = Math.sqrt((x - bump[0]) * (x - bump[0]) + (z - bump[1]) * (z - bump[1]));
                    h += bump[3] * Math.max(0, 1 - d / bump[2]);
                }
                int top = (int) Math.round(h);
                for (int y = -16; y <= top; y++) {
                    BlockState s = y == top ? Blocks.GRASS_BLOCK.defaultBlockState()
                            : y >= top - 2 ? Blocks.DIRT.defaultBlockState() : Blocks.STONE.defaultBlockState();
                    w.set(x, y, z, s);
                }
            }
        }
        // patches of gravel and sand at the surface
        for (int i = 0; i < 12; i++) {
            int cx = R.nextInt(70) - 35;
            int cz = R.nextInt(70) - 35;
            BlockState s = R.nextBoolean() ? Blocks.GRAVEL.defaultBlockState() : Blocks.SAND.defaultBlockState();
            for (int x = cx - 2; x <= cx + 2; x++) {
                for (int z = cz - 2; z <= cz + 2; z++) {
                    w.set(x, surface(w, x, z) - 1, z, s);
                }
            }
        }
        // trees
        for (int i = 0; i < 10; i++) {
            int x = R.nextInt(70) - 35;
            int z = R.nextInt(70) - 35;
            int y = surface(w, x, z);
            int trunk = 3 + R.nextInt(3);
            for (int dx = -2; dx <= 2; dx++) {
                for (int dz = -2; dz <= 2; dz++) {
                    for (int dy = trunk - 1; dy <= trunk + 1; dy++) {
                        if (Math.abs(dx) + Math.abs(dz) <= 3 && w.get(x + dx, y + dy, z + dz).isAir()) {
                            w.set(x + dx, y + dy, z + dz, Blocks.OAK_LEAVES.defaultBlockState());
                        }
                    }
                }
            }
            for (int dy = 0; dy < trunk; dy++) {
                w.set(x, y + dy, z, Blocks.OAK_LOG.defaultBlockState());
            }
        }
        // flowers and grass
        for (int i = 0; i < 150; i++) {
            int x = R.nextInt(80) - 40;
            int z = R.nextInt(80) - 40;
            int y = surface(w, x, z);
            if (w.get(x, y - 1, z).is(Blocks.GRASS_BLOCK)) {
                w.set(x, y, z, R.nextBoolean() ? Blocks.SHORT_GRASS.defaultBlockState() : Blocks.POPPY.defaultBlockState());
            }
        }
        return w;
    }

    /**
     * The first y from the top whose block below is not air.
     */
    static int surface(ExecWorld w, int x, int z) {
        for (int y = w.minY + w.height - 1; y > w.minY; y--) {
            if (!w.get(x, y - 1, z).isAir()) {
                return y;
            }
        }
        return w.minY;
    }

    static List<Scenario> scenarioList() {
        List<Scenario> list = new ArrayList<>();
        JsonObject none = new JsonObject();
        Vec3 start = new Vec3(0.5, 1.0, 0.5);

        list.add(new Scenario("flat", flat(), start, new BlockPos(12, 1, 5), basicItems(), 0, none, 200));

        ExecWorld stairs = flat();
        for (int n = 1; n <= 4; n++) {
            stairs.fill(2 + n, 1, 0, 2 + n, n, 0, Blocks.STONE.defaultBlockState());
        }
        list.add(new Scenario("stairs_up", stairs, start, new BlockPos(6, 5, 0), basicItems(), 0, none, 300));

        ExecWorld tower = flat();
        tower.fill(0, 1, 0, 0, 4, 0, Blocks.STONE.defaultBlockState());
        for (int n = 1; n <= 3; n++) {
            tower.fill(n, 1, 0, n, 4 - n, 0, Blocks.STONE.defaultBlockState());
        }
        list.add(new Scenario("stairs_down", tower.copy(), new Vec3(0.5, 5.0, 0.5), new BlockPos(5, 1, 0), basicItems(), 0, none, 300));
        tower.fill(-3, 1, 0, -1, 1, 0, Blocks.STONE.defaultBlockState());
        list.add(new Scenario("fall", tower, new Vec3(0.5, 5.0, 0.5), new BlockPos(-3, 2, 0), basicItems(), 0, none, 300));

        ExecWorld wall = flat();
        wall.fill(5, 1, -48, 5, 4, 47, Blocks.STONE.defaultBlockState());
        list.add(new Scenario("wall", wall, start, new BlockPos(9, 1, 0), basicItems(), 0, none, 400));

        ExecWorld gap = flat();
        gap.fill(4, -10, -20, 5, 0, 20, AIR);
        list.add(new Scenario("bridge", gap, start, new BlockPos(8, 1, 0), basicItems(), 0, none, 400));

        ExecWorld ledge = flat();
        ledge.fill(0, 5, 8, 0, 5, 12, Blocks.STONE.defaultBlockState());
        list.add(new Scenario("pillar", ledge, start, new BlockPos(0, 6, 10), basicItems(), 0, none, 500));

        ExecWorld zigzag = flat();
        int[] walls = {8, 16, 24, 32};
        for (int i = 0; i < walls.length; i++) {
            int gapZ = i % 2 == 0 ? 30 : -30;
            for (int z = -48; z < 48; z++) {
                if (z != gapZ) {
                    zigzag.fill(walls[i], 1, z, walls[i], 3, z, Blocks.BEDROCK.defaultBlockState());
                }
            }
        }
        list.add(new Scenario("segments", zigzag, start, new BlockPos(40, 1, 0), basicItems(), 0,
                config("primaryTimeoutMS", 0, "planAheadPrimaryTimeoutMS", 0), 600));

        ExecWorld hole = flat();
        list.add(new Scenario("dig_down", hole, start, new BlockPos(0, -4, 0), basicItems(), 0, none, 400));

        ExecWorld gaps = flat();
        for (int x = 3; x <= 30; x += 4) {
            gaps.fill(x, -3, -48, x + (x % 8 == 3 ? 1 : 0), 0, 47, AIR);
        }
        list.add(new Scenario("parkour", gaps, start, new BlockPos(32, 1, 0), basicItems(), 0,
                config("allowParkour", true, "allowPlace", false), 400));

        ExecWorld diagonal = flat();
        for (int i = 1; i <= 6; i++) {
            diagonal.fill(i, 1, -i, 47, 1, -i, Blocks.STONE.defaultBlockState());
        }
        list.add(new Scenario("diagonal", diagonal, start, new BlockPos(10, 1, 10), basicItems(), 0,
                config("allowDiagonalDescend", true, "allowDiagonalAscend", true), 300));

        // items off the hotbar: InventoryBehavior swaps a pickaxe and throwaways in
        ItemStack[] stored = new ItemStack[36];
        java.util.Arrays.fill(stored, ItemStack.EMPTY);
        stored[20] = stack(Items.IRON_PICKAXE, 1);
        stored[25] = stack(Items.DIRT, 64);
        list.add(new Scenario("inventory", wall.copy(), start, new BlockPos(9, 1, 0), stored, 0,
                config("allowInventory", true), 400));

        // planned next segments taken over whole, or early, instead of spliced
        list.add(new Scenario("no_splice", zigzag.copy(), start, new BlockPos(40, 1, 0), basicItems(), 0,
                config("primaryTimeoutMS", 0, "planAheadPrimaryTimeoutMS", 0, "splicePath", false), 600));

        // a wall appears across the path: the next movements become impossible
        list.add(new Scenario("blocked", flat(), start, new BlockPos(20, 1, 0), basicItems(), 0, none, 500,
                List.of(Event.fill(8, new BlockPos(10, 1, -48), new BlockPos(10, 3, 47), Blocks.BEDROCK.defaultBlockState()))));

        // the floor ahead is dug out: blocks to place change
        list.add(new Scenario("floor_removed", flat(), start, new BlockPos(20, 1, 0), basicItems(), 0, none, 500,
                List.of(Event.fill(6, new BlockPos(6, -3, -2), new BlockPos(8, 0, 2), AIR))));

        // set back a few blocks, then far off the path
        list.add(new Scenario("teleport_back", flat(), start, new BlockPos(20, 1, 0), basicItems(), 0, none, 300,
                List.of(Event.teleport(25, new Vec3(1.5, 1.0, 0.5)))));
        list.add(new Scenario("teleport_far", flat(), start, new BlockPos(20, 1, 0), basicItems(), 0, none, 300,
                List.of(Event.teleport(25, new Vec3(6.5, 1.0, 9.5)))));

        // a goal boxed in by bedrock: the calculation fails
        ExecWorld boxed = flat();
        boxed.fill(9, 0, -1, 11, 4, 1, Blocks.BEDROCK.defaultBlockState());
        boxed.fill(10, 1, 0, 10, 2, 0, AIR);
        list.add(new Scenario("unreachable", boxed, start, new BlockPos(10, 1, 0), basicItems(), 0, none, 100));

        list.add(new Scenario("disconnect", flat(), start, new BlockPos(6, 1, 0), basicItems(), 0,
                config("disconnectOnArrival", true), 120));

        // a tower with a ladder on its side, up and down
        ExecWorld ladders = flat();
        ladders.fill(4, 1, 0, 4, 6, 0, Blocks.STONE.defaultBlockState());
        ladders.fill(3, 1, 0, 3, 6, 0, Blocks.LADDER.defaultBlockState().setValue(LadderBlock.FACING, Direction.WEST));
        list.add(new Scenario("ladder_up", ladders.copy(), start, new BlockPos(4, 7, 0), basicItems(), 0,
                config("allowPlace", false, "allowBreak", false), 400));
        list.add(new Scenario("ladder_down", ladders, new Vec3(4.5, 7.0, 0.5), new BlockPos(0, 1, 0), basicItems(), 0,
                config("allowPlace", false, "allowBreak", false), 400));

        // a wall with a closed door, and one with a closed fence gate
        ExecWorld door = flat();
        door.fill(5, 1, -48, 5, 3, 47, Blocks.STONE.defaultBlockState());
        BlockState lower = Blocks.OAK_DOOR.defaultBlockState().setValue(DoorBlock.FACING, Direction.EAST);
        door.set(5, 1, 0, lower);
        door.set(5, 2, 0, lower.setValue(DoorBlock.HALF, DoubleBlockHalf.UPPER));
        list.add(new Scenario("door", door, start, new BlockPos(9, 1, 0), basicItems(), 0, config("allowBreak", false), 300));
        ExecWorld gate = flat();
        gate.fill(5, 1, -48, 5, 3, 47, Blocks.STONE.defaultBlockState());
        gate.set(5, 1, 0, Blocks.OAK_FENCE_GATE.defaultBlockState().setValue(FenceGateBlock.FACING, Direction.EAST));
        gate.set(5, 2, 0, AIR);
        list.add(new Scenario("gate", gate, start, new BlockPos(9, 1, 0), basicItems(), 0,
                config("allowBreak", false, "allowPlace", false), 300));

        // inventory moves wait until the player stands still, pausing the path
        list.add(new Scenario("pause", wall.copy(), start, new BlockPos(9, 1, 0), stored, 0,
                config("allowInventory", true, "inventoryMoveOnlyIfStationary", true, "ticksBetweenInventoryMoves", 3), 400));

        // the goal is past the loaded chunks: its Y is dropped, and the path stops at the edge
        ExecWorld edge = flat();
        for (int cx = 1; cx <= 2; cx++) {
            for (int cz = -3; cz <= 2; cz++) {
                edge.loaded.remove(ChunkPos.pack(cx, cz));
            }
        }
        list.add(new Scenario("edge", edge, start, new BlockPos(40, 1, 0), basicItems(), 0, none, 200));

        // a cliff too high to fall without a water bucket (the sim does not place water)
        ExecWorld cliff = flat();
        cliff.fill(-2, 1, -3, 2, 10, 3, Blocks.STONE.defaultBlockState());
        ItemStack[] bucket = basicItems();
        bucket[2] = stack(Items.WATER_BUCKET, 1);
        bucket[3] = stack(Items.BUCKET, 1);
        list.add(new Scenario("bucket_fall", cliff, new Vec3(0.5, 11.0, 0.5), new BlockPos(6, 1, 0), bucket, 0,
                config("allowPlace", false, "allowBreak", false), 200));

        // stairs down with a cactus past the last step: descending in safe mode
        ExecWorld cactus = flat();
        cactus.fill(0, 1, -2, 0, 3, 2, Blocks.STONE.defaultBlockState());
        cactus.fill(1, 1, -2, 1, 2, 2, Blocks.STONE.defaultBlockState());
        cactus.fill(2, 1, -2, 2, 1, 2, Blocks.STONE.defaultBlockState());
        cactus.set(4, 1, 0, Blocks.SAND.defaultBlockState());
        cactus.set(4, 0, 0, Blocks.SAND.defaultBlockState());
        cactus.set(4, 2, 0, Blocks.CACTUS.defaultBlockState());
        list.add(new Scenario("descend_safe", cactus, new Vec3(0.5, 4.0, 0.5), new BlockPos(3, 1, 0), basicItems(), 0,
                config("allowBreak", false), 200));

        for (int i = 0; i < 6; i++) {
            ExecWorld t = terrain();
            int gx = R.nextInt(60) - 30;
            int gz = R.nextInt(60) - 30;
            // no plants where the player starts and the goal is
            clearPlants(t, 0, 0);
            clearPlants(t, gx, gz);
            Vec3 s = new Vec3(0.5, surface(t, 0, 0), 0.5);
            JsonObject c = i % 2 == 0 ? none : config("allowDiagonalDescend", true, "allowParkour", true, "sprintAscends", i % 3 == 0);
            list.add(new Scenario("terrain_" + i, t, s, new BlockPos(gx, surface(t, gx, gz), gz), basicItems(), 0, c, 600));
        }

        list.addAll(processScenarios());
        return list;
    }

    /**
     * Timeouts long enough that every calculation ends by finding its goal or running out of
     * nodes: processes path toward goals that cannot be reached (running away, exploring).
     * No planning ahead: when a process changes its goal, the pathing control manager cancels
     * the calculation of the next segment, which then ends as cancelled or, if it finished
     * first, as failed, a race between threads that neither side can pin down. Dropped items
     * are not scanned for: upstream matches item stacks through a mixin ({@code IItemStack})
     * the stand-in items lack, and its anticipated drops expire by the wall clock.
     */
    static JsonObject processConfig(Object... kv) {
        JsonObject c = config("primaryTimeoutMS", 60000, "failureTimeoutMS", 60000,
                "planAheadPrimaryTimeoutMS", 60000, "planAheadFailureTimeoutMS", 60000,
                "planningTickLookahead", 0, "mineScanDroppedItems", false);
        JsonObject extra = config(kv);
        for (String key : extra.keySet()) {
            c.add(key, extra.get(key));
        }
        return c;
    }

    /**
     * A bedrock floor at y = 0 over x and z in [x0, x0 + size): nothing to dig, so a search
     * that cannot reach its goal soon runs out of nodes.
     */
    static ExecWorld platform(int x0, int size) {
        ExecWorld w = new ExecWorld(x0, x0, size, size, -16, 64);
        w.fill(x0, 0, x0, x0 + size - 1, 0, x0 + size - 1, Blocks.BEDROCK.defaultBlockState());
        return w;
    }

    static EntitySpec item(int id, double x, double y, double z, net.minecraft.world.item.Item item) {
        return new EntitySpec(id, new Vec3(x, y, z), true, stack(item, 1));
    }

    static List<Scenario> processScenarios() {
        List<Scenario> list = new ArrayList<>();
        Vec3 start = new Vec3(0.5, 1.0, 0.5);
        ItemStack[] axe = basicItems();
        axe[2] = stack(Items.IRON_AXE, 1);
        // no blocks to place: searches stay on the ground
        ItemStack[] pickaxe = new ItemStack[36];
        java.util.Arrays.fill(pickaxe, ItemStack.EMPTY);
        pickaxe[0] = stack(Items.STONE_PICKAXE, 1);
        String[] oakLog = {"oak_log"};
        String[] coalOre = {"coal_ore"};

        // logs around: above the start (mined from below, the shaft), a pair, a trunk and a
        // floating one; the process stops when none are left
        ExecWorld logs = flat();
        logs.set(0, 3, 0, Blocks.OAK_LOG.defaultBlockState());
        logs.set(5, 1, 3, Blocks.OAK_LOG.defaultBlockState());
        logs.set(6, 1, 3, Blocks.OAK_LOG.defaultBlockState());
        logs.fill(-4, 1, -6, -4, 3, -6, Blocks.OAK_LOG.defaultBlockState());
        logs.set(10, 3, 2, Blocks.OAK_LOG.defaultBlockState());
        list.add(new Scenario("mine_logs", logs, start, null, axe, 0,
                processConfig("exploreForBlocks", false), 700)
                .process("type", "mine", "quantity", 0, "blocks", oakLog));

        // coal ore buried in the stone, in the surface, and just under it
        ExecWorld ores = flat();
        ores.set(3, -3, 0, Blocks.COAL_ORE.defaultBlockState());
        ores.set(6, 0, 4, Blocks.COAL_ORE.defaultBlockState());
        ores.set(-5, -1, -2, Blocks.COAL_ORE.defaultBlockState());
        ores.set(-5, -2, -2, Blocks.COAL_ORE.defaultBlockState());
        list.add(new Scenario("mine_ores", ores.copy(), start, null, basicItems(), 0,
                processConfig("exploreForBlocks", false), 700)
                .process("type", "mine", "quantity", 0, "blocks", coalOre));
        list.add(new Scenario("mine_exposed", ores.copy(), start, null, basicItems(), 0,
                processConfig("exploreForBlocks", false, "allowOnlyExposedOres", true, "forceInternalMining", false), 400)
                .process("type", "mine", "quantity", 0, "blocks", coalOre));
        list.add(new Scenario("mine_legit", ores, start, null, basicItems(), 0,
                processConfig("legitMine", true), 300)
                .process("type", "mine", "quantity", 0, "blocks", coalOre));

        // locked in a bedrock room with the logs outside: every search fails, the closest log
        // is blacklisted, then the other, then there is nothing left
        ExecWorld room = flat();
        room.fill(-3, 0, -3, 3, 4, 3, Blocks.BEDROCK.defaultBlockState());
        room.fill(-2, 1, -2, 2, 3, 2, AIR);
        room.set(8, 1, 0, Blocks.OAK_LOG.defaultBlockState());
        room.set(-9, 1, 4, Blocks.OAK_LOG.defaultBlockState());
        list.add(new Scenario("mine_blacklist", room, start, null, axe, 0, processConfig(), 150)
                .process("type", "mine", "quantity", 0, "blocks", oakLog));

        // nothing to mine: go away from the branch point, at legitMineYLevel
        list.add(new Scenario("mine_explore", platform(-24, 48), start, null, pickaxe, 0, processConfig(), 150)
                .process("type", "mine", "quantity", 0, "blocks", oakLog));

        // get to a crafting table and open it
        ExecWorld table = flat();
        table.set(8, 1, 5, Blocks.CRAFTING_TABLE.defaultBlockState());
        list.add(new Scenario("get_to_crafting_table", table, start, null, basicItems(), 0, processConfig(), 250)
                .process("type", "get_to_block", "block", "crafting_table"));
        ExecWorld gold = flat();
        gold.set(-7, 1, 9, Blocks.GOLD_BLOCK.defaultBlockState());
        gold.set(12, 1, -3, Blocks.GOLD_BLOCK.defaultBlockState());
        list.add(new Scenario("get_to_gold", gold, start, null, basicItems(), 0, processConfig(), 250)
                .process("type", "get_to_block", "block", "gold_block"));
        // none known: run away from the start
        list.add(new Scenario("get_to_missing", platform(-24, 48), start, null, pickaxe, 0, processConfig(), 150)
                .process("type", "get_to_block", "block", "diamond_block"));

        // follow an item around, then it is gone
        EntitySpec stick = item(1, 6.5, 1.0, 0.5, Items.STICK);
        list.add(new Scenario("follow", flat(), start, null, basicItems(), 0, processConfig(), 320,
                List.of(Event.entities(60, item(1, 12.5, 1.0, 5.5, Items.STICK)),
                        Event.entities(150, item(1, 4.5, 1.0, 12.5, Items.STICK)),
                        Event.entities(250)))
                .process("type", "follow", "entity_type", "minecraft:item")
                .entities(stick));
        list.add(new Scenario("follow_offset", flat(), start, null, basicItems(), 0,
                processConfig("followOffsetDistance", 3.0, "followOffsetDirection", 90.0, "followRadius", 1), 200)
                .process("type", "follow", "entity_type", "minecraft:item")
                .entities(item(7, -8.5, 1.0, 6.25, Items.STICK)));
        // pick up the wheat, not the stick; then the wheat is picked up
        list.add(new Scenario("pickup", flat(), start, null, basicItems(), 0, processConfig(), 200,
                List.of(Event.entities(120, item(3, -6.5, 1.0, -4.5, Items.STICK))))
                .process("type", "pickup", "item", "minecraft:wheat")
                .entities(item(2, 5.5, 1.0, 2.25, Items.WHEAT), item(3, -6.5, 1.0, -4.5, Items.STICK)));

        // explore: chunks load around the player as it goes
        ExecWorld explore = platform(-80, 160);
        explore.loaded.clear();
        for (int x = -2; x <= 2; x++) {
            for (int z = -2; z <= 2; z++) {
                explore.loaded.add(ChunkPos.pack(x, z));
            }
        }
        list.add(new Scenario("explore", explore, start, null, pickaxe, 0, processConfig(), 500)
                .process("type", "explore", "x", 0, "z", 0)
                .loadRadius(2));

        // farm: ripe and unripe crops, open farmland, a jungle log for cocoa, sugar cane and an
        // item to pick up
        ExecWorld farm = flat();
        for (int x = 2; x <= 5; x++) {
            farm.set(x, 0, 2, Blocks.FARMLAND.defaultBlockState());
            farm.set(x, 0, 3, Blocks.FARMLAND.defaultBlockState());
        }
        farm.set(2, 1, 2, Blocks.WHEAT.defaultBlockState().setValue(CropBlock.AGE, 7));
        farm.set(3, 1, 2, Blocks.WHEAT.defaultBlockState().setValue(CropBlock.AGE, 2));
        farm.set(4, 1, 2, Blocks.CARROTS.defaultBlockState().setValue(CropBlock.AGE, 7));
        farm.set(2, 1, 3, Blocks.BEETROOTS.defaultBlockState().setValue(BeetrootBlock.AGE, 3));
        farm.set(-4, 1, 4, Blocks.JUNGLE_LOG.defaultBlockState());
        farm.set(-2, 0, -4, Blocks.SAND.defaultBlockState());
        farm.set(-2, 1, -4, Blocks.SUGAR_CANE.defaultBlockState());
        farm.set(-2, 2, -4, Blocks.SUGAR_CANE.defaultBlockState());
        ItemStack[] farming = basicItems();
        farming[2] = stack(Items.WHEAT_SEEDS, 8);
        farming[3] = stack(Items.BONE_MEAL, 8);
        farming[4] = stack(Items.COCOA_BEANS, 4);
        list.add(new Scenario("farm", farm, start, null, farming, 0, processConfig(), 600)
                .process("type", "farm", "range", 0)
                .entities(item(4, -5.5, 1.0, -1.5, Items.WHEAT)));

        // farm: a grown bamboo stalk (its top done growing) with its base in reach and the block
        // above not: bone meal would not grow the stalk, so the player walks up and harvests it
        ExecWorld bamboo = flat();
        BlockState stalk = Blocks.BAMBOO.defaultBlockState().setValue(BambooStalkBlock.AGE, 1);
        bamboo.set(4, 1, 2, stalk);
        bamboo.set(4, 2, 2, stalk);
        bamboo.set(4, 3, 2, stalk.setValue(BambooStalkBlock.STAGE, BambooStalkBlock.STAGE_DONE_GROWING));
        ItemStack[] boneMeal = basicItems();
        boneMeal[3] = stack(Items.BONE_MEAL, 8);
        list.add(new Scenario("farm_grown_bamboo", bamboo, start, null, boneMeal, 0, processConfig(), 150)
                .process("type", "farm", "range", 0));

        // backfill: the wall dug through is filled in again behind the player
        ExecWorld wall = flat();
        wall.fill(5, 1, -48, 5, 4, 47, Blocks.STONE.defaultBlockState());
        list.add(new Scenario("backfill", wall, start, new BlockPos(9, 1, 0), basicItems(), 0,
                processConfig("backfill", true), 400));
        return list;
    }

    static void clearPlants(ExecWorld w, int x, int z) {
        for (int y = w.minY; y < w.minY + w.height; y++) {
            if (w.get(x, y, z).is(Blocks.POPPY) || w.get(x, y, z).is(Blocks.SHORT_GRASS)) {
                w.set(x, y, z, AIR);
            }
        }
    }

    private static JsonArray scenarios() throws Exception {
        JsonArray out = new JsonArray();
        long seed = 0x1234;
        for (Scenario s : scenarioList()) {
            JsonObject o = new JsonObject();
            o.addProperty("name", s.name);
            o.add("world", s.world.json());
            o.add("start", bits(s.start));
            if (s.goal != null) {
                o.add("goal", blockPos(s.goal));
            }
            JsonArray items = new JsonArray();
            for (ItemStack item : s.items) {
                items.add(PathRefGen.itemJson(item));
            }
            JsonObject inventory = new JsonObject();
            inventory.add("items", items);
            inventory.addProperty("selected", s.selected);
            o.add("inventory", inventory);
            o.add("config", s.config);
            o.addProperty("seed", seed);
            JsonArray events = new JsonArray();
            for (Event e : s.events) {
                events.add(e.json());
            }
            o.add("events", events);
            if (s.process != null) {
                o.add("process", s.process);
            }
            o.add("entities", entitiesJson(s.entities));
            if (s.loadRadius >= 0) {
                o.addProperty("load_radius", s.loadRadius);
            }

            BlockRefGen.apply(s.config);
            ExecWorld world = s.world.copy();
            Sim sim = new Sim(world, s.start, s.items, s.selected, seed);
            sim.loadRadius = s.loadRadius;
            sim.setEntities(s.entities);
            start(sim, s);
            JsonArray ticks = new JsonArray();
            for (int t = 0; t < s.ticks; t++) {
                ticks.add(sim.tick());
                for (Event e : s.events) {
                    if (e.tick == t) {
                        e.apply(sim);
                    }
                }
            }
            o.add("ticks", ticks);
            out.add(o);
            seed++;
        }
        BaritoneAPI.provider = null;
        return out;
    }

    // endregion

    // region geometry

    /**
     * A box of random blocks, dense around shapes that matter to raytraces (offset plants,
     * interaction shapes, partial blocks), with a floor.
     */
    static ExecWorld jumble() {
        List<BlockState> all = new ArrayList<>();
        Block.BLOCK_STATE_REGISTRY.forEach(all::add);
        List<BlockState> picks = new ArrayList<>();
        for (Block b : new Block[]{Blocks.POPPY, Blocks.BAMBOO, Blocks.POINTED_DRIPSTONE, Blocks.HOPPER, Blocks.CAULDRON,
                Blocks.WATER_CAULDRON, Blocks.COMPOSTER, Blocks.SCAFFOLDING, Blocks.OAK_STAIRS, Blocks.OAK_SLAB, Blocks.OAK_FENCE,
                Blocks.OAK_DOOR, Blocks.LADDER, Blocks.SNOW, Blocks.FIRE, Blocks.SHORT_GRASS, Blocks.TALL_GRASS, Blocks.GLASS,
                Blocks.STONE, Blocks.CHEST, Blocks.WATER, Blocks.LAVA, Blocks.OAK_LEAVES, Blocks.COBWEB, Blocks.FLOWER_POT}) {
            picks.addAll(b.getStateDefinition().getPossibleStates());
        }
        ExecWorld w = new ExecWorld(-16, -16, 32, 32, -16, 32);
        w.fill(-16, -16, -16, 15, -12, 15, Blocks.STONE.defaultBlockState());
        for (int x = -16; x < 16; x++) {
            for (int y = -11; y < 16; y++) {
                for (int z = -16; z < 16; z++) {
                    double p = R.nextDouble();
                    if (p < 0.12) {
                        w.set(x, y, z, picks.get(R.nextInt(picks.size())));
                    } else if (p < 0.16) {
                        w.set(x, y, z, all.get(R.nextInt(all.size())));
                    }
                }
            }
        }
        return w;
    }

    private static JsonObject geometry() throws Exception {
        JsonObject out = new JsonObject();
        ExecWorld w = jumble();
        out.add("world", w.json());
        Sim sim = new Sim(w, new Vec3(0.5, 0.0, 0.5), basicItems(), 0, 42);
        FakeLevel level = sim.level;
        FakePlayer player = sim.player;

        // Level.clip through RayTraceUtils, from random eyes in random directions
        JsonArray clips = new JsonArray();
        for (int i = 0; i < 4000; i++) {
            Vec3 pos = new Vec3(R.nextDouble() * 28 - 14, R.nextDouble() * 24 - 10, R.nextDouble() * 28 - 14);
            placePlayer(player, pos, R.nextInt(8) == 0);
            Rotation rotation = new Rotation(R.nextFloat() * 720 - 360, R.nextFloat() * 180 - 90);
            double reach = R.nextInt(4) == 0 ? 20.0 : 4.5;
            boolean sneak = R.nextInt(4) == 0;
            HitResult hit = RayTraceUtils.rayTraceTowards(player, rotation, reach, sneak);
            JsonObject c = new JsonObject();
            c.add("position", bits(pos));
            c.add("old", bits(new Vec3(player.xo, player.yo, player.zo)));
            c.addProperty("eye_height", Float.floatToRawIntBits(player.getEyeHeight()));
            c.addProperty("yaw", Float.floatToRawIntBits(rotation.getYaw()));
            c.addProperty("pitch", Float.floatToRawIntBits(rotation.getPitch()));
            c.addProperty("reach", Double.doubleToRawLongBits(reach));
            c.addProperty("sneak", sneak);
            c.add("hit", hitJson((BlockHitResult) hit));
            clips.add(c);
        }
        out.add("clips", clips);

        // VecUtils.calculateBlockCenter everywhere
        JsonArray centers = new JsonArray();
        for (int x = -16; x < 16; x++) {
            for (int y = -12; y < 16; y++) {
                for (int z = -16; z < 16; z++) {
                    centers.add(bits(VecUtils.calculateBlockCenter(level, new BlockPos(x, y, z))));
                }
            }
        }
        out.add("block_centers", centers);

        // RotationUtils.reachable with the look behavior's aim processor, playerFeet and pathStart
        JsonArray reach = new JsonArray();
        for (int i = 0; i < 1500; i++) {
            Vec3 pos = new Vec3(R.nextInt(24) - 12 + R.nextDouble(), R.nextInt(20) - 8 + (R.nextBoolean() ? 0.0 : R.nextDouble()),
                    R.nextInt(24) - 12 + R.nextDouble());
            placePlayer(player, pos, false);
            BlockRefGen.setField(Entity.class, player, "onGround", R.nextBoolean());
            player.setYRot(R.nextFloat() * 360 - 180);
            player.setXRot(R.nextFloat() * 180 - 90);
            sim.baritone.getLookBehavior().onTick(new TickEvent(EventState.PRE, TickEvent.Type.IN, i)); // advances the aim processor
            BlockPos target = BlockPos.containing(pos).offset(R.nextInt(9) - 4, R.nextInt(7) - 2, R.nextInt(9) - 4);
            boolean sneak = R.nextInt(3) == 0;
            double distance = R.nextInt(4) == 0 ? 20.0 : 4.5;
            Optional<Rotation> r = RotationUtils.reachable(sim.baritone.getPlayerContext(), target, distance, sneak);
            JsonObject o = new JsonObject();
            o.addProperty("tick", i);
            o.add("position", bits(pos));
            o.add("old", bits(new Vec3(player.xo, player.yo, player.zo)));
            o.addProperty("on_ground", player.onGround());
            o.addProperty("yaw", Float.floatToRawIntBits(player.getYRot()));
            o.addProperty("pitch", Float.floatToRawIntBits(player.getXRot()));
            JsonArray t = new JsonArray();
            t.add(target.getX());
            t.add(target.getY());
            t.add(target.getZ());
            o.add("target", t);
            o.addProperty("sneak", sneak);
            o.addProperty("distance", Double.doubleToRawLongBits(distance));
            if (r.isPresent()) {
                JsonArray rot = new JsonArray();
                rot.add(Float.floatToRawIntBits(r.get().getYaw()));
                rot.add(Float.floatToRawIntBits(r.get().getPitch()));
                o.add("reachable", rot);
            }
            BetterBlockPos feet = sim.baritone.getPlayerContext().playerFeet();
            o.add("feet", blockPos(feet));
            o.add("path_start", blockPos(sim.pathing.pathStart()));
            reach.add(o);
        }
        out.add("reachable", reach);
        BaritoneAPI.provider = null;
        return out;
    }

    static JsonArray blockPos(BlockPos p) {
        JsonArray a = new JsonArray();
        a.add(p.getX());
        a.add(p.getY());
        a.add(p.getZ());
        return a;
    }

    static void placePlayer(FakePlayer player, Vec3 pos, boolean crouching) throws ReflectiveOperationException {
        setPosition(player, pos);
        player.xo = pos.x - 0.1 + R.nextDouble() * 0.2;
        player.yo = pos.y - 0.1 + R.nextDouble() * 0.2;
        player.zo = pos.z - 0.1 + R.nextDouble() * 0.2;
        BlockRefGen.setField(Entity.class, player, "eyeHeight", crouching ? 1.27F : 1.62F);
        BlockRefGen.setField(Entity.class, player, "bb", Sim.playerBox(pos, crouching));
    }

    static JsonObject hitJson(BlockHitResult hit) {
        JsonObject o = new JsonObject();
        o.addProperty("miss", hit.getType() == HitResult.Type.MISS);
        o.add("pos", blockPos(hit.getBlockPos()));
        o.addProperty("direction", hit.getDirection().getName());
        o.add("location", bits(hit.getLocation()));
        o.addProperty("inside", hit.isInside());
        return o;
    }

    // endregion
}
