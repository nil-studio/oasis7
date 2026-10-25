use oasis7_proto::distributed_bft::BftCommitCertificateV1;

use super::binding::BftSignerError;
use super::validation::BftSignerStateV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftSignerBackendSnapshotV1 {
    pub state: BftSignerStateV1,
    pub lease_generation: u64,
    pub lease_token_digest: String,
    pub protected_time_floor_ms: i64,
}

/// Atomic storage and monotonic-clock contract required by the provider.
/// Implementations must reject unknown/unprovisioned bindings, persist lease
/// generations without rollback, fence older generations before granting a
/// new one, and fail closed when they cannot prove that fencing.
pub trait BftSignerBackendV1: Send + Sync {
    /// Opens previously provisioned state. This must never initialize/reset it.
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError>;

    /// Atomically fences any former lease, persists the new generation/token
    /// digest, and returns the same existing journal. It succeeds only when
    /// the current generation equals `expected_generation`; a partitioned or
    /// uncertain backend must refuse instead of timing out the old owner.
    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError>;

    /// Releases the exact live lease and advances the durable generation.
    /// If a newer generation already fenced this lease, implementations must
    /// return that newer snapshot unchanged. Any ambiguous result must be an
    /// error; the service will poison itself and stop signing.
    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError>;

    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError>;

    /// Returns a fresh provider-owned time sample after durably advancing its
    /// separate monotonic time floor. This operation does not change the BFT
    /// journal revision or digest.
    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerError>;

    /// Atomically checks the active lease generation, expected revision and
    /// digest, protected clock floor, and stores the complete next state. The
    /// backend samples its clock again and refuses if it is below its durable
    /// clock floor or below `minimum_event_time_ms`.
    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV1,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError>;
}

/// Provisioning is deliberately separate from `acquire_lease`. Operators or
/// hardware setup code must create a genesis state and register the exact
/// binding before the service can open. This helper only derives the expected
/// genesis state; it does not persist or install a key.
pub fn bft_signer_genesis_state_v1(
    binding: &super::binding::BftSignerBindingV1,
) -> Result<BftSignerStateV1, BftSignerError> {
    BftSignerStateV1::genesis(binding)
}

/// A backend stores only raw committed QC history, not serialized verified
/// capabilities. Every open/recovery replays that history from fixed genesis.
pub fn bft_signer_history_bound_v1(
    history: &[BftCommitCertificateV1],
) -> Result<(), BftSignerError> {
    super::validation::validate_history(history)
}
