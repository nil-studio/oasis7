use super::*;

#[test]
fn ordinary_starter_identity_constants_and_original_source_bytes_stay_available() {
    assert_eq!(SUPPLIER_ID, "ordinary-starter-supplier-v1");
    assert_eq!(LOCATION_ID, "ordinary-starter-yard-v1");
    assert_eq!(SLOT_COUNT, 8);
    assert!(initializer_source_bytes().starts_with(
        b"//! Explicit fresh-world finite public inventory; no participant receives assets here."
    ));
}
