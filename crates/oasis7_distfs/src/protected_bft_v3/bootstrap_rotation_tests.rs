//! Protocol fixtures with a real signed qualified H and native authenticated TCP.
//! The backend/key doubles do not establish physical custody or World approval.
use super::*;
use crate::protected_bft_v3::rotation::tests::{
    binding, body as admitted_body, hash, key, native, public,
};
use crate::protected_bft_v3::*;
use ed25519_dalek::Signer;
use oasis7_agent_api::world_action::{FinalityKeyRotationRoleV1, GovernanceAuthorizationV1};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn body(binding: &BftSignerBindingV3) -> BootstrapFinalityKeyRotationBodyV1 {
    let schedule = binding.initial_schedule();
    BootstrapFinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: GovernanceAuthorizationV1 {
            schema_version: 1,
            world_id: binding.scope.world_id.clone(),
            chain_id: binding.scope.chain_id.clone(),
            genesis_digest: binding.scope.genesis_hash.clone(),
            parent_position: schedule.parent_tick(),
            parent_record_id: schedule.parent_commit_ref().into(),
            parent_state_root: schedule.parent_state_root().into(),
            policy_digest: hash(71),
            controller_account_id: "world-must-check-controller".into(),
            nonce: 1,
            valid_from_position: schedule.height(),
            valid_through_position: schedule.height(),
            signatures: BTreeMap::new(),
        },
        rotation_id: "original-bootstrap-rotation".into(),
        bootstrap_registry_digest_hex: hash(72),
        bootstrap_public_key_hex: public(31),
        node_id: binding.local_validator_id.clone(),
        expected_generation: 0,
        old_public_key_hex: public(31),
        new_public_key_hex: public(32),
        decision_height: schedule.height(),
        activation_height: schedule.height() + 2,
        target_epoch: 1,
    }
}
fn record(
    binding: &BftSignerBindingV3,
    id: &str,
    revision: u64,
) -> BftBootstrapFinalityRotationProofRecordV3 {
    let mut body = body(binding);
    body.rotation_id = id.into();
    let proof = BootstrapFinalityKeyRotationProofV1 {
        schema_version: 1,
        role: BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        signature_hex: hex::encode(
            key(31)
                .sign(
                    &body
                        .signing_bytes(BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization)
                        .unwrap(),
                )
                .to_bytes(),
        ),
    };
    BftBootstrapFinalityRotationProofRecordV3 {
        body,
        public_key_hex: public(31),
        proof,
        recorded_revision: revision,
    }
}

