use crate::controlled_authority::formal::*;
use crate::controlled_authority::replicated_protocol::{ArtifactRole, EndpointRole};
use ed25519_dalek::{Signer, SigningKey};
use std::collections::BTreeMap;

pub fn h(n: u8) -> String {
    hex::encode([n; 32])
}
pub fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
pub fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().to_bytes())
}
pub fn sign(bytes: Vec<u8>, n: u8) -> String {
    hex::encode(key(n).sign(&bytes).to_bytes())
}
fn roots() -> BTreeMap<ArtifactRole, String> {
    [
        ArtifactRole::Input,
        ArtifactRole::Result,
        ArtifactRole::Snapshot,
        ArtifactRole::Journal,
        ArtifactRole::NonceIndex,
        ArtifactRole::EffectOutbox,
        ArtifactRole::ExecutionManifest,
        ArtifactRole::Wasm,
        ArtifactRole::Rules,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, role)| (role, h(40 + i as u8)))
    .collect()
}
pub fn policy() -> RecoveryPolicyV1 {
    RecoveryPolicyV1 {
        schema_version: 1,
        recovery_issuer_public_key: public(1),
        fencing_provider_public_key: public(3),
        authorized_fencing_resource_bindings: vec![h(35), h(36)],
        issuer_anchor_public_key: public(2),
        allow_complete_prepared: true,
        allow_replay_frozen: true,
        successor_rule: SuccessorRuleV1::IncrementEpochDistinctBindings,
        terminates_at_profile_transition: true,
    }
}
fn endpoint(role: EndpointRole, n: u8, epoch: u64) -> EndpointSignerBindingV1 {
    EndpointSignerBindingV1 {
        instance: fencing::EndpointInstanceBindingV1 {
            schema_version: 1,
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_digest: h(8),
            authority_epoch: epoch,
            endpoint_role: role,
            endpoint_id: format!("endpoint-{n}"),
            endpoint_public_key: public(n),
            instance_id: h(n + 10),
            storage_id: h(n + 20),
            storage_fingerprint: h(n + 30),
        },
        fault_domain_id: format!("domain-{n}"),
    }
}
pub fn initial() -> (
    SignedInitialActivationV1,
    VerifiedAuthorityV1,
    FormalQualifiedCommitProofV1,
    VerifiedFormalCommitV1,
) {
    initial_with_profile(1)
}
pub fn initial_with_profile(
    profile_version: u32,
) -> (
    SignedInitialActivationV1,
    VerifiedAuthorityV1,
    FormalQualifiedCommitProofV1,
    VerifiedFormalCommitV1,
) {
    let policy = policy();
    let mut roots = roots();
    roots.remove(&ArtifactRole::Input);
    let definition = AuthorityDefinitionV1 {
        schema_version: 1,
        world: FormalWorldIdentityV1 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_digest: h(8),
        },
        profile: FORMAL_PROFILE.into(),
        profile_version,
        epoch: 1,
        writer_public_key: public(4),
        primary: endpoint(EndpointRole::Primary, 5, 1),
        replica: endpoint(EndpointRole::Replica, 6, 1),
        recovery_policy_digest: policy.digest().unwrap(),
        predecessor_authority_id: None,
        valid_from_position: 1,
    };
    let body = InitialActivationBodyV1 {
        schema_version: 1,
        authority: definition,
        recovery_policy: policy.clone(),
        initial_state_root: roots[&ArtifactRole::Snapshot].clone(),
        runtime_manifest_digest: roots[&ArtifactRole::ExecutionManifest].clone(),
        governing_manifest_digest: h(81),
        execution_policy_digest: h(50),
        artifact_roots: roots.clone(),
    };
    let certificate = SignedInitialActivationV1 {
        signature_hex: sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: h(81),
        recovery_policy_digest: policy.digest().unwrap(),
    };
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    let input = blake3::hash(&initial_activation_input_bytes(&certificate).unwrap())
        .to_hex()
        .to_string();
    roots.insert(ArtifactRole::Input, input.clone());
    let record = CanonicalRecordV1 {
        schema_version: 1,
        durable_set_transition_digest: None,
        kind: FormalRecordKindV1::InitialActivation,
        world: authority.definition.world.clone(),
        position: 1,
        parent_record_id: authority.definition.world.genesis_head().unwrap().record_id,
        original_authority_id: authority.authority_id().unwrap(),
        original_epoch: 1,
        activation_ref: authority.activation_ref.clone(),
        request_digest: h(51),
        payload_digest: input,
        frozen_reservation_id: None,
        runtime_manifest_digest: certificate.body.runtime_manifest_digest.clone(),
        governing_manifest_digest: h(81),
        execution_policy_digest: h(50),
        before_state_root: certificate.body.initial_state_root.clone(),
        after_state_root: certificate.body.initial_state_root.clone(),
        action_root: h(52),
        event_root: h(53),
        result_root: h(54),
        projection_root: h(55),
        artifact_roots: roots,
        handoff: None,
    };
    let proof = proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let commit = verify_formal_qualified(&proof, &authority, None, None).unwrap();
    (certificate, authority, proof, commit)
}
#[expect(
    clippy::too_many_arguments,
    reason = "The signed proof fixture keeps writer and both independent endpoint signers explicit."
)]
pub fn proof(
    record: CanonicalRecordV1,
    finalizer: &VerifiedAuthorityV1,
    branch: FormalCompletionBranchV1,
    certificate: Option<String>,
    frozen: Option<FormalFrozenCertificateV1>,
    writer: u8,
    p: u8,
    r: u8,
) -> FormalQualifiedCommitProofV1 {
    let receipt = |role, binding: &EndpointSignerBindingV1, n| {
        let body = FormalFinalReceiptBodyV1 {
            schema_version: 1,
            record_id: record.record_id().unwrap(),
            position: record.position,
            finalizing_authority_id: finalizer.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            branch,
            recovery_certificate_digest: certificate.clone(),
        };
        FormalFinalReceiptV1 {
            signature_hex: sign(formal_final_receipt_signing_bytes(&body).unwrap(), n),
            body,
        }
    };
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), writer),
        primary: receipt(EndpointRole::Primary, &finalizer.definition.primary, p),
        replica: receipt(EndpointRole::Replica, &finalizer.definition.replica, r),
        record,
        branch,
        frozen,
    };
    let complete = |role, binding: &EndpointSignerBindingV1, n| {
        let body = FormalProofCompleteBodyV1 {
            schema_version: 1,
            record_id: decision.record.record_id().unwrap(),
            position: decision.record.position,
            decision_proof_digest: formal_decision_proof_digest(&decision).unwrap(),
            finalizing_authority_id: finalizer.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            branch,
            recovery_certificate_digest: certificate.clone(),
        };
        FormalProofCompleteReceiptV1 {
            signature_hex: sign(formal_proof_complete_signing_bytes(&body).unwrap(), n),
            body,
        }
    };
    FormalQualifiedCommitProofV1 {
        schema_version: 1,
        primary_complete: complete(EndpointRole::Primary, &finalizer.definition.primary, p),
        replica_complete: complete(EndpointRole::Replica, &finalizer.definition.replica, r),
        decision,
    }
}
pub fn frozen(
    authority: &VerifiedAuthorityV1,
    cut: &VerifiedFormalCommitV1,
    handoff: Option<ProfileHandoffV1>,
) -> FormalFrozenCertificateV1 {
    let body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: authority.definition.world.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition.epoch,
        activation_ref: authority.activation_ref.clone(),
        parent: cut.head(),
        next_position: cut.record().position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: h(62),
        governing_manifest_digest: h(81),
        execution_policy_digest: cut.record().execution_policy_digest.clone(),
        pre_state_root: cut.record().after_state_root.clone(),
        frozen_material_digest: h(64),
        handoff,
    };
    let receipt = |role, binding: &EndpointSignerBindingV1, n| {
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: body.reservation_id().unwrap(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&body).unwrap(), n),
            body,
        }
    };
    FormalFrozenCertificateV1 {
        primary: receipt(
            EndpointRole::Primary,
            &authority.definition.primary,
            if authority.definition.epoch == 1 {
                5
            } else {
                11
            },
        ),
        replica: receipt(
            EndpointRole::Replica,
            &authority.definition.replica,
            if authority.definition.epoch == 1 {
                6
            } else {
                12
            },
        ),
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(
                frozen_signing_bytes(&body).unwrap(),
                if authority.definition.epoch == 1 {
                    4
                } else {
                    10
                },
            ),
            body,
        },
    }
}
pub fn execution(
    authority: &VerifiedAuthorityV1,
    frozen: &FormalFrozenCertificateV1,
) -> CanonicalRecordV1 {
    let body = &frozen.reservation.body;
    let mut roots = roots();
    roots.insert(ArtifactRole::Input, body.payload_digest.clone());
    CanonicalRecordV1 {
        schema_version: 1,
        durable_set_transition_digest: None,
        kind: FormalRecordKindV1::Execution,
        world: body.world.clone(),
        position: body.next_position,
        parent_record_id: body.parent.record_id.clone(),
        original_authority_id: authority.authority_id().unwrap(),
        original_epoch: authority.definition.epoch,
        activation_ref: body.activation_ref.clone(),
        request_digest: body.request_digest.clone(),
        payload_digest: body.payload_digest.clone(),
        frozen_reservation_id: Some(body.reservation_id().unwrap()),
        runtime_manifest_digest: body.runtime_manifest_digest.clone(),
        governing_manifest_digest: h(81),
        execution_policy_digest: body.execution_policy_digest.clone(),
        before_state_root: body.pre_state_root.clone(),
        after_state_root: h(65),
        action_root: h(66),
        event_root: h(67),
        result_root: h(68),
        projection_root: h(69),
        artifact_roots: roots,
        handoff: body.handoff.clone(),
    }
}
pub fn recovery_fixture() -> (
    VerifiedRecoveryIntent,
    VerifiedAuthorityV1,
    VerifiedFormalCommitV1,
    FormalFrozenCertificateV1,
    CanonicalRecordV1,
) {
    recovery_fixture_for(FormalCompletionBranchV1::ReplayFrozen, None)
}
pub fn recovery_fixture_for(
    branch: FormalCompletionBranchV1,
    handoff: Option<ProfileHandoffV1>,
) -> (
    VerifiedRecoveryIntent,
    VerifiedAuthorityV1,
    VerifiedFormalCommitV1,
    FormalFrozenCertificateV1,
    CanonicalRecordV1,
) {
    recovery_fixture_for_writer(branch, handoff, 10)
}
pub fn alternate_recovery_fixture() -> VerifiedRecoveryIntent {
    recovery_fixture_for_writer(FormalCompletionBranchV1::ReplayFrozen, None, 99).0
}
fn recovery_fixture_for_writer(
    branch: FormalCompletionBranchV1,
    handoff: Option<ProfileHandoffV1>,
    writer: u8,
) -> (
    VerifiedRecoveryIntent,
    VerifiedAuthorityV1,
    VerifiedFormalCommitV1,
    FormalFrozenCertificateV1,
    CanonicalRecordV1,
) {
    recovery_fixture_with_profile(branch, handoff, writer, 1)
}
pub fn recovery_fixture_with_profile(
    branch: FormalCompletionBranchV1,
    handoff: Option<ProfileHandoffV1>,
    writer: u8,
    profile: u32,
) -> (
    VerifiedRecoveryIntent,
    VerifiedAuthorityV1,
    VerifiedFormalCommitV1,
    FormalFrozenCertificateV1,
    CanonicalRecordV1,
) {
    let (_, authority, _, cut) = initial_with_profile(profile);
    let frozen = frozen(&authority, &cut, handoff.clone());
    let record = execution(&authority, &frozen);
    let candidate = SignedCanonicalCandidateV1 {
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 4),
        record: record.clone(),
    };
    let tail = if branch == FormalCompletionBranchV1::ReplayFrozen {
        RecoveryTailV1::Frozen {
            position: 2,
            reservation_id: frozen.reservation.body.reservation_id().unwrap(),
            handoff,
        }
    } else {
        RecoveryTailV1::Prepared {
            position: 2,
            reservation_id: frozen.reservation.body.reservation_id().unwrap(),
            record_id: record.record_id().unwrap(),
            handoff,
        }
    };
    let mut successor = authority.definition.clone();
    successor.epoch = 2;
    successor.writer_public_key = public(writer);
    successor.primary = endpoint(EndpointRole::Primary, 11, 2);
    successor.replica = endpoint(EndpointRole::Replica, 12, 2);
    successor.predecessor_authority_id = Some(authority.authority_id().unwrap());
    successor.valid_from_position = 2;
    let sealed_tail = fencing::SealedTailV1 {
        kind: if branch == FormalCompletionBranchV1::ReplayFrozen {
            fencing::SealedTailKindV1::Frozen
        } else {
            fencing::SealedTailKindV1::Prepared
        },
        request: Some(crate::controlled_authority::LocalRequestIdentity {
            verified_subject: "writer".into(),
            operation_domain: "formal-execution".into(),
            nonce_scope: "world".into(),
            request_id: "request-2".into(),
        }),
        position: Some(2),
        frozen_input_digest: Some(frozen.reservation.body.reservation_id().unwrap()),
        frozen_certificate_digest: None,
        proposal_digest: if branch == FormalCompletionBranchV1::ReplayFrozen {
            None
        } else {
            Some(record.record_id().unwrap())
        },
        replica_prepare_digest: None,
        primary_receipt_digest: None,
        replica_receipt_digest: None,
    };
    let head = crate::controlled_authority::replicated_protocol::HeadAnchor {
        position: 1,
        decision_hash: cut.head().record_id,
        qualified: true,
    };
    let seal = verified_seal(
        fencing::EpochSealBodyV1 {
            schema_version: 1,
            instance: authority.definition.primary.instance.clone(),
            recovery_operation: h(70),
            highest_complete: head.clone(),
            checkpoint_before: fencing::EndpointCheckpointBindingV1 {
                revision: 1,
                commitment: h(71),
                head,
            },
            tail: sealed_tail,
            next_epoch_floor: 2,
        },
        5,
    );
    let mut replica_seal_body = seal.seal_body().clone();
    replica_seal_body.instance = authority.definition.replica.instance.clone();
    let replica_seal = verified_seal(replica_seal_body, 6);
    let normal = if record.handoff.is_some() {
        None
    } else {
        Some(3)
    };
    let body = RecoveryCertificateBodyV1 {
        retained_context: None,
        schema_version: 1,
        world: authority.definition.world.clone(),
        operation_id: h(70),
        predecessor_authority_id: authority.authority_id().unwrap(),
        predecessor_epoch: 1,
        sealed_cut: cut.head(),
        tail,
        fencing_evidence_digest: recovery_fencing_digest([&seal, &replica_seal]).unwrap(),
        successor,
        first_normal_position: normal,
    };
    let witness = if branch == FormalCompletionBranchV1::ReplayFrozen {
        RecoveryTailWitnessV1::Frozen(frozen.clone())
    } else {
        RecoveryTailWitnessV1::Prepared {
            frozen: frozen.clone(),
            candidate,
        }
    };
    let intent =
        VerifiedRecoveryIntent::verify(body, &authority, &cut, [&seal, &replica_seal], &witness)
            .unwrap();
    (intent, authority, cut, frozen, record)
}
pub fn verified_recovery(
    intent: &VerifiedRecoveryIntent,
    authority: &VerifiedAuthorityV1,
) -> VerifiedRecovery {
    use issuer::*;
    let certificate = SignedRecoveryCertificateV1 {
        body: intent.body().clone(),
        signature_hex: sign(recovery_signing_bytes(intent.body()).unwrap(), 1),
    };
    let witness = SignedIssuanceWitnessV1::sign(
        IssuanceWitnessBodyV1 {
            schema_version: 1,
            capability: IssuerWitnessCapabilityV1::NonrollbackUniqueSuccessorV1,
            request: IssuanceRequestBodyV1 {
                schema_version: 1,
                issuer_public_key: public(1),
                policy_binding: issuer::test_policy_binding(intent.policy()).unwrap(),
                predecessor_authority_id: intent.body().predecessor_authority_id.clone(),
                certificate_digest: recovery_certificate_digest(&certificate).unwrap(),
                observation_nonce: h(72),
                minimum_anchor: IssuerAnchorV1::initial(intent.policy()).unwrap(),
            },
            grant_revision: 1,
            current_anchor: IssuerAnchorV1 {
                revision: 1,
                digest: h(73),
            },
        },
        &key(2),
    )
    .unwrap();
    let issuance = verify_issuance_witness(&witness, &certificate, intent.policy()).unwrap();
    VerifiedRecovery::verify(&certificate, intent, authority, &issuance).unwrap()
}

