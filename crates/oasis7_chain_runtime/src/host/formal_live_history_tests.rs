use super::business::OriginHistoryProofPages;
use super::*;
#[cfg(unix)]
use oasis7::world_service::formal_read::{
    FormalReadHistoryPageV1, FormalReadHistoryV1, FormalReadReplayCursorV1,
};
use oasis7_distfs::controlled_authority::{
    LocalRequestIdentity,
    fencing::*,
    formal::*,
    replicated_protocol::{ArtifactRole, EndpointRole, HeadAnchor},
};
use serde::Serialize;
#[cfg(unix)]
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
static TEMP_PROOF_PAGES: AtomicU64 = AtomicU64::new(0);

use crate::formal_read_fixtures as fixtures;

use crate::formal_live_recovery_test_support as recovery_test_support;

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

fn epoch_seal_with_tail(
    predecessor: &VerifiedAuthorityV1,
    cut: &VerifiedFormalReadAnchorV1,
    next_epoch: u64,
    signer_key: u8,
    role: EndpointRole,
    tail: SealedTailV1,
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
        tail,
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
    permission_evidence_with_tail(
        cut,
        trust,
        epoch,
        next_signers,
        predecessor_sealers,
        RecoveryTailWitnessV1::Empty,
        SealedTailV1::empty(),
        prior,
        floor,
    )
}

#[allow(clippy::too_many_arguments)]
fn permission_evidence_with_tail(
    cut: &VerifiedFormalReadAnchorV1,
    trust: &InitialActivationTrustV1,
    epoch: u64,
    next_signers: [u8; 3],
    predecessor_sealers: [u8; 2],
    witness: RecoveryTailWitnessV1,
    sealed_tail: SealedTailV1,
    prior: Option<&VerifiedRecoveryBootstrapV1>,
    floor: &issuer::IssuerAnchorV1,
) -> (RecoveryPermissionBootstrapV1, VerifiedRecoveryBootstrapV1) {
    let successor = successor(cut.authority(), cut, epoch, next_signers);
    let seals = [
        epoch_seal_with_tail(
            cut.authority(),
            cut,
            epoch,
            predecessor_sealers[0],
            EndpointRole::Primary,
            sealed_tail.clone(),
        ),
        epoch_seal_with_tail(
            cut.authority(),
            cut,
            epoch,
            predecessor_sealers[1],
            EndpointRole::Replica,
            sealed_tail,
        ),
    ];
    recovery_test_support::permission_for_sealed_endpoints(
        cut,
        trust,
        successor,
        witness,
        seals,
        prior,
        &fixtures::key(1),
        &fixtures::key(2),
        floor.clone(),
    )
    .unwrap()
}

#[test]
fn local_history_proofs_are_reverified_and_forks_cannot_reach_the_exact_target() {
    let (_world, _trust, origin, _) = fixtures::actual_world_initial_fixture();
    let proof = normal_execution_proof(origin.authority(), origin.head(), 71, [4, 5, 6]);
    let target = advance_normal_history_proof(&origin, &proof).unwrap();
    require_exact_history_target(&target, &target).unwrap();

    let mut tampered = proof.clone();
    tampered.primary_complete.signature_hex = fixtures::h(72);
    assert!(advance_normal_history_proof(&origin, &tampered).is_err());

    // This is independently valid under the same parent but names another
    // qualified record. Cached proofs remain candidates until they reach the
    // already published opaque target exactly.
    let fork = normal_execution_proof(origin.authority(), origin.head(), 73, [4, 5, 6]);
    let fork_cursor = advance_normal_history_proof(&origin, &fork).unwrap();
    assert_ne!(fork_cursor.head().head(), target.head().head());
    assert!(require_exact_history_target(&fork_cursor, &target).is_err());
}

