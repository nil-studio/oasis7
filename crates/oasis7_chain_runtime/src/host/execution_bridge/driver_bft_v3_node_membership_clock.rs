//! Test-only monotonic elapsed clock for pending recovery and membership harnesses.
//! The underlying snapshot/lease backend remains an explicit memory test double;
//! this does not attest protected hardware or time across process replacement.
use super::*;

/// The original base and one-shot origin outlive every provider wrapper.
/// Reopening a backend must clone this anchor, never capture its current floor.
pub(super) struct ElapsedClock {
    original_floor: i64,
    started: std::sync::OnceLock<Instant>,
}
impl ElapsedClock {
    pub(super) fn new(original_floor: i64) -> Self {
        assert!(original_floor >= 0);
        Self {
            original_floor,
            started: std::sync::OnceLock::new(),
        }
    }
    pub(super) fn start(&self) -> Result<(), Instant> {
        self.started.set(Instant::now())
    }
    pub(super) fn elapsed_ms(&self) -> Result<i64, BftSignerErrorV3> {
        i64::try_from(
            self.started
                .get()
                .map_or(0, |origin| origin.elapsed().as_millis()),
        )
        .map_err(|_| BftSignerErrorV3("network test elapsed clock overflow".into()))
    }
    fn sample_at_elapsed(&self, current_floor: i64, elapsed: i64) -> Result<i64, BftSignerErrorV3> {
        if elapsed < 0 {
            return Err(BftSignerErrorV3(
                "network test negative elapsed clock".into(),
            ));
        }
        let sampled = self
            .original_floor
            .checked_add(elapsed)
            .ok_or_else(|| BftSignerErrorV3("network test clock floor overflow".into()))?;
        Ok(current_floor.max(sampled))
    }
    pub(super) fn diagnostic(&self) -> (i64, Result<i64, BftSignerErrorV3>) {
        (self.original_floor, self.elapsed_ms())
    }
}
pub(super) fn elapsed_backend(
    inner: MemoryTestSignerBackend,
    clock: Arc<ElapsedClock>,
) -> Box<dyn BftSignerBackendV3> {
    Box::new(ElapsedBackend { inner, clock })
}
struct ElapsedBackend {
    inner: MemoryTestSignerBackend,
    clock: Arc<ElapsedClock>,
}
/// Two complete policy rounds followed by one actual private operation. The
/// exact action/height oracle remains bounded independently of empty progress.
pub(super) fn recovery_action_budget(
    timeouts: oasis7_consensus::tendermint_v1::BftTimeoutsV1,
    private_operation: Duration,
) -> Duration {
    let zero = timeouts
        .propose_ms
        .checked_add(timeouts.prevote_ms)
        .and_then(|v| v.checked_add(timeouts.precommit_ms))
        .unwrap();
    let one = zero
        .checked_add(timeouts.round_delta_ms.checked_mul(3).unwrap())
        .unwrap();
    Duration::from_millis(zero.checked_add(one).unwrap())
        .checked_add(private_operation)
        .unwrap()
}

#[test]
fn elapsed_clock_reopen_keeps_original_anchor_and_monotonic_floor() {
    let anchor = Arc::new(ElapsedClock::new(100));
    assert_eq!(anchor.sample_at_elapsed(100, 0).unwrap(), 100);
    let durable = anchor.sample_at_elapsed(100, 5_000).unwrap();
    assert_eq!(durable, 5_100);
    anchor.start().unwrap();
    let original_origin = *anchor.started.get().unwrap();
    let reopened = anchor.clone();
    assert!(Arc::ptr_eq(&anchor, &reopened));
    assert!(reopened.start().is_err());
    assert_eq!(*reopened.started.get().unwrap(), original_origin);
    assert_eq!(reopened.sample_at_elapsed(durable, 6_000).unwrap(), 6_100);
    assert_ne!(
        reopened.sample_at_elapsed(durable, 6_000).unwrap(),
        11_100,
        "reopen must not add prior elapsed again"
    );
    assert_eq!(reopened.sample_at_elapsed(6_100, 5_000).unwrap(), 6_100);
    assert!(reopened.sample_at_elapsed(6_100, -1).is_err());
    assert!(
        ElapsedClock::new(i64::MAX)
            .sample_at_elapsed(i64::MAX, 1)
            .is_err()
    );
}

#[test]
fn recovery_budget_is_two_policy_rounds_and_exactly_one_private_operation() {
    let policy = oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
        propose_ms: 90_000,
        prevote_ms: 30_000,
        precommit_ms: 30_000,
        round_delta_ms: 10_000,
    };
    assert_eq!(
        recovery_action_budget(policy, Duration::ZERO),
        Duration::from_secs(330)
    );
    assert_eq!(
        recovery_action_budget(policy, PRIVATE_OPERATION_TIMEOUT),
        Duration::from_secs(390)
    );
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
        snapshot.protected_time_floor_ms = self
            .clock
            .sample_at_elapsed(snapshot.protected_time_floor_ms, self.clock.elapsed_ms()?)?;
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
