//! Reuses the genuine Rule VM fixture and its independently initialized old World.
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_modify_publishes_original_override_after_world_drop() {
    super::rule_phase::run_rule_fixture_with_override(1, true);
}
