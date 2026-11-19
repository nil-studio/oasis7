//! Linux-only tiny test binary; zero tests on another platform is not VM evidence.
#![cfg(all(target_os = "linux", feature = "wasmtime", feature = "test_tier_full"))]
use oasis7_runtime::{IwsRuleCoreCaseV1, run_iws_rule_core_case_v1};
#[test]
fn selected_core_consumes_actual_nonempty_rule_modify_and_pricing() {
    run_iws_rule_core_case_v1(IwsRuleCoreCaseV1::ModifyWithPricing);
}
#[test]
fn selected_core_rejected_move_settles_general_cost_without_pricing_cost() {
    run_iws_rule_core_case_v1(IwsRuleCoreCaseV1::RejectedMoveGeneralCost);
}
#[test]
fn selected_core_insufficient_rule_cost_preserves_position_and_balance() {
    run_iws_rule_core_case_v1(IwsRuleCoreCaseV1::InsufficientCost);
}
#[test]
fn selected_core_genuine_rule_denial_preserves_balance_and_position() {
    run_iws_rule_core_case_v1(IwsRuleCoreCaseV1::Deny);
}

#[test]
fn selected_rule_modify_form_alliance_to_material_transfer_keeps_guard_and_current_effects() {
    run_iws_rule_core_case_v1(IwsRuleCoreCaseV1::ModifyAllianceToMaterialTransfer);
}

#[test]
fn selected_rule_modify_form_alliance_to_resource_transfer_keeps_guard_and_current_effects() {
    run_iws_rule_core_case_v1(IwsRuleCoreCaseV1::ModifyAllianceToResourceTransfer);
}

#[test]
fn selected_rule_modify_form_alliance_to_main_token_transfer_keeps_fees_and_nonce() {
    run_iws_rule_core_case_v1(IwsRuleCoreCaseV1::ModifyAllianceToMainTokenTransfer);
}
