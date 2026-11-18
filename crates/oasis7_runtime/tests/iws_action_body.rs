//! Closed entries to original action-body cases with their original paid fixtures.
#![cfg(not(target_arch = "wasm32"))]
use oasis7_runtime::{IwsActionBodyCaseV1, run_iws_action_body_case_v1};

#[test]
fn actual_queued_body_actions_match_world_and_read_prior_current_write() {
    run_iws_action_body_case_v1(IwsActionBodyCaseV1::QueuedBodyActions);
}

#[test]
fn queued_alliance_lifecycle_preserves_outer_risk_war_rejections_and_exact_world_originals() {
    run_iws_action_body_case_v1(IwsActionBodyCaseV1::AllianceLifecycle);
}
