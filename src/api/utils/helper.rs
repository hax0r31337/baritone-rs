// Ported from baritone src/api/java/baritone/api/utils/Helper.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// Only the logging methods. Upstream prints to chat, toasts and desktop notifications; the
// port has no UI, so messages go to the `log` crate (target `baritone`) and the host decides
// what to show. Chat messages are `info`, notifications `warn` (`error` when upstream flags
// them as errors), and upstream's `System.out.println` debugging output is `debug`.

use crate::settings::settings;

/// `logDebug(String)`: Send a message to chat only if chatDebug is on
pub fn log_debug(message: &str) {
    if !settings().chat_debug {
        //System.out.println("Suppressed debug message:");
        //System.out.println(message);
        return;
    }
    // We won't log debug chat into toasts
    // Because only a madman would want that extreme spam -_-
    log_direct(message);
}

/// `logDirect(String)`: Send a message to chat regardless of chatDebug (should only be used
/// for critically important messages, or as a direct response to a chat command)
pub fn log_direct(message: &str) {
    log::info!(target: "baritone", "{message}");
}

/// `logNotification(String, boolean)`: Send a message as a desktop notification
pub fn log_notification(message: &str, error: bool) {
    if settings().desktop_notifications {
        log_notification_direct(message, error);
    }
}

/// `logNotificationDirect(String, boolean)`
pub fn log_notification_direct(message: &str, error: bool) {
    if error {
        log::error!(target: "baritone", "{message}");
    } else {
        log::warn!(target: "baritone", "{message}");
    }
}

/// `System.out.println(String)` in upstream code.
pub fn println(message: &str) {
    log::debug!(target: "baritone", "{message}");
}
