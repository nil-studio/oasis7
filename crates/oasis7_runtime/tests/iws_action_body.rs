//! Closed entry to the original queued-body case with its original paid fixtures.
#![cfg(not(target_arch = "wasm32"))]
use oasis7_runtime::{IwsActionBodyCaseV1, run_iws_action_body_case_v1};

#[test]
fn actual_queued_body_actions_match_world_and_read_prior_current_write() {
    run_iws_action_body_case_v1(IwsActionBodyCaseV1::QueuedBodyActions);
}
