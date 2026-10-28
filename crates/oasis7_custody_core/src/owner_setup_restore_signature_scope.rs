//! Synchronous restore-only bounded reuse of complete exact successful crypto tuples.
use oasis7_client_api::world_service::private_goal::PreparedOwnerSignature;
#[derive(Default, Clone, Copy, serde::Serialize)]
pub struct RestoreSignatureSummary {
    pub calls: u64,
    pub successes: u64,
    pub failures: u64,
    pub repeated_successes: u64,
    pub tracked_tuples: u64,
    pub overflow_tuples: u64,
}
#[derive(Default)]
pub(crate) struct RestoreSignatureScope {
    entries: Vec<PreparedOwnerSignature>,
    bytes: usize,
    summary: RestoreSignatureSummary,
}
impl RestoreSignatureScope {
    const MAX_ENTRIES: usize = 256;
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    pub(crate) fn verify(&mut self, prepared: PreparedOwnerSignature) -> Result<(), String> {
        self.summary.calls += 1;
        // Only a prior successful strict verification of this complete exact
        // SDK-owned tuple can be reused, within this one synchronous restore.
        if self.entries.iter().any(|old| exact(old, &prepared)) {
            self.summary.repeated_successes += 1;
            self.summary.successes += 1;
            return Ok(());
        }
        if let Err(error) = prepared.verify_strict() {
            self.summary.failures += 1;
            return Err(error);
        }
        self.summary.successes += 1;
        let bytes = prepared.canonical_bytes().len()
            + prepared.domain().len()
            + prepared.public_key_hex().len()
            + prepared.signature_hex().len()
            + prepared.profile().len();
        if self.entries.len() >= Self::MAX_ENTRIES
            || bytes > Self::MAX_BYTES.saturating_sub(self.bytes)
        {
            self.summary.overflow_tuples += 1;
            return Ok(());
        }
        self.bytes += bytes;
        self.entries.push(prepared);
        self.summary.tracked_tuples += 1;
        Ok(())
    }
    #[cfg(any(test, feature = "signature-metrics"))]
    pub(crate) fn summary(&self) -> RestoreSignatureSummary {
        self.summary
    }
}
fn exact(a: &PreparedOwnerSignature, b: &PreparedOwnerSignature) -> bool {
    a.profile() == b.profile()
        && a.domain() == b.domain()
        && a.public_key_hex() == b.public_key_hex()
        && a.signature_hex() == b.signature_hex()
        && a.canonical_bytes() == b.canonical_bytes()
}
#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_client_api::world_service::{
        private_goal::prepare_owner_signature_strict, sign_read_request,
    };
    fn prepared(domain: &str, value: u64, seed: u8) -> PreparedOwnerSignature {
        let signed = sign_read_request(domain, value, &format!("{seed:02x}").repeat(32)).unwrap();
        prepare_owner_signature_strict(
            domain,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )
        .unwrap()
    }
    #[test]
    fn restore_scope_reuses_only_exact_prior_successes() {
        let mut scope = RestoreSignatureScope::default();
        #[cfg(feature = "signature-metrics")]
        crate::signature_metrics::begin_scope();
        scope.verify(prepared("scope-a", 1, 42)).unwrap();
        scope.verify(prepared("scope-a", 1, 42)).unwrap();
        scope.verify(prepared("scope-b", 1, 42)).unwrap();
        scope.verify(prepared("scope-a", 2, 42)).unwrap();
        scope.verify(prepared("scope-a", 1, 43)).unwrap();
        let m = scope.summary();
        assert_eq!(
            (m.calls, m.successes, m.repeated_successes, m.tracked_tuples),
            (5, 5, 1, 4)
        );
        #[cfg(feature = "signature-metrics")]
        {
            let sdk = crate::signature_metrics::take_summary();
            assert_eq!((sdk.calls, sdk.strict_attempts, sdk.successes), (4, 4, 4));
        }
        let mut new_scope = RestoreSignatureScope::default();
        let fresh = prepared("scope-a", 1, 42);
        #[cfg(feature = "signature-metrics")]
        crate::signature_metrics::begin_scope();
        new_scope.verify(fresh).unwrap();
        assert_eq!(new_scope.summary().repeated_successes, 0);
        #[cfg(feature = "signature-metrics")]
        assert_eq!(crate::signature_metrics::take_summary().strict_attempts, 1);
    }
    #[cfg(feature = "signature-metrics")]
    #[test]
    fn restore_scope_same_tuple_requires_only_one_strict_attempt_per_local_scope() {
        // All signing and SDK encoding precede the measured verification scope.
        let first = prepared("scope-performance", 1, 42);
        let second = prepared("scope-performance", 1, 42);
        let mut scope = RestoreSignatureScope::default();
        crate::signature_metrics::begin_scope();
        scope.verify(first).unwrap();
        scope.verify(second).unwrap();
        let measured = crate::signature_metrics::take_summary();
        assert_eq!(scope.summary().repeated_successes, 1);
        // Performance oracle only: the second complete exact successful tuple
        // shares the first strict result within this local scope.
        assert_eq!(measured.strict_attempts, 1);
        assert_eq!(measured.failures, 0);
    }
    #[cfg(feature = "signature-metrics")]
    #[test]
    fn restore_scope_cached_success_does_not_accept_changed_tuple_fields() {
        let good = sign_read_request("cached-negative", 1u64, &"2a".repeat(32)).unwrap();
        let other = sign_read_request("cached-negative", 1u64, &"2b".repeat(32)).unwrap();
        let mut bad_signature = good.signature_hex.clone();
        bad_signature.replace_range(
            ..2,
            if &bad_signature[..2] == "00" {
                "01"
            } else {
                "00"
            },
        );
        let prepare = |domain: &str, payload: &u64, key: &str, signature: &str| {
            prepare_owner_signature_strict(domain, payload, key, signature).unwrap()
        };
        let mut scope = RestoreSignatureScope::default();
        scope
            .verify(prepare(
                "cached-negative",
                &good.request,
                &good.subject_public_key,
                &good.signature_hex,
            ))
            .unwrap();
        let changed = [
            prepare(
                "cached-negative",
                &good.request,
                &good.subject_public_key,
                &bad_signature,
            ),
            prepare(
                "changed-domain",
                &good.request,
                &good.subject_public_key,
                &good.signature_hex,
            ),
            prepare(
                "cached-negative",
                &good.request,
                &other.subject_public_key,
                &good.signature_hex,
            ),
            prepare(
                "cached-negative",
                &2u64,
                &good.subject_public_key,
                &good.signature_hex,
            ),
        ];
        for candidate in changed {
            crate::signature_metrics::begin_scope();
            assert!(scope.verify(candidate).is_err());
            let sdk = crate::signature_metrics::take_summary();
            assert_eq!(
                (sdk.strict_attempts, sdk.failures, sdk.successes),
                (1, 1, 0)
            );
            assert_eq!(
                (
                    scope.summary().tracked_tuples,
                    scope.summary().repeated_successes
                ),
                (1, 0)
            );
        }
        assert_eq!(scope.summary().failures, 4);
    }
    #[test]
    fn restore_scope_never_records_failures_or_rejects_capacity_overflow() {
        let good = sign_read_request("scope-a", 1u64, &"2a".repeat(32)).unwrap();
        let mut scope = RestoreSignatureScope::default();
        for _ in 0..2 {
            let bad = prepare_owner_signature_strict(
                "different-domain",
                &good.request,
                &good.subject_public_key,
                &good.signature_hex,
            )
            .unwrap();
            assert!(scope.verify(bad).is_err());
        }
        assert_eq!(
            (scope.summary().failures, scope.summary().tracked_tuples),
            (2, 0)
        );
        // Each altered tuple reaches real strict crypto and is never recorded.
        let other_key = sign_read_request("scope-a", 1u64, &"2b".repeat(32)).unwrap();
        let mut bad_signature = good.signature_hex.clone();
        bad_signature.replace_range(
            ..2,
            if &bad_signature[..2] == "00" {
                "01"
            } else {
                "00"
            },
        );
        for bad in [
            prepare_owner_signature_strict(
                "scope-a",
                &2u64,
                &good.subject_public_key,
                &good.signature_hex,
            )
            .unwrap(),
            prepare_owner_signature_strict(
                "scope-a",
                &good.request,
                &other_key.subject_public_key,
                &good.signature_hex,
            )
            .unwrap(),
            prepare_owner_signature_strict(
                "scope-a",
                &good.request,
                &good.subject_public_key,
                &bad_signature,
            )
            .unwrap(),
        ] {
            assert!(scope.verify(bad).is_err());
        }
        assert_eq!(
            (scope.summary().failures, scope.summary().tracked_tuples),
            (5, 0)
        );
        // Capacity overflow still verifies each miss and never grants trust to
        // another occurrence of that unrecorded tuple.
        let overflow_a = prepared("overflow", 1, 42);
        let overflow_b = prepared("overflow", 1, 42);
        let overflow_bad = prepare_owner_signature_strict(
            "overflow",
            &2u64,
            &good.subject_public_key,
            &good.signature_hex,
        )
        .unwrap();
        let mut full = RestoreSignatureScope {
            bytes: RestoreSignatureScope::MAX_BYTES,
            ..Default::default()
        };
        #[cfg(feature = "signature-metrics")]
        crate::signature_metrics::begin_scope();
        full.verify(overflow_a).unwrap();
        full.verify(overflow_b).unwrap();
        assert!(full.verify(overflow_bad).is_err());
        #[cfg(feature = "signature-metrics")]
        {
            let sdk = crate::signature_metrics::take_summary();
            assert_eq!((sdk.strict_attempts, sdk.failures), (3, 1));
        }
        assert_eq!(
            (
                full.summary().tracked_tuples,
                full.summary().repeated_successes,
                full.summary().overflow_tuples
            ),
            (0, 0, 2)
        );
        // Force only the bounded storage capacity, never a verification result.
        scope.bytes = RestoreSignatureScope::MAX_BYTES;
        scope.verify(prepared("scope-a", 1, 42)).unwrap();
        assert_eq!(
            (scope.summary().successes, scope.summary().overflow_tuples),
            (1, 1)
        );
        assert_eq!(scope.summary().tracked_tuples, 0);
        scope.bytes = 0;
        for i in 0..=RestoreSignatureScope::MAX_ENTRIES {
            scope.verify(prepared("scope-a", i as u64, 42)).unwrap();
        }
        assert_eq!(
            scope.summary().tracked_tuples,
            RestoreSignatureScope::MAX_ENTRIES as u64
        );
        assert_eq!(scope.summary().overflow_tuples, 2);
    }
}
