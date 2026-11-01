//! Actual qualified-H selected config; no provider connection or execution.
use super::*;
use crate::formal_live_service::task_cleanup_fixture as actual_task_fixture;
#[test]
fn closed_task_bft_v3_selected_config_refuses_before_hook_admission() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(case)
        .unwrap()
        .join()
        .unwrap();
}
fn case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let node = runtime(
        root.path(),
        actual,
        &binding,
        listener.local_addr().unwrap(),
    );
    let (_, original, _) = actual_task_fixture::fixture();
    let fence = &original.request.permitted_branches.exact_initial_active;
    let q = oasis7_client_api::world_service::task_cleanup_v2::TaskCleanupSubmitV2 {
        api_revision: 2,
        expected_world: oasis7_client_api::world_service::WorldIdentity {
            world_id: fence.world_id.clone(),
            genesis_digest: fence.genesis_digest.clone(),
        },
        original,
    };
    q.verify_owner_signature().unwrap();
    let frame = oasis7::world_service::correlation::encode_cleanup_consensus_v2(&q).unwrap();
    let err = node.submit_consensus_action_payload(83, frame).unwrap_err();
    assert!(
        matches!(err,oasis7_node::NodeError::Consensus {reason} if reason=="closed task mutation requires non-BFT Formal profile; not queued")
    );
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
