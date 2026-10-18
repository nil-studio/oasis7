//! RED/GREEN coverage for the authorization lifecycle findings RT-CAP-008..014.
//!
//! These tests deliberately exercise only the public runtime boundary.  A
//! provider response is not an authority source, and a stale checkpoint must
//! not be able to erase or rewrite journaled authorization state.

use super::super::*;
use super::capability_grant_v2::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_wasm_abi::canonical_hash;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

fn finality_signing_key_2() -> SigningKey {
    let seed = super::super::util::sha256_hex(b"oasis7-test-capability-finality-signer-2-v1");
    let seed_bytes = hex::decode(seed).expect("decode capability finality signing seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("capability finality signing seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}

fn resign_authority_proof(
    world: &World,
    record: &CapabilityAuthorityRecord,
) -> CapabilityAuthorityFinalityProof {
    let previous = world
        .capability_revocation_state()
        .authority_finality_proofs
        .get(ISSUER_ID)
        .cloned()
        .expect("fixture authority proof");
    let mut proof = previous;
    proof.binding = CapabilityAuthorityFinalityBinding::from_record(record)
        .expect("bind rotated authority record");
    proof.signatures.clear();
    for (node_id, signing_key) in [
        (ISSUER_ID, capability_issuer_signing_key()),
        (FINALITY_SIGNER_2, finality_signing_key_2()),
    ] {
        let payload = proof
            .signing_payload_v1(node_id)
            .expect("encode rotated authority proof payload");
        let signature = signing_key.sign(payload.as_slice());
        proof.signatures.insert(
            node_id.to_string(),
            format!(
                "{}{}",
                CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }
    proof
}

fn authority_transition(
    world: &World,
    mutate: impl FnOnce(&mut CapabilityAuthorityRecord),
) -> (CapabilityAuthorityRecord, CapabilityAuthorityFinalityProof) {
    let mut record = world
        .capability_revocation_state()
        .authority_records
        .get(ISSUER_ID)
        .cloned()
        .expect("fixture authority record");
    mutate(&mut record);
    let proof = resign_authority_proof(world, &record);
    (record, proof)
}

fn command_event_index(journal: &Journal) -> usize {
    journal
        .events
        .iter()
        .position(|event| {
            matches!(
                event.body,
                WorldEventBody::CapabilityAuthorization(
                    CapabilityAuthorizationEvent::CommandCommitted { .. }
                )
            )
        })
        .expect("command commit event")
}

fn merge_test_json(base: &mut serde_json::Value, overrides: serde_json::Value) {
    match (base, overrides) {
        (serde_json::Value::Object(base), serde_json::Value::Object(overrides)) => {
            for (key, value) in overrides {
                match base.get_mut(&key) {
                    Some(existing) => merge_test_json(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overrides) => *base = overrides,
    }
}

#[derive(Serialize)]
struct TestCapabilityAuthorizationRootBody<'a> {
    grants: &'a BTreeMap<String, serde_json::Value>,
    revocation: &'a CapabilityRevocationState,
    invocation_contexts: &'a BTreeMap<String, CapabilityInvocationContext>,
    budget_accounts: &'a BTreeMap<String, CapabilityBudgetAccount>,
    nonce_records: &'a BTreeMap<String, CapabilityAuthorizationNonceRecord>,
    receipts: &'a BTreeMap<String, CapabilityAuthorizationAuditReceipt>,
    effect_receipt_links: &'a BTreeMap<String, CapabilityEffectReceiptLink>,
}

fn test_capability_authorization_root(snapshot: &Snapshot) -> String {
    canonical_hash(&TestCapabilityAuthorizationRootBody {
        grants: &snapshot.capability_grants_v2,
        revocation: &snapshot.capability_revocation_state,
        invocation_contexts: &snapshot.capability_invocation_contexts,
        budget_accounts: &snapshot.capability_budget_accounts,
        nonce_records: &snapshot.capability_nonce_records,
        receipts: &snapshot.capability_authorization_receipts,
        effect_receipt_links: &snapshot.capability_effect_receipt_links,
    })
    .expect("compute test capability authorization root")
}

fn accepted_command_stale_checkpoint() -> (Snapshot, Journal) {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-replay-grant"
    })));
    install_budget_for_grant(&mut world, &grant, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let stale_snapshot = world.snapshot();
    let mut sandbox = RecordingSandbox::default();
    execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
        .expect("fixture command should commit before replay tampering");
    (stale_snapshot, world.journal().clone())
}

fn mutate_command_commit<F>(journal: &mut Journal, mutate: F)
where
    F: FnOnce(&mut CapabilityAuthorizationAuditReceipt, &mut CapabilityBudgetAccount),
{
    let mut mutate = Some(mutate);
    for event in &mut journal.events {
        if let WorldEventBody::CapabilityAuthorization(
            CapabilityAuthorizationEvent::CommandCommitted {
                budget_account,
                receipt,
                ..
            },
        ) = &mut event.body
        {
            mutate.take().expect("command commit mutator")(receipt, budget_account);
            return;
        }
    }
    panic!("fixture command commit event");
}

#[test]
fn rt_cap_008_rejects_pending_or_unverified_registration_but_accepts_verified() {
    let mut world = fixture_world();
    let initial_snapshot = world.snapshot();
    let initial_journal = world.journal().clone();

    for (index, status) in ["pending", "unverified"].into_iter().enumerate() {
        let grant = signed_grant(grant_json(json!({
            "grant_nonce": format!("lifecycle-status-{index}"),
            "status": status
        })));
        let error = world
            .register_capability_grant_v2(grant)
            .expect_err("only verified grants may enter durable authorization state");
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        assert_eq!(world.snapshot(), initial_snapshot);
        assert_eq!(world.journal(), &initial_journal);
    }

    let verified = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-status-verified"
    })));
    let verified_id = verified.grant_id.clone();
    world
        .register_capability_grant_v2(verified)
        .expect("verified grant follows the valid registration lifecycle");
    assert!(world.capability_grants_v2().contains_key(&verified_id));
}

