use oasis7_node::NodeSnapshot;

use super::{ChainNodeObservabilityAlert, push_observability_alert};

pub(super) fn push_runtime_error_alerts(
    alerts: &mut Vec<ChainNodeObservabilityAlert>,
    snapshot: &NodeSnapshot,
) {
    if let Some(error) = snapshot.consensus_progress_observer_error.as_ref() {
        if error.code.as_deref() == Some("state_persist_failed") {
            push_observability_alert(
                alerts,
                "critical",
                "state_persist_failed",
                format!("publication lifecycle state persistence failed: {error}"),
            );
        }
        push_observability_alert(
            alerts,
            "critical",
            "consensus_progress_observer_error",
            format!("consensus progress observer error is set: {error}"),
        );
    }
    if let Some(error) = snapshot.last_error.as_ref() {
        push_observability_alert(
            alerts,
            "critical",
            "runtime_last_error",
            format!("runtime last_error is set: {error}"),
        );
    }
}
