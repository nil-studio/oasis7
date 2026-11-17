//! Closed entry points to the original GEN and Normal event regression bodies.
#![cfg(not(target_arch = "wasm32"))]

use oasis7_runtime::{IwsGenesisEventCaseV1, run_iws_genesis_event_case_v1};

#[test]
fn projected_genesis_event_commitment_retains_the_complete_journal_and_original_context() {
    run_iws_genesis_event_case_v1(IwsGenesisEventCaseV1::CompleteJournal);
}

#[test]
fn projected_genesis_event_capture_refuses_hash_failure_before_physical_emissions() {
    run_iws_genesis_event_case_v1(IwsGenesisEventCaseV1::CaptureHashFailure);
}

#[test]
fn normal_event_builder_preserves_original_sequence_and_exact_byte_debit() {
    run_iws_genesis_event_case_v1(IwsGenesisEventCaseV1::NormalSequence);
}

#[test]
fn event_sequence_hash_preserves_late_cancellation_and_final_byte_refusal() {
    run_iws_genesis_event_case_v1(IwsGenesisEventCaseV1::HashRefusals);
}
