package refgen;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.io.Writer;
import java.net.ServerSocket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.function.Predicate;
import java.util.stream.Stream;

/**
 * Generates a world with the vanilla dedicated server: starts it on a fresh world with the
 * given seed, force-loads a square of chunks in every dimension {@link Bench} uses (which
 * generates them completely: terrain, carvers, features, structures), saves and stops it. The
 * region files are then read by {@link RegionWorld}.
 * <p>
 * The server does not start unless its EULA is agreed to, so this writes {@code eula=true}
 * only when the environment says {@code MINECRAFT_EULA=true}.
 */
final class ServerWorldGen {

    /** Dimensions generated for every seed. */
    static final String[] DIMENSIONS = {"overworld", "the_nether"};

    /** {@code /forceload add} takes at most this many chunks. */
    private static final int FORCELOAD_LIMIT = 256;

    private static final long TIMEOUT_SECONDS = 600;

    private ServerWorldGen() {}

    /**
     * The server directory for {@code seed}, generated (chunks {@code cmin..cmax} on both axes)
     * unless a previous run already did.
     */
    static Path generate(Path serverJar, Path dir, long seed, int cmin, int cmax) throws Exception {
        Path done = dir.resolve("done");
        String key = seed + " " + cmin + " " + cmax + "\n";
        if (Files.exists(done) && Files.readString(done).equals(key)) {
            return dir;
        }
        if (!"true".equals(System.getenv("MINECRAFT_EULA"))) {
            throw new IllegalStateException("generating worlds runs the Minecraft server, which needs you to agree to the "
                    + "Minecraft EULA (https://aka.ms/MinecraftEULA): set MINECRAFT_EULA=true");
        }
        deleteRecursively(dir);
        Files.createDirectories(dir);
        Files.writeString(dir.resolve("eula.txt"), "# agreed to through MINECRAFT_EULA=true\neula=true\n");
        int port;
        try (ServerSocket socket = new ServerSocket(0)) {
            port = socket.getLocalPort();
        }
        Files.writeString(dir.resolve("server.properties"), String.join("\n",
                "level-seed=" + seed,
                "online-mode=false",
                "server-ip=127.0.0.1",
                "server-port=" + port,
                "enable-query=false",
                "enable-rcon=false",
                "spawn-protection=0",
                "view-distance=2",
                "simulation-distance=2",
                "sync-chunk-writes=false",
                "max-tick-time=-1",
                // a paused server runs no commands
                "pause-when-empty-seconds=0",
                ""));

        String java = ProcessHandle.current().info().command().orElse("java");
        Process server = new ProcessBuilder(java, "-Xmx4G", "-jar", serverJar.toAbsolutePath().toString(), "--nogui")
                .directory(dir.toFile())
                .redirectErrorStream(true)
                .start();
        BlockingQueue<String> lines = new LinkedBlockingQueue<>();
        Thread reader = new Thread(() -> {
            try (BufferedReader in = new BufferedReader(new InputStreamReader(server.getInputStream(), StandardCharsets.UTF_8));
                 Writer log = Files.newBufferedWriter(dir.resolve("server.log"))) {
                String line;
                while ((line = in.readLine()) != null) {
                    log.write(line);
                    log.write('\n');
                    log.flush();
                    lines.add(line);
                }
            } catch (IOException e) {
                lines.add("reader failed: " + e);
            }
        }, "server output");
        reader.setDaemon(true);
        reader.start();
        PrintWriter console = new PrintWriter(server.getOutputStream(), true, StandardCharsets.UTF_8);
        try {
            await(lines, server, l -> l.contains("Done ("));
            int width = cmax - cmin + 1;
            int rows = Math.max(1, FORCELOAD_LIMIT / width);
            for (String dimension : DIMENSIONS) {
                for (int z = cmin; z <= cmax; z += rows) {
                    int z1 = Math.min(z + rows - 1, cmax);
                    console.println("execute in minecraft:" + dimension + " run forceload add "
                            + (cmin << 4) + " " + (z << 4) + " " + ((cmax << 4) + 15) + " " + ((z1 << 4) + 15));
                    await(lines, server, l -> l.contains("to be force loaded") || l.contains("No chunks were marked"));
                }
            }
            // One test per chunk: only the last condition of an execute chain reports, a failing
            // one before it ends the command silently. The console runs commands in order, so
            // the answers come in the order of the tests.
            List<String> pending = new ArrayList<>();
            for (String dimension : DIMENSIONS) {
                for (int cx = cmin; cx <= cmax; cx++) {
                    for (int cz = cmin; cz <= cmax; cz++) {
                        pending.add("execute in minecraft:" + dimension + " if loaded " + (cx << 4) + " 0 " + (cz << 4));
                    }
                }
            }
            while (!pending.isEmpty()) {
                pending.forEach(console::println);
                List<String> failed = new ArrayList<>();
                for (String test : pending) {
                    if (await(lines, server, l -> l.contains("Test passed") || l.contains("Test failed")).contains("Test failed")) {
                        failed.add(test);
                    }
                }
                pending = failed;
                if (!pending.isEmpty()) {
                    Thread.sleep(1000);
                }
            }
            console.println("save-all flush");
            await(lines, server, l -> l.contains("Saved the game"));
            console.println("stop");
            if (!server.waitFor(TIMEOUT_SECONDS, TimeUnit.SECONDS)) {
                throw new IllegalStateException("the server did not stop");
            }
        } finally {
            if (server.isAlive()) {
                server.destroyForcibly();
            }
        }
        if (server.exitValue() != 0) {
            throw new IllegalStateException("the server exited with " + server.exitValue() + ", see " + dir.resolve("server.log"));
        }
        Files.writeString(done, key);
        return dir;
    }

    /** The region directory of {@code dimension} in a generated world. */
    static Path regionDir(Path dir, String dimension) {
        return dir.resolve("world/dimensions/minecraft").resolve(dimension).resolve("region");
    }

    private static String await(BlockingQueue<String> lines, Process server, Predicate<String> match) throws Exception {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(TIMEOUT_SECONDS);
        while (System.nanoTime() < deadline) {
            String line = lines.poll(1, TimeUnit.SECONDS);
            if (line != null && match.test(line)) {
                return line;
            }
            if (line == null && !server.isAlive()) {
                throw new IllegalStateException("the server exited with " + server.exitValue());
            }
        }
        throw new IllegalStateException("timed out waiting for the server");
    }

    private static void deleteRecursively(Path dir) throws IOException {
        if (!Files.exists(dir)) {
            return;
        }
        try (Stream<Path> paths = Files.walk(dir)) {
            for (Path p : paths.sorted(Comparator.reverseOrder()).toList()) {
                Files.delete(p);
            }
        }
    }
}