#[test]
fn rt_cap_009_proof_bound_revocation_denies_grant_without_blocking_valid_grant() {
    let revoked = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-revoked"
    })));
    let mut world = fixture_world_with_revocations(
        [revoked.grant_id.clone()]
            .into_iter()
            .collect::<BTreeSet<_>>(),
    );
    let error = world
        .register_capability_grant_v2(revoked)
        .expect_err("proof-bound revocation must deny the revoked grant");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));

    let valid = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-revocation-valid"
    })));
    let valid_id = valid.grant_id.clone();
    world
        .register_capability_grant_v2(valid)
        .expect("a non-revoked grant remains valid under the same proof");
    assert!(world.capability_grants_v2().contains_key(&valid_id));
}

#[test]
fn rt_cap_009_accepts_signed_authority_rotation_and_applies_supersession() {
    let mut world = fixture_world();
    let superseded = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-superseded",
        "issuer": {
            "key_id": "governance-key-2",
            "issuer_key_epoch": 4,
            "authority_rotation_receipt_id": "rotation-receipt-5"
        }
    })));
    let replacement = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-replacement",
        "issuer": {
            "key_id": "governance-key-2",
            "issuer_key_epoch": 4,
            "authority_rotation_receipt_id": "rotation-receipt-5"
        }
    })));

    let mut rotated = world
        .capability_revocation_state()
        .authority_records
        .get(ISSUER_ID)
        .cloned()
        .expect("fixture authority record");
    rotated.key_id = "governance-key-2".to_string();
    rotated.issuer_key_epoch = 4;
    rotated.authority_rotation_receipt_id = Some("rotation-receipt-5".to_string());
    rotated
        .superseded_by
        .insert(superseded.grant_id.clone(), replacement.grant_id.clone());
    let proof = resign_authority_proof(&world, &rotated);

    world
        .install_capability_authority_record_with_finality_proof(rotated, proof)
        .expect("proof-bearing key rotation should be a valid authority transition");
    let error = world
        .register_capability_grant_v2(superseded)
        .expect_err("superseded grant must be rejected after the signed transition");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    world
        .register_capability_grant_v2(replacement)
        .expect("replacement grant follows the rotated trust root");
}

