//! Current-v1 signed record time checks. State is process-local, not trusted time.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use oasis7_proto::distributed_dht::PeerRecord;

use crate::error::WorldError;

const MAX_FUTURE_SKEW_MS: i64 = 30_000;

#[derive(Debug, Default)]
struct ClockState {
    wall_highwater: Option<i64>,
    anchor: Option<(i64, Instant)>,
    last_instant: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DiscoveryClock(Arc<Mutex<ClockState>>);

impl PartialEq for DiscoveryClock {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for DiscoveryClock {}

fn invalid(reason: &str) -> WorldError {
    WorldError::NetworkProtocolUnavailable {
        protocol: format!("peer record time policy: {reason}"),
    }
}

fn wall_now_ms() -> Result<i64, WorldError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("wall clock precedes Unix epoch"))?;
    i64::try_from(elapsed.as_millis()).map_err(|_| invalid("wall clock exceeds i64 milliseconds"))
}

impl DiscoveryClock {
    /// Reject known backward wall samples while retaining the largest sampled wall time.
    pub(super) fn observe(&self, now: i64, instant: Instant) -> Result<i64, WorldError> {
        let mut state = self.0.lock().map_err(|_| invalid("clock lock poisoned"))?;
        if now < 0 {
            return Err(invalid("negative wall time"));
        }
        if state.wall_highwater.is_some_and(|previous| now < previous) {
            return Err(invalid("wall clock moved backward"));
        }
        if state
            .last_instant
            .is_some_and(|previous| instant < previous)
        {
            return Err(invalid("monotonic clock moved backward"));
        }
        state.wall_highwater = Some(now);
        state.last_instant = Some(instant);
        match state.anchor {
            Some((wall, anchor)) => {
                // Only move the anchor forward when wall time overtakes monotonic time.
                // A stalled wall clock can therefore never extend an accepted deadline.
                let wall_advance = Duration::from_millis((now - wall) as u64);
                if wall_advance > instant.duration_since(anchor) {
                    state.anchor = Some((now, instant));
                }
            }
            None => state.anchor = Some((now, instant)),
        }
        Ok(now)
    }

    pub(super) fn check(&self, record: &PeerRecord) -> Result<RecordLease, WorldError> {
        self.check_at(record, wall_now_ms()?, Instant::now())
    }

