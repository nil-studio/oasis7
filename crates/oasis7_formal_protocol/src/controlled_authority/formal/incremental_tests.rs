use super::super::test_fixtures as fixture;
use super::*;
use crate::controlled_authority::replicated_protocol::ClosedRecord;
use crate::incremental_execution::{
    ExecutionFactKindV1, ExecutionFactRefV1, ExecutionParent, ExecutionTransaction,
    TransactionLimits,
};
use crate::incremental_history::HeaderPreparation;
use fixture::incremental::{Empty, frozen, setup};
use oasis7_proto::memory_budget::MemoryBudget;
#[test]
fn actual_iws_genesis_uses_original_dual_proof_protocol_with_distinct_content_id() {
    let s = setup();
    verify_commit_binding(
        s.commit.record(),
        &s.authority,
        None,
        s.material(),
        &s.memory,
    )
    .unwrap();
    assert_ne!(
        s.commit.head().record_id,
        hex::encode(s.history.body.digest().unwrap())
    );
    assert_eq!(s.commit.record().position, s.roots.height + 1);
    assert!(
        !s.commit
            .record()
            .artifact_roots
            .contains_key(&ArtifactRole::Snapshot)
    );
    let mut missing_completion = s.proof.clone();
    missing_completion.replica_complete = missing_completion.primary_complete.clone();
    assert!(verify_formal_qualified(&missing_completion, &s.authority, None, None).is_err());
    let mut wrong_domain = s.proof.clone();
    wrong_domain.decision.replica = wrong_domain.decision.primary.clone();
    assert!(verify_formal_qualified(&wrong_domain, &s.authority, None, None).is_err());
}

#[test]
fn incremental_execution_preserves_original_frozen_and_parent_qualification() {
    let s = setup();
    let frozen = frozen(&s);
    verify_formal_reservation(&frozen.reservation, &s.authority, &s.commit).unwrap();
    frozen
        .verify(s.authority.definition(), s.authority.activation_ref())
        .unwrap();
    let parent = ExecutionParent {
        commit: s.history.body.digest().unwrap(),
        roots: s.roots.clone(),
        authority: authority_binding(&s.authority, &s.memory).unwrap(),
    };
    let fence = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(
        parent,
        [60; 32],
        &Empty,
        &s.memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 8,
            max_facts: 8,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    txn.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: [61; 32],
    })
    .unwrap();
    let guard = txn.guard();
    let transition = txn.seal(&guard, &fence, 0).unwrap();
    let catalog = s
        .history
        .body
        .catalog_before
        .extend(&s.history.body, &Empty, &s.memory, 4 << 20)
        .unwrap();
    let history = HeaderPreparation {
        config: &s.config,
        catalog: &catalog.after,
        roots: &transition.record.after,
        parent: Some(&s.history.body),
        parent_history: Some(&s.history.prefix),
        transition: Some(&transition.record),
        genesis_facts: &[],
        operation: [60; 32],
        authority: authority_binding(&s.authority, &s.memory).unwrap(),
        execution_environment: s.config.execution_environment,
        checkpoint: false,
    }
    .prepare(&s.memory)
    .unwrap();
    let mut record = s.commit.record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = 2;
    record.parent_record_id = s.commit.head().record_id;
    record.request_digest = fixture::h(60);
    record.payload_digest = fixture::h(61);
    record.runtime_manifest_digest = fixture::h(62);
    record.frozen_reservation_id = Some(frozen.reservation.body.reservation_id().unwrap());
    record.after_state_root = hex::encode(transition.record.after.digest().unwrap());
    for (role, value) in [
        (ArtifactRole::Input, record.payload_digest.clone()),
        (
            ArtifactRole::ExecutionManifest,
            record.runtime_manifest_digest.clone(),
        ),
        (
            ArtifactRole::IncrementalRootSet,
            record.after_state_root.clone(),
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            hex::encode(history.body.digest().unwrap()),
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            hex::encode(history.prefix.digest().unwrap()),
        ),
    ] {
        record.artifact_roots.insert(role, value);
    }
    let proof = fixture::proof(
        record,
        &s.authority,
        FormalCompletionBranchV1::Normal,
        None,
        Some(frozen.clone()),
        4,
        5,
        6,
    );
    let commit = verify_formal_qualified(&proof, &s.authority, Some(&s.commit), None).unwrap();
    verify_commit_binding(
        commit.record(),
        &s.authority,
        Some(&s.commit),
        CommitMaterialRefs {
            config: &s.config,
            roots: &transition.record.after,
            body: &history.body,
            history: &history.prefix,
        },
        &s.memory,
    )
    .unwrap();
    let mut downgraded = frozen.reservation.clone();
    downgraded.body.schema_version = 3;
    downgraded.signature_hex = fixture::sign(frozen_signing_bytes(&downgraded.body).unwrap(), 4);
    assert!(verify_formal_reservation(&downgraded, &s.authority, &s.commit).is_err());
}

