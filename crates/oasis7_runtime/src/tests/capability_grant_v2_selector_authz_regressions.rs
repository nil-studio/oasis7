//! Focused selector and canonical-CBOR authorization regressions.

use super::super::*;
use super::capability_grant_v2::*;
use oasis7_wasm_abi::{ModuleEffectIntent, ModuleOutput};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn wasm_authz_001_denies_nested_or_case_variant_targets_when_command_selectors_omitted() {
    let cases = [
        (
            "nested target",
            json!({
                "metadata": {
                    "entity_id": "station-1",
                    "resource_id": "weather.read"
                }
            }),
        ),
        (
            "case-variant target",
            json!({
                "Entity_ID": "station-1",
                "Resource_ID": "weather.read"
            }),
        ),
    ];

    for (label, target_payload) in cases {
        let mut world = fixture_world();
        let grant = signed_grant(grant_json(json!({
            "grant_nonce": format!("wasm-authz-001-command-{label}"),
        })));
        install_budget_for_grant(&mut world, &grant, 128);
        let payload = serde_json::to_vec(&target_payload).expect("encode targeted command payload");
        let (catalog, response) = prepared_invocation(
            &world,
            &grant,
            catalog_json(json!({})),
            response_json(json!({
                "envelope": {"payload": payload}
            })),
        );
        install_invocation_context(&mut world, &grant, &catalog, &response);
        let mut sandbox = RecordingSandbox::default();

        let error =
            execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
                .expect_err("omitted selectors must not authorize hidden command targets");
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} must be denied, got {error:?}"
        );
        assert_eq!(sandbox.calls, 0, "{label} must fail before sandbox");
    }
}

#[test]
fn wasm_authz_001_denies_nested_or_case_variant_targets_when_effect_selectors_omitted() {
    let cases = [
        (
            "nested target",
            json!({
                "metadata": {
                    "entity_id": "station-1",
                    "resource_id": "weather.read"
                }
            }),
        ),
        (
            "case-variant target",
            json!({
                "Entity_ID": "station-1",
                "Resource_ID": "weather.read"
            }),
        ),
    ];

    for (label, target_payload) in cases {
        let effect_grant = signed_effect_grant();
        let mut world = fixture_world_with_revocations_and_budget_and_effect_grant(
            BTreeSet::new(),
            128,
            effect_grant.clone(),
        );
        let grant = signed_grant(grant_json(json!({
            "grant_nonce": format!("wasm-authz-001-effect-{label}"),
        })));
        install_budget_for_grant(&mut world, &grant, 128);
        let (catalog, response) = prepared_invocation(
            &world,
            &grant,
            catalog_json(json!({})),
            response_json(json!({})),
        );
        install_invocation_context(&mut world, &grant, &catalog, &response);
        let mut sandbox = ConfiguredSandbox {
            calls: 0,
            output: ModuleOutput {
                new_state: Some(vec![0xa1]),
                effects: vec![ModuleEffectIntent {
                    kind: "weather.publish".to_string(),
                    params: target_payload,
                    cap_ref: effect_grant.grant_id,
                    cap_slot: None,
                }],
                emits: Vec::new(),
                tick_lifecycle: None,
                output_bytes: 0,
            },
        };

        let error =
            execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
                .expect_err("omitted selectors must not authorize hidden effect targets");
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} must be denied, got {error:?}"
        );
        assert_eq!(
            world.pending_effects_len(),
            0,
            "{label} must not queue an effect"
        );
        assert!(
            world.capability_nonce_records().is_empty(),
            "{label} must not commit the command nonce"
        );
    }
}

#[test]
fn agent_cap_001_accepts_a_valid_canonical_cbor_command_payload() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "agent-cap-001-canonical-cbor-command",
    })));
    install_budget_for_grant(&mut world, &grant, 128);
    let payload = oasis7_wasm_abi::encode_canonical_cbor(&BTreeMap::from([(
        "message".to_string(),
        "sunny".to_string(),
    )]))
    .expect("encode canonical-CBOR command payload");
    assert!(
        serde_json::from_slice::<serde_json::Value>(&payload).is_err(),
        "the fixture must exercise a non-JSON canonical-CBOR payload"
    );
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({
            "envelope": {"payload": payload}
        })),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();

    let receipt =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect("valid canonical-CBOR commands must pass scope checks");
    assert_eq!(receipt.decision, "accepted");
    assert_eq!(sandbox.calls, 1);
}
