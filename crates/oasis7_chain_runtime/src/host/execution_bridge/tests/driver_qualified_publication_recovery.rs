use super::*;
use super::super::driver::NodeRuntimeExecutionDriver;
use super::driver_authoritative_recovery::replay_context;
use oasis7_node::NodeExecutionHook;

#[test]
fn qualified_publication_restore_checks_binding_before_mutation_and_replays_exact_original() {
    let dir=temp_dir("qualified-publication-restore");
    let mut driver=NodeRuntimeExecutionDriver::new(dir.join("state.json"),dir.join("world"),
        dir.join("records"),dir.join("store")).unwrap();
    let first=replay_context("node-a","block-1",vec![]);
    let expected_first=driver.on_commit(first.clone()).unwrap();
    let mut second=first.clone();second.height=2;second.slot=1;
    second.node_block_hash="block-2".into();second.committed_at_unix_ms=2000;
    let expected_second=driver.on_commit(second.clone()).unwrap();
    let before=driver.state.clone();
    let mut wrong=expected_first.clone();wrong.execution_state_root="wrong-root".into();
    assert!(driver.restore_to_verified_commit(&first,&wrong).is_err());
    assert_eq!(driver.state,before);
    let mut wrong_context=first.clone();wrong_context.node_block_hash="wrong-block".into();
    assert!(driver.restore_to_verified_commit(&wrong_context,&expected_first).is_err());
    assert_eq!(driver.state,before);
    assert!(driver.restore_to_verified_commit(&first,&expected_first).unwrap());
    assert_eq!(driver.state.last_applied_committed_height,1);
    assert_eq!(driver.state.last_execution_state_root.as_deref(),Some(expected_first.execution_state_root.as_str()));
    let replayed=driver.on_commit_with_expected(second,
        Some(&expected_second.execution_block_hash),Some(&expected_second.execution_state_root)).unwrap();
    assert_eq!(replayed.execution_height,2);
    assert_eq!(replayed.execution_block_hash,expected_second.execution_block_hash);
    assert_eq!(replayed.execution_state_root,expected_second.execution_state_root);
    drop(driver);std::fs::remove_dir_all(dir).unwrap();
}
