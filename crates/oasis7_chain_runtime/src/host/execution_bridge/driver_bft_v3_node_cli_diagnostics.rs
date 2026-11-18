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
            let category = if category == "driver_recovery" {
                match stderr
                    .split(
                        "header Node actual qualified H driver recovery rejected; recovery_stage=",
                    )
                    .nth(1)
                    .and_then(|tail| tail.split_whitespace().next())
                {
                    Some("storage_paths") => "driver_recovery_storage_paths",
                    Some("private_store") => "driver_recovery_private_store",
                    Some("ledger") => "driver_recovery_ledger",
                    Some("ledger_handoff") => "driver_recovery_ledger_handoff",
                    Some("ledger_open") => "driver_recovery_ledger_open",
                    Some("ledger_preload") => "driver_recovery_ledger_preload",
                    Some("ledger_replay") => "driver_recovery_ledger_replay",
                    Some("ledger_origin") => "driver_recovery_ledger_origin",
                    Some("h_index") => "driver_recovery_h_index",
                    Some("bind_origin") => "driver_recovery_bind_origin",
                    _ => category,
                }
            } else {
                category
            };
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
    fn recovery_stage_is_visible_without_echoing_private_details() {
        assert_eq!(
            classify_stderr(
                "unrelated recovery_stage=private_store\nheader Node actual qualified H driver recovery rejected; recovery_stage=ledger\n"
            ),
            "driver_recovery_ledger"
        );
        for (stage, expected) in [
            ("storage_paths", "driver_recovery_storage_paths"),
            ("private_store", "driver_recovery_private_store"),
            ("ledger", "driver_recovery_ledger"),
            ("ledger_handoff", "driver_recovery_ledger_handoff"),
            ("ledger_open", "driver_recovery_ledger_open"),
            ("ledger_preload", "driver_recovery_ledger_preload"),
            ("ledger_replay", "driver_recovery_ledger_replay"),
            ("ledger_origin", "driver_recovery_ledger_origin"),
            ("h_index", "driver_recovery_h_index"),
            ("bind_origin", "driver_recovery_bind_origin"),
            ("foreign", "driver_recovery"),
        ] {
            assert_eq!(
                classify_stderr(&format!(
                    "header Node actual qualified H driver recovery rejected; recovery_stage={stage}\n"
                )),
                expected
            );
        }
    }

    #[test]
    fn absent_ambiguous_and_oversize_evidence_stays_distinct() {
        assert_eq!(classify_stderr(""), "empty");
        assert_eq!(classify_stderr("private path/token/error"), "unknown");
        assert_eq!(
            classify_stderr(
                "header Node could not stop cleanly; header Node private action admission rejected"
            ),
            "multiple"
        );
        assert_eq!(classify_stderr(&"x".repeat(16 * 1024 + 1)), "oversize");
    }
}