#[test]
fn native_bootstrap_proofs_bind_source_domains_and_cold_exact_results() {
    let binding = binding();
    let body = body(&binding);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        successor_archive_transfer: None,
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (provider, server) = native(&binding, &snapshot, None);
    let mut lease = provider.acquire(&binding).unwrap();
    let initial = lease.checkpoint().unwrap();
    assert!(
        lease
            .sign_bootstrap_finality_rotation_proof(
                initial.state.revision,
                &initial.state.state_digest,
                &body,
                BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, initial.state);
    let enabled = lease
        .enable_membership(initial.state.revision, &initial.state.state_digest)
        .unwrap();
    let (old, cp) = lease
        .sign_bootstrap_finality_rotation_proof(
            enabled.state.revision,
            &enabled.state.state_digest,
            &body,
            BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        )
        .unwrap();
    let (new, completed) = lease
        .sign_bootstrap_finality_rotation_proof(
            cp.state.revision,
            &cp.state.state_digest,
            &body,
            BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
        )
        .unwrap();
    assert_eq!(
        completed
            .state
            .bootstrap_finality_rotation_proofs
            .as_ref()
            .unwrap()
            .records
            .len(),
        2
    );
    assert_eq!(completed.state.journal, enabled.state.journal);
    assert_eq!(completed.state.history, enabled.state.history);
    assert_eq!(
        completed.state.membership_install,
        enabled.state.membership_install
    );
    verify_proof(
        &body,
        &old,
        BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
    )
    .unwrap();
    verify_proof(
        &body,
        &new,
        BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
    )
    .unwrap();
    assert!(
        verify_proof(
            &body,
            &old,
            BootstrapFinalityKeyRotationRoleV1::NewKeyPossession
        )
        .is_err()
    );
    let admitted = admitted_body(&binding);
    let signature =
        ed25519_dalek::Signature::from_slice(&hex::decode(&old.signature_hex).unwrap()).unwrap();
    assert!(
        key(31)
            .verifying_key()
            .verify_strict(
                &admitted
                    .signing_bytes(FinalityKeyRotationRoleV1::OldKeyAuthorization)
                    .unwrap(),
                &signature
            )
            .is_err()
    );
    for field in 0..3 {
        let mut wrong = body.clone();
        match field {
            0 => wrong.bootstrap_registry_digest_hex = hash(73),
            1 => wrong.bootstrap_public_key_hex = public(32),
            _ => wrong.authorization.nonce += 1,
        }
        assert!(
            lease
                .sign_bootstrap_finality_rotation_proof(
                    completed.state.revision,
                    &completed.state.state_digest,
                    &wrong,
                    BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization
                )
                .is_err()
        );
    }
    let mut conflict = admitted.clone();
    conflict.rotation_id = body.rotation_id.clone();
    assert!(
        lease
            .sign_finality_rotation_proof(
                completed.state.revision,
                &completed.state.state_digest,
                &conflict,
                FinalityKeyRotationRoleV1::OldKeyAuthorization
            )
            .is_err()
    );
    let (_, mixed) = lease
        .sign_finality_rotation_proof(
            completed.state.revision,
            &completed.state.state_digest,
            &admitted,
            FinalityKeyRotationRoleV1::OldKeyAuthorization,
        )
        .unwrap();
    let mut conflict = body.clone();
    conflict.rotation_id = admitted.rotation_id;
    assert!(
        lease
            .sign_bootstrap_finality_rotation_proof(
                mixed.state.revision,
                &mixed.state.state_digest,
                &conflict,
                BootstrapFinalityKeyRotationRoleV1::NewKeyPossession
            )
            .is_err()
    );
    let mut corrupt = mixed.state.clone();
    corrupt
        .bootstrap_finality_rotation_proofs
        .as_mut()
        .unwrap()
        .records[0]
        .body
        .bootstrap_registry_digest_hex = hash(74);
    assert!(validate_archive(&binding, &corrupt).is_err());
    let mut erased = mixed.state.candidate();
    erased.bootstrap_finality_rotation_proofs = None;
    assert!(
        lease
            .advance(mixed.state.revision, &mixed.state.state_digest, erased)
            .is_err()
    );
    assert_eq!(lease.checkpoint().unwrap().state, mixed.state);
    drop(lease);
    server.join().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, server) = native(&binding, &snapshot, Some(calls.clone()));
    let mut lease = provider.acquire(&binding).unwrap();
    for (role, expected) in [
        (BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization, old),
        (BootstrapFinalityKeyRotationRoleV1::NewKeyPossession, new),
    ] {
        let (actual, cp) = lease
            .sign_bootstrap_finality_rotation_proof(
                enabled.state.revision,
                &enabled.state.state_digest,
                &body,
                role,
            )
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(cp.state, mixed.state);
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "cold exact retry cannot access either unavailable private key"
    );
    drop(lease);
    server.join().unwrap();
}

#[test]
fn mixed_archives_share_count_bytes_ids_and_revision_capacity() {
    use crate::protected_bft_v3::rotation::validate_shared_archives;
    let binding = binding();
    let mut admitted = BftFinalityRotationProofArchiveV3 {
        schema_version: 1,
        records: Vec::new(),
    };
    for i in 0..32 {
        let mut b = admitted_body(&binding);
        b.rotation_id = format!("admitted-{i}");
        let p = oasis7_agent_api::world_action::FinalityKeyRotationProofV1 {
            schema_version: 1,
            role: FinalityKeyRotationRoleV1::OldKeyAuthorization,
            signature_hex: "0".repeat(128),
        };
        admitted.records.push(BftFinalityRotationProofRecordV3 {
            body: b,
            public_key_hex: public(31),
            proof: p,
            recorded_revision: i + 1,
        });
    }
    let mut bootstrap = BftBootstrapFinalityRotationProofArchiveV3 {
        schema_version: 1,
        records: (0..32)
            .map(|i| record(&binding, &format!("bootstrap-{i}"), i + 33))
            .collect(),
    };
    validate_shared_archives(&Some(admitted.clone()), &Some(bootstrap.clone())).unwrap();
    bootstrap.records.push(record(&binding, "overflow", 65));
    assert!(validate_shared_archives(&Some(admitted.clone()), &Some(bootstrap.clone())).is_err());
    bootstrap.records.pop();
    bootstrap.records[0].body.rotation_id = admitted.records[0].body.rotation_id.clone();
    assert!(validate_shared_archives(&Some(admitted.clone()), &Some(bootstrap.clone())).is_err());
    bootstrap.records[0].body.rotation_id = "bootstrap-0".into();
    bootstrap.records[0].recorded_revision = 1;
    assert!(validate_shared_archives(&Some(admitted.clone()), &Some(bootstrap.clone())).is_err());
    bootstrap.records[0].recorded_revision = 33;
    admitted.records.truncate(1);
    bootstrap.records.truncate(1);
    admitted.records[0].body.rotation_id = "a".repeat(600_000);
    bootstrap.records[0].body.rotation_id = "b".repeat(600_000);
    assert!(
        validate_shared_archives(&Some(admitted), &Some(bootstrap)).is_err(),
        "two individually bounded archives must not get separate byte budgets"
    );
}

