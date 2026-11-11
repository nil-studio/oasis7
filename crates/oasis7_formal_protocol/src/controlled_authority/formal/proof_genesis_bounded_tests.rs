//! Cryptographic DATA fixtures only; no pages, execution or Accepted GEN claim.
use super::*;
use crate::controlled_authority::formal::{
    bounded_context::FormalBoundedWorkV1, test_fixtures as f,
};
pub(super) fn fixture() -> (
    FormalQualifiedCommitProofV1,
    SignedCanonicalCandidateV1,
    VerifiedCandidateContextV1,
) {
    let (mut certificate, _, legacy, _) = f::initial();
    certificate.body.schema_version = 3;
    certificate.body.authority.profile_version = 3;
    certificate.body.initial_state_root = f::h(91);
    certificate.body.artifact_roots = [
        (
            ArtifactRole::IncrementalGenesisConfig,
            certificate.body.authority.world.genesis_digest.clone(),
        ),
        (
            ArtifactRole::IncrementalRootSet,
            certificate.body.initial_state_root.clone(),
        ),
        (
            ArtifactRole::ExecutionManifest,
            certificate.body.runtime_manifest_digest.clone(),
        ),
        (ArtifactRole::Wasm, f::h(92)),
        (ArtifactRole::Rules, f::h(93)),
        (ArtifactRole::NativeExecutionEnvironment, f::h(94)),
    ]
    .into_iter()
    .collect();
    certificate.signature_hex = f::sign(
        initial_activation_signing_bytes(&certificate.body).unwrap(),
        9,
    );
    let trust = InitialActivationTrustV1 {
        issuer_public_key: f::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    let input = blake3::hash(&initial_activation_input_bytes(&certificate).unwrap())
        .to_hex()
        .to_string();
    let mut record = legacy.decision.record;
    record.schema_version = 7;
    record.original_authority_id = authority.authority_id().unwrap();
    record.activation_ref = authority.activation_ref().into();
    record.before_state_root = certificate.body.initial_state_root.clone();
    record.after_state_root = certificate.body.initial_state_root.clone();
    record.payload_digest = input.clone();
    record.artifact_roots = certificate.body.artifact_roots.clone();
    record.artifact_roots.insert(ArtifactRole::Input, input);
    record
        .artifact_roots
        .insert(ArtifactRole::IncrementalCommitBody, f::h(95));
    record
        .artifact_roots
        .insert(ArtifactRole::IncrementalHistoryPrefix, f::h(96));
    let proof = f::proof(
        record,
        &authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let candidate = SignedCanonicalCandidateV1 {
        record: proof.decision.record.clone(),
        writer_signature_hex: proof.decision.writer_signature_hex.clone(),
    };
    let context = verify_formal_candidate(
        &candidate,
        None,
        &authority,
        None,
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    (proof, candidate, context)
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(32 << 20, 0).unwrap()
}
fn work() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(10_000_000, 128 << 20).unwrap()
}
#[test]
fn genuine_dual_genesis_quorum_matches_original_and_retains_exact_proof() {
    let (proof, candidate, context) = fixture();
    let original =
        verify_formal_qualified(&proof, context.finalizing_authority(), None, None).unwrap();
    let memory = memory();
    let work = work();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let decision =
        verify_genesis_decision_bounded(&proof.decision, &candidate, &context, &mut ctx).unwrap();
    assert_eq!(
        &*decision
            .complete_receipt_body_bounded(EndpointRole::Primary, &mut ctx)
            .unwrap(),
        &proof.primary_complete.body
    );
    let final_body =
        genesis_final_receipt_body_bounded(&candidate, &context, EndpointRole::Replica, &mut ctx)
            .unwrap();
    assert_eq!(&*final_body, &proof.decision.replica.body);
    let paid = verify_genesis_qualified_bounded(&proof, &candidate, &context, &mut ctx).unwrap();
    assert_eq!(
        paid.verified().verified_record_id(),
        original.verified_record_id()
    );
    assert_eq!(paid.verified().record(), original.record());
    assert_eq!(paid.proof(), &proof);
    assert_eq!(
        memory.used(),
        paid.reserved_bytes() + decision.reserved_bytes() + final_body.reserved_bytes()
    );
    drop(paid);
    drop(decision);
    drop(final_body);
    assert_eq!(memory.used(), 0);
}
#[test]
fn correctly_signed_wrong_context_or_role_cannot_mint_verified_genesis() {
    let (proof, candidate, context) = fixture();
    for mode in 0..4 {
        let mut bad = proof.clone();
        match mode {
            0 => {
                bad.primary_complete.body.decision_proof_digest = f::h(100);
                bad.primary_complete.signature_hex = f::sign(
                    formal_proof_complete_signing_bytes(&bad.primary_complete.body).unwrap(),
                    5,
                );
            }
            1 => {
                bad.decision.primary.body.record_id = f::h(101);
                bad.decision.primary.signature_hex = f::sign(
                    formal_final_receipt_signing_bytes(&bad.decision.primary.body).unwrap(),
                    5,
                );
            }
            2 => {
                bad.replica_complete.signature_hex = f::sign(
                    formal_proof_complete_signing_bytes(&bad.replica_complete.body).unwrap(),
                    5,
                )
            }
            _ => bad.decision.writer_signature_hex = "zz".repeat(64),
        }
        let memory = memory();
        let work = work();
        let mut stop = || false;
        assert!(
            verify_genesis_qualified_bounded(
                &bad,
                &candidate,
                &context,
                &mut FormalBoundedContextV1::new(&memory, &work, &mut stop)
            )
            .is_err()
        );
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn cancelled_zero_memory_and_foreign_core_never_release_a_bare_commit() {
    let (proof, candidate, context) = fixture();
    let memory = memory();
    let work = work();
    let mut stop = || true;
    assert!(
        verify_genesis_qualified_bounded(
            &proof,
            &candidate,
            &context,
            &mut FormalBoundedContextV1::new(&memory, &work, &mut stop)
        )
        .is_err()
    );
    assert_eq!(memory.used(), 0);
    let zero = MemoryBudget::new(0, 0).unwrap();
    let mut stop = || false;
    assert!(
        verify_genesis_qualified_bounded(
            &proof,
            &candidate,
            &context,
            &mut FormalBoundedContextV1::new(&zero, &work, &mut stop)
        )
        .is_err()
    );
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let paid = verify_genesis_qualified_bounded(&proof, &candidate, &context, &mut ctx).unwrap();
    let other = FormalBoundedWorkV1::new(10_000_000, 128 << 20).unwrap();
    let mut no = || false;
    assert!(
        paid.validate_context(&mut FormalBoundedContextV1::new(&memory, &other, &mut no))
            .is_err()
    );
    drop(paid);
    assert_eq!(memory.used(), 0);
}
