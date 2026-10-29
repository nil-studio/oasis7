use super::*;
use crate::protected_bft_v3::successor_public_archive::*;

#[test]
fn whole_state_signed_index_is_complete_and_charges_one_physical_object() {
    let (binding, backend) = fixture_kind(true);
    let state = backend.lock().unwrap().state.clone();
    let checkpoint = BftSignerCheckpointV3 {
        state: state.clone(),
        lease_generation: 1,
        challenge: String::new(),
        request_digest: String::new(),
        provider_public_key: String::new(),
        provider_signature_hex: String::new(),
    };
    let (_, mut manifest) = request(&binding, &checkpoint);
    let legacy_bytes = oasis7_cas::to_canonical_cbor(&manifest).unwrap();
    let mut legacy_value = serde_json::to_value(&manifest).unwrap();
    assert!(legacy_value.get("public_index").is_none());
    legacy_value.as_object_mut().unwrap().remove("public_index");
    assert_eq!(
        legacy_bytes,
        oasis7_cas::to_canonical_cbor(&legacy_value).unwrap()
    );
    let legacy = manifest.clone();
    index_whole_state_signed_results_v1(&mut manifest, &state).unwrap();
    assert!(manifest.entries.len() > 1);
    // All aliases consume one physical S, independent of entry count.
    manifest.usage.objects = 1;
    manifest.usage.total_bytes = manifest.entries[0].object.bytes;
    manifest.validate().unwrap();
    validate_actual_public_manifest(&manifest, &state).unwrap();
    assert_ne!(legacy.digest().unwrap(), manifest.digest().unwrap());
    let mut missing = manifest.clone();
    missing
        .entries
        .retain(|e| e.identity_digest == state.state_digest);
    assert!(validate_actual_public_manifest(&missing, &state).is_err());
    let mut unknown = manifest.clone();
    unknown.entries.push(BftSuccessorWindowArchiveEntryV1 {
        identity_digest: "f".repeat(64),
        object: manifest.entries[0].object.clone(),
    });
    unknown
        .entries
        .sort_by(|a, b| a.identity_digest.cmp(&b.identity_digest));
    assert!(validate_actual_public_manifest(&unknown, &state).is_err());
    let mut extra_object = manifest.clone();
    extra_object.entries[0].object.digest = "e".repeat(64);
    assert!(validate_actual_public_manifest(&extra_object, &state).is_err());
    let mut execution = manifest;
    execution.kind = BftSuccessorWindowArchiveKindV1::Execution;
    assert!(execution.validate().is_err());
}

#[test]
fn signed_index_uses_raw_preimage_and_requires_real_signature() {
    let (_, backend) = fixture_kind(true);
    let mut state = backend.lock().unwrap().state.clone();
    let proposal = state
        .completed_journals
        .iter()
        .flatten()
        .flat_map(|j| &j.events)
        .find_map(|e| match e {
            BftSigningJournalEventV3::Proposal {
                proposal,
                local: true,
                ..
            } => Some(proposal.clone()),
            _ => None,
        })
        .unwrap();
    let identity = BftSuccessorWindowArchiveIdentityV1::SigningSlot {
        public_key_hex: proposal.header.public_key_hex.clone(),
        height: proposal.header.height,
        round: proposal.header.round,
        phase: "proposal".into(),
        preimage_digest: blake3::hash(&proposal.header.signing_payload().unwrap())
            .to_hex()
            .to_string(),
    };
    assert_eq!(
        resolve_whole_state_signed_result_v1(&state, &identity).unwrap(),
        Some(BftSuccessorWindowOriginalSignedResultV1::Proposal(proposal))
    );
    let mut conflict = identity;
    if let BftSuccessorWindowArchiveIdentityV1::SigningSlot {
        preimage_digest, ..
    } = &mut conflict
    {
        *preimage_digest = "d".repeat(64);
    }
    assert!(resolve_whole_state_signed_result_v1(&state, &conflict).is_err());
    for journal in state.completed_journals.iter_mut().flatten() {
        for event in &mut journal.events {
            if let BftSigningJournalEventV3::Proposal {
                proposal,
                local: true,
                ..
            } = event
            {
                proposal.header.signature_hex = "00".repeat(64);
            }
        }
    }
    assert!(whole_state_signed_result_entries_v1(&state).is_err());
}