#[test]
fn rt_cap_009_rejects_invalid_supersession_transitions() {
    let superseded = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-invalid-superseded"
    })));
    let replacement = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-invalid-replacement"
    })));
    let cases = [
        (
            "self",
            BTreeMap::from([(superseded.grant_id.clone(), superseded.grant_id.clone())]),
            true,
        ),
        (
            "missing source",
            BTreeMap::from([("missing-grant".to_string(), replacement.grant_id.clone())]),
            false,
        ),
        (
            "missing replacement",
            BTreeMap::from([(
                superseded.grant_id.clone(),
                "missing-replacement".to_string(),
            )]),
            true,
        ),
        (
            "cycle",
            BTreeMap::from([
                (superseded.grant_id.clone(), replacement.grant_id.clone()),
                (replacement.grant_id.clone(), superseded.grant_id.clone()),
            ]),
            true,
        ),
    ];
    for (label, superseded_by, register_source) in cases {
        let mut world = fixture_world();
        if register_source {
            world
                .register_capability_grant_v2(superseded.clone())
                .expect("supersession source fixture");
        }
        if label == "cycle" {
            world
                .register_capability_grant_v2(replacement.clone())
                .expect("supersession cycle replacement fixture");
        }
        let (record, proof) = authority_transition(&world, |record| {
            record.superseded_by = superseded_by;
        });
        let error = world
            .install_capability_authority_record_with_finality_proof(record, proof)
            .expect_err(label);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} supersession must be denied, got {error:?}"
        );
    }
}

#[test]
fn rt_cap_009_requires_monotonic_same_issuer_key_rotation() {
    for (label, key_epoch) in [("same epoch", 3), ("regressed epoch", 2)] {
        let mut world = fixture_world();
        let (record, proof) = authority_transition(&world, |record| {
            record.key_id = "governance-key-2".to_string();
            record.issuer_key_epoch = key_epoch;
            record.authority_rotation_receipt_id = Some("rotation-receipt-invalid".to_string());
        });
        let error = world
            .install_capability_authority_record_with_finality_proof(record, proof)
            .expect_err(label);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} key transition must be denied, got {error:?}"
        );
    }
}

#[test]
fn rt_cap_009_denies_grants_signed_by_the_previous_issuer_key_after_rotation() {
    let mut world = fixture_world();
    let (record, proof) = authority_transition(&world, |record| {
        record.key_id = "governance-key-2".to_string();
        record.issuer_key_epoch = 4;
        record.authority_rotation_receipt_id = Some("rotation-receipt-valid".to_string());
    });
    world
        .install_capability_authority_record_with_finality_proof(record, proof)
        .expect("valid monotonic rotation fixture");

    let old_key_grant = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-old-key",
    })));
    let error = world
        .register_capability_grant_v2(old_key_grant)
        .expect_err("the old issuer key metadata must not authorize new grants");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
}

#[test]
fn rt_cap_010_rejects_opaque_target_fields_but_accepts_un_targeted_payload() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-target-fields"
    })));
    let payload = serde_json::to_vec(&json!({"target_id": "station-1"}))
        .expect("encode opaque target payload");
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({})),
        response_json(json!({"envelope": {"payload": payload}})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();
    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("opaque target fields must not bypass exact scope binding");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);

    let mut valid_world = fixture_world();
    let valid_grant = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-target-fields-valid"
    })));
    install_budget_for_grant(&mut valid_world, &valid_grant, 128);
    let (valid_catalog, valid_response) = prepared_invocation(
        &valid_world,
        &valid_grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(
        &mut valid_world,
        &valid_grant,
        &valid_catalog,
        &valid_response,
    );
    valid_world
        .execute_trusted_module_command(
            valid_grant,
            valid_catalog,
            valid_response,
            &mut (),
            &mut RecordingSandbox::default(),
        )
        .expect("un-targeted payload remains valid");
}

