use oasis7_client_api::chain_pos_defaults::{
    chain_pos_timing_defaults, parse_chain_pos_timing_defaults,
};

#[test]
fn repo_chain_pos_defaults_parse_to_current_profile() {
    let defaults = chain_pos_timing_defaults();
    assert_eq!(defaults.slot_duration_ms, 8_000);
    assert_eq!(defaults.ticks_per_slot, 10);
    assert_eq!(defaults.proposal_tick_phase, 9);
    assert_eq!(defaults.max_past_slot_lag, 256);
}

#[test]
fn repo_chain_pos_defaults_reject_invalid_phase() {
    let err = parse_chain_pos_timing_defaults(
        "\
POS_SLOT_DURATION_MS=8000
POS_TICKS_PER_SLOT=10
POS_PROPOSAL_TICK_PHASE=10
POS_MAX_PAST_SLOT_LAG=256
",
    )
    .expect_err("phase equal to ticks per slot must be rejected");
    assert!(err.contains("must be less than"));
}
