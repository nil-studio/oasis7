use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, FormalBoundedWorkV1,
};
use oasis7_proto::memory_budget::MemoryBudget;

#[test]
fn paid_action_root_matches_original_payload_for_empty_and_nonempty_inputs() {
    for actions in [
        vec![],
        vec![
            NodeConsensusAction::from_payload(1, "actor-a", vec![0xa0]).unwrap(),
            NodeConsensusAction::from_payload(7, "玩家", vec![42; 65_537]).unwrap(),
        ],
    ] {
        let expected = compute_consensus_action_root(&actions).unwrap();
        let memory = MemoryBudget::new(16 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10_000_000, 16 << 20).unwrap();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let paid = compute_consensus_action_root_bounded(&actions, &mut ctx).unwrap();
        assert_eq!(*paid, expected);
        assert_eq!(memory.used(), paid.reserved_bytes());
        drop(paid);
        assert_eq!(memory.used(), 0);
    }
}
#[test]
fn paid_action_root_preserves_order_rejection_and_releases_on_cancel_or_admission_failure() {
    let action = NodeConsensusAction::from_payload(3, "actor", vec![0xa0]).unwrap();
    let repeated = vec![action.clone(), action];
    let original = compute_consensus_action_root(&repeated).unwrap_err();
    for (bytes, stopped) in [(16 << 20, false), (0, false), (16 << 20, true)] {
        let memory = MemoryBudget::new(bytes, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000, 16 << 20).unwrap();
        let mut stop = || stopped;
        let mut ctx = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let error = compute_consensus_action_root_bounded(&repeated, &mut ctx).unwrap_err();
        if bytes != 0 && !stopped {
            assert!(format!("{error:?}").contains(&original.reason));
        }
        assert_eq!(memory.used(), 0);
    }
}
