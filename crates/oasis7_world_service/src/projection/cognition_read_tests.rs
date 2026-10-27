//! Differential fixtures for the two cognition projection reads, not admission.
use super::*;
use serde_json::{Value, json};

#[path = "cognition_read_legacy.rs"]
mod legacy;

fn continuation(id: &str, agent: &str, status: &str) -> Value {
    json!({"schema_version":"agent-continuation.v1", "continuation_id":id,
        "wake_id":"wake", "world_id":"read-world", "branch_id":"main",
        "finality_epoch":0, "finality_status":"pending", "reorg_epoch":0,
        "runtime_manifest_hash":"manifest", "agent_id":agent, "agent_session_id":"session",
        "agent_turn_id":"turn", "decision_request_id":"request", "origin_turn_id":"origin",
        "origin_request_digest":"origin-digest", "continuation_proposal_id":"proposal",
        "proposal_digest":"proposal-digest", "wake_conditions":[],
        "remaining_budget":{"unit":"steps","value":1}, "precondition_digest":"precondition",
        "wake_seq":0, "status":status, "unknown_legacy_field":"ignored"})
}

fn context() -> Value {
    json!({"baseline_observation_digest":"baseline", "goal_digest":"goal",
        "policy_digest":"policy", "policy_revision":7,
        "precondition_summary":"summary", "precondition_digest":"precondition",
        "unknown_future_field":{"ignored":true}})
}

fn world_with(update: impl FnOnce(&mut Value)) -> World {
    let world = super::read_contract_tests::populated_world_with_binding("read-world");
    with_cognition(world, update)
}

fn with_cognition(world: World, update: impl FnOnce(&mut Value)) -> World {
    let mut value = serde_json::to_value(world).unwrap();
    update(&mut value["cognition"]);
    serde_json::from_value(value).unwrap()
}

pub(super) fn with_populated_cognition(world: World) -> World {
    with_cognition(world, |cognition| {
        cognition["continuations"] = json!([
            continuation("z-terminal", "7", "completed"),
            continuation("other", "other-agent", "scheduled"),
            continuation("a-active", "7", "scheduled"),
            continuation("z-terminal", "7", "completed")
        ]);
        cognition["continuation_contexts"] = json!({
            "z-terminal":context(), "a-active":context(), "other":null, "orphan":null});
    })
}

fn compare_once(
    world: &World,
    agent: Option<&str>,
    reader: &str,
) -> Result<WorldServiceProjection, String> {
    let before = serde_json::to_value(world).unwrap();
    let old = legacy::from_committed_world(world, agent, reader);
    let new = super::from_committed_world(world, agent, reader);
    match (&old, &new) {
        (Ok(old), Ok(new)) => {
            assert_eq!(
                serde_json::to_vec(old).unwrap(),
                serde_json::to_vec(new).unwrap()
            );
            assert_eq!(
                serde_cbor::to_vec(old).unwrap(),
                serde_cbor::to_vec(new).unwrap()
            );
            assert_eq!(
                crate::authority::signing_bytes("view", old).unwrap(),
                crate::authority::signing_bytes("view", new).unwrap()
            );
            let audience = match agent {
                None => crate::formal_projection::ProjectionAudienceV1::public(),
                Some(id) => {
                    crate::formal_projection::ProjectionAudienceV1::agent(id, reader.to_owned())
                        .unwrap()
                }
            };
            assert_eq!(
                crate::formal_projection::projection_leaf(&audience, old).unwrap(),
                crate::formal_projection::projection_leaf(&audience, new).unwrap()
            );
        }
        (Err(old), Err(new)) => assert_eq!(old, new),
        _ => panic!("cognition read changed success/error disposition"),
    }
    assert_eq!(
        before,
        serde_json::to_value(world).unwrap(),
        "read must not hydrate/write"
    );
    new
}

fn restore(world: &World) -> World {
    let snapshot = world.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: "read-world",
            chain_id: "read-chain",
            genesis_ref: None,
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: world.state().time,
        },
        "config",
        "generation",
    );
    let snapshot_json = oasis7_world_state::codec::snapshot_json(&snapshot).unwrap();
    let snapshot = oasis7_world_state::codec::snapshot_from_json(&snapshot_json).unwrap();
    let journal_json = oasis7_world_state::codec::journal_json(world.journal()).unwrap();
    let journal = oasis7_world_state::codec::journal_from_json(&journal_json).unwrap();
    World::from_snapshot(snapshot, journal).unwrap()
}

fn compare(
    world: &World,
    agent: Option<&str>,
    reader: &str,
) -> Result<WorldServiceProjection, String> {
    let online = compare_once(world, agent, reader);
    let restored = compare_once(&restore(world), agent, reader);
    assert_eq!(
        online.as_ref().map(|p| serde_json::to_value(p).unwrap()),
        restored.as_ref().map(|p| serde_json::to_value(p).unwrap())
    );
    online
}

