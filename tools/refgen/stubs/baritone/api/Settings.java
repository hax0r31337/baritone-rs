package baritone.api;

/**
 * Stub for refgen: only the settings read by the upstream classes compiled into refgen,
 * with upstream default values. The real Settings initializes Blocks/Items, which needs a
 * bootstrapped registry.
 */
public final class Settings {

    public final Setting<Double> costHeuristic = new Setting<>(3.563);

    public final Setting<Integer> axisHeight = new Setting<>(120);

    public final Setting<Boolean> censorCoordinates = new Setting<>(false);

    public final class Setting<T> {

        public T value;
        public final T defaultValue;

        private Setting(T value) {
            this.value = value;
            this.defaultValue = value;
        }
    }
}
