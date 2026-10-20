//! Public transport filtering, applied only to the disposable outbound snapshot.
use crate::runtime::{Action, Snapshot};
use serde_json::Value;

pub(super) fn contains_private_canonical_input(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            matches!(
                object.get("kind").and_then(Value::as_str),
                Some("agent_chat" | "safe_alternative")
            ) || object.get("type").and_then(Value::as_str)
                == Some("SafeAlternativeCommandRecorded")
                || object.values().any(contains_private_canonical_input)
        }
        Value::Array(values) => values.iter().any(contains_private_canonical_input),
        _ => false,
    }
}

pub(super) fn omit_private_canonical_chat(snapshot: &mut Snapshot) {
    snapshot
        .capability_revocation_state
        .safe_alternatives
        .clear();
    snapshot
        .capability_revocation_state
        .world_service_results
        .retain(|_, record| !contains_private_canonical_input(record));
    snapshot.pending_actions.retain(|envelope| {
        !matches!(&envelope.action, Action::WorldServiceIntent { request }
            if contains_private_canonical_input(request))
    });
    snapshot
        .pending_effects
        .retain(|effect| !contains_private_canonical_input(&effect.params));
    snapshot
        .inflight_effects
        .retain(|_, effect| !contains_private_canonical_input(&effect.params));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_chat_detection_covers_nested_input_and_preserves_other_kinds() {
        assert!(contains_private_canonical_input(
            &serde_json::json!({"request":{"signed_payload":{"kind":"agent_chat", "payload":{"message":"private"}}}})
        ));
        assert!(contains_private_canonical_input(
            &serde_json::json!([{"kind":"agent_chat"}])
        ));
        assert!(!contains_private_canonical_input(
            &serde_json::json!({"kind":"cognition_proposal", "message":"agent_chat"})
        ));
    }
    #[test]
    fn safe_alternative_public_snapshot_drops_private_lineages_results_and_pending_commands() {
        let world =
            crate::runtime_host::world::new_with_state(crate::runtime::WorldState::default());
        let mut snapshot = world.snapshot();
        let private = serde_json::json!({"request":{"signed_payload":{"kind":"safe_alternative","payload":{"goal":"PRIVATE_ALTERNATIVE_GOAL","signature_hex":"PRIVATE_SIGNATURE"}}}});
        snapshot
            .capability_revocation_state
            .safe_alternatives
            .insert(
                "lineage".into(),
                serde_json::json!({"original":{"goal":"PRIVATE_ALTERNATIVE_GOAL"}}),
            );
        snapshot
            .capability_revocation_state
            .world_service_results
            .insert("private".into(), private.clone());
        snapshot
            .capability_revocation_state
            .world_service_results
            .insert(
                "public".into(),
                serde_json::json!({"kind":"cognition","receipt":"public"}),
            );
        snapshot
            .pending_actions
            .push(crate::runtime::ActionEnvelope {
                id: 1,
                committed_recipe_origin: None,
                action: Action::WorldServiceIntent { request: private },
            });
        let authority_original = snapshot.clone();
        omit_private_canonical_chat(&mut snapshot);
        let public = serde_json::to_string(&snapshot).unwrap();
        assert!(!public.contains("PRIVATE_ALTERNATIVE_GOAL"));
        assert!(!public.contains("PRIVATE_SIGNATURE"));
        assert!(
            snapshot
                .capability_revocation_state
                .safe_alternatives
                .is_empty()
        );
        assert!(
            snapshot
                .capability_revocation_state
                .world_service_results
                .contains_key("public")
        );
        assert!(
            serde_json::to_string(&authority_original)
                .unwrap()
                .contains("PRIVATE_ALTERNATIVE_GOAL")
        );
        assert!(contains_private_canonical_input(
            &serde_json::json!({"nested":[{"type":"SafeAlternativeCommandRecorded","data":{"signed":"private"}}]})
        ));
    }
}
