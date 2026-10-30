use super::*;
#[test]
fn failed_material_reservation_cannot_make_a_retry_skip_the_limit() {
    let mut capacity = Capacity::default();
    capacity.charge(MAX_MATERIAL).unwrap();
    assert!(capacity.object("not-retained", 1).is_err());
    assert!(capacity.object("not-retained", 1).is_err());
    assert!(!capacity.objects.contains("not-retained"));
}
#[test]
fn publication_held_to_actual_is_deduplicated_and_never_refunded() {
    let mut original = Capacity::default();
    original.object("already-retained", 7).unwrap();
    original.charge(100).unwrap();
    let before = original.clone();
    let (transferred, consumed) = original
        .plan_held_to_actual(100, |capacity| {
            capacity.object("already-retained", 7)?;
            capacity.object("new-object", 23)
        })
        .unwrap();
    assert_eq!(consumed, 23);
    assert_eq!(transferred.bytes, before.bytes);
    assert!(transferred.objects.contains("new-object"));
    assert_eq!(original.bytes, before.bytes);
    assert_eq!(original.objects, before.objects);
    let (retry, retry_consumed) = transferred
        .plan_held_to_actual(77, |capacity| capacity.object("new-object", 23))
        .unwrap();
    assert_eq!(retry_consumed, 0);
    assert_eq!(retry.bytes, before.bytes);
    assert!(
        original
            .plan_held_to_actual(10, |capacity| capacity.object("oversized", 11))
            .is_err()
    );
    assert_eq!(original.bytes, before.bytes);
    assert!(!original.objects.contains("oversized"));
}
#[test]
fn publication_recovery_transfer_near_material_cap_does_not_double_charge() {
    // Arithmetic boundary only: the actual Some(ref) WAL/pin/Record tests
    // separately exercise these transfers through the production consumer.
    for already_actual in [0, 17, 41] {
        let mut capacity = Capacity::default();
        capacity.charge(MAX_MATERIAL - 100).unwrap();
        capacity
            .object("fixed-QC-material", already_actual)
            .unwrap();
        capacity.charge(100 - already_actual).unwrap();
        assert_eq!(capacity.bytes, MAX_MATERIAL);
        assert!(
            capacity
                .clone()
                .object("remaining-original-material", 59)
                .is_err()
        );
        let (complete, used) = capacity
            .plan_held_to_actual(100 - already_actual, |next| {
                next.object("fixed-QC-material", already_actual)?;
                next.object("remaining-original-material", 59)
            })
            .unwrap();
        assert_eq!(used, 59);
        assert_eq!(complete.bytes, MAX_MATERIAL);
        let (retry, used) = complete
            .plan_held_to_actual(41 - already_actual, |next| {
                next.object("remaining-original-material", 59)
            })
            .unwrap();
        assert_eq!(used, 0);
        assert_eq!(retry.bytes, MAX_MATERIAL);
    }
}
