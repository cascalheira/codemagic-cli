//! Native desktop notifications for build outcomes.
//!
//! No-op on platforms without a desktop notification centre (iOS/Android), so
//! callers don't need their own `cfg` gates.

/// Posts a "build finished" notification. Best-effort: failures are ignored,
/// since a missing notification must never disrupt the app.
#[allow(unused_variables)]
pub fn build_finished(title: &str, body: &str) {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        let _ = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show();
    }
}