#[test]
fn startup_cache_observer_runs_only_after_exact_proof_advances_anchor() {
    let (_world, _trust, origin, _) = fixtures::actual_world_initial_fixture();
    let proof = normal_execution_proof(origin.authority(), origin.head(), 74, [4, 5, 6]);
    let target = advance_normal_history_proof(&origin, &proof).unwrap();
    let mut observed = 0;
    let replayed = replay_exact_origin_after_verified(
        &origin,
        &read_anchor_ref(&target),
        &[],
        Instant::now() + Duration::from_secs(2),
        |_, position, _, _| {
            if position != proof.decision.record.position {
                return Err(ReplayFailure::Invalid("unexpected proof position".into()));
            }
            Ok(ReplayStepProof {
                proof: proof.clone(),
                candidate: None,
            })
        },
        |predecessor, verified, observed_proof, candidate| {
            assert_eq!(predecessor.checkpoint(), origin.checkpoint());
            assert_eq!(verified.checkpoint(), target.checkpoint());
            assert_eq!(observed_proof, &proof);
            assert!(candidate.is_none());
            observed += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(replayed.anchor.checkpoint(), target.checkpoint());
    assert_eq!(observed, 1);

    let mut invalid = proof;
    invalid.primary_complete.signature_hex = fixtures::h(75);
    let mut observed_invalid = 0;
    let rejected = replay_exact_origin_after_verified(
        &origin,
        &read_anchor_ref(&target),
        &[],
        Instant::now() + Duration::from_secs(2),
        |_, _, _, _| {
            Ok(ReplayStepProof {
                proof: invalid.clone(),
                candidate: None,
            })
        },
        |_, _, _, _| {
            observed_invalid += 1;
            Ok(())
        },
    );
    assert!(rejected.is_err());
    assert_eq!(observed_invalid, 0);
}

#[test]
fn prewarm_head_hint_mismatch_skips_only_after_valid_replay() {
    let (_world, _trust, origin, _) = fixtures::actual_world_initial_fixture();
    let proof = normal_execution_proof(origin.authority(), origin.head(), 75, [4, 5, 6]);
    let target = advance_normal_history_proof(&origin, &proof).unwrap();
    let wrong_head = FormalReadAnchorRefV1 {
        record_id: fixtures::h(76),
        ..read_anchor_ref(&target)
    };
    let mut observed = 0;
    let result = replay_exact_origin_after_verified_with_policy(
        &origin,
        &wrong_head,
        &[],
        Instant::now() + Duration::from_secs(2),
        ReplayTargetPolicy::ProtectedHeadHint,
        |_, position, _, _| {
            assert_eq!(position, proof.decision.record.position);
            Ok(ReplayStepProof {
                proof: proof.clone(),
                candidate: None,
            })
        },
        |_, _, _, _| {
            observed += 1;
            Ok(())
        },
    );
    assert!(matches!(result, Err(ReplayFailure::TargetMismatch)));
    assert_eq!(
        observed, 1,
        "the prefix must verify before mismatch is optional"
    );

    let mut corrupted = proof;
    corrupted.primary_complete.signature_hex = fixtures::h(77);
    let mut observed_corrupt = 0;
    let rejected = replay_exact_origin_after_verified_with_policy(
        &origin,
        &wrong_head,
        &[],
        Instant::now() + Duration::from_secs(2),
        ReplayTargetPolicy::ProtectedHeadHint,
        |_, _, _, _| {
            Ok(ReplayStepProof {
                proof: corrupted.clone(),
                candidate: None,
            })
        },
        |_, _, _, _| {
            observed_corrupt += 1;
            Ok(())
        },
    );
    assert!(matches!(rejected, Err(ReplayFailure::Invalid(_))));
    assert_eq!(
        observed_corrupt, 0,
        "a bad proof must never become a hint miss"
    );
}

#[test]
fn prewarm_partial_prefix_never_reaches_installable_target() {
    let (_world, _trust, origin, _) = fixtures::actual_world_initial_fixture();
    let first = normal_execution_proof(origin.authority(), origin.head(), 78, [4, 5, 6]);
    let first_cut = advance_normal_history_proof(&origin, &first).unwrap();
    let second = normal_execution_proof(origin.authority(), first_cut.head(), 79, [4, 5, 6]);
    let target = advance_normal_history_proof(&first_cut, &second).unwrap();
    let mut observed = Vec::new();

    let result = replay_exact_origin_after_verified_with_policy(
        &origin,
        &read_anchor_ref(&target),
        &[],
        Instant::now() + Duration::from_secs(2),
        ReplayTargetPolicy::ProtectedHeadHint,
        |_, position, _, _| match position {
            p if p == first.decision.record.position => Ok(ReplayStepProof {
                proof: first.clone(),
                candidate: None,
            }),
            p if p == second.decision.record.position => {
                Err(ReplayFailure::LocalPublicationMissing)
            }
            _ => Err(ReplayFailure::Invalid(format!(
                "unexpected position {position}"
            ))),
        },
        |_, anchor, _, _| {
            observed.push(anchor.head().record().position);
            Ok(())
        },
    );

    assert!(matches!(
        result,
        Err(ReplayFailure::LocalPublicationMissing)
    ));
    assert_eq!(observed, [first.decision.record.position]);
    // A partial prefix is not an installable result: production only seeds
    // StartupBusinessNodeStage after this replay returns the exact target.
}

fn read_anchor_ref(anchor: &VerifiedFormalReadAnchorV1) -> FormalReadAnchorRefV1 {
    FormalReadAnchorRefV1 {
        position: anchor.head().record().position,
        record_id: anchor.head().head().record_id.clone(),
        active_authority_id: anchor.authority().authority_id().unwrap(),
    }
}

#[cfg(unix)]
fn replay_cursor(anchor: &VerifiedFormalReadAnchorV1) -> FormalReadReplayCursorV1 {
    FormalReadReplayCursorV1 {
        schema_version: 1,
        anchor: read_anchor_ref(anchor),
        checkpoint_digest: anchor.checkpoint().digest().unwrap(),
        last_recovery_digest: None,
        pending_recovery_digest: None,
        requires_v2_checkpoint: false,
    }
}

#[cfg(unix)]
fn proof_page(
    start: &VerifiedFormalReadAnchorV1,
    proof: FormalQualifiedCommitProofV1,
    end: &VerifiedFormalReadAnchorV1,
) -> FormalReadHistoryPageV1 {
    FormalReadHistoryPageV1 {
        schema_version: 1,
        start: replay_cursor(start),
        history: FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![FormalReadStepV1::Commit(Box::new(proof))],
        },
        end: replay_cursor(end),
    }
}

#[cfg(unix)]
struct PrivateProofPages(PathBuf);

#[cfg(unix)]
impl PrivateProofPages {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;

        let nonce = TEMP_PROOF_PAGES.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-formal-origin-pages-{}-{now}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }

    fn write(&self, index: u64, bytes: &[u8]) {
        use std::os::unix::fs::OpenOptionsExt;

        let path = self.0.join(format!("{index:020}.cbor"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

#[cfg(unix)]
impl Drop for PrivateProofPages {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[cfg(unix)]
fn proof_only_origin_step_keeps_replay_running_to_later_local_candidate() {
    let (_world, _trust, origin, _) = fixtures::actual_world_initial_fixture();
    let first_proof = normal_execution_proof(origin.authority(), origin.head(), 76, [4, 5, 6]);
    let first_cut = advance_normal_history_proof(&origin, &first_proof).unwrap();
    let second_proof = normal_execution_proof(origin.authority(), first_cut.head(), 77, [4, 5, 6]);
    let second_cut = advance_normal_history_proof(&first_cut, &second_proof).unwrap();

    let pages_dir = PrivateProofPages::new();
    pages_dir.write(
        0,
        &proof_page(&origin, first_proof.clone(), &first_cut)
            .encode_bytes()
            .unwrap(),
    );
    let mut pages = OriginHistoryProofPages::open(pages_dir.path()).unwrap();
    let mut observed_seed_positions = Vec::new();
    let replayed = replay_exact_origin_after_verified(
        &origin,
        &read_anchor_ref(&second_cut),
        &[],
        Instant::now() + Duration::from_secs(2),
        |_, position, _, _| {
            if position == first_proof.decision.record.position {
                return Ok(ReplayStepProof {
                    proof: pages
                        .proof_at(position)?
                        .ok_or(ReplayFailure::LocalPublicationMissing)?,
                    candidate: None,
                });
            }
            if position == second_proof.decision.record.position {
                let record = second_proof.decision.record.clone();
                return Ok(ReplayStepProof {
                    proof: second_proof.clone(),
                    candidate: Some(FormalCandidateMaterialV1 {
                        candidate: SignedCanonicalCandidateV1 {
                            record,
                            writer_signature_hex: second_proof
                                .decision
                                .writer_signature_hex
                                .clone(),
                        },
                        material: ClosedRecord {
                            payload_digest: String::new(),
                            before_state_root: String::new(),
                            after_state_root: String::new(),
                            roots: Default::default(),
                            objects: Vec::new(),
                        },
                        frozen: second_proof.decision.frozen.clone(),
                        frozen_material: None,
                        branch: FormalCompletionBranchV1::Normal,
                    }),
                });
            }
            Err(ReplayFailure::Invalid(format!(
                "unexpected proof position {position}"
            )))
        },
        |_, verified, proof, candidate| {
            assert_eq!(
                proof.decision.record.position,
                verified.head().record().position
            );
            if candidate.is_some() {
                observed_seed_positions.push(verified.head().record().position);
            }
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(replayed.anchor.checkpoint(), second_cut.checkpoint());
    assert_eq!(
        observed_seed_positions,
        [second_proof.decision.record.position]
    );
}

#[test]
#[cfg(unix)]
fn corrupt_origin_proof_page_fails_before_seed_observer() {
    let (_world, _trust, origin, _) = fixtures::actual_world_initial_fixture();
    let proof = normal_execution_proof(origin.authority(), origin.head(), 78, [4, 5, 6]);
    let target = advance_normal_history_proof(&origin, &proof).unwrap();
    let mut corrupted = proof.clone();
    corrupted.primary_complete.signature_hex = fixtures::h(79);
    let pages_dir = PrivateProofPages::new();
    pages_dir.write(
        0,
        &proof_page(&origin, corrupted, &target)
            .encode_bytes()
            .unwrap(),
    );
    let mut pages = OriginHistoryProofPages::open(pages_dir.path()).unwrap();
    let mut observed = 0;
    let result = replay_exact_origin_after_verified(
        &origin,
        &read_anchor_ref(&target),
        &[],
        Instant::now() + Duration::from_secs(2),
        |_, position, _, _| {
            Ok(ReplayStepProof {
                proof: pages
                    .proof_at(position)?
                    .ok_or(ReplayFailure::LocalPublicationMissing)?,
                candidate: None,
            })
        },
        |_, _, _, _| {
            observed += 1;
            Ok(())
        },
    );
    assert!(matches!(result, Err(ReplayFailure::Invalid(_))));
    assert_eq!(observed, 0);
}

#[test]
fn only_exact_verified_pending_recovery_target_is_optional_for_prewarm() {
    let (_world, trust, origin, _) = fixtures::actual_world_initial_fixture();
    let initial_floor = issuer::IssuerAnchorV1::initial(origin.authority().policy()).unwrap();
    let tail_proof = normal_execution_proof(origin.authority(), origin.head(), 80, [4, 5, 6]);
    let frozen = tail_proof.decision.frozen.clone().unwrap();
    let reservation_id = frozen.reservation.body.reservation_id().unwrap();
    let sealed_tail = SealedTailV1 {
        kind: SealedTailKindV1::Frozen,
        request: Some(LocalRequestIdentity {
            verified_subject: "formal-request-digest".into(),
            operation_domain: "oasis7.formal.request.v1".into(),
            nonce_scope: origin.authority().authority_id().unwrap(),
            request_id: frozen.reservation.body.request_digest.clone(),
        }),
        position: Some(tail_proof.decision.record.position),
        frozen_input_digest: Some(reservation_id),
        frozen_certificate_digest: None,
        proposal_digest: None,
        replica_prepare_digest: None,
        primary_receipt_digest: None,
        replica_receipt_digest: None,
    };
    let (_raw, pending_permission) = permission_evidence_with_tail(
        &origin,
        &trust,
        2,
        [14, 15, 16],
        [5, 6],
        RecoveryTailWitnessV1::Frozen(frozen),
        sealed_tail,
        None,
        &initial_floor,
    );
    let permissions = [pending_permission.clone()];
    let pending_target = origin
        .with_pending_recovery(pending_permission.recovery())
        .unwrap();
    let exact_target = read_anchor_ref(&pending_target);
    let no_proof_needed = |_: &VerifiedFormalReadAnchorV1, _: u64, _: Option<usize>, _: Instant| {
        Err(ReplayFailure::Invalid(
            "unexpected proof request for same-cut pending recovery".into(),
        ))
    };
    let exact = replay_exact_origin_after_verified(
        &origin,
        &exact_target,
        &permissions,
        Instant::now() + Duration::from_secs(2),
        no_proof_needed,
        |_, _, _, _| panic!("pending baseline must not seed material"),
    );
    assert!(matches!(
        exact,
        Err(ReplayFailure::UnqualifiedPendingRecoveryTarget(expected))
            if expected.as_ref() == &pending_target.checkpoint()
    ));

    let forged_target = FormalReadAnchorRefV1 {
        active_authority_id: fixtures::h(81),
        ..exact_target.clone()
    };
    let forged = replay_exact_origin_after_verified(
        &origin,
        &forged_target,
        &permissions,
        Instant::now() + Duration::from_secs(2),
        no_proof_needed,
        |_, _, _, _| panic!("forged pending target must not seed material"),
    );
    assert!(matches!(forged, Err(ReplayFailure::Invalid(_))));

    let unpermitted = replay_exact_origin_after_verified(
        &origin,
        &exact_target,
        &[],
        Instant::now() + Duration::from_secs(2),
        no_proof_needed,
        |_, _, _, _| panic!("unpermitted pending target must not seed material"),
    );
    assert!(matches!(unpermitted, Err(ReplayFailure::Invalid(_))));
}

#[test]
fn external_and_cached_epoch_origins_replay_consecutive_recovery_chain_exactly() {
    let (_world, trust, origin, _) = fixtures::actual_world_initial_fixture();
    let initial_floor = issuer::IssuerAnchorV1::initial(origin.authority().policy()).unwrap();

    let first_proof = normal_execution_proof(origin.authority(), origin.head(), 81, [4, 5, 6]);
    let first_cut = advance_normal_history_proof(&origin, &first_proof).unwrap();
    let (recovery2_raw, recovery2) = permission_evidence_at_cut(
        &first_cut,
        &trust,
        2,
        [14, 15, 16],
        [5, 6],
        None,
        &initial_floor,
    );
    let issuer_floor2 = recovery2_raw.issuer_witness.body.current_anchor.clone();
    let e2 = first_cut.with_empty_recovery(recovery2.recovery()).unwrap();
    let (_recovery3_raw, recovery3) = permission_evidence_at_cut(
        &e2,
        &trust,
        3,
        [24, 25, 26],
        [15, 16],
        Some(&recovery2),
        &issuer_floor2,
    );
    let permissions = vec![recovery2.clone(), recovery3.clone()];
    let context3 = recovery3.retained_context().unwrap();
    let e3 = e2
        .with_verified_retained_recovery(recovery2.recovery(), context3, recovery3.recovery())
        .unwrap();

    // A fresh reader naming C0 remains anchored at C0 even though the latest
    // checked authority is E3. The DTO never selects that authority itself.
    let fresh = replay_exact_origin_from(
        &origin,
        &read_anchor_ref(&origin),
        &permissions,
        Instant::now() + Duration::from_secs(2),
        |_, _, _, _| Err("unexpected proof lookup at external origin".into()),
    )
    .unwrap();
    assert_eq!(fresh.anchor.checkpoint(), origin.checkpoint());

    // Replaying C0's local/native proof plus the E2 recovery produces the
    // exact cached E2 origin without applying the later same-cut E3 recovery.
    let e2_ref = read_anchor_ref(&e2);
    let e2_replayed = replay_exact_origin_from(
        &origin,
        &e2_ref,
        &permissions,
        Instant::now() + Duration::from_secs(2),
        |_, position, _, _| {
            (position == first_proof.decision.record.position)
                .then(|| first_proof.clone())
                .ok_or_else(|| "unexpected proof position".to_owned())
        },
    )
    .unwrap();
    assert_eq!(e2_replayed.anchor.checkpoint(), e2.checkpoint());

    // A fresh C0 reader can replay the complete E1 -> E2 -> E3 chain at the
    // exact seal position, with both authority and canonical record matched.
    let e3_replayed = replay_exact_origin_from(
        &origin,
        &read_anchor_ref(&e3),
        &permissions,
        Instant::now() + Duration::from_secs(2),
        |_, position, _, _| {
            (position == first_proof.decision.record.position)
                .then(|| first_proof.clone())
                .ok_or_else(|| "unexpected proof position".to_owned())
        },
    )
    .unwrap();
    assert_eq!(e3_replayed.anchor.checkpoint(), e3.checkpoint());

    // A client whose exact cached origin is E2 needs only the retained E3
    // recovery step; the verified E2 permission remains the typed predecessor.
    let mut cached_cursor = e2.clone();
    let mut next_permission = 0;
    let mut previous_permission = None;
    let mut previous_emitted_in_page = false;
    let mut pending_permission = None;
    let mut steps = Vec::new();
    apply_ready_recoveries(
        &mut cached_cursor,
        &permissions,
        &mut next_permission,
        &mut previous_permission,
        &mut previous_emitted_in_page,
        &mut pending_permission,
        &mut steps,
        MAX_FORMAL_HISTORY_STEPS,
    )
    .unwrap();
    require_exact_history_target(&cached_cursor, &e3).unwrap();
    assert_eq!(steps.len(), 1);
    assert!(matches!(steps[0], FormalReadStepV1::Recovery(_)));

    // A valid alternate record at the same position cannot satisfy the
    // recovery's exact sealed-cut binding.
    let fork = normal_execution_proof(origin.authority(), origin.head(), 83, [4, 5, 6]);
    assert!(
        replay_exact_origin_from(
            &origin,
            &read_anchor_ref(&e3),
            &permissions,
            Instant::now() + Duration::from_secs(2),
            |_, position, _, _| {
                (position == fork.decision.record.position)
                    .then(|| fork.clone())
                    .ok_or_else(|| "unexpected proof position".to_owned())
            },
        )
        .is_err()
    );
}

#[test]
fn external_and_cached_epoch_origins_replay_normal_progress_between_recoveries() {
    let (_world, trust, origin, _) = fixtures::actual_world_initial_fixture();
    let initial_floor = issuer::IssuerAnchorV1::initial(origin.authority().policy()).unwrap();

    let first_proof = normal_execution_proof(origin.authority(), origin.head(), 91, [4, 5, 6]);
    let first_cut = advance_normal_history_proof(&origin, &first_proof).unwrap();
    let (recovery2_raw, recovery2) = permission_evidence_at_cut(
        &first_cut,
        &trust,
        2,
        [14, 15, 16],
        [5, 6],
        None,
        &initial_floor,
    );
    let issuer_floor2 = recovery2_raw.issuer_witness.body.current_anchor.clone();
    let e2 = first_cut.with_empty_recovery(recovery2.recovery()).unwrap();

    // E2 must sign a normal completion under its own authority identity. This
    // catches fixtures that accidentally retain E1 authority fields in the
    // record while signing the frozen body as E2.
    let second_proof = normal_execution_proof(e2.authority(), e2.head(), 92, [14, 15, 16]);
    let second_cut = advance_normal_history_proof(&e2, &second_proof).unwrap();
    let (_recovery3_raw, recovery3) = permission_evidence_at_cut(
        &second_cut,
        &trust,
        3,
        [24, 25, 26],
        [15, 16],
        None,
        &issuer_floor2,
    );
    let e3 = second_cut
        .with_empty_recovery(recovery3.recovery())
        .unwrap();
    let permissions = vec![recovery2, recovery3];

    let replayed = replay_exact_origin_from(
        &origin,
        &read_anchor_ref(&e3),
        &permissions,
        Instant::now() + Duration::from_secs(3),
        |_, position, _, _| match position {
            p if p == first_proof.decision.record.position => Ok(first_proof.clone()),
            p if p == second_proof.decision.record.position => Ok(second_proof.clone()),
            _ => Err(format!("unexpected proof position {position}")),
        },
    )
    .unwrap();
    require_exact_history_target(&replayed.anchor, &e3).unwrap();

    // A cached reader already at the E2 normal cut must apply only E3's
    // recovery transition and keep the exact record, position, and authority.
    let mut cached_cursor = second_cut.clone();
    let mut next_permission = 0;
    let mut previous_permission = None;
    let mut previous_emitted_in_page = false;
    let mut pending_permission = None;
    let mut steps = Vec::new();
    apply_ready_recoveries(
        &mut cached_cursor,
        &permissions,
        &mut next_permission,
        &mut previous_permission,
        &mut previous_emitted_in_page,
        &mut pending_permission,
        &mut steps,
        MAX_FORMAL_HISTORY_STEPS,
    )
    .unwrap();
    require_exact_history_target(&cached_cursor, &e3).unwrap();
    assert_eq!(steps.len(), 1);
    assert!(matches!(steps[0], FormalReadStepV1::Recovery(_)));
}
