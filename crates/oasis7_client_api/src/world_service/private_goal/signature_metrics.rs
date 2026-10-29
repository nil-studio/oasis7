//! Harness-only bounded counters. No signature result is memoized or returned here.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::BTreeSet};
const MAX_TUPLES: usize = 1024;
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SignatureMetricsSummary {
    pub calls: u64,
    pub strict_attempts: u64,
    pub memo_hits: u64,
    pub successes: u64,
    pub failures: u64,
    pub canonical_bytes: u64,
    pub max_canonical_bytes: u64,
    pub distinct_tuples: u64,
    pub duplicate_tuples: u64,
    pub overflow_tuples: u64,
    pub tracked_tuples: u64,
}
#[derive(Default)]
struct Scope {
    summary: SignatureMetricsSummary,
    tuples: BTreeSet<[u8; 32]>,
}
thread_local! { static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) }; }
/// Starts a fresh synchronous scope on this thread/Wasm instance, discarding any prior scope.
pub fn begin_scope() {
    SCOPE.with(|s| *s.borrow_mut() = Some(Scope::default()));
}
/// Ends collection and exposes counters only. Digests and proof material never leave the scope.
pub fn take_summary() -> SignatureMetricsSummary {
    SCOPE.with(|s| {
        s.borrow_mut()
            .take()
            .map_or_else(SignatureMetricsSummary::default, |s| s.summary)
    })
}
fn update(f: impl FnOnce(&mut Scope)) {
    SCOPE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            f(s);
        }
    });
}
pub(super) struct Call {
    active: bool,
    succeeded: bool,
}
impl Call {
    pub(super) fn new() -> Self {
        let active = SCOPE.with(|s| s.borrow().is_some());
        if active {
            update(|s| s.summary.calls = s.summary.calls.saturating_add(1));
        }
        Self {
            active,
            succeeded: false,
        }
    }
    pub(super) fn strict_attempt(&self, public: &[u8; 32], signature: &[u8; 64], canonical: &[u8]) {
        if !self.active {
            return;
        }
        let mut hasher = Sha256::new();
        // Length framing binds the algorithm, profile version, key, signature and FULL actual base.
        for field in [
            b"ed25519-strict".as_slice(),
            b"oasis7-owner-signature-v1",
            public.as_slice(),
            signature.as_slice(),
            canonical,
        ] {
            hasher.update((field.len() as u64).to_be_bytes());
            hasher.update(field);
        }
        let tuple: [u8; 32] = hasher.finalize().into();
        update(|s| {
            s.summary.strict_attempts = s.summary.strict_attempts.saturating_add(1);
            s.summary.canonical_bytes = s
                .summary
                .canonical_bytes
                .saturating_add(canonical.len() as u64);
            s.summary.max_canonical_bytes =
                s.summary.max_canonical_bytes.max(canonical.len() as u64);
            if s.tuples.contains(&tuple) {
                s.summary.duplicate_tuples = s.summary.duplicate_tuples.saturating_add(1);
            } else if s.tuples.len() < MAX_TUPLES {
                s.tuples.insert(tuple);
                s.summary.distinct_tuples = s.summary.distinct_tuples.saturating_add(1);
                s.summary.tracked_tuples = s.tuples.len() as u64;
            } else {
                s.summary.overflow_tuples = s.summary.overflow_tuples.saturating_add(1);
            }
        });
    }
    pub(super) fn memo_hit(&self) {
        if self.active {
            update(|s| s.summary.memo_hits = s.summary.memo_hits.saturating_add(1));
        }
    }
    pub(super) fn success(&mut self) {
        self.succeeded = true;
    }
}
impl Drop for Call {
    fn drop(&mut self) {
        if self.active {
            update(|s| {
                if self.succeeded {
                    s.summary.successes = s.summary.successes.saturating_add(1);
                } else {
                    s.summary.failures = s.summary.failures.saturating_add(1);
                }
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metrics_are_scoped_bounded_and_never_a_verification_result() {
        let mut ignored = Call::new();
        ignored.strict_attempt(&[1; 32], &[2; 64], b"private");
        ignored.success();
        drop(ignored);
        assert_eq!(take_summary(), SignatureMetricsSummary::default());
        begin_scope();
        for index in 0..1025u64 {
            let mut c = Call::new();
            c.strict_attempt(&[1; 32], &[2; 64], &index.to_be_bytes());
            c.success();
        }
        let c = Call::new();
        c.strict_attempt(&[1; 32], &[2; 64], &0u64.to_be_bytes());
        drop(c);
        let s = take_summary();
        assert_eq!((s.calls, s.successes, s.failures), (1026, 1025, 1));
        assert_eq!(
            (
                s.distinct_tuples,
                s.duplicate_tuples,
                s.overflow_tuples,
                s.tracked_tuples
            ),
            (1024, 1, 1, 1024)
        );
        assert_eq!(s.canonical_bytes, 1026 * 8);
        assert_eq!(s.max_canonical_bytes, 8);
        assert_eq!(take_summary(), SignatureMetricsSummary::default());
    }
    #[test]
    fn actual_strict_boundary_counts_repeated_success_and_failure_independently() {
        use ed25519_dalek::{Signer, SigningKey};
        let key = SigningKey::from_bytes(&[7; 32]);
        let request = serde_json::json!({"private":"not exposed by metrics"});
        let domain = "test-only-signature-metrics";
        let bytes = super::super::signing_bytes(domain, &request).unwrap();
        let signature = hex::encode(key.sign(&bytes).to_bytes());
        let public = hex::encode(key.verifying_key().to_bytes());
        begin_scope();
        for _ in 0..2 {
            super::super::verify_owner_signature_strict(domain, &request, &public, &signature)
                .unwrap();
        }
        assert!(
            super::super::verify_owner_signature_strict(
                domain,
                &serde_json::json!({"private":"changed"}),
                &public,
                &signature
            )
            .is_err()
        );
        assert!(
            super::super::verify_owner_signature_strict(domain, &request, &public, "malformed")
                .is_err()
        );
        let s = take_summary();
        assert_eq!(
            (s.calls, s.strict_attempts, s.successes, s.failures),
            (4, 3, 2, 2)
        );
        assert_eq!(
            (s.distinct_tuples, s.duplicate_tuples, s.overflow_tuples),
            (2, 1, 0)
        );
        assert_eq!(s.max_canonical_bytes, bytes.len() as u64);
    }
}
