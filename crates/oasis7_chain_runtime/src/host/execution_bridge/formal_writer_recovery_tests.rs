//! Exact retained payload comparison is separate from its completion wrapper.
use super::*;

#[test]
fn terminal_recovery_wrapper_preserves_signed_candidate_and_frozen_closure() {
    let policy = ReleaseSecurityPolicy::default();
    let (world, _, anchor, _) = fixtures::actual_world_initial_fixture_with_execution_policy(
        formal_package::execution_policy_material(&policy)
            .unwrap()
            .2,
    );
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-terminal-recovery-payload-{}",
        std::process::id()
    ));
    assert!(!root.exists());
    let mut writer = FormalWriter::open(
        root.clone(),
        fixtures::key(4),
        policy,
        world.clone(),
        anchor.clone(),
        endpoint(&anchor, EndpointRole::Primary, false),
        endpoint(&anchor, EndpointRole::Replica, false),
    )
    .unwrap();
    writer.prepare(&continuation(&world)).unwrap();
    let original = writer.candidate_for_resume().unwrap();
    let mut completed = original.clone();
    completed.branch = FormalCompletionBranchV1::CompletePrepared;
    assert!(recovery_runtime::same_candidate_payload(
        &original, &completed
    ));
    assert_eq!(
        original.candidate.record.record_id(),
        completed.candidate.record.record_id()
    );
    let mut changed = completed.clone();
    changed.candidate.writer_signature_hex = fixtures::h(99);
    assert!(!recovery_runtime::same_candidate_payload(
        &original, &changed
    ));
    let mut changed = completed.clone();
    changed.material.objects[0].bytes.push(0);
    assert!(!recovery_runtime::same_candidate_payload(
        &original, &changed
    ));
    let mut changed = completed;
    changed.frozen_material.as_mut().unwrap().objects[0]
        .bytes
        .push(0);
    assert!(!recovery_runtime::same_candidate_payload(
        &original, &changed
    ));
    drop(writer);
    std::fs::remove_dir_all(root).unwrap();
}
