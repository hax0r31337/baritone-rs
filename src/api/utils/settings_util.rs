// Ported from baritone src/api/java/baritone/api/utils/SettingsUtil.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Only `maybeCensor` is ported; reading/writing settings files is replaced by serde on
// `Settings`.

use crate::settings::settings;

pub fn maybe_censor(coord: i32) -> String {
    if settings().censor_coordinates {
        return "<censored>".to_owned();
    }

    coord.to_string()
}
