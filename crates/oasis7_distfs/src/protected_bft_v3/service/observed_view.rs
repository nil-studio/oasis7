//! Local provider-only public serving view. No wire DTO may mint this token;
//! it never acquires a lease or creates a World/driver replay authority.
use super::*;
use oasis7_consensus::tendermint_v3::BftHeaderHeightScheduleV3;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Instant;

#[derive(Clone, PartialEq, Eq)]
struct ViewIdentity {
    lease_generation: u64,
    lease_token_digest: String,
    state_digest: String,
    revision: u64,
}
impl ViewIdentity {
    fn of(snapshot: &BftSignerBackendSnapshotV3) -> Self {
        Self {
            lease_generation: snapshot.lease_generation,
            lease_token_digest: snapshot.lease_token_digest.clone(),
            state_digest: snapshot.state.state_digest.clone(),
            revision: snapshot.state.revision,
        }
    }
}

/// Kept in the service runtime. Every State/lease mutation (including unknown
/// outcomes and poisoning) must call invalidate before attempting publication.
/// Read-only transfer counters and monotonic clock observations do not reset it.
pub(super) struct OriginalInputViewEpochV1 {
    generation: Arc<AtomicU64>,
    identity: Option<ViewIdentity>,
    release_guards: Vec<std::sync::Weak<AtomicBool>>,
}
impl Default for OriginalInputViewEpochV1 {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            identity: None,
            release_guards: Vec::new(),
        }
    }
}
impl OriginalInputViewEpochV1 {
    pub(super) fn invalidate(&mut self) -> Result<(), BftSignerErrorV3> {
        for guard in self
            .release_guards
            .drain(..)
            .filter_map(|weak| weak.upgrade())
        {
            guard.store(false, Ordering::Release);
        }
        self.identity = None;
        Ok(())
    }
    fn observe(
        &mut self,
        snapshot: &BftSignerBackendSnapshotV3,
    ) -> Result<(u64, Arc<AtomicBool>), BftSignerErrorV3> {
        let identity = ViewIdentity::of(snapshot);
        let Some(generation) = snapshot
            .state
            .revision
            .checked_add(snapshot.lease_generation)
        else {
            self.invalidate()?;
            return Err(invalid("original input serving generation overflow"));
        };
        let previous_generation = self.generation.load(Ordering::Acquire);
        if generation < previous_generation
            || self
                .identity
                .as_ref()
                .is_some_and(|old| old != &identity && generation <= previous_generation)
        {
            self.invalidate()?;
            return Err(invalid(
                "original input serving identity/generation regressed",
            ));
        }
        if self.identity.as_ref().is_some_and(|old| old != &identity) {
            self.invalidate()?;
        }
        self.generation.store(generation, Ordering::Release);
        self.identity = Some(identity);
        self.release_guards.retain(|weak| weak.strong_count() != 0);
        let release_guard = Arc::new(AtomicBool::new(true));
        self.release_guards.push(Arc::downgrade(&release_guard));
        Ok((generation, release_guard))
    }
}

/// Opaque public signer schedule/lease continuity for the original-input server
/// only. It is not an execution head, signer lease or private ReadGrant.
pub struct ObservedBftOriginalInputServingViewV1 {
    deadline: Instant,
    binding_digest: String,
    provider_public_key: String,
    identity: ViewIdentity,
    schedule: BftHeaderHeightScheduleV3,
    generation: u64,
    epoch: Arc<AtomicU64>,
    release_guard: Arc<AtomicBool>,
}
impl ObservedBftOriginalInputServingViewV1 {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn binding_digest(&self) -> &str {
        &self.binding_digest
    }
    pub fn provider_public_key(&self) -> &str {
        &self.provider_public_key
    }
    pub fn schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.schedule
    }
    pub fn lease_generation(&self) -> u64 {
        self.identity.lease_generation
    }
    pub fn revision(&self) -> u64 {
        self.identity.revision
    }
    pub fn state_digest(&self) -> &str {
        &self.identity.state_digest
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn is_current(&self) -> bool {
        self.release_guard.load(Ordering::Acquire)
            && self.epoch.load(Ordering::Acquire) == self.generation
    }
    pub fn release_guard(&self) -> Arc<AtomicBool> {
        self.release_guard.clone()
    }
    /// Network adapters must retain this original token and test its fence for
    /// each partial release, in addition to the independent issuer's gate.
    pub fn release_fence(&self) -> impl Fn() -> bool + Send + Sync + 'static {
        let epoch = self.epoch.clone();
        let generation = self.generation;
        let allowed = self.release_guard.clone();
        move || allowed.load(Ordering::Acquire) && epoch.load(Ordering::Acquire) == generation
    }
}

