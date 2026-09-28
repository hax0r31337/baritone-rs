package baritone.api;

/**
 * Stub for refgen: the real BaritoneAPI reads the settings file and boots the Baritone
 * provider. Upstream code under test only needs {@link #getSettings()}, which returns the real
 * {@link Settings} with upstream defaults (needs a bootstrapped registry).
 */
public final class BaritoneAPI {

    private static final Settings settings = new Settings();

    public static IBaritoneProvider getProvider() {
        throw new UnsupportedOperationException("refgen has no Baritone provider");
    }

    public static Settings getSettings() {
        return BaritoneAPI.settings;
    }
}
