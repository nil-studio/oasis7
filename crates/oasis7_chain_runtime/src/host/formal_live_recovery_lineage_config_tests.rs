//! Real signed ordinary page and retained permission chain loader regressions.
use super::*;
use crate::formal_live_recovery_test_support as recovery_test_support;
use oasis7::world_service::formal_read::{
    FormalReadHistoryPageV1, FormalReadHistoryV1, FormalReadReplayCursorV1, FormalReadStepV1,
};
use oasis7_distfs::controlled_authority::{
    fencing::*,
    formal::*,
    replicated_protocol::{ArtifactRole, EndpointRole, HeadAnchor},
};
use serde::Serialize;
fn normal_execution_proof(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    salt: u8,
    signer_keys: [u8; 3],
) -> FormalQualifiedCommitProofV1 {
    let request_digest = fixtures::h(salt);
    let payload_digest = fixtures::h(salt.wrapping_add(1));
    let after_state_root = fixtures::h(salt.wrapping_add(2));
    let frozen_body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: parent.record().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition().epoch,
        activation_ref: authority.activation_ref().to_owned(),
        parent: parent.head(),
        next_position: parent.record().position + 1,
        request_digest: request_digest.clone(),
        payload_digest: payload_digest.clone(),
        runtime_manifest_digest: parent.record().runtime_manifest_digest.clone(),
        governing_manifest_digest: parent.record().governing_manifest_digest.clone(),
        execution_policy_digest: parent.record().execution_policy_digest.clone(),
        pre_state_root: parent.record().after_state_root.clone(),
        frozen_material_digest: fixtures::h(salt.wrapping_add(3)),
        handoff: None,
    };
    let reservation_id = frozen_body.reservation_id().unwrap();
    let frozen_receipt = |role, binding: &EndpointSignerBindingV1, key_id| {
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: reservation_id.clone(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: fixtures::sign(frozen_receipt_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        reservation: SignedFormalFrozenV1 {
            signature_hex: fixtures::sign(
                frozen_signing_bytes(&frozen_body).unwrap(),
                signer_keys[0],
            ),
            body: frozen_body,
        },
        primary: frozen_receipt(
            EndpointRole::Primary,
            &authority.definition().primary,
            signer_keys[1],
        ),
        replica: frozen_receipt(
            EndpointRole::Replica,
            &authority.definition().replica,
            signer_keys[2],
        ),
    };

    let mut record = parent.record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position += 1;
    record.parent_record_id = parent.head().record_id;
    record.original_authority_id = authority.authority_id().unwrap();
    record.original_epoch = authority.definition().epoch;
    record.activation_ref = authority.activation_ref().to_owned();
    record.request_digest = request_digest;
    record.payload_digest = payload_digest.clone();
    record.frozen_reservation_id = Some(reservation_id);
    record.before_state_root = parent.record().after_state_root.clone();
    record.after_state_root = after_state_root.clone();
    record.action_root = fixtures::h(salt.wrapping_add(4));
    record.event_root = fixtures::h(salt.wrapping_add(5));
    record.result_root = fixtures::h(salt.wrapping_add(6));
    record
        .artifact_roots
        .insert(ArtifactRole::Input, payload_digest);
    record
        .artifact_roots
        .insert(ArtifactRole::Snapshot, after_state_root);
    fixtures::qualified_proof(
        record,
        Some(frozen),
        authority,
        Some(parent),
        None,
        signer_keys[0],
        signer_keys[1],
        signer_keys[2],
    )
}

fn domain_bytes<T: Serialize>(domain: &str, value: &T) -> Vec<u8> {
    let mut bytes = (domain.len() as u64).to_be_bytes().to_vec();
    bytes.extend(domain.as_bytes());
    bytes.extend(serde_cbor::to_vec(value).unwrap());
    bytes
}

fn epoch_seal(
    predecessor: &VerifiedAuthorityV1,
    cut: &VerifiedFormalReadAnchorV1,
    next_epoch: u64,
    signer_key: u8,
    role: EndpointRole,
) -> EpochSealReceiptV1 {
    let instance = match role {
        EndpointRole::Primary => predecessor.definition().primary.instance.clone(),
        EndpointRole::Replica => predecessor.definition().replica.instance.clone(),
    };
    let complete = HeadAnchor {
        position: cut.head().record().position,
        decision_hash: cut.head().head().record_id.clone(),
        qualified: true,
    };
    let checkpoint_before = EndpointCheckpointBindingV1 {
        revision: 4,
        commitment: fixtures::h(120 + signer_key),
        head: complete.clone(),
    };
    let body = EpochSealBodyV1 {
        schema_version: 1,
        instance: instance.clone(),
        recovery_operation: fixtures::h(150 + next_epoch as u8),
        highest_complete: complete.clone(),
        checkpoint_before: checkpoint_before.clone(),
        tail: SealedTailV1::empty(),
        next_epoch_floor: next_epoch,
    };
    let checkpoint = EndpointCheckpointBindingV1 {
        revision: checkpoint_before.revision + 1,
        commitment: fixtures::h(160 + signer_key),
        head: complete,
    };
    let resource = predecessor.policy().authorized_fencing_resource_bindings[0].clone();
    let statement = MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: resource.clone(),
        instance_digest: instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        epoch_floor: next_epoch,
        seal_digest: Some(body.digest().unwrap()),
    };
    let provider = MonotonicProviderEvidenceV1 {
        signature: fixtures::sign(
            domain_bytes("oasis7.endpoint.monotonic-provider.v1", &statement),
            3,
        ),
        statement,
    };
    let mut receipt =
        EpochSealReceiptV1::sign(body.clone(), provider, &fixtures::key(signer_key)).unwrap();
    let request = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance.digest().unwrap(),
        checkpoint,
        challenge: fixtures::h(170 + signer_key),
        purpose: ProtectedEndpointSignaturePurposeV1::EpochSeal,
        preimage: domain_bytes("oasis7.endpoint.epoch-seal.v1", &body),
    };
    let protected_body = ProtectedEndpointSignatureBodyV1 {
        schema_version: 1,
        custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
        instance_digest: instance.digest().unwrap(),
        resource_binding: resource,
        checkpoint: request.checkpoint.clone(),
        request_digest: request.digest().unwrap(),
        challenge: request.challenge.clone(),
        endpoint_signature: receipt.endpoint_signature.clone(),
    };
    receipt.protected_signature = Some(ProtectedEndpointSignatureV1 {
        provider_signature: fixtures::sign(
            domain_bytes(
                "oasis7.formal.protected_endpoint_signature.v1",
                &protected_body,
            ),
            3,
        ),
        body: protected_body,
    });
    receipt
}