#[test]
fn rt_cap_011_governance_epoch_snapshot_survives_stale_checkpoint_replay() {
    let mut world = fixture_world();
    let stale_snapshot = world.snapshot();
    let mut next_epoch = world
        .governance_finality_epoch_snapshots()
        .get(&4)
        .cloned()
        .expect("fixture governance epoch snapshot");
    next_epoch.epoch_id = 5;
    world
        .set_governance_finality_epoch_snapshot(next_epoch)
        .expect("install next governance epoch snapshot");
    let expected = world.governance_finality_epoch_snapshots().clone();

    let restored = World::from_snapshot(stale_snapshot, world.journal().clone())
        .expect("journaled governance history should replay from stale checkpoint");
    assert_eq!(restored.governance_finality_epoch_snapshots(), &expected);
}

#[test]
fn rt_cap_012_replay_rejects_tampered_audit_receipt_binding() {
    let (snapshot, mut journal) = accepted_command_stale_checkpoint();
    mutate_command_commit(&mut journal, |receipt, _| {
        receipt.scope_hash = "tampered-scope-hash".to_string();
    });
    let error = World::from_snapshot(snapshot, journal)
        .expect_err("replay must verify the persisted audit receipt binding");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
}

#[test]
fn rt_cap_012_replay_rejects_tampered_state_hash_binding() {
    let (snapshot, mut journal) = accepted_command_stale_checkpoint();
    mutate_command_commit(&mut journal, |receipt, _| {
        receipt.state_hash_after = Some("tampered-state-hash".to_string());
    });
    let error = World::from_snapshot(snapshot, journal)
        .expect_err("replay must verify the persisted state hash binding");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
}

#[test]
fn rt_cap_012_replay_rejects_tampered_budget_transition() {
    let (snapshot, mut journal) = accepted_command_stale_checkpoint();
    mutate_command_commit(&mut journal, |receipt, budget| {
        budget.remaining_units = budget.remaining_units.saturating_add(1);
        budget.spent_units = budget.spent_units.saturating_sub(1);
        receipt.budget_after = Some(budget.remaining_units.saturating_sub(1));
    });
    let error = World::from_snapshot(snapshot, journal)
        .expect_err("replay must verify the persisted budget transition");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
}

#[test]
fn rt_cap_013_requires_explicit_parent_for_delegated_grant() {
    let mut world = fixture_world();
    let child = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-orphan-child",
        "delegation_depth": 1
    })));
    install_budget_for_grant(&mut world, &child, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &child,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &child, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();
    let error =
        execute_without_invocation_context(&mut world, child, catalog, response, &mut sandbox)
            .expect_err("delegation depth requires an explicit parent authorization");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);
}

#[test]
fn rt_cap_013_accepts_attenuated_child_with_explicit_parent() {
    let mut world = fixture_world();
    let parent = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-parent",
        "delegation_depth": 2,
        "scope": {
            "entity_selector": ["station-1", "station-2"],
            "resource_selector": ["weather.read"]
        }
    })));
    world
        .register_capability_grant_v2(parent.clone())
        .expect("parent grant should be durably authorized");
    let child = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-child",
        "delegation_depth": 1,
        "parent_grant_id": parent.grant_id,
        "scope": {
            "entity_selector": ["station-1"],
            "resource_selector": ["weather.read"]
        }
    })));
    install_budget_for_grant(&mut world, &child, 128);
    let payload = serde_json::to_vec(&json!({
        "entity_id": "station-1",
        "resource_id": "weather.read"
    }))
    .expect("encode delegated target payload");
    let (catalog, response) = prepared_invocation(
        &world,
        &child,
        catalog_json(json!({})),
        response_json(json!({"envelope": {"payload": payload}})),
    );
    install_invocation_context(&mut world, &child, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();
    let receipt =
        execute_without_invocation_context(&mut world, child, catalog, response, &mut sandbox)
            .expect("attenuated child with explicit parent should execute");
    assert_eq!(receipt.decision, "accepted");
    assert_eq!(sandbox.calls, 1);
}

