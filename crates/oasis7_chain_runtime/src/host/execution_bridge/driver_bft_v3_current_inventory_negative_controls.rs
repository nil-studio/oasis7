//! Typed component controls run with a real authenticated backend context and C0.
//! They do not tamper with files or claim a well-formed physical attack fixture.
use super::*;

pub(super) fn verify(
    store: &NodePrivateStore,
    base: &super::super::super::successor_verifier::VerifiedPrivateSuccessorBaseV1,
    namespace: &str,
    current: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
    active_originals: &[&Slot],
) {
    assert!(
        active_originals.len() >= 1,
        "current inventory controls need a real non-carry original"
    );
    let original = active_originals[0];
    let kind = SourceSlotKindV1::from_data(&original.data).unwrap();
    assert!(
        store
            .verify_current_active_slot(
                base,
                namespace,
                current,
                original,
                kind,
                &mut BTreeSet::new()
            )
            .unwrap()
    );
    let mut foreign = original.clone();
    foreign.data.ledger_origin_digest = if namespace == "f".repeat(64) {
        "e".repeat(64)
    } else {
        "f".repeat(64)
    };
    // Namespace is a syntactically valid hash and Data remains decodable, but
    // has no authority to become a different namespace in this current owner.
    foreign.data.validate().unwrap();
    assert_eq!(
        store
            .verify_current_active_slot(
                base,
                namespace,
                current,
                &foreign,
                kind,
                &mut BTreeSet::new()
            )
            .unwrap_err(),
        "active preparation contains unproved private candidates"
    );
    assert!(
        store
            .verify_current_active_slot(
                base,
                namespace,
                current,
                original,
                kind,
                &mut BTreeSet::new()
            )
            .unwrap()
    );
    // Empty carry is a valid C0. Extra-original controls above still run;
    // only the carry-specific controls require an inherited candidate.
    if base.pending.is_empty() {
        return;
    }
    let carry = store
        .candidates
        .values()
        .chain(store.values.values())
        .find(|slot| {
            base.pending.iter().any(|(roles, package)| {
                let expected = if roles
                    .contains(&super::super::super::successor_archive::PendingRoleV1::RetainedLocal)
                {
                    SourceSlotKindV1::Local
                } else {
                    SourceSlotKindV1::Received
                };
                SourceSlotKindV1::from_data(&slot.data).ok() == Some(expected)
                    && slot.data.value.as_ref() == Some(&package.candidate.public_value)
                    && slot.data.actions_cbor == package.candidate.actions_cbor
            })
        })
        .expect("actual current inventory control requires an original carry");
    let carry_kind = SourceSlotKindV1::from_data(&carry.data).unwrap();
    let mut seen = BTreeSet::new();
    assert!(
        !store
            .verify_current_active_slot(base, namespace, current, carry, carry_kind, &mut seen)
            .unwrap()
    );
    assert_eq!(
        store
            .verify_current_active_slot(base, namespace, current, carry, carry_kind, &mut seen)
            .unwrap_err(),
        "active preparation contains duplicate carry slot"
    );
    let mut counterpart = carry.clone();
    counterpart.data.schema_version = match carry.data.schema_version {
        3 => 4,
        4 => 3,
        6 => 7,
        7 => 6,
        _ => panic!("first Source C0 carry must use original or migrated WAL domains"),
    };
    if counterpart.data.schema_version == 3 {
        counterpart.data.domain = DOMAIN.into();
    }
    if counterpart.data.schema_version == 4 {
        counterpart.data.domain = MULTI_DOMAIN.into();
    }
    counterpart.data.validate().unwrap();
    let counterpart_kind = SourceSlotKindV1::from_data(&counterpart.data).unwrap();
    assert!(counterpart_kind != carry_kind);
    assert_eq!(
        store
            .verify_current_active_slot(
                base,
                namespace,
                current,
                &counterpart,
                counterpart_kind,
                &mut BTreeSet::new()
            )
            .unwrap_err(),
        "active preparation contains unproved private candidates"
    );
}