/// Signed wire fixture only. Callers must still verify it under their explicit
/// trusted provider policy; this helper never constructs opaque evidence.
pub fn signed_epoch_seal_receipt(
    body: fencing::EpochSealBodyV1,
    endpoint_key: &SigningKey,
    provider_key: &SigningKey,
    resource_binding: String,
) -> fencing::EpochSealReceiptV1 {
    let checkpoint = fencing::EndpointCheckpointBindingV1 {
        revision: body.checkpoint_before.revision.checked_add(1).unwrap(),
        commitment: body.digest().unwrap(),
        head: body.checkpoint_before.head.clone(),
    };
    let statement = fencing::MonotonicProviderStatementV1 {
        schema_version: 1,
        capability: fencing::ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
        resource_binding: resource_binding.clone(),
        instance_digest: body.instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        epoch_floor: body.next_epoch_floor,
        seal_digest: Some(body.digest().unwrap()),
    };
    let provider = fencing::MonotonicProviderEvidenceV1 {
        signature: fencing::sign(fencing::PROVIDER_DOMAIN, &statement, provider_key).unwrap(),
        statement,
    };
    let mut receipt = fencing::EpochSealReceiptV1::sign(body, provider, endpoint_key).unwrap();
    let request = fencing::ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: receipt.body.instance.digest().unwrap(),
        checkpoint: checkpoint.clone(),
        challenge: h(75),
        purpose: fencing::ProtectedEndpointSignaturePurposeV1::EpochSeal,
        preimage: fencing::epoch_seal_signing_bytes(&receipt.body).unwrap(),
    };
    let protected = fencing::ProtectedEndpointSignatureBodyV1 {
        schema_version: 1,
        custody: fencing::ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
        instance_digest: request.instance_digest.clone(),
        resource_binding,
        checkpoint,
        request_digest: request.digest().unwrap(),
        challenge: request.challenge,
        endpoint_signature: receipt.endpoint_signature.clone(),
    };
    receipt.protected_signature = Some(fencing::ProtectedEndpointSignatureV1 {
        provider_signature: hex::encode(
            provider_key
                .sign(&fencing::protected_endpoint_signature_signing_bytes(&protected).unwrap())
                .to_bytes(),
        ),
        body: protected,
    });
    receipt
}
fn verified_seal(body: fencing::EpochSealBodyV1, signer: u8) -> fencing::FencingVerdict {
    let resource_binding = body.instance.storage_fingerprint.clone();
    let receipt = signed_epoch_seal_receipt(body, &key(signer), &key(3), resource_binding.clone());
    fencing::verify_epoch_seal(
        &receipt,
        &fencing::TrustedFencingPolicyV1 {
            provider_public_key: public(3),
            authorized_resource_bindings: vec![resource_binding],
        },
    )
    .unwrap()
}

#[path = "test_fixtures_incremental.rs"]
pub mod incremental;