#[test]
fn rt_cap_013_rejects_child_subject_audience_issuer_and_depth_mismatches() {
    let cases = [
        (
            "subject mismatch",
            json!({
                "subject": {"owner_binding": "owner-other"}
            }),
        ),
        (
            "audience mismatch",
            json!({
                "audience": {
                    "target_kind": "institution",
                    "target_id": "institution-1"
                }
            }),
        ),
        (
            "issuer mismatch",
            json!({
                "issuer": {"governance_epoch": 10}
            }),
        ),
        ("depth mismatch", json!({"delegation_depth": 2})),
    ];
    for (index, (label, mismatch)) in cases.into_iter().enumerate() {
        let mut world = fixture_world();
        let parent = signed_grant(grant_json(json!({
            "grant_nonce": format!("lifecycle-parent-mismatch-{index}"),
            "delegation_depth": 2,
            "scope": {
                "entity_selector": ["station-1", "station-2"],
                "resource_selector": ["weather.read"]
            }
        })));
        world
            .register_capability_grant_v2(parent.clone())
            .expect("parent mismatch fixture");
        let mut child_json = grant_json(json!({
            "grant_nonce": format!("lifecycle-child-mismatch-{index}"),
            "parent_grant_id": parent.grant_id,
            "delegation_depth": 1,
            "scope": {
                "entity_selector": ["station-1"],
                "resource_selector": ["weather.read"]
            }
        }));
        merge_test_json(&mut child_json, mismatch);
        let child = signed_grant(child_json);
        let error = world.register_capability_grant_v2(child).expect_err(label);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} must be denied, got {error:?}"
        );
    }
}

#[test]
fn rt_cap_013_rejects_child_when_its_parent_is_revoked() {
    let mut world = fixture_world();
    let parent = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-revoked-parent",
        "delegation_depth": 2
    })));
    world
        .register_capability_grant_v2(parent.clone())
        .expect("parent revocation fixture");

    let (record, proof) = authority_transition(&world, |record| {
        record.revocation_epoch = 3;
        record.revoked_grant_ids.insert(parent.grant_id.clone());
    });
    world
        .install_capability_authority_record_with_finality_proof(record, proof)
        .expect("signed parent revocation transition");

    let child = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-revoked-parent-child",
        "parent_grant_id": parent.grant_id,
        "delegation_depth": 1
    })));
    let error = world
        .register_capability_grant_v2(child)
        .expect_err("a revoked parent must not authorize a delegated child");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
}

#[test]
fn rt_cap_013_rejects_delegation_parent_cycle_from_tampered_registry() {
    let mut world = fixture_world();
    let parent = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-cycle-parent",
        "delegation_depth": 4,
        "scope": {
            "entity_selector": ["station-1", "station-2"],
            "resource_selector": ["weather.read"]
        }
    })));
    world
        .register_capability_grant_v2(parent.clone())
        .expect("cycle parent fixture");
    let cycle = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-cycle-node",
        "parent_grant_id": parent.grant_id,
        "delegation_depth": 3,
        "scope": {
            "entity_selector": ["station-1", "station-2"],
            "resource_selector": ["weather.read"]
        }
    })));
    world
        .register_capability_grant_v2(cycle.clone())
        .expect("cycle node fixture");
    let child = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-cycle-child",
        "parent_grant_id": parent.grant_id,
        "delegation_depth": 1,
        "scope": {
            "entity_selector": ["station-1"],
            "resource_selector": ["weather.read"]
        }
    })));
    world
        .register_capability_grant_v2(child.clone())
        .expect("cycle child fixture");
    install_budget_for_grant(&mut world, &child, 128);
    let (catalog, response) = prepared_invocation(
        &world,
        &child,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(&mut world, &child, &catalog, &response);

    let mut snapshot = world.snapshot();
    let cycle_json = snapshot
        .capability_grants_v2
        .get(&cycle.grant_id)
        .cloned()
        .expect("cycle grant snapshot entry");
    snapshot
        .capability_grants_v2
        .insert(parent.grant_id.clone(), cycle_json);
    snapshot.capability_authorization_root = test_capability_authorization_root(&snapshot);
    let mut restored = World::from_snapshot(snapshot, world.journal().clone())
        .expect("tampered registry remains structurally recoverable for cycle validation");
    let mut sandbox = RecordingSandbox::default();
    let error =
        execute_without_invocation_context(&mut restored, child, catalog, response, &mut sandbox)
            .expect_err("delegation parent cycles must not reach the sandbox");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);
}

