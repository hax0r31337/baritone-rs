package refgen;

import baritone.Baritone;
import baritone.utils.BlockStateInterface;
import baritone.utils.pathing.BetterWorldBorder;
import com.mojang.serialization.Codec;
import net.minecraft.client.multiplayer.ClientChunkCache;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.PalettedContainer;
import net.minecraft.world.level.chunk.Strategy;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.dimension.BuiltinDimensionTypes;
import net.minecraft.world.level.dimension.DimensionType;

import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;
import java.util.zip.GZIPInputStream;
import java.util.zip.GZIPOutputStream;
import java.util.zip.InflaterInputStream;

/**
 * A square of chunks read from the region files the server wrote, as the client would hold
 * them: real {@link LevelChunk}s whose sections are real {@link LevelChunkSection}s, decoded
 * with the game's own block state container codec. Upstream's real
 * {@link BlockStateInterface} reads them through a stand-in chunk cache ({@link #bsi}).
 */
final class RegionWorld {

    private static final BlockState AIR = Blocks.AIR.defaultBlockState();

    private static final Codec<PalettedContainer<BlockState>> BLOCK_STATES =
            PalettedContainer.codecRW(BlockState.CODEC, Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY), AIR);

    final String dimension;
    final boolean nether;
    final int minY;
    final int height;
    /** Loaded chunks are cmin..cmax on both axes. */
    final int cmin;
    final int cmax;
    /** [cx - cmin][cz - cmin] */
    final LevelChunk[][] chunks;
    final ClientLevel level;

    private RegionWorld(String dimension, DimensionType type, int cmin, int cmax) throws Exception {
        this.dimension = dimension;
        this.nether = dimension.equals("the_nether");
        this.minY = type.minY();
        this.height = type.height();
        this.cmin = cmin;
        this.cmax = cmax;
        this.chunks = new LevelChunk[cmax - cmin + 1][cmax - cmin + 1];
        this.level = BlockRefGen.allocate(ClientLevel.class);
        BlockRefGen.setField(Level.class, level, "dimensionTypeRegistration", Holder.direct(type));
        BlockRefGen.setField(Level.class, level, "dimension", nether ? Level.NETHER : Level.OVERWORLD);
        BlockRefGen.setField(ClientLevel.class, level, "worldBorder", new WorldBorder());
    }

    /**
     * Reads chunks {@code cmin..cmax} of {@code dimension} from {@code regionDir}. Every one must
     * have been generated completely.
     */
    static RegionWorld load(Path regionDir, String dimension, int cmin, int cmax) throws Exception {
        ResourceKey<DimensionType> typeKey = switch (dimension) {
            case "overworld" -> BuiltinDimensionTypes.OVERWORLD;
            case "the_nether" -> BuiltinDimensionTypes.NETHER;
            default -> throw new IllegalArgumentException(dimension);
        };
        DimensionType type = BlockRefGen.LOOKUP.lookupOrThrow(Registries.DIMENSION_TYPE).getOrThrow(typeKey).value();
        RegionWorld world = new RegionWorld(dimension, type, cmin, cmax);
        Map<Long, byte[]> regions = new LinkedHashMap<>();
        for (int cx = cmin; cx <= cmax; cx++) {
            for (int cz = cmin; cz <= cmax; cz++) {
                long key = ChunkPos.pack(cx >> 5, cz >> 5);
                byte[] region = regions.get(key);
                if (region == null) {
                    region = Files.readAllBytes(regionDir.resolve("r." + (cx >> 5) + "." + (cz >> 5) + ".mca"));
                    regions.put(key, region);
                }
                CompoundTag tag = readChunk(region, cx & 31, cz & 31);
                if (tag == null) {
                    throw new IllegalStateException("chunk " + cx + ", " + cz + " of " + dimension + " was not generated");
                }
                String status = tag.getStringOr("Status", "");
                if (!status.equals("minecraft:full")) {
                    throw new IllegalStateException("chunk " + cx + ", " + cz + " of " + dimension + " is " + status);
                }
                world.chunks[cx - cmin][cz - cmin] = world.chunk(cx, cz, tag);
            }
        }
        return world;
    }

    /** One chunk's NBT from a region file (see {@code RegionFile}), or null if it has none. */
    private static CompoundTag readChunk(byte[] region, int lx, int lz) throws IOException {
        ByteBuffer buf = ByteBuffer.wrap(region);
        int entry = buf.getInt(4 * (lx + lz * 32));
        int sector = entry >>> 8;
        if (sector == 0) {
            return null;
        }
        int start = sector * 4096;
        int length = buf.getInt(start);
        byte compression = region[start + 4];
        if ((compression & 0x80) != 0) {
            throw new IOException("chunk " + lx + ", " + lz + " is stored outside its region file");
        }
        InputStream raw = new ByteArrayInputStream(region, start + 5, length - 1);
        InputStream in = switch (compression) {
            case 1 -> new GZIPInputStream(raw);
            case 2 -> new InflaterInputStream(raw);
            case 3 -> raw;
            default -> throw new IOException("unsupported region compression " + compression);
        };
        return NbtIo.read(new DataInputStream(new BufferedInputStream(in)));
    }

    private LevelChunk chunk(int cx, int cz, CompoundTag tag) throws Exception {
        LevelChunkSection[] sections = new LevelChunkSection[height >> 4];
        ListTag list = tag.getListOrEmpty("sections");
        for (int i = 0; i < list.size(); i++) {
            CompoundTag section = list.getCompoundOrEmpty(i);
            int index = section.getByteOr("Y", (byte) 0) - (minY >> 4);
            Tag states = section.get("block_states");
            if (index < 0 || index >= sections.length || states == null) {
                continue;
            }
            // biomes are never read
            sections[index] = new LevelChunkSection(BLOCK_STATES.parse(NbtOps.INSTANCE, states).getOrThrow(), null);
        }
        for (int i = 0; i < sections.length; i++) {
            if (sections[i] == null) {
                sections[i] = new LevelChunkSection(new PalettedContainer<>(AIR, Strategy.createForBlockStates(Block.BLOCK_STATE_REGISTRY)), null);
            }
        }
        LevelChunk chunk = BlockRefGen.allocate(LevelChunk.class);
        BlockRefGen.setField(ChunkAccess.class, chunk, "chunkPos", new ChunkPos(cx, cz));
        BlockRefGen.setField(ChunkAccess.class, chunk, "sections", sections);
        return chunk;
    }

    boolean isLoaded(int cx, int cz) {
        return cx >= cmin && cx <= cmax && cz >= cmin && cz <= cmax;
    }

    /** The block at a loaded position (for picking queries, not for path calculation). */
    BlockState get(int x, int y, int z) {
        if (y < minY || y >= minY + height || !isLoaded(x >> 4, z >> 4)) {
            return AIR;
        }
        return chunks[(x >> 4) - cmin][(z >> 4) - cmin].getSections()[(y - minY) >> 4].getBlockState(x & 15, y & 15, z & 15);
    }

    // region upstream's BlockStateInterface over these chunks

    /**
     * The client chunk cache: the chunks in a flat array, like {@code ClientChunkCache.Storage}
     * holds them.
     */
    static final class ChunkCache extends ClientChunkCache {
        RegionWorld world;

        @SuppressWarnings("unused")
        private ChunkCache() {
            super(null, 0);
        }

        @Override
        public LevelChunk getChunk(int x, int z, ChunkStatus targetStatus, boolean loadOrGenerate) {
            if (!world.isLoaded(x, z)) {
                return null;
            }
            return world.chunks[x - world.cmin][z - world.cmin];
        }

        @Override
        public boolean hasChunk(int x, int z) {
            return world.isLoaded(x, z);
        }
    }

    private ChunkCache cache;

    /**
     * A new upstream {@link BlockStateInterface}, with the fields its constructor sets for a
     * client without world data (so every lookup reads the loaded chunks). The constructor
     * itself needs a player context on the client thread.
     */
    BlockStateInterface bsi() throws Exception {
        if (cache == null) {
            cache = BlockRefGen.allocate(ChunkCache.class);
            cache.world = this;
        }
        BlockStateInterface bsi = BlockRefGen.allocate(BlockStateInterface.class);
        Set<String> set = new HashSet<>();
        PathRefGen.FieldSetter put = (name, value) -> {
            BlockRefGen.setField(BlockStateInterface.class, bsi, name, value);
            set.add(name);
        };
        put.set("world", level);
        put.set("worldBorder", new BetterWorldBorder(level.getWorldBorder()));
        put.set("worldData", null);
        put.set("provider", cache);
        put.set("useTheRealWorld", !Baritone.settings().pathThroughCachedOnly.value);
        put.set("isPassableBlockPos", new BlockPos.MutableBlockPos());
        Constructor<?> wrapper = Class.forName("baritone.utils.BlockStateInterfaceAccessWrapper")
                .getDeclaredConstructor(BlockStateInterface.class);
        wrapper.setAccessible(true);
        put.set("access", wrapper.newInstance(bsi));
        put.set("prev", null);
        put.set("prevCached", null);
        for (Field field : BlockStateInterface.class.getDeclaredFields()) {
            if (!Modifier.isStatic(field.getModifiers()) && !set.contains(field.getName())) {
                throw new IllegalStateException("BlockStateInterface." + field.getName() + " is not set");
            }
        }
        return bsi;
    }

    // endregion

    /**
     * The chunks for the port (see {@code examples/bench.rs}), gzipped, big-endian:
     * min y, height, cmin, cmax, then for every chunk (x outermost, then z) and section
     * (bottom first): the palette size, the palette (state ids) and, if it has more than one
     * entry, the palette index of every block in the game's order ({@code (y << 8) | (z << 4) | x}).
     */
    void export(Path out) throws IOException {
        try (DataOutputStream o = new DataOutputStream(new BufferedOutputStream(new GZIPOutputStream(Files.newOutputStream(out), 1 << 16)))) {
            o.writeInt(minY);
            o.writeInt(height);
            o.writeInt(cmin);
            o.writeInt(cmax);
            short[] indices = new short[4096];
            for (LevelChunk[] row : chunks) {
                for (LevelChunk chunk : row) {
                    for (LevelChunkSection section : chunk.getSections()) {
                        Map<Integer, Integer> palette = new LinkedHashMap<>();
                        for (int i = 0; i < 4096; i++) {
                            BlockState state = section.getBlockState(i & 15, i >> 8, (i >> 4) & 15);
                            int id = Block.BLOCK_STATE_REGISTRY.getId(state);
                            Integer index = palette.get(id);
                            if (index == null) {
                                index = palette.size();
                                palette.put(id, index);
                            }
                            indices[i] = (short) (int) index;
                        }
                        o.writeShort(palette.size());
                        for (int id : palette.keySet()) {
                            o.writeInt(id);
                        }
                        if (palette.size() > 1) {
                            for (short index : indices) {
                                o.writeShort(index);
                            }
                        }
                    }
                }
            }
        }
    }
}
