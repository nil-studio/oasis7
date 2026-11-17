//! Test-only bounded marker classification, not proof of the underlying failure cause.
//! A missing/filtered tracing message stays unknown; categories never echo stderr.

pub(super) fn classify_stderr(stderr: &str) -> &'static str {
    if stderr.len() > 16 * 1024 {
        return "oversize";
    }
    if stderr.is_empty() {
        return "empty";
    }
    let mut found = None;
    for (marker, category) in [
        ("public header Node report delivery failed", "delivery"),
        (
            "header Node actual qualified H driver recovery rejected",
            "driver_recovery",
        ),
        (
            "header Node actual startup recovery rejected",
            "startup_recovery",
        ),
        (
            "header Node canonical private actions rejected",
            "action_encoding",
        ),
        (
            "header Node private action admission rejected",
            "action_admission",
        ),
        (
            "header Node blocked before complete applied receipt",
            "receipt_blocked",
        ),
        (
            "header Node timed out before complete applied receipt",
            "receipt_timeout",
        ),
        ("header Node could not stop cleanly", "shutdown"),
    ] {
        if stderr.contains(marker) {
            if found.is_some() {
                return "multiple";
            }
            found = Some(category);
        }
    }
    found.unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_markers_emit_only_static_categories() {
        assert_eq!(
            classify_stderr("header Node actual startup recovery rejected"),
            "startup_recovery"
        );
        assert_eq!(
            classify_stderr("prefix header Node could not stop cleanly private-input"),
            "shutdown"
        );
        assert_eq!(
            classify_stderr("public header Node report delivery failed"),
            "delivery"
        );
    }

    #[test]
    fn absent_ambiguous_and_oversize_evidence_stays_distinct() {
        assert_eq!(classify_stderr(""), "empty");
        assert_eq!(classify_stderr("private path/token/error"), "unknown");
        assert_eq!(
            classify_stderr("header Node could not stop cleanly; header Node private action admission rejected"),
            "multiple"
        );
        assert_eq!(classify_stderr(&"x".repeat(16 * 1024 + 1)), "oversize");
    }
}