#[test]
fn rt_cap_014_rejects_non_world_audience_target_and_keeps_world_path_valid() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-audience-target",
        "audience": {
            "target_kind": "institution",
            "target_id": "institution-1"
        }
    })));
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({
            "audience": {
                "target_kind": "institution",
                "target_id": "institution-1"
            }
        })),
        response_json(json!({
            "audience": {
                "target_kind": "institution",
                "target_id": "institution-1"
            }
        })),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();
    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("audience target must be resolved against a live runtime target");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);

    let mut valid_world = fixture_world();
    let valid_grant = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-audience-world"
    })));
    install_budget_for_grant(&mut valid_world, &valid_grant, 128);
    let (valid_catalog, valid_response) = prepared_invocation(
        &valid_world,
        &valid_grant,
        catalog_json(json!({})),
        response_json(json!({})),
    );
    install_invocation_context(
        &mut valid_world,
        &valid_grant,
        &valid_catalog,
        &valid_response,
    );
    valid_world
        .execute_trusted_module_command(
            valid_grant,
            valid_catalog,
            valid_response,
            &mut (),
            &mut RecordingSandbox::default(),
        )
        .expect("world audience follows the valid lifecycle");
}

#[test]
fn rt_cap_014_rejects_targetless_and_unknown_non_world_audiences() {
    let cases = [
        (
            "targetless institution",
            json!({
                "target_kind": "institution",
                "target_id": null
            }),
        ),
        (
            "unknown institution",
            json!({
                "target_kind": "institution",
                "target_id": "institution-unknown"
            }),
        ),
    ];
    for (index, (label, target)) in cases.into_iter().enumerate() {
        let mut world = fixture_world();
        let grant = signed_grant(grant_json(json!({
            "grant_nonce": format!("lifecycle-audience-negative-{index}"),
            "audience": target
        })));
        install_budget_for_grant(&mut world, &grant, 128);
        let audience = serde_json::to_value(&grant.audience).expect("encode audience");
        let (catalog, response) = prepared_invocation(
            &world,
            &grant,
            catalog_json(json!({"audience": audience.clone()})),
            response_json(json!({"audience": audience})),
        );
        install_invocation_context(&mut world, &grant, &catalog, &response);
        let mut sandbox = RecordingSandbox::default();
        let error =
            execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
                .expect_err(label);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} must be denied, got {error:?}"
        );
        assert_eq!(sandbox.calls, 0);
    }
}

#[test]
fn rt_cap_014_rejects_stale_branch_and_finality_audiences() {
    let cases = [
        ("abandoned branch", json!({"branch_id": "branch-abandoned"})),
        ("stale finality epoch", json!({"finality_epoch": 3})),
    ];
    for (index, (label, audience_fields)) in cases.into_iter().enumerate() {
        let mut world = fixture_world();
        let grant = signed_grant(grant_json(json!({
            "grant_nonce": format!("lifecycle-stale-audience-{index}"),
            "audience": audience_fields
        })));
        let error = world.register_capability_grant_v2(grant).expect_err(label);
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { .. }),
            "{label} must be denied, got {error:?}"
        );
    }
}

