//! Original unit registrations. The feature facade uses only the shared fixture.
use super::rule_phase_fixture_run::run_rule_fixture_composed;
pub(super) use super::rule_phase_fixture_run::run_rule_fixture_with_override;

fn run_rule_fixture(case: u8) {
    run_rule_fixture_with_override(case, false);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_phase_matches_legacy_after_world_drop() {
    run_rule_fixture(0);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_modify_runs_nonempty_pricing_on_derived_input() {
    run_rule_fixture(1);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_invalid_payload_preserves_json_error_event() {
    run_rule_fixture(2);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_wrong_action_id_preserves_error_event() {
    run_rule_fixture(3);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_multiple_emits_preserves_error_event() {
    run_rule_fixture(4);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_invalid_first_of_two_emits_preserves_first_error() {
    run_rule_fixture(5);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_wrong_id_first_of_two_emits_preserves_first_error() {
    run_rule_fixture(6);
}

#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_consumes_actual_nonempty_rule_modify_and_pricing() {
    run_rule_fixture_composed(1, true, 1);
}

#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_rejected_move_settles_general_cost_without_pricing_cost() {
    run_rule_fixture_composed(1, true, 2);
}

#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_insufficient_rule_cost_preserves_position_and_balance() {
    run_rule_fixture_composed(1, true, 3);
}

#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_genuine_rule_denial_preserves_balance_and_position() {
    run_rule_fixture_composed(7, false, 4);
}
