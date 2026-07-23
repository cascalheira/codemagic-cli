//! Interpretation of Codemagic build status strings.

/// Returns `true` for any status meaning the build is still in progress.
pub fn is_running(status: &str) -> bool {
    matches!(
        status,
        "building"
            | "queued"
            | "preparing"
            | "fetching"
            | "initializing"
            | "testing"
            | "publishing"
            | "finishing"
    )
}

/// Whether a build in this status can still be cancelled, i.e. it hasn't
/// reached a terminal state.
pub fn is_cancellable(status: &str) -> bool {
    !matches!(
        status,
        "finished" | "failed" | "canceled" | "timeout" | "skipped"
    )
}

/// Whether a finished build succeeded. `None` for statuses that aren't a
/// success/failure outcome (still running, skipped, cancelled).
pub fn outcome(status: &str) -> Option<bool> {
    match status {
        "finished" => Some(true),
        "failed" | "timeout" => Some(false),
        _ => None,
    }
}
