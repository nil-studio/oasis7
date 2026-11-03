//! Faults at the new metadata handler's actual backend seams. The original
//! read_current fault1/2 tests remain separate and are not this evidence.
use super::*;

pub(super) struct MetadataFault {
    inner: Box<dyn BftSignerBackendV3>,
    mode: Arc<AtomicUsize>,
    fresh_fault: usize,
}
impl MetadataFault {
    pub fn new(inner: Box<dyn BftSignerBackendV3>, mode: Arc<AtomicUsize>) -> Self {
        Self {
            inner,
            mode,
            fresh_fault: 0,
        }
    }
}
macro_rules! forward {
    ($name:ident($($argument:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&mut self, $($argument: $ty),*) -> $result { self.inner.$name($($argument),*) }
    };
}
impl BftSignerBackendV3 for MetadataFault {
    forward!(read_private_retention_creation(generation: u64, token: &str) -> Result<ValidatedBftPrivateRetentionCreationV1, BftSignerErrorV3>);
    forward!(load_private_retention_current(binding: &str) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3>);
    forward!(read_private_retention_current(generation: u64, token: &str) -> Result<BftPrivateRetentionCurrentV1, BftSignerErrorV3>);
    forward!(compare_and_swap_private_retention_seal(request: &BftPrivateRetentionSealCasV1) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3>);
    forward!(load_private_retention_input(binding: &str, reference: &BftPrivateRetentionAdmissionInputRefV1) -> Result<Arc<BftPrivateRetentionAdmissionInputV1>, BftSignerErrorV3>);
    forward!(compare_and_swap_private_retention(request: &BftPrivateRetentionCasV1) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3>);
    forward!(read_private_retention_operation(generation: u64, token: &str, operation: &str) -> Result<BftPrivateRetentionSnapshotV1, BftSignerErrorV3>);
    forward!(load_provisioned(binding: &str) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    forward!(acquire_lease(binding: &str, expected: u64, token: &str) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3>);
    forward!(release_lease(generation: u64, token: &str) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    forward!(protected_time_ms(generation: u64, token: &str) -> Result<i64, BftSignerErrorV3>);
    forward!(reserve_successor_archive_transfer(generation: u64, token: &str, revision: u64, digest: &str, expected: u64, amount: u64) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    forward!(compare_and_swap(generation: u64, token: &str, revision: u64, digest: &str, minimum: Option<i64>, next: BftSignerStateV3) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>);
    fn read_private_retention_sealed_current(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftPrivateRetentionSealedCurrentV1, BftSignerErrorV3> {
        let mut sealed = self
            .inner
            .read_private_retention_sealed_current(generation, token)?;
        match self.mode.swap(0, Ordering::SeqCst) {
            0 => {}
            1 => {
                return Err(BftSignerErrorV3(
                    "metadata sealed-current injected read failure".into(),
                ));
            }
            2 => {
                sealed.current.signer.protected_time_floor_ms += 1;
            }
            3 => {
                assert!(!sealed.history.is_empty());
                sealed.history.clear();
            }
            mode @ (4 | 5) => self.fresh_fault = mode,
            _ => unreachable!(),
        }
        Ok(sealed)
    }
    fn read_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut original = self.inner.read_for_lease(generation, token)?;
        match std::mem::take(&mut self.fresh_fault) {
            0 => {}
            4 => {
                return Err(BftSignerErrorV3(
                    "metadata fresh-current injected read failure".into(),
                ));
            }
            5 => original.protected_time_floor_ms += 1,
            _ => unreachable!(),
        }
        Ok(original)
    }
}
#[test]
fn private_retention_metadata_sealed_faults_revoke_actual_serving_guards_without_mutating_originals()
 {
    for mode in 1..=5 {
        run_retention_case_with_metadata_fault(0, false, Some(mode));
    }
}

pub(super) fn assert_guard_revocation(
    mode: usize,
    binding: &BftSignerBindingV3,
    retained: &mut BftPrivateRetentionMemoryBackendV1,
    lease: &mut dyn BftSignerLeaseV3,
    service: &BftSignerServiceV3,
    metadata_mode: &AtomicUsize,
    deadline: impl Fn() -> Instant,
) {
    let before = retained
        .load_provisioned(&binding.digest().unwrap())
        .unwrap();
    let originals = retained
        .read_private_retention_sealed_current(before.lease_generation, &before.lease_token_digest)
        .unwrap();
    let (digest, original) = originals.history.first_key_value().unwrap();
    let (guard, fence) = service
        .with_observed_original_input_view(deadline(), |view| {
            assert!(view.is_current());
            let fence: Box<dyn Fn() -> bool + Send + Sync + 'static> =
                Box::new(view.release_fence());
            Ok((view.release_guard(), fence))
        })
        .unwrap();
    assert!(
        guard.load(Ordering::Acquire) && fence(),
        "actual queued guard is live before fault {mode}"
    );
    metadata_mode.store(mode, Ordering::SeqCst);
    let error = lease
        .read_private_retention_operation_proof(digest.clone(), deadline())
        .unwrap_err();
    let expected = match mode {
        1 => "sealed-current injected read failure",
        2 => "atomic current changed",
        3 => "original",
        4 => "fresh-current injected read failure",
        5 => "lease/head changed during encoding",
        _ => unreachable!(),
    };
    assert!(
        error.to_string().contains(expected),
        "must reach the metadata backend seam {mode}: {error}"
    );
    assert!(
        !guard.load(Ordering::Acquire) && !fence(),
        "detected metadata fault {mode} must revoke the old actual release guard"
    );
    assert_eq!(
        retained
            .load_provisioned(&binding.digest().unwrap())
            .unwrap(),
        before,
        "metadata failure before release cannot mutate State, receipt or transfer counter"
    );
    assert!(
        retained
            .read_private_retention_sealed_current(
                before.lease_generation,
                &before.lease_token_digest
            )
            .unwrap()
            == originals,
        "injection cannot rewrite the actual original history/inventory"
    );
    let fresh_guard = service
        .with_observed_original_input_view(deadline(), |view| Ok(view.release_guard()))
        .unwrap();
    assert!(fresh_guard.load(Ordering::Acquire));
    assert!(
        !guard.load(Ordering::Acquire),
        "reauthenticating the same State cannot revive a revoked guard"
    );
    let proof = lease
        .read_private_retention_operation_proof(digest.clone(), deadline())
        .unwrap();
    assert_eq!(proof.state, original.signer.state);
    assert_eq!(proof.ledger, original.retention);
    assert_eq!(lease.checkpoint().unwrap().state, before.state);
}