#[test]
fn rt_cap_014_rejects_unknown_system_identity_but_preserves_bound_subjects() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "lifecycle-unknown-system",
        "subject": {
            "kind": "system",
            "system_id": "system-unknown",
            "epoch": 1
        }
    })));
    if let Err(error) = world.register_capability_grant_v2(grant.clone()) {
        assert!(matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. }
        ));
        return;
    }
    install_budget_for_grant(&mut world, &grant, 128);
    let subject = serde_json::to_value(&grant.subject).expect("encode system subject");
    let (catalog, response) = prepared_invocation(
        &world,
        &grant,
        catalog_json(json!({"subject": subject.clone()})),
        response_json(json!({"subject": subject})),
    );
    install_invocation_context(&mut world, &grant, &catalog, &response);
    let mut sandbox = RecordingSandbox::default();
    let error =
        execute_without_invocation_context(&mut world, grant, catalog, response, &mut sandbox)
            .expect_err("unknown system identity must not execute");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { .. }
    ));
    assert_eq!(sandbox.calls, 0);
}

#[test]
fn rt_cap_012_replay_rejects_missing_command_event() {
    let (snapshot, mut missing_journal) = accepted_command_stale_checkpoint();
    let command_index = command_event_index(&missing_journal);
    missing_journal.events.remove(command_index);
    let error = World::from_snapshot(snapshot.clone(), missing_journal)
        .expect_err("a missing authorization commit must not replay as an accepted command");
    assert!(
        matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. } | WorldError::JournalMismatch
        ),
        "missing command event must be rejected, got {error:?}"
    );
}

#[test]
fn rt_cap_012_replay_rejects_duplicate_command_event() {
    let (snapshot, mut duplicate_journal) = accepted_command_stale_checkpoint();
    let command_index = command_event_index(&duplicate_journal);
    let mut duplicate = duplicate_journal.events[command_index].clone();
    duplicate.id = duplicate_journal
        .events
        .last()
        .expect("command journal tail")
        .id
        .saturating_add(1);
    duplicate_journal.events.push(duplicate);
    let error = World::from_snapshot(snapshot, duplicate_journal)
        .expect_err("a duplicate authorization commit must not replay twice");
    assert!(
        matches!(
            error,
            WorldError::CapabilityAuthorizationDenied { .. } | WorldError::JournalMismatch
        ),
        "duplicate command event must be rejected, got {error:?}"
    );
}

#[test]
fn rt_cap_011_governance_epoch_mutation_replays_with_exact_predecessor() {
    let mut world = World::new();
    let signing_key = capability_issuer_signing_key();
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    world
        .bind_node_identity("governance-node-1", public_key_hex.as_str())
        .expect("bind initial governance signer");
    world
        .bind_node_identity("governance-node-2", public_key_hex.as_str())
        .expect("bind replacement governance signer");
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id: 1,
            threshold_bps: 10_000,
            min_unique_signers: 1,
            threshold: 1,
            signer_node_ids: vec!["governance-node-1".to_string()],
            validator_stakes: BTreeMap::from([("governance-node-1".to_string(), 100)]),
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("install initial governance epoch");
    let stale_snapshot = world.snapshot();
    let mut mutated = world
        .governance_finality_epoch_snapshots()
        .get(&1)
        .cloned()
        .expect("initial governance epoch");
    mutated.signer_node_ids = vec!["governance-node-2".to_string()];
    mutated.validator_stakes = BTreeMap::from([("governance-node-2".to_string(), 100)]);
    mutated.validator_set_hash.clear();
    mutated.stake_root.clear();
    world
        .set_governance_finality_epoch_snapshot(mutated)
        .expect("mutate governance epoch");
    let expected = world.governance_finality_epoch_snapshots().clone();
    let restored = World::from_snapshot(stale_snapshot, world.journal().clone())
        .expect("governance epoch mutation should replay from a stale checkpoint");
    assert_eq!(restored.governance_finality_epoch_snapshots(), &expected);
}