#[test]
fn typed_proof_aliases_verify_all_four_roles_and_canonical_body_digest() {
    use crate::protected_bft_v3::{bootstrap_rotation::*, rotation::*};
    use ed25519_dalek::Signer;
    use oasis7_agent_api::world_action::*;
    let (binding, backend) = fixture_kind(true);
    let mut state = backend.lock().unwrap().state.clone();
    let body = super::super::body(&binding);
    let bootstrap = BootstrapFinalityKeyRotationBodyV1 {
        schema_version: 1,
        authorization: body.authorization.clone(),
        rotation_id: "bootstrap".into(),
        bootstrap_registry_digest_hex: hash(72),
        bootstrap_public_key_hex: public(31),
        node_id: body.node_id.clone(),
        expected_generation: 0,
        old_public_key_hex: public(31),
        new_public_key_hex: public(32),
        decision_height: body.decision_height,
        activation_height: body.activation_height,
        target_epoch: body.target_epoch,
    };
    let mut admitted_records = Vec::new();
    let mut bootstrap_records = Vec::new();
    for (n, role, bootstrap_role) in [
        (
            31,
            FinalityKeyRotationRoleV1::OldKeyAuthorization,
            BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        ),
        (
            32,
            FinalityKeyRotationRoleV1::NewKeyPossession,
            BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
        ),
    ] {
        admitted_records.push(BftFinalityRotationProofRecordV3 {
            body: body.clone(),
            public_key_hex: public(n),
            recorded_revision: 1,
            proof: FinalityKeyRotationProofV1 {
                schema_version: 1,
                role,
                signature_hex: hex::encode(
                    key(n).sign(&body.signing_bytes(role).unwrap()).to_bytes(),
                ),
            },
        });
        bootstrap_records.push(BftBootstrapFinalityRotationProofRecordV3 {
            body: bootstrap.clone(),
            public_key_hex: public(n),
            recorded_revision: 1,
            proof: BootstrapFinalityKeyRotationProofV1 {
                schema_version: 1,
                role: bootstrap_role,
                signature_hex: hex::encode(
                    key(n)
                        .sign(&bootstrap.signing_bytes(bootstrap_role).unwrap())
                        .to_bytes(),
                ),
            },
        });
    }
    state.finality_rotation_proofs = Some(BftFinalityRotationProofArchiveV3 {
        schema_version: 1,
        records: admitted_records,
    });
    state.bootstrap_finality_rotation_proofs = Some(BftBootstrapFinalityRotationProofArchiveV3 {
        schema_version: 1,
        records: bootstrap_records,
    });
    let entries = whole_state_signed_result_entries_v1(&state).unwrap();
    for (typed_body, roles) in [
        (
            oasis7_cas::to_canonical_cbor(&body).unwrap(),
            ["admitted_old", "admitted_new"],
        ),
        (
            oasis7_cas::to_canonical_cbor(&bootstrap).unwrap(),
            ["bootstrap_old", "bootstrap_new"],
        ),
    ] {
        for role in roles {
            let identity = BftSuccessorWindowArchiveIdentityV1::FinalityProof {
                body_digest: blake3::hash(&typed_body).to_hex().to_string(),
                role: role.into(),
            };
            assert!(
                entries
                    .iter()
                    .any(|e| e.identity_digest == identity.digest().unwrap())
            );
            assert!(
                resolve_whole_state_signed_result_v1(&state, &identity)
                    .unwrap()
                    .is_some()
            );
        }
    }
    state
        .bootstrap_finality_rotation_proofs
        .as_mut()
        .unwrap()
        .records[1]
        .proof
        .signature_hex = "00".repeat(64);
    assert!(whole_state_signed_result_entries_v1(&state).is_err());
}

#[test]
fn remote_events_are_excluded_and_legacy_cannot_silently_enable_aliases() {
    let (binding, backend) = fixture_kind(true);
    let mut state = backend.lock().unwrap().state.clone();
    for journal in state.completed_journals.iter_mut().flatten() {
        for event in &mut journal.events {
            match event {
                BftSigningJournalEventV3::Proposal { local, .. }
                | BftSigningJournalEventV3::Vote { local, .. } => *local = false,
                _ => {}
            }
        }
    }
    assert_eq!(
        whole_state_signed_result_entries_v1(&state).unwrap().len(),
        1
    );
    let checkpoint = BftSignerCheckpointV3 {
        state: backend.lock().unwrap().state.clone(),
        lease_generation: 1,
        challenge: String::new(),
        request_digest: String::new(),
        provider_public_key: String::new(),
        provider_signature_hex: String::new(),
    };
    let (_, mut manifest) = request(&binding, &checkpoint);
    index_whole_state_signed_results_v1(&mut manifest, &checkpoint.state).unwrap();
    manifest.public_index = None;
    assert!(validate_actual_public_manifest(&manifest, &checkpoint.state).is_err());
}
