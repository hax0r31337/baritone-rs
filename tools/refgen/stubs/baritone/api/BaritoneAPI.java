package baritone.api;

/**
 * Stub for refgen: the real BaritoneAPI reads the settings file and boots the Baritone
 * provider. Upstream code under test needs {@link #getSettings()}, which returns the real
 * {@link Settings} with upstream defaults (needs a bootstrapped registry), and execution code
 * asks {@link #getProvider()} for the Baritone of the player, which is whatever the harness put
 * in {@link #provider}.
 */
public final class BaritoneAPI {

    private static final Settings settings = new Settings();

    /**
     * Set by ExecRefGen to the provider of its one Baritone.
     */
    public static IBaritoneProvider provider;

    public static IBaritoneProvider getProvider() {
        if (provider == null) {
            throw new UnsupportedOperationException("refgen has no Baritone provider");
        }
        return provider;
    }

    public static Settings getSettings() {
        return BaritoneAPI.settings;
    }
}