    pub(super) fn check_at(
        &self,
        record: &PeerRecord,
        now: i64,
        instant: Instant,
    ) -> Result<RecordLease, WorldError> {
        let expiry = record.expires_at_ms().map_err(|reason| invalid(&reason))?;
        self.observe(now, instant)?;
        if now >= expiry {
            return Err(invalid("record expired"));
        }
        if record.published_at_ms.saturating_sub(now) > MAX_FUTURE_SKEW_MS {
            return Err(invalid("publication exceeds future skew allowance"));
        }
        let state = self.0.lock().map_err(|_| invalid("clock lock poisoned"))?;
        let (wall, anchor) = state
            .anchor
            .ok_or_else(|| invalid("missing clock anchor"))?;
        let remaining = expiry
            .checked_sub(wall)
            .filter(|value| *value > 0)
            .ok_or_else(|| invalid("record expired"))?;
        let deadline = anchor
            .checked_add(Duration::from_millis(remaining as u64))
            .ok_or_else(|| invalid("monotonic deadline overflow"))?;
        if instant >= deadline {
            return Err(invalid("record monotonic lease expired"));
        }
        Ok(RecordLease {
            clock: self.clone(),
            expiry,
            deadline,
            receiver_guard: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecordLease {
    clock: DiscoveryClock,
    expiry: i64,
    deadline: Instant,
    receiver_guard: Option<super::receiver_floor::ReceiverLeaseGuard>,
}

impl RecordLease {
    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }

    pub(super) fn validate(&self) -> Result<(), WorldError> {
        self.validate_at(wall_now_ms()?, Instant::now())
    }

    #[cfg(test)]
    pub(super) fn is_valid_at(&self, now: i64, instant: Instant) -> bool {
        self.validate_at(now, instant).is_ok()
    }

    pub(super) fn validate_at(&self, now: i64, instant: Instant) -> Result<(), WorldError> {
        self.clock.observe(now, instant)?;
        if now >= self.expiry || instant >= self.deadline {
            return Err(invalid("record lease expired"));
        }
        if let Some(guard) = &self.receiver_guard {
            guard.validate()?;
        }
        Ok(())
    }

    pub(super) fn with_receiver_guard(
        mut self,
        guard: super::receiver_floor::ReceiverLeaseGuard,
    ) -> Self {
        self.receiver_guard = Some(guard);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(published: i64, ttl: i64) -> PeerRecord {
        PeerRecord {
            peer_id: "peer".into(),
            node_id: "node".into(),
            world_id: "world".into(),
            network_id: "network".into(),
            node_role: "observer_light".into(),
            deployment_mode: oasis7_proto::distributed_dht::PeerDeploymentMode::Private,
            reachability_class: oasis7_proto::distributed_dht::PeerReachabilityClass::Private,
            direct_addrs: vec![],
            hole_punch_addrs: vec![],
            relay_addrs: vec![],
            discovery_sources: vec![],
            capability_lanes: vec![],
            source_operator: None,
            source_asn: None,
            published_at_ms: published,
            ttl_ms: ttl,
        }
    }

    #[test]
    fn time_policy_enforces_exact_expiry_and_future_skew_boundaries() {
        let start = Instant::now();
        for (published, ttl, accepted) in [
            (1_000, 1, true),
            (999, 1, false),
            (31_000, 1, true),
            (31_001, 1, false),
            (-1, 1, false),
            (1_000, 0, false),
            (1_000, 3_600_000, true),
            (1_000, 3_600_001, false),
            (i64::MAX, 1, false),
        ] {
            assert_eq!(
                DiscoveryClock::default()
                    .check_at(&record(published, ttl), 1_000, start)
                    .is_ok(),
                accepted
            );
        }
    }

    #[test]
    fn rollback_fails_closed_until_wall_recovers_to_actual_highwater() {
        let clock = DiscoveryClock::default();
        let start = Instant::now();
        let lease = clock
            .check_at(&record(1_000, 10_000), 1_000, start)
            .unwrap();
        assert!(
            clock
                .observe(1_100, start + Duration::from_millis(100))
                .is_ok()
        );
        assert!(
            lease
                .validate_at(1_099, start + Duration::from_millis(101))
                .is_err()
        );
        assert!(
            clock
                .observe(-1, start + Duration::from_millis(102))
                .is_err()
        );
        assert!(
            lease
                .validate_at(1_100, start + Duration::from_millis(103))
                .is_ok()
        );
        assert!(
            lease
                .validate_at(1_099, start + Duration::from_millis(104))
                .is_err()
        );
    }

    #[test]
    fn monotonic_expiry_and_retransmission_cannot_revive_or_extend_lease() {
        let clock = DiscoveryClock::default();
        let start = Instant::now();
        let record = record(1_000, 100);
        let first = clock.check_at(&record, 1_000, start).unwrap();
        let repeated = clock
            .check_at(&record, 1_000, start + Duration::from_millis(50))
            .unwrap();
        assert_eq!(first.deadline(), repeated.deadline());
        assert!(first.is_valid_at(1_000, start + Duration::from_millis(99)));
        assert!(
            first
                .validate_at(1_000, start + Duration::from_millis(100))
                .is_err()
        );
        assert!(
            clock
                .check_at(&record, 1_000, start + Duration::from_millis(101))
                .is_err()
        );
        assert!(
            first
                .validate_at(1_050, start + Duration::from_millis(102))
                .is_err()
        );
    }

    #[test]
    fn forward_wall_jump_only_shortens_subsequent_deadlines() {
        let clock = DiscoveryClock::default();
        let start = Instant::now();
        let record = record(1_000, 1_000);
        let first = clock.check_at(&record, 1_000, start).unwrap();
        let later = clock
            .check_at(&record, 1_500, start + Duration::from_millis(100))
            .unwrap();
        assert!(later.deadline() < first.deadline());
        assert!(
            later
                .validate_at(1_500, start + Duration::from_millis(600))
                .is_err()
        );
    }

    #[test]
    fn wall_millisecond_quantization_does_not_trigger_false_rollback() {
        let clock = DiscoveryClock::default();
        let start = Instant::now();
        assert!(clock.observe(1_000, start).is_ok());
        assert!(
            clock
                .observe(1_000, start + Duration::from_micros(900))
                .is_ok()
        );
        assert!(
            clock
                .observe(1_001, start + Duration::from_micros(1_900))
                .is_ok()
        );
    }
}
