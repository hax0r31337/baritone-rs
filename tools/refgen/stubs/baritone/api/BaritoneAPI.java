package baritone.api;

/**
 * Stub for refgen: the real BaritoneAPI boots the whole client (settings file, provider).
 * Upstream code under test only needs {@link #getSettings()}.
 */
public final class BaritoneAPI {

    private static final Settings settings = new Settings();

    public static Settings getSettings() {
        return BaritoneAPI.settings;
    }
}