fn successor(
    predecessor: &VerifiedAuthorityV1,
    cut: &VerifiedFormalReadAnchorV1,
    epoch: u64,
    signer_keys: [u8; 3],
) -> AuthorityDefinitionV1 {
    let mut next = predecessor.definition().clone();
    next.epoch = epoch;
    next.writer_public_key = fixtures::public(signer_keys[0]);
    next.predecessor_authority_id = Some(predecessor.authority_id().unwrap());
    next.valid_from_position = cut.head().record().position + 1;
    for (binding, signer, role) in [
        (&mut next.primary, signer_keys[1], EndpointRole::Primary),
        (&mut next.replica, signer_keys[2], EndpointRole::Replica),
    ] {
        binding.instance.authority_epoch = epoch;
        binding.instance.endpoint_role = role;
        binding.instance.endpoint_id = format!("history-{epoch}-{role:?}");
        binding.instance.endpoint_public_key = fixtures::public(signer);
        binding.instance.instance_id = fixtures::h(signer + 10);
        binding.instance.storage_id = fixtures::h(signer + 20);
        binding.instance.storage_fingerprint = fixtures::h(signer + 30);
        binding.fault_domain_id = format!("history-domain-{epoch}-{role:?}");
    }
    next
}

fn permission_evidence_at_cut(
    cut: &VerifiedFormalReadAnchorV1,
    trust: &InitialActivationTrustV1,
    epoch: u64,
    next_signers: [u8; 3],
    predecessor_sealers: [u8; 2],
    prior: Option<&VerifiedRecoveryBootstrapV1>,
    floor: &issuer::IssuerAnchorV1,
) -> (RecoveryPermissionBootstrapV1, VerifiedRecoveryBootstrapV1) {
    let successor = successor(cut.authority(), cut, epoch, next_signers);
    let seals = [
        epoch_seal(
            cut.authority(),
            cut,
            epoch,
            predecessor_sealers[0],
            EndpointRole::Primary,
        ),
        epoch_seal(
            cut.authority(),
            cut,
            epoch,
            predecessor_sealers[1],
            EndpointRole::Replica,
        ),
    ];
    recovery_test_support::permission_for_sealed_endpoints(
        cut,
        trust,
        successor,
        RecoveryTailWitnessV1::Empty,
        seals,
        prior,
        &fixtures::key(1),
        &fixtures::key(2),
        floor.clone(),
    )
    .unwrap()
}

