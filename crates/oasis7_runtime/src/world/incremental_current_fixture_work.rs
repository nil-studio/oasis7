//! Initial fixture allowances shared by library and real Rule integration tests.
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::STATE_MAX_PAGE_BYTES,
};
use oasis7_world_state::incremental_world_current_complete::WorldCurrentStateWorkV1;

// Each complete-field pass may visit a full 256-bit SMT path plus its leaf.
// The actual GEN producer asserts 98 native and 46 runtime fields. Set only
// fresh fixture work from the actual roots; this is not a spent-work refill.
pub(super) fn set_current_fixture_outer_scan_work(
    roots: &RootSetV1,
    work: &mut WorldCurrentStateWorkV1,
) {
    let leaves = roots
        .state
        .iter()
        .try_fold(0u64, |sum, root| sum.checked_add(root.leaf_count))
        .unwrap();
    let native = leaves
        .checked_mul(257)
        .and_then(|n| n.checked_mul(98))
        .unwrap();
    let runtime = leaves
        .checked_mul(257)
        .and_then(|n| n.checked_mul(46))
        .unwrap();
    work.native_outer_scan.nodes_remaining = native;
    work.native_outer_scan.read_bytes_remaining =
        native.checked_mul(STATE_MAX_PAGE_BYTES as u64).unwrap();
    work.native_outer_scan.leaves_remaining = leaves.checked_mul(98).unwrap();
    work.runtime.outer_scan.nodes_remaining = runtime;
    work.runtime.outer_scan.read_bytes_remaining =
        runtime.checked_mul(STATE_MAX_PAGE_BYTES as u64).unwrap();
    work.runtime.outer_scan.leaves_remaining = leaves.checked_mul(46).unwrap();
}
