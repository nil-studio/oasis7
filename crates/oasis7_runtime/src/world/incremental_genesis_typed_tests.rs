use super::*;
#[test]
fn runtime_typed_genesis_inventory_is_closed() {
    inventory().unwrap();
    assert_eq!(RUNTIME_TYPED_GENESIS_FIELDS_V1.len(), 46);
    assert_eq!(RUNTIME_TYPED_RAW_FIELDS_V1.len(), 9);
}