impl BftSignerServiceV3 {
    fn original_input_view_deadline(
        &self,
        requested: Instant,
    ) -> Result<Instant, BftSignerErrorV3> {
        let support = self.private_retention_support.as_ref().ok_or_else(|| {
            invalid("original input serving view requires explicit private retention support")
        })?;
        Ok(requested.min(support.deadline(Instant::now())?))
    }

    fn observe_original_input_view(
        &self,
        deadline: Instant,
    ) -> Result<ObservedBftOriginalInputServingViewV1, BftSignerErrorV3> {
        if Instant::now() >= deadline {
            return Err(invalid("original input serving view deadline expired"));
        }
        // No backend or service mutex is held across a callback/network read.
        let mut runtime = self
            .runtime
            .try_lock()
            .map_err(|_| invalid("original input serving view provider busy or poisoned"))?;
        if runtime.poisoned {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid("original input serving view provider poisoned"));
        }
        let Some(active) = runtime.active_lease.clone() else {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid(
                "original input serving view requires current original lease",
            ));
        };
        let snapshot = match runtime
            .backend
            .read_for_lease(active.generation, &active.token_digest)
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
        };
        if let Err(error) = validate_snapshot_shape(&snapshot, None, Some(&active.token_digest)) {
            runtime.input_view_epoch.invalidate()?;
            return Err(error);
        }
        if snapshot.lease_generation != active.generation {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid("original input serving view lease fenced"));
        }
        let previous_deadline = runtime.successor_deadline;
        runtime.successor_deadline = Some(deadline);
        let validation = self.verify_observed_snapshot(
            &mut runtime,
            &snapshot,
            &BftSignerLeaseCredentialV3 {
                generation: active.generation,
                token: String::new(),
            },
        );
        runtime.successor_deadline = previous_deadline;
        if let Err(error) = validation {
            runtime.input_view_epoch.invalidate()?;
            return Err(error);
        }
        let verified = match self.validate_provider_state(&snapshot.state, Some(deadline)) {
            Ok(verified) => verified,
            Err(error) => {
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
        };
        let (generation, release_guard) = runtime.input_view_epoch.observe(&snapshot)?;
        let view = ObservedBftOriginalInputServingViewV1 {
            deadline,
            binding_digest: self.binding.digest()?,
            provider_public_key: self.provider_key.public_key_hex(),
            identity: ViewIdentity::of(&snapshot),
            schedule: verified.schedule,
            generation,
            epoch: runtime.input_view_epoch.generation.clone(),
            release_guard,
        };
        if Instant::now() >= deadline {
            return Err(invalid("original input serving view deadline expired"));
        }
        Ok(view)
    }

    /// Build an authorization response or encoded material only inside this
    /// scope. The independent issuer must still authorize each actual release.
    pub fn with_observed_original_input_view<T>(
        &self,
        deadline: Instant,
        make_response: impl FnOnce(
            &ObservedBftOriginalInputServingViewV1,
        ) -> Result<T, BftSignerErrorV3>,
    ) -> Result<T, BftSignerErrorV3> {
        let deadline = self.original_input_view_deadline(deadline)?;
        let before = self.observe_original_input_view(deadline)?;
        let response = make_response(&before)?;
        let after = self.observe_original_input_view(deadline)?;
        if !before.is_current()
            || !after.is_current()
            || before.identity != after.identity
            || before.generation != after.generation
            || before.schedule != after.schedule
        {
            return Err(invalid(
                "original input serving view changed during authorized callback",
            ));
        }
        Ok(response)
    }
    /// The provider retained-input Noise endpoint must use this path for every
    /// successful manifest/chunk/hint response. Buffer ownership is separate:
    /// this atomically charges the existing protected namespace transfer ledger.
    pub fn with_charged_original_input_response(
        &self,
        deadline: Instant,
        make_response: impl FnOnce(
            &ObservedBftOriginalInputServingViewV1,
        ) -> Result<Vec<u8>, BftSignerErrorV3>,
    ) -> Result<Vec<u8>, BftSignerErrorV3> {
        let deadline = self.original_input_view_deadline(deadline)?;
        let before = self.observe_original_input_view(deadline)?;
        let response = make_response(&before)?;
        let amount = bft_original_input_network_response_bytes_v1(&response)?;
        if Instant::now() >= deadline {
            return Err(invalid("original input response expired before charge"));
        }
        let mut runtime = self
            .runtime
            .try_lock()
            .map_err(|_| invalid("original input response provider busy or poisoned"))?;
        if runtime.poisoned {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid("original input response provider poisoned"));
        }
        let Some(active) = runtime.active_lease.clone() else {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid("original input response lease unavailable"));
        };
        if active.generation != before.identity.lease_generation
            || active.token_digest != before.identity.lease_token_digest
        {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid("original input response owner changed"));
        }
        let snapshot = match runtime
            .backend
            .read_for_lease(active.generation, &active.token_digest)
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
        };
        let previous_deadline = runtime.successor_deadline;
        runtime.successor_deadline = Some(deadline);
        let validation = self.verify_observed_snapshot(
            &mut runtime,
            &snapshot,
            &BftSignerLeaseCredentialV3 {
                generation: active.generation,
                token: String::new(),
            },
        );
        runtime.successor_deadline = previous_deadline;
        if let Err(error) = validation {
            runtime.input_view_epoch.invalidate()?;
            return Err(error);
        }
        let (generation, _) = runtime.input_view_epoch.observe(&snapshot)?;
        if ViewIdentity::of(&snapshot) != before.identity
            || generation != before.generation
            || !before.is_current()
        {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid(
                "original input response State changed during callback",
            ));
        }
        // reserve_archive_transfer performs the original expected-counter CAS,
        // preserves State/lease/time and verifies its exact atomic result.
        let charged = self.reserve_archive_transfer(&mut runtime, &snapshot, amount)?;
        if ViewIdentity::of(&charged) != before.identity
            || !before.is_current()
            || Instant::now() >= deadline
        {
            return Err(invalid(
                "original input response changed or expired after charge",
            ));
        }
        Ok(response)
    }
}

/// Exact CBOR size of the pinned request-response codec's NetworkResponse {
/// payload: Vec<u8> }. Its logical frame has no separate length prefix. A real
/// Codec::write_response regression must guard this coupling before shipping.
pub fn bft_original_input_network_response_bytes_v1(
    payload: &[u8],
) -> Result<u64, BftSignerErrorV3> {
    if payload.is_empty() || payload.len() > 64 * 1024 {
        return Err(invalid("original input encoded response frame capacity"));
    }
    let length =
        u64::try_from(payload.len()).map_err(|_| invalid("original response length conversion"))?;
    let array_header = match length {
        0..=23 => 1u64,
        24..=255 => 2,
        256..=65_535 => 3,
        65_536..=4_294_967_295 => 5,
        _ => 9,
    };
    payload
        .iter()
        .try_fold(9u64 + array_header, |bytes, value| {
            bytes
                .checked_add(if *value < 24 { 1 } else { 2 })
                .ok_or_else(|| invalid("original response encoded length overflow"))
        })
}

#[cfg(test)]
#[path = "observed_view_tests.rs"]
mod tests;
