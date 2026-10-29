//! Test-only monotonic elapsed clock for the partitioned membership harness.
//! The underlying snapshot/lease backend remains an explicit memory test double;
//! this does not attest protected hardware or time across process replacement.
use super::*;

/// All providers share this one-shot origin. Before activation the existing
/// durable floor stays unchanged; setting it cannot reset a lease or timer.
pub(super) fn elapsed_backend(
    inner: MemoryTestSignerBackend,
    started: Arc<std::sync::OnceLock<Instant>>,
) -> Box<dyn BftSignerBackendV3> {
    let initial_floor = inner
        .lock()
        .map(|snapshot| snapshot.protected_time_floor_ms);
    Box::new(ElapsedBackend {
        inner,
        started,
        initial_floor,
    })
}
struct ElapsedBackend {
    inner: MemoryTestSignerBackend,
    started: Arc<std::sync::OnceLock<Instant>>,
    initial_floor: Result<i64, BftSignerErrorV3>,
}
impl BftSignerBackendV3 for ElapsedBackend {
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.load_provisioned(binding_digest)
    }
    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.inner
            .acquire_lease(binding_digest, expected_generation, new_lease_token_digest)
    }
    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner
            .release_lease(expected_generation, expected_lease_token_digest)
    }
    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner
            .read_for_lease(lease_generation, lease_token_digest)
    }
    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV3> {
        let mut snapshot = self.inner.lock()?;
        MemoryTestSignerBackend::check_lease(&snapshot, lease_generation, lease_token_digest)?;
        let initial_floor = self.initial_floor.as_ref().map_err(Clone::clone)?;
        let elapsed_ms = self
            .started
            .get()
            .map_or(0, |origin| origin.elapsed().as_millis());
        let elapsed = i64::try_from(elapsed_ms)
            .map_err(|_| BftSignerErrorV3("membership test elapsed clock overflow".into()))?;
        let sampled = initial_floor
            .checked_add(elapsed)
            .ok_or_else(|| BftSignerErrorV3("membership test clock floor overflow".into()))?;
        snapshot.protected_time_floor_ms = snapshot.protected_time_floor_ms.max(sampled);
        Ok(snapshot.protected_time_floor_ms)
    }
    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.compare_and_swap(
            lease_generation,
            lease_token_digest,
            expected_revision,
            expected_state_digest,
            minimum_event_time_ms,
            next_state,
        )
    }
}
