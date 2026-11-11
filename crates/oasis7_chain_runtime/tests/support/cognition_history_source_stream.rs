//! Actual signed History input: valid mathematical caches are not source proof.
#[test]
fn actual_signed_cbor_head_source_matches_cache() {
    super::v2_signed::signed_v2(false);
}
#[test]
fn rebound_mathematical_cache_cannot_replace_actual_record_source() {
    super::v2_signed::signed_v2_source_mode(false, true);
}
