//! Independent under-reservation boundaries: byte slack cannot pay objects.
use super::*;
#[test]
fn protected_carry_rejects_missing_pin_selection_bytes_or_objects() {
    // Existing closure/WAL charge=1000/100 includes old init=100/16.
    // Fully recomputed carry needs 300/24, hence committed must cover1200/108.
    assert!(validate_committed((1000, 100), (100, 16), (300, 24), (1200, 108)).is_ok());
    assert!(validate_committed((1000, 100), (100, 16), (300, 24), (1199, 108)).is_err());
    assert!(validate_committed((1000, 100), (100, 16), (300, 24), (1200, 107)).is_err());
    assert!(validate_committed((1000, 100), (100, 16), (300, 24), (u64::MAX, 107)).is_err());
    assert!(validate_committed((1000, 100), (100, 16), (300, 24), (1199, u64::MAX)).is_err());
}
#[test]
fn legacy_initialization_charge_is_unchanged_and_overflow_fails_closed() {
    assert!(validate_committed((1000, 100), (100, 16), (100, 16), (1000, 100)).is_ok());
    assert!(validate_committed((u64::MAX, 100), (100, 16), (101, 16), (u64::MAX, 100)).is_err());
    assert!(validate_committed((1000, u64::MAX), (100, 16), (100, 17), (1000, u64::MAX)).is_err());
}
