//! Public transport filtering, applied only to the disposable outbound snapshot.
use crate::runtime::{Action, Snapshot};
use serde_json::Value;

fn contains_canonical_chat(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.get("kind").and_then(Value::as_str) == Some("agent_chat")
                || object.values().any(contains_canonical_chat)
        }
        Value::Array(values) => values.iter().any(contains_canonical_chat),
        _ => false,
    }
}

pub(super) fn omit_private_canonical_chat(snapshot: &mut Snapshot) {
    snapshot
        .capability_revocation_state
        .world_service_results
        .retain(|_, record| !contains_canonical_chat(record));
    snapshot.pending_actions.retain(|envelope| {
        !matches!(&envelope.action, Action::WorldServiceIntent { request }
            if contains_canonical_chat(request))
    });
    snapshot
        .pending_effects
        .retain(|effect| !contains_canonical_chat(&effect.params));
    snapshot
        .inflight_effects
        .retain(|_, effect| !contains_canonical_chat(&effect.params));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_chat_detection_covers_nested_input_and_preserves_other_kinds() {
        assert!(contains_canonical_chat(
            &serde_json::json!({"request":{"signed_payload":{"kind":"agent_chat", "payload":{"message":"private"}}}})
        ));
        assert!(contains_canonical_chat(
            &serde_json::json!([{"kind":"agent_chat"}])
        ));
        assert!(!contains_canonical_chat(
            &serde_json::json!({"kind":"cognition_proposal", "message":"agent_chat"})
        ));
    }
}