fn owner() -> String {
    use ed25519_dalek::SigningKey;
    hex::encode(SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes())
}

#[test]
fn cognition_populated_reads_preserve_terminal_sparse_order_duplicates_and_restore() {
    let world = with_populated_cognition(super::read_contract_tests::populated_world_with_binding(
        "read-world",
    ));
    let restored = restore(&world);
    for world in [&world, &restored] {
        let projection = compare(world, Some("7"), &owner()).unwrap();
        assert_eq!(
            projection
                .continuations
                .iter()
                .map(|c| c.continuation_id.as_str())
                .collect::<Vec<_>>(),
            ["z-terminal", "a-active", "z-terminal"]
        );
        assert_eq!(projection.continuation_contexts.len(), 2);
        assert!(
            projection
                .continuations
                .iter()
                .all(|c| c.continuation_status_digest.is_none())
        );
        assert_eq!(projection.continuations[0].logical_tick, 0);
        assert_eq!(
            projection.continuation_contexts["a-active"].policy_revision,
            7
        );
        assert_eq!(
            projection.continuation_contexts["a-active"].goal_digest,
            "goal"
        );
        assert!(compare(world, None, "").unwrap().continuations.is_empty());
    }
}

#[test]
fn cognition_continuation_shapes_preserve_missing_empty_and_full_decode_failure() {
    for value in [
        None,
        Some(json!([])),
        Some(Value::Null),
        Some(json!({})),
        Some(json!("invalid")),
        Some(json!([continuation("own", "7", "scheduled"),
            {"agent_id":"other-agent"}])),
    ] {
        let world = world_with(|cognition| {
            cognition.as_object_mut().unwrap().remove("continuations");
            if let Some(value) = value.clone() {
                cognition["continuations"] = value;
            }
        });
        let result = compare(&world, Some("7"), &owner());
        assert_eq!(result.is_ok(), value.is_none() || value == Some(json!([])));
        assert!(compare(&world, None, "").unwrap().continuations.is_empty());
    }
}

#[test]
fn cognition_context_registry_get_semantics_and_referenced_value_errors_are_unchanged() {
    let mut missing_field = context();
    missing_field
        .as_object_mut()
        .unwrap()
        .remove("policy_revision");
    let mut wrong_type = context();
    wrong_type["policy_revision"] = json!("seven");
    let registries = [
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!(17)),
        Some(json!("invalid")),
        Some(json!({"orphan":null})),
        Some(json!({"own":null})),
        Some(json!({"own":7})),
        Some(json!({"own":missing_field})),
        Some(json!({"own":wrong_type})),
        Some(json!({"own":context(),"other":null,"orphan":null})),
    ];
    for (index, registry) in registries.into_iter().enumerate() {
        let world = world_with(|cognition| {
            cognition["continuations"] = json!([
                continuation("own", "7", "scheduled"),
                continuation("other", "other-agent", "completed")
            ]);
            cognition
                .as_object_mut()
                .unwrap()
                .remove("continuation_contexts");
            if let Some(value) = registry {
                cognition["continuation_contexts"] = value;
            }
        });
        let result = compare(&world, Some("7"), &owner());
        assert_eq!(result.is_ok(), !(6..10).contains(&index));
        if index < 6 {
            assert!(result.unwrap().continuation_contexts.is_empty());
        }
    }
}

#[test]
fn cognition_read_errors_keep_continuation_before_context_and_context_iteration_order() {
    let world = world_with(|cognition| {
        cognition["continuations"] = json!([{"agent_id":"other-agent"}]);
        cognition["continuation_contexts"] = json!({"own":null});
    });
    assert!(
        compare(&world, Some("7"), &owner())
            .unwrap_err()
            .contains("schema_version")
    );
    let world = world_with(|cognition| {
        cognition["continuations"] = json!([
            continuation("z-first", "7", "scheduled"),
            continuation("a-next", "7", "scheduled")
        ]);
        cognition["continuation_contexts"] = json!({"z-first":null,"a-next":{}});
    });
    assert!(
        compare(&world, Some("7"), &owner())
            .unwrap_err()
            .contains("null")
    );
}

#[test]
fn cognition_each_context_field_keeps_its_required_type_and_error_message() {
    for field in [
        "baseline_observation_digest",
        "goal_digest",
        "policy_digest",
        "policy_revision",
        "precondition_summary",
        "precondition_digest",
    ] {
        for missing in [true, false] {
            let mut raw = context();
            if missing {
                raw.as_object_mut().unwrap().remove(field);
            } else {
                raw[field] = if field == "policy_revision" {
                    json!(-1)
                } else {
                    json!(7)
                };
            }
            let world = world_with(|cognition| {
                cognition["continuations"] = json!([continuation("own", "7", "pending")]);
                cognition["continuation_contexts"] = json!({"own":raw});
            });
            let error = compare(&world, Some("7"), &owner()).unwrap_err();
            assert!(error.contains(if missing { "missing field" } else { "invalid" }));
        }
    }
}
