use super::*;

#[test]
fn exact_key_framing_preserves_empty_and_opaque_utf8_state_keys() {
    for state_key in ["", "m#1", "m#2", "m\0雪"] {
        let mut expected = DOMAIN.to_vec();
        expected.extend((state_key.len() as u16).to_be_bytes());
        expected.extend(state_key.as_bytes());
        let encoded = ModuleStateKeyV1::new(state_key).unwrap();
        assert_eq!(encoded.as_bytes(), expected);
        assert_eq!(
            ModuleStateKeyV1::state_key_from_bytes(&expected).unwrap(),
            state_key
        );
    }
}

#[test]
fn rejects_wrong_domain_length_trailing_bytes_and_invalid_utf8() {
    let valid = ModuleStateKeyV1::new("x").unwrap();
    assert!(ModuleStateKeyV1::is_state_key(valid.as_bytes()));
    assert!(!ModuleStateKeyV1::is_state_key(
        b"oasis7.iws.module-state-key.v2\0"
    ));
    assert!(ModuleStateKeyV1::state_key_from_bytes(b"unknown").is_err());
    assert!(
        ModuleStateKeyV1::state_key_from_bytes(&valid.as_bytes()[..valid.as_bytes().len() - 1])
            .is_err()
    );
    let mut trailing = valid.as_bytes().to_vec();
    trailing.push(0);
    assert!(ModuleStateKeyV1::state_key_from_bytes(&trailing).is_err());
    let mut invalid_utf8 = valid.as_bytes().to_vec();
    *invalid_utf8.last_mut().unwrap() = 0xff;
    assert!(ModuleStateKeyV1::state_key_from_bytes(&invalid_utf8).is_err());
}

#[test]
fn key_length_boundary_is_checked_without_changing_the_original_bytes() {
    let maximum = "x".repeat(MODULE_STATE_KEY_MAX_BYTES - DOMAIN.len() - 2);
    let encoded = ModuleStateKeyV1::new(&maximum).unwrap();
    assert_eq!(encoded.as_bytes().len(), MODULE_STATE_KEY_MAX_BYTES);
    assert_eq!(
        ModuleStateKeyV1::state_key_from_bytes(encoded.as_bytes()).unwrap(),
        maximum
    );
    assert!(ModuleStateKeyV1::new(&(maximum + "x")).is_err());
}
