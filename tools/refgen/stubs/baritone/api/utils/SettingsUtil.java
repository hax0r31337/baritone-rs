package baritone.api.utils;

import baritone.api.BaritoneAPI;

/**
 * Stub for refgen: only {@code maybeCensor}, copied verbatim from upstream.
 */
public class SettingsUtil {

    public static String maybeCensor(int coord) {
        if (BaritoneAPI.getSettings().censorCoordinates.value) {
            return "<censored>";
        }

        return Integer.toString(coord);
    }
}
