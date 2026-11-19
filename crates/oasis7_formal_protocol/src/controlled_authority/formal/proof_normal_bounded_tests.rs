use super::*;
use crate::controlled_authority::formal::{
    bounded_context::FormalBoundedWorkV1, test_fixtures as f,
};
use crate::controlled_authority::replicated_protocol::ArtifactRole;
fn fixture() -> (f::incremental::Setup, FormalQualifiedCommitProofV1) {
    let s = f::incremental::setup();
    let frozen = f::incremental::frozen(&s);
    let b = &frozen.reservation.body;
    let mut r = s.commit.record().clone();
    r.kind = FormalRecordKindV1::Execution;
    r.position = b.next_position;
    r.parent_record_id = b.parent.record_id.clone();
    r.request_digest = b.request_digest.clone();
    r.payload_digest = b.payload_digest.clone();
    r.runtime_manifest_digest = b.runtime_manifest_digest.clone();
    r.frozen_reservation_id = Some(b.reservation_id().unwrap());
    r.before_state_root = b.pre_state_root.clone();
    r.after_state_root = f::h(98);
    r.artifact_roots
        .insert(ArtifactRole::Input, r.payload_digest.clone());
    r.artifact_roots.insert(
        ArtifactRole::ExecutionManifest,
        r.runtime_manifest_digest.clone(),
    );
    r.artifact_roots
        .insert(ArtifactRole::IncrementalRootSet, r.after_state_root.clone());
    let proof = f::proof(
        r,
        &s.authority,
        FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    (s, proof)
}
#[test]
fn actual_normal_quorum_matches_original_dual_final_and_complete_proof() {
    let (s, p) = fixture();
    let old = verify_formal_qualified(&p, &s.authority, Some(&s.commit), None).unwrap();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let paid = verify_normal_qualified_bounded(&p, &s.authority, &s.commit, &mut ctx).unwrap();
    assert_eq!(paid.verified().record(), old.record());
    assert_eq!(
        paid.verified().verified_record_id(),
        old.verified_record_id()
    );
    assert_eq!(paid.proof(), &p);
    assert!(memory.used() > 0);
    paid.validate_context(&mut ctx).unwrap();
    drop(paid);
    assert_eq!(memory.used(), 0);
}
#[test]
fn tampered_writer_frozen_final_and_complete_keep_original_first_error() {
    let (s, p) = fixture();
    for kind in 0..4 {
        let mut bad = p.clone();
        match kind {
            0 => {
                let replacement = if bad.decision.writer_signature_hex.starts_with("00") {
                    "01"
                } else {
                    "00"
                };
                bad.decision
                    .writer_signature_hex
                    .replace_range(..2, replacement)
            }
            1 => bad.decision.frozen.as_mut().unwrap().primary.body.role = EndpointRole::Replica,
            2 => bad.decision.primary.body.position += 1,
            _ => {
                let replacement = if bad.replica_complete.signature_hex.starts_with("00") {
                    "01"
                } else {
                    "00"
                };
                bad.replica_complete
                    .signature_hex
                    .replace_range(..2, replacement)
            }
        };
        let old = verify_formal_qualified(&bad, &s.authority, Some(&s.commit), None).unwrap_err();
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let work = FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap();
        let mut stop = || false;
        let error = verify_normal_qualified_bounded(
            &bad,
            &s.authority,
            &s.commit,
            &mut FormalBoundedContextV1::new(&memory, &work, &mut stop),
        )
        .unwrap_err();
        assert_eq!(error, old);
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn real_prepare_final_decision_complete_bodies_match_original() {
    let (s, p) = fixture();
    let candidate = SignedCanonicalCandidateV1 {
        record: p.decision.record.clone(),
        writer_signature_hex: p.decision.writer_signature_hex.clone(),
    };
    let frozen = p.decision.frozen.as_ref().unwrap();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let original = verify_formal_candidate(
        &candidate,
        Some(frozen),
        &s.authority,
        Some(&s.commit),
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    for role in [EndpointRole::Primary, EndpointRole::Replica] {
        let body = normal_final_receipt_body_bounded(
            &candidate,
            frozen,
            &s.authority,
            &s.commit,
            role,
            &mut ctx,
        )
        .unwrap();
        assert_eq!(*body, original.receipt_body(role).unwrap());
    }
    let decision =
        verify_normal_decision_bounded(&p.decision, &s.authority, &s.commit, &mut ctx).unwrap();
    let old = verify_formal_decision(&p.decision, &s.authority, Some(&s.commit), None).unwrap();
    for role in [EndpointRole::Primary, EndpointRole::Replica] {
        let body = decision
            .complete_receipt_body_bounded(role, &mut ctx)
            .unwrap();
        assert_eq!(*body, old.complete_receipt_body(role).unwrap());
    }
    drop(decision);
    assert_eq!(memory.used(), 0);
}
#[test]
fn normal_owner_rejects_foreign_core_pool_genesis_and_cancelled_context() {
    let (s, p) = fixture();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap();
    let mut stop = || false;
    let owner = verify_normal_qualified_bounded(
        &p,
        &s.authority,
        &s.commit,
        &mut FormalBoundedContextV1::new(&memory, &work, &mut stop),
    )
    .unwrap();
    let other = FormalBoundedWorkV1::new(1000, 1 << 20).unwrap();
    assert!(
        owner
            .validate_context(&mut FormalBoundedContextV1::new(&memory, &other, &mut stop))
            .is_err()
    );
    let foreign = MemoryBudget::new(1 << 20, 0).unwrap();
    assert!(
        owner
            .validate_context(&mut FormalBoundedContextV1::new(&foreign, &work, &mut stop))
            .is_err()
    );
    drop(owner);
    assert_eq!(memory.used(), 0);
    assert!(
        verify_normal_qualified_bounded(
            &s.proof,
            &s.authority,
            &s.commit,
            &mut FormalBoundedContextV1::new(&memory, &work, &mut stop)
        )
        .is_err()
    );
    let mut calls = 0;
    let mut cancel = || {
        calls += 1;
        true
    };
    assert!(
        verify_normal_qualified_bounded(
            &p,
            &s.authority,
            &s.commit,
            &mut FormalBoundedContextV1::new(&memory, &work, &mut cancel)
        )
        .is_err()
    );
    assert_eq!(calls, 1);
    assert_eq!(memory.used(), 0);
}
#[test]
fn actual_candidate_context_retains_full_authority_and_rejects_foreign_context() {
    let (s, p) = fixture();
    let candidate = SignedCanonicalCandidateV1 {
        record: p.decision.record.clone(),
        writer_signature_hex: p.decision.writer_signature_hex.clone(),
    };
    let frozen = p.decision.frozen.as_ref().unwrap();
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(100_000_000, 1 << 30).unwrap();
    let mut stop = || false;
    let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let old = verify_formal_candidate(
        &candidate,
        Some(frozen),
        &s.authority,
        Some(&s.commit),
        None,
        FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let owner =
        normal_candidate_context_bounded(&candidate, frozen, &s.authority, &s.commit, &mut ctx)
            .unwrap();
    owner.validate_context(&mut ctx).unwrap();
    assert_eq!(owner.value().record(), old.record());
    assert_eq!(
        owner
            .value()
            .finalizing_authority()
            .context_digest()
            .unwrap(),
        old.finalizing_authority().context_digest().unwrap()
    );
    assert_eq!(owner.value().branch(), FormalCompletionBranchV1::Normal);
    assert!(memory.used() > 0);
    let other = FormalBoundedWorkV1::new(100, 1 << 20).unwrap();
    assert!(
        owner
            .validate_context(&mut FormalBoundedContextV1::new(&memory, &other, &mut stop))
            .is_err()
    );
    let foreign = MemoryBudget::new(1 << 20, 0).unwrap();
    assert!(
        owner
            .validate_context(&mut FormalBoundedContextV1::new(&foreign, &work, &mut stop))
            .is_err()
    );
    drop(owner);
    assert_eq!(memory.used(), 0);
}
