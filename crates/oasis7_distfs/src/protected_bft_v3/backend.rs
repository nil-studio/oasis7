pub const BFT_SIGNER_BACKEND_NAMESPACE_V3: &str = "oasis7.bft.protected-signer.v3.backend";
use super::binding::BftSignerErrorV3;
use super::state::BftSignerStateV3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftSignerBackendSnapshotV3 {
    pub state: BftSignerStateV3,
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
pub enum BftSignerAcquireOutcomeV3 {
    Acquired(BftSignerBackendSnapshotV3),
    Conflict(BftSignerBackendSnapshotV3),
}

/// Atomic backend contract for the V3 provider. Implementations must persist
/// the complete state and lease generation together, reject unprovisioned
/// bindings, and refuse new owners if they cannot prove the prior owner was
/// fenced. This interface alone does not establish hardware properties.
pub trait BftSignerBackendV3: Send + Sync {
    /// Opens previously provisioned V3 state and never initializes or resets.
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>;

    /// Fences the old owner and installs a new lease only if the expected
    /// durable generation still matches.
    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3>;

    /// Releases precisely the live lease and advances the durable generation.
    /// A delayed release from an older lease must leave a newer owner intact.
    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>;

    /// Returns a full snapshot after exact active lease verification.
    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>;

    /// Atomically checks the active generation/token, samples provider-owned
    /// monotonic time and advances its independently durable floor. It does
    /// not load or rewrite the journal and cannot change its revision/digest.
    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV3>;

    /// Persists one fully validated state transition under the exact lease,
    /// revision, digest and protected-time floor.
    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3>;
}
