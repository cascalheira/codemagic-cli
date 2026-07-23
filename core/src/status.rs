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

/// Whether a finished build or step succeeded. `None` for statuses that
/// aren't a success/failure outcome (still running, skipped, cancelled).
///
/// Builds and steps use different words for the same thing — a build is
/// `finished`, a step is `success` — so both vocabularies are accepted.
pub fn outcome(status: &str) -> Option<bool> {
    match status {
        "finished" | "success" => Some(true),
        "failed" | "failure" | "error" | "timeout" => Some(false),
        _ => None,
    }
}

/// A coarse class for colouring a build or step status.
pub fn class(status: &str) -> &'static str {
    match outcome(status) {
        Some(true) => "ok",
        Some(false) => "fail",
        None if is_running(status) || status == "running" || status == "executing" => "run",
        None if matches!(status, "canceled" | "cancelled" | "skipped") => "cancel",
        None => "neutral",
    }
}

#[cfg(test)]
mod tests {
    use super::{class, outcome};

    /// Steps say "success" where builds say "finished". Missing that made
    /// every step pill render grey instead of green.
    #[test]
    fn step_and_build_vocabularies_agree() {
        assert_eq!(outcome("finished"), Some(true));
        assert_eq!(outcome("success"), Some(true));
        assert_eq!(class("success"), "ok");
        assert_eq!(class("finished"), "ok");
    }

    #[test]
    fn failures_are_recognised_under_either_name() {
        for s in ["failed", "failure", "error", "timeout"] {
            assert_eq!(outcome(s), Some(false), "{s}");
            assert_eq!(class(s), "fail", "{s}");
        }
    }

    #[test]
    fn in_flight_statuses_are_neither() {
        for s in ["building", "queued", "running", "executing"] {
            assert_eq!(outcome(s), None, "{s}");
            assert_eq!(class(s), "run", "{s}");
        }
    }

    #[test]
    fn cancelled_and_skipped_are_their_own_class() {
        for s in ["canceled", "cancelled", "skipped"] {
            assert_eq!(class(s), "cancel", "{s}");
        }
    }

    #[test]
    fn an_unknown_status_is_neutral_not_a_failure() {
        assert_eq!(outcome("who-knows"), None);
        assert_eq!(class("who-knows"), "neutral");
    }
}