fn cursor(a: &VerifiedFormalReadAnchorV1) -> FormalReadReplayCursorV1 {
    FormalReadReplayCursorV1 {
        schema_version: 1,
        anchor: oasis7_client_api::world_service::FormalReadAnchorRefV1 {
            position: a.head().record().position,
            record_id: a.head().head().record_id,
            active_authority_id: a.authority().authority_id().unwrap(),
        },
        checkpoint_digest: a.checkpoint().digest().unwrap(),
        last_recovery_digest: None,
        pending_recovery_digest: None,
        requires_v2_checkpoint: false,
    }
}
fn page(
    origin: &VerifiedFormalReadAnchorV1,
    salt: u8,
) -> (FormalReadHistoryPageV1, VerifiedFormalReadAnchorV1) {
    let proof = normal_execution_proof(origin.authority(), origin.head(), salt, [4, 5, 6]);
    let commit =
        verify_formal_qualified(&proof, origin.authority(), Some(origin.head()), None).unwrap();
    let cut = origin
        .after_verified_commit(origin.authority().clone(), commit)
        .unwrap();
    (
        FormalReadHistoryPageV1 {
            schema_version: 1,
            start: cursor(origin),
            history: FormalReadHistoryV1 {
                schema_version: 1,
                steps: vec![FormalReadStepV1::Commit(Box::new(proof))],
            },
            end: cursor(&cut),
        },
        cut,
    )
}
#[test]
fn formal_recovery_loader_advanced_cut_and_retained_chain_preserve_c0_and_reject_forks() {
    let fixture = Fixture::new();
    let c0 = fixture.anchor.clone();
    let trust_bytes = fs::read(fixture.root.0.join("formal-read-trust.json")).unwrap();
    let trust: FormalReadTrustConfigV1 = serde_json::from_slice(&trust_bytes).unwrap();
    let (first_page, cut) = page(&c0, 71);
    let initial_floor = issuer::IssuerAnchorV1::initial(c0.authority().policy()).unwrap();
    let (raw2, verified2) = permission_evidence_at_cut(
        &cut,
        &trust.initial_trust,
        2,
        [10, 11, 12],
        [5, 6],
        None,
        &initial_floor,
    );
    let active2 = cut.with_empty_recovery(verified2.recovery()).unwrap();
    let (raw3, verified3) = permission_evidence_at_cut(
        &active2,
        &trust.initial_trust,
        3,
        [13, 14, 15],
        [11, 12],
        Some(&verified2),
        &raw2.issuer_witness.body.current_anchor,
    );
    let directory = fixture.root.0.join("origin-pages");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let page_path = directory.join("00000000000000000000.cbor");
    write_private(&page_path, &first_page.encode_bytes().unwrap());
    let permission_path = fixture.root.0.join("permission.json");
    write_private(&permission_path, &serde_json::to_vec(&raw3).unwrap());
    let floor_path = fixture.root.0.join("issuer-minimum.json");
    let floor = recovery_config::FormalRecoveryIssuerMinimumFileV1 {
        schema_version: 1,
        world: c0.authority().definition().world.clone(),
        full_policy_digest: c0.authority().policy().digest().unwrap(),
        recovery_issuer_public_key: c0.authority().policy().recovery_issuer_public_key.clone(),
        issuer_anchor_public_key: c0.authority().policy().issuer_anchor_public_key.clone(),
        anchor: initial_floor.clone(),
    };
    write_private(&floor_path, &serde_json::to_vec(&floor).unwrap());
    let load = || {
        recovery_config::load_recovery_permission(
            &permission_path,
            &floor_path,
            Some(&directory),
            &trust,
            &c0,
        )
    };
    let actual = load().unwrap();
    assert_eq!(
        actual.external_anchor().checkpoint(),
        verified3.external_anchor().checkpoint()
    );
    assert_eq!(actual.cut().head(), cut.head().head());
    assert!(
        recovery_config::load_recovery_permission(&permission_path, &floor_path, None, &trust, &c0)
            .is_err()
    );
    let (fork_page, fork_cut) = page(&c0, 81);
    write_private(&page_path, &fork_page.encode_bytes().unwrap());
    assert!(load().is_err());
    write_private(&page_path, &first_page.encode_bytes().unwrap());
    let (fork2, _) = permission_evidence_at_cut(
        &fork_cut,
        &trust.initial_trust,
        2,
        [10, 11, 12],
        [5, 6],
        None,
        &initial_floor,
    );
    let mut bad = raw3.clone();
    bad.prior_permission = Some(Box::new(fork2));
    write_private(&permission_path, &serde_json::to_vec(&bad).unwrap());
    assert!(load().is_err());
    let mut bad = raw3;
    bad.predecessor_checkpoint.head_record.request_digest = fixtures::h(222);
    write_private(&permission_path, &serde_json::to_vec(&bad).unwrap());
    assert!(load().is_err());
    assert_eq!(
        fs::read(fixture.root.0.join("formal-read-trust.json")).unwrap(),
        trust_bytes
    );
    assert_eq!(
        fs::read_dir(fixture.root.0.join("writer")).unwrap().count(),
        0
    );
}
