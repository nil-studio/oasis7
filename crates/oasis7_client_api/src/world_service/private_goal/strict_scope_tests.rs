use super::*;
use crate::world_service::{private_goal::prepare_owner_signature_strict, sign_read_request};
fn prepared<T: serde::Serialize>(domain: &str, value: &T) -> PreparedOwnerSignature {
    let signed = sign_read_request(domain, value, &"2a".repeat(32)).unwrap();
    prepare_owner_signature_strict(
        domain,
        value,
        &signed.subject_public_key,
        &signed.signature_hex,
    )
    .unwrap()
}
#[test]
fn nested_scope_reuses_exact_successes_and_shares_the_single_direct_map() {
    let first = prepared("passive", &1u64);
    let second = prepared("passive", &1u64);
    let direct = prepared("passive", &1u64);
    let mut scope = StrictOwnerSignatureScope::default();
    scope.verify_prepared_nested(first).unwrap();
    scope.verify_prepared_nested(second).unwrap();
    assert_eq!(
        (
            scope.summary().nested_calls,
            scope.summary().nested_repeats,
            scope.summary().strict_attempts,
            scope.summary().tracked_tuples
        ),
        (2, 1, 1, 1)
    );
    scope.verify_prepared_reusing(direct).unwrap();
    assert_eq!(
        (
            scope.summary().strict_attempts,
            scope.summary().reuse_hits,
            scope.summary().tracked_tuples
        ),
        (1, 2, 1)
    );
    let mut fresh = StrictOwnerSignatureScope::default();
    fresh
        .verify_prepared_nested(prepared("passive", &1u64))
        .unwrap();
    assert_eq!(
        (
            fresh.summary().strict_attempts,
            fresh.summary().nested_repeats
        ),
        (1, 0)
    );
}
#[test]
fn nested_scope_changed_tuple_and_repeated_failure_never_reuse_success() {
    let signed = sign_read_request("passive", 1u64, &"2a".repeat(32)).unwrap();
    let other = sign_read_request("passive", 1u64, &"2b".repeat(32)).unwrap();
    let mut scope = StrictOwnerSignatureScope::default();
    scope
        .verify_prepared_reusing(prepared("passive", &1u64))
        .unwrap();
    let mut sig = signed.signature_hex.clone();
    sig.replace_range(..2, if &sig[..2] == "00" { "01" } else { "00" });
    for (domain, value, key, signature) in [
        (
            "other",
            1,
            &signed.subject_public_key,
            &signed.signature_hex,
        ),
        (
            "passive",
            2,
            &signed.subject_public_key,
            &signed.signature_hex,
        ),
        (
            "passive",
            1,
            &other.subject_public_key,
            &signed.signature_hex,
        ),
        ("passive", 1, &signed.subject_public_key, &sig),
        ("passive", 1, &signed.subject_public_key, &sig),
    ] {
        let before = scope.summary().strict_attempts;
        let p = prepare_owner_signature_strict(domain, &value, key, signature).unwrap();
        assert!(scope.verify_prepared_nested(p).is_err());
        assert_eq!(scope.summary().strict_attempts, before + 1);
        assert_eq!(scope.summary().tracked_tuples, 1);
    }
    assert_eq!(scope.summary().failures, 5);
}
#[test]
fn nested_scope_caps_are_shared_and_overflow_never_rejects_or_skips_crypto() {
    let mut scope = StrictOwnerSignatureScope::default();
    for i in 0..StrictOwnerSignatureScope::MAX_ENTRIES {
        scope
            .verify_prepared_reusing(prepared("cap", &(i as u64)))
            .unwrap();
    }
    for _ in 0..2 {
        scope
            .verify_prepared_nested(prepared("overflow", &1u64))
            .unwrap();
    }
    assert_eq!(
        (
            scope.summary().tracked_tuples,
            scope.summary().overflow_tuples
        ),
        (256, 2)
    );
    let mut byte_scope = StrictOwnerSignatureScope::default();
    let oversized = "x".repeat(StrictOwnerSignatureScope::MAX_BYTES);
    for _ in 0..2 {
        byte_scope
            .verify_prepared_nested(prepared("bytes", &oversized))
            .unwrap();
    }
    assert_eq!(
        (
            byte_scope.summary().tracked_tuples,
            byte_scope.summary().overflow_tuples,
            byte_scope.summary().strict_attempts
        ),
        (0, 2, 2)
    );
    assert!(byte_scope.entries.is_empty());
}

#[cfg(feature = "signature-metrics")]
#[test]
fn nested_goal_scope_repeated_exact_signature_requires_one_strict_attempt() {
    // Construct/sign/encode both identical valid signing images before metrics.
    let first = prepared("nested-performance", &1u64);
    let second = prepared("nested-performance", &1u64);
    let mut scope = StrictOwnerSignatureScope::default();
    crate::world_service::private_goal::signature_metrics::begin_scope();
    scope.verify_prepared_nested(first).unwrap();
    scope.verify_prepared_nested(second).unwrap();
    let measured = crate::world_service::private_goal::signature_metrics::take_summary();
    assert_eq!(scope.summary().nested_repeats, 1);
    // The independently established performance RED target remains unchanged:
    // only the prior full exact actual success may skip this second Ed strict.
    assert_eq!(measured.strict_attempts, 1);
    assert_eq!(measured.failures, 0);
}

#[test]
fn nested_scope_keeps_fixed_profile_and_rejects_noncanonical_encoding_after_success() {
    let signed = sign_read_request("fixed-profile", 1u64, &"2a".repeat(32)).unwrap();
    let good = prepare_owner_signature_strict(
        "fixed-profile",
        &1u64,
        &signed.subject_public_key,
        &signed.signature_hex,
    )
    .unwrap();
    // Profile is fixed by the opaque SDK factory, never a caller field or mode.
    assert_eq!(
        good.profile(),
        "ed25519-verify-strict/oasis7.world-service.v1"
    );
    let mut scope = StrictOwnerSignatureScope::default();
    scope.verify_prepared_nested(good).unwrap();
    for (key, signature) in [
        (
            signed.subject_public_key.to_uppercase(),
            signed.signature_hex.clone(),
        ),
        (
            signed.subject_public_key.clone(),
            signed.signature_hex.to_uppercase(),
        ),
        (
            "01".to_owned() + &"00".repeat(31),
            signed.signature_hex.clone(),
        ),
    ] {
        assert!(prepare_owner_signature_strict("fixed-profile", &1u64, &key, &signature).is_err());
        assert_eq!(scope.summary().tracked_tuples, 1);
    }
    let mut noncanonical = signed.signature_hex;
    noncanonical.replace_range(64.., &"ff".repeat(32));
    for _ in 0..2 {
        let prepared = prepare_owner_signature_strict(
            "fixed-profile",
            &1u64,
            &signed.subject_public_key,
            &noncanonical,
        )
        .unwrap();
        let before = scope.summary().strict_attempts;
        assert!(scope.verify_prepared_nested(prepared).is_err());
        assert_eq!(scope.summary().strict_attempts, before + 1);
        assert_eq!(scope.summary().tracked_tuples, 1);
    }
    assert_eq!(scope.summary().failures, 2);
}
