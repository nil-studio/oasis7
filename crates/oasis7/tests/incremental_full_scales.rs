//! Original large-scale contracts, independent from the required behavior harnesses.
#![cfg(all(unix, not(target_arch = "wasm32"), feature = "test_tier_full"))]

#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/incremental_cognition_fixture.rs"]
mod incremental_cognition_fixture;
#[path = "support/incremental_cognition_scaling.rs"]
mod incremental_cognition_scaling;
#[path = "support/incremental_factory_authority_fixture.rs"]
mod incremental_factory_authority_fixture;
#[path = "support/incremental_factory_authority_scaling.rs"]
mod incremental_factory_authority_scaling;
#[path = "support/incremental_main_token_policy_fixture.rs"]
mod incremental_main_token_policy_fixture;
#[path = "support/incremental_main_token_policy_scaling.rs"]
mod incremental_main_token_policy_scaling;
#[path = "support/incremental_module_state_step.rs"]
mod incremental_module_state_step;
#[path = "support/incremental_runtime_effect_fixture.rs"]
mod incremental_runtime_effect_fixture;
#[path = "support/incremental_runtime_effect_scaling.rs"]
mod incremental_runtime_effect_scaling;
#[allow(dead_code)]
#[path = "support/incremental_agent_event.rs"]
mod support;

#[test]
fn module_state_event_genesis_step_original_and_recovery_are_raw_zstd_stable() {
    incremental_module_state_step::exercise_module_state(32 << 20);
}

#[test]
fn point_and_receiver_skip_unchanged_symbol_through_thirty_two_mib() {
    incremental_main_token_policy_scaling::exercise_unchanged_symbol(&[64, 1 << 20, 32 << 20]);
}

#[test]
fn scalar_receiver_cost_excludes_unchanged_site_text_and_allowlist() {
    incremental_factory_authority_scaling::exercise_site_scaling(&[
        (64, 3),
        (1 << 20, 32),
        (32 << 20, 64),
    ]);
}

#[test]
fn unchanged_params_do_not_enter_handoff_sender_or_complete_typed_receiver() {
    let sizes = [64, 32 << 20, 64 << 20];
    for &size in &sizes[1..] {
        assert!(incremental_runtime_effect_scaling::RECEIVER_POOL_BYTES < size);
    }
    incremental_runtime_effect_scaling::exercise_unchanged_params(&sizes);
}

#[test]
fn cognition_fixed_delta_receiver_does_not_visit_unchanged_large_json() {
    let small = incremental_cognition_scaling::run(64);
    let medium = incremental_cognition_scaling::run(1 << 20);
    let large = incremental_cognition_scaling::run(32 << 20);
    small.assert_fixed(&medium);
    small.assert_fixed(&large);
}