#[test]
fn bootstrap_archive_declared_count_and_empty_extension_fail_closed() {
    let mut bytes = vec![0xa1];
    bytes.extend(serde_cbor::to_vec(&"records").unwrap());
    bytes.extend([0x98, 0x41]);
    let error = serde_cbor::from_slice::<BftBootstrapFinalityRotationProofArchiveV3>(&bytes)
        .unwrap_err()
        .to_string();
    assert!(error.contains("record count exceeds bound"), "{error}");
    let binding = binding();
    let mut state = BftSignerStateV3::from_handoff(&binding).unwrap();
    state.membership_install = Some(BftMembershipInstallStateV3::enabled());
    state.bootstrap_finality_rotation_proofs = Some(BftBootstrapFinalityRotationProofArchiveV3 {
        schema_version: 1,
        records: Vec::new(),
    });
    assert!(validate_archive(&binding, &state).is_err());
}

// Before-bootstrap reader shape: new nonempty state must not be silently dropped.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    finality_rotation_proofs: Option<BftFinalityRotationProofArchiveV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    membership_install: Option<BftMembershipInstallStateV3>,
    schema_version: u32,
    binding_digest: String,
    revision: u64,
    history: Vec<oasis7_formal_protocol::bft::FormalBftCertifiedHeaderV3>,
    journal: Option<oasis7_consensus::tendermint_v3::BftSigningJournalV3>,
    completed_journals: Vec<Option<oasis7_consensus::tendermint_v3::BftSigningJournalV3>>,
    pending_driver_apply: Option<BftDriverApplyIntentV3>,
    applied_installs: Vec<BftDriverAppliedReceiptV3>,
    state_digest: String,
}
#[test]
fn absent_extension_preserves_old_state_bytes_and_digest_and_old_reader_rejects_new() {
    let binding = binding();
    let state = BftSignerStateV3::from_handoff(&binding).unwrap();
    let bytes = serde_cbor::to_vec(&state).unwrap();
    let legacy: LegacyState = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(serde_cbor::to_vec(&legacy).unwrap(), bytes);
    let roundtrip: BftSignerStateV3 =
        serde_cbor::from_slice(&serde_cbor::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(roundtrip, state);
    roundtrip.validate(&binding, &public(33)).unwrap();
    let mut nonempty = state;
    nonempty.bootstrap_finality_rotation_proofs =
        Some(BftBootstrapFinalityRotationProofArchiveV3 {
            schema_version: 1,
            records: vec![record(&binding, "nonempty", 1)],
        });
    assert!(
        serde_cbor::from_slice::<LegacyState>(&serde_cbor::to_vec(&nonempty).unwrap()).is_err()
    );
    let mut explicit_none = serde_cbor::value::to_value(&roundtrip).unwrap();
    let serde_cbor::Value::Map(ref mut map) = explicit_none else {
        panic!("state map")
    };
    map.insert(
        serde_cbor::Value::Text("bootstrap_finality_rotation_proofs".into()),
        serde_cbor::Value::Null,
    );
    assert!(
        crate::protected_bft_v3::crypto::decode_bounded::<BftSignerStateV3>(
            &serde_cbor::to_vec(&explicit_none).unwrap(),
            crate::protected_bft_v3::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3
        )
        .is_err()
    );
}