#[test]
fn old_snapshot_entrypoints_and_cross_format_roots_cannot_accept_iws() {
    let s = setup();
    let material = ClosedRecord {
        payload_digest: fixture::h(1),
        before_state_root: fixture::h(2),
        after_state_root: fixture::h(3),
        roots: BTreeMap::new(),
        objects: vec![],
    };
    assert!(
        verify_record_material(s.commit.record(), &material)
            .unwrap_err()
            .to_string()
            .contains("incremental record requires")
    );
    assert!(
        verify_frozen_material(&frozen(&s).reservation.body, &material)
            .unwrap_err()
            .to_string()
            .contains("incremental frozen requires")
    );
    let mut legacy = s.commit.record().clone();
    legacy.schema_version = 5;
    assert!(legacy.validate().is_err());
    let mut alias = s.commit.record().clone();
    let root = alias
        .artifact_roots
        .remove(&ArtifactRole::IncrementalRootSet)
        .unwrap();
    alias.artifact_roots.insert(ArtifactRole::Snapshot, root);
    assert!(alias.validate().is_err());
    let mut authorization = s.certificate.body.clone();
    authorization.authority.profile_version = 2;
    assert!(authorization.validate().is_err());
    let mut dropped = s.proof.clone();
    dropped
        .decision
        .record
        .artifact_roots
        .insert(ArtifactRole::IncrementalCommitBody, fixture::h(99));
    // Signers can attest a body commitment; material binding remains required.
    let rebound = fixture::proof(
        dropped.decision.record,
        &s.authority,
        FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let qualified = verify_formal_qualified(&rebound, &s.authority, None, None).unwrap();
    assert!(
        verify_commit_binding(
            qualified.record(),
            &s.authority,
            None,
            s.material(),
            &s.memory
        )
        .is_err()
    );
    let (_, old_authority, _, _) = fixture::initial();
    assert!(
        SignedCanonicalCandidateV1 {
            record: s.commit.record().clone(),
            writer_signature_hex: s.proof.decision.writer_signature_hex.clone()
        }
        .verify_original(&old_authority)
        .is_err()
    );
    assert!(authority_binding(&old_authority, &s.memory).is_err());
}

#[test]
fn binding_admission_is_typed_and_releases_workspace() {
    let s = setup();
    let denied = MemoryBudget::new(64 * 1024, 0).unwrap();
    assert!(matches!(
        verify_commit_binding(s.commit.record(), &s.authority, None, s.material(), &denied),
        Err(BindingError::Memory(_))
    ));
    assert!(matches!(
        authority_binding(&s.authority, &denied),
        Err(BindingError::Memory(_))
    ));
    assert_eq!(denied.used(), 0);
    let before = s.memory.used();
    verify_commit_binding(
        s.commit.record(),
        &s.authority,
        None,
        s.material(),
        &s.memory,
    )
    .unwrap();
    assert_eq!(s.memory.used(), before);
}
