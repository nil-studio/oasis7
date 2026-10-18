//! Shared test-only cryptographic fixtures over an actual restored World.
//! Fixture signatures assume trusted test authorities; they do not qualify a
//! production provider, deploy endpoints or prove physical fault independence.
#![allow(
    dead_code,
    reason = "Shared test fixtures are included by several binaries with different helper needs"
)]
use ed25519_dalek::{Signer, SigningKey};
use crate::{World, blake3_hex};
use oasis7_distfs::controlled_authority::{
    fencing::EndpointInstanceBindingV1,
    formal::*,
    replicated_protocol::{ArtifactRole, EndpointRole},
};
use std::collections::BTreeMap;
pub(crate) fn h(n: u8) -> String {
    hex::encode([n; 32])
}
pub(crate) fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
pub(crate) fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().to_bytes())
}
pub(crate) fn sign(bytes: Vec<u8>, n: u8) -> String {
    hex::encode(key(n).sign(&bytes).to_bytes())
}
pub(crate) fn roots() -> BTreeMap<ArtifactRole, String> {
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
fn endpoint(role: EndpointRole, n: u8, world: &FormalWorldIdentityV1) -> EndpointSignerBindingV1 {
    EndpointSignerBindingV1 {
        instance: EndpointInstanceBindingV1 {
            schema_version: 1,
            world_id: world.world_id.clone(),
            chain_id: world.chain_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            authority_epoch: 1,
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn qualified_proof(
    record: CanonicalRecordV1,
    frozen: Option<FormalFrozenCertificateV1>,
    authority: &VerifiedAuthorityV1,
    parent: Option<&VerifiedFormalCommitV1>,
    recovery: Option<&VerifiedRecovery>,
    writer: u8,
    p: u8,
    r: u8,
) -> FormalQualifiedCommitProofV1 {
    let candidate = SignedCanonicalCandidateV1 {
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), writer),
        record,
    };
    let context = verify_formal_candidate(
        &candidate,
        frozen.as_ref(),
        authority,
        parent,
        recovery,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let receipt = |role, n| {
        let body = context.receipt_body(role).unwrap();
        FormalFinalReceiptV1 {
            signature_hex: sign(formal_final_receipt_signing_bytes(&body).unwrap(), n),
            body,
        }
    };
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: candidate.record,
        writer_signature_hex: candidate.writer_signature_hex,
        branch: FormalCompletionBranchV1::Normal,
        frozen,
        primary: receipt(EndpointRole::Primary, p),
        replica: receipt(EndpointRole::Replica, r),
    };
    let verified = verify_formal_decision(&decision, authority, parent, recovery).unwrap();
    let complete = |role, n| {
        let body = verified.complete_receipt_body(role).unwrap();
        FormalProofCompleteReceiptV1 {
            signature_hex: sign(formal_proof_complete_signing_bytes(&body).unwrap(), n),
            body,
        }
    };
    FormalQualifiedCommitProofV1 {
        schema_version: 1,
        primary_complete: complete(EndpointRole::Primary, p),
        replica_complete: complete(EndpointRole::Replica, r),
        decision,
    }
}
pub(crate) fn actual_world_initial_fixture() -> (
    World,
    InitialActivationTrustV1,
    VerifiedFormalReadAnchorV1,
    FormalQualifiedCommitProofV1,
) {
    actual_world_initial_fixture_with_execution_policy(h(50))
}
pub(crate) fn actual_world_initial_fixture_with_execution_policy(
    execution_policy_digest: String,
) -> (
    World,
    InitialActivationTrustV1,
    VerifiedFormalReadAnchorV1,
    FormalQualifiedCommitProofV1,
) {
    let mut original = World::new_production_hardened();
    original
        .bind_cognition_runtime("formal-fixture-world", "main", 0, None, "pending", 0)
        .unwrap();
    let genesis = h(8);
    let snapshot = original.snapshot_with_chain_resource_context(
        crate::ChainResourceDerivationContext {
            world_id: "formal-fixture-world",
            chain_id: "formal-fixture-chain",
            genesis_ref: Some(&genesis),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: original.state().time,
        },
        h(10),
        h(11),
    );
    let world = World::from_snapshot(snapshot, original.journal().clone()).unwrap();
    let snapshot = world.snapshot();
    let state_root = blake3_hex(&serde_cbor::to_vec(&snapshot).unwrap());
    let binding = world.current_cognition_runtime_binding().unwrap();
    let governing = binding
        .runtime_manifest_hash
        .to_string()
        .strip_prefix("blake3:")
        .unwrap()
        .to_owned();
    let identity = FormalWorldIdentityV1 {
        world_id: snapshot.chain_resource_manifest.world_id.clone(),
        chain_id: snapshot.chain_resource_manifest.chain_id.clone(),
        genesis_digest: h(8),
    };
    let policy = RecoveryPolicyV1 {
        schema_version: 1,
        recovery_issuer_public_key: public(1),
        fencing_provider_public_key: public(3),
        issuer_anchor_public_key: public(2),
        authorized_fencing_resource_bindings: vec![h(35), h(36)],
        allow_complete_prepared: true,
        allow_replay_frozen: true,
        successor_rule: SuccessorRuleV1::IncrementEpochDistinctBindings,
        terminates_at_profile_transition: true,
    };
    let definition = AuthorityDefinitionV1 {
        schema_version: 1,
        world: identity.clone(),
        profile: FORMAL_PROFILE.into(),
        profile_version: 1,
        epoch: 1,
        writer_public_key: public(4),
        primary: endpoint(EndpointRole::Primary, 5, &identity),
        replica: endpoint(EndpointRole::Replica, 6, &identity),
        recovery_policy_digest: policy.digest().unwrap(),
        predecessor_authority_id: None,
        valid_from_position: 1,
    };
    let mut roots = roots();
    roots.remove(&ArtifactRole::Input);
    roots.insert(ArtifactRole::Snapshot, state_root.clone());
    roots.insert(
        ArtifactRole::Journal,
        blake3_hex(&serde_cbor::to_vec(world.journal()).unwrap()),
    );
    let body = InitialActivationBodyV1 {
        schema_version: 1,
        authority: definition,
        recovery_policy: policy.clone(),
        initial_state_root: state_root.clone(),
        runtime_manifest_digest: roots[&ArtifactRole::ExecutionManifest].clone(),
        governing_manifest_digest: governing.clone(),
        execution_policy_digest: execution_policy_digest.clone(),
        artifact_roots: roots.clone(),
    };
    let certificate = SignedInitialActivationV1 {
        signature_hex: sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: public(9),
        world: identity.clone(),
        initial_state_root: state_root.clone(),
        governing_manifest_digest: governing.clone(),
        recovery_policy_digest: policy.digest().unwrap(),
    };
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    let input = blake3_hex(&initial_activation_input_bytes(&certificate).unwrap());
    roots.insert(ArtifactRole::Input, input.clone());
    let projection_root = blake3_hex(b"runtime-native-fixture-projection-v1");
    let record = CanonicalRecordV1 {
        schema_version: 1,
        durable_set_transition_digest: None,
        kind: FormalRecordKindV1::InitialActivation,
        world: identity.clone(),
        position: 1,
        parent_record_id: identity.genesis_head().unwrap().record_id,
        original_authority_id: authority.authority_id().unwrap(),
        original_epoch: 1,
        activation_ref: authority.activation_ref().into(),
        request_digest: h(51),
        payload_digest: input,
        frozen_reservation_id: None,
        runtime_manifest_digest: certificate.body.runtime_manifest_digest.clone(),
        governing_manifest_digest: governing,
        execution_policy_digest: execution_policy_digest.clone(),
        before_state_root: state_root.clone(),
        after_state_root: state_root,
        action_root: h(52),
        event_root: h(53),
        result_root: h(54),
        projection_root,
        artifact_roots: roots,
        handoff: None,
    };
    let proof = qualified_proof(record, None, &authority, None, None, 4, 5, 6);
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    (world, trust, anchor, proof)
}
/// Actual restored World cuts with test-authority light envelopes. This helper
/// does not exercise the production executor or certify endpoint key custody.
pub(crate) fn actual_world_successor_fixture(
    parent: &VerifiedFormalReadAnchorV1,
    mut world: World,
) -> (
    World,
    VerifiedFormalReadAnchorV1,
    FormalQualifiedCommitProofV1,
) {
    world.step().expect("actual World fixture tick");
    let identity = &parent.head().record().world;
    let snapshot = world.snapshot_with_chain_resource_context(
        crate::ChainResourceDerivationContext {
            world_id: &identity.world_id,
            chain_id: &identity.chain_id,
            genesis_ref: Some(&identity.genesis_digest),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: world.state().time,
        },
        h(10),
        h(11),
    );
    let world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let authority = parent.authority();
    let body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: identity.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition().epoch,
        activation_ref: authority.activation_ref().into(),
        parent: parent.head().head(),
        next_position: parent.head().record().position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: h(62),
        governing_manifest_digest: parent.head().record().governing_manifest_digest.clone(),
        execution_policy_digest: parent.head().record().execution_policy_digest.clone(),
        pre_state_root: parent.head().record().after_state_root.clone(),
        frozen_material_digest: h(64),
        handoff: None,
    };
    let receipt = |role, binding: &EndpointSignerBindingV1, n| {
        let receipt = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: body.reservation_id().unwrap(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&receipt).unwrap(), n),
            body: receipt,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        primary: receipt(EndpointRole::Primary, &authority.definition().primary, 5),
        replica: receipt(EndpointRole::Replica, &authority.definition().replica, 6),
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&body).unwrap(), 4),
            body: body.clone(),
        },
    };
    let mut record = parent.head().record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = body.next_position;
    record.parent_record_id = body.parent.record_id.clone();
    record.request_digest = body.request_digest.clone();
    record.payload_digest = body.payload_digest.clone();
    record.frozen_reservation_id = Some(body.reservation_id().unwrap());
    record.runtime_manifest_digest = body.runtime_manifest_digest.clone();
    record.before_state_root = body.pre_state_root;
    record.after_state_root = blake3_hex(&serde_cbor::to_vec(&world.snapshot()).unwrap());
    record.governing_manifest_digest = world
        .current_cognition_runtime_binding()
        .unwrap()
        .runtime_manifest_hash
        .to_string()
        .strip_prefix("blake3:")
        .unwrap()
        .into();
    record
        .artifact_roots
        .insert(ArtifactRole::Snapshot, record.after_state_root.clone());
    record.artifact_roots.insert(
        ArtifactRole::Journal,
        blake3_hex(&serde_cbor::to_vec(world.journal()).unwrap()),
    );
    record.artifact_roots.insert(
        ArtifactRole::ExecutionManifest,
        record.runtime_manifest_digest.clone(),
    );
    record.projection_root =
        blake3_hex(b"runtime-native-fixture-projection-v1");
    let proof = qualified_proof(
        record,
        Some(frozen),
        authority,
        Some(parent.head()),
        None,
        4,
        5,
        6,
    );
    let committed = verify_formal_qualified(&proof, authority, Some(parent.head()), None).unwrap();
    let anchor = parent
        .after_verified_commit(authority.clone(), committed)
        .unwrap();
    (world, anchor, proof)
}
