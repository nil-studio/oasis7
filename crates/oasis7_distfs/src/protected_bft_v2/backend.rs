use super::binding::BftSignerErrorV2;
use super::state::BftSignerStateV2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftSignerBackendSnapshotV2 {
    pub state: BftSignerStateV2,
    pub lease_generation: u64,
    pub lease_token_digest: String,
    pub protected_time_floor_ms: i64,
}

/// A proved acquire conflict never installs the attempted lease token. It
/// returns either a newer generation or the expected generation with an
/// existing active owner. Only a newer generation without an active owner
/// permits a bounded retry; an active owner must not be fenced or poisoned.
/// Generic errors remain ambiguous and the service poisons itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BftSignerAcquireOutcomeV2 {
    Acquired(BftSignerBackendSnapshotV2),
    Conflict(BftSignerBackendSnapshotV2),
}

/// Atomic backend contract for the V2 provider. Implementations must persist
/// the complete state and lease generation together, reject unprovisioned
/// bindings, and refuse new owners if they cannot prove the prior owner was
/// fenced. This interface alone does not establish hardware properties.
pub trait BftSignerBackendV2: Send + Sync {
    /// Opens previously provisioned V2 state and never initializes or resets.
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2>;

    /// Fences the old owner and installs a new lease only if the expected
    /// durable generation still matches.
    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV2, BftSignerErrorV2>;

    /// Releases precisely the live lease and advances the durable generation.
    /// A delayed release from an older lease must leave a newer owner intact.
    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2>;

    /// Returns a full snapshot after exact active lease verification.
    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2>;

    /// Atomically checks the active generation/token, samples provider-owned
    /// monotonic time and advances its independently durable floor. It does
    /// not load or rewrite the journal and cannot change its revision/digest.
    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV2>;

    /// Persists one fully validated state transition under the exact lease,
    /// revision, digest and protected-time floor.
    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2>;

    /// Atomically installs an acknowledged validator transition, persists its
    /// receipt, advances the signer generation and rotates the same live
    /// connection to `new_lease_token_digest`.
    // Keep the atomic CAS inputs explicit at this backend boundary.
    #[allow(clippy::too_many_arguments)]
    fn complete_transition_install(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: i64,
        next_lease_token_digest: &str,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2>;
}
