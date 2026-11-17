//! Closed runners reuse the original lib-test bodies and fixed budgets.
#![cfg(not(target_arch = "wasm32"))]
use oasis7_runtime::{IwsModuleStateCaseV1, run_iws_module_state_case_v1};

#[test]
fn step_dispatch_updates_opaque_state_and_original_without_mailbox() {
    run_iws_module_state_case_v1(IwsModuleStateCaseV1::OpaqueUpdate);
}

#[test]
fn module_state_receiver_preserves_payload_updates_among_real_insertions_and_deletions() {
    run_iws_module_state_case_v1(IwsModuleStateCaseV1::MixedMembership);
}

#[test]
fn module_state_receiver_rejects_wrong_before_and_foreign_roots_before_inheriting_keys() {
    run_iws_module_state_case_v1(IwsModuleStateCaseV1::RejectWrongBeforeAndForeignRoots);
}
