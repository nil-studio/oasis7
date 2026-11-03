pub const BFT_SIGNER_BACKEND_NAMESPACE_V3: &str = "oasis7.bft.protected-signer.v3.backend";
use super::binding::{BftSignerErrorV3, invalid};
use super::state::BftSignerStateV3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftSignerBackendSnapshotV3 {
    pub state: BftSignerStateV3,
    /// Explicitly provisioned protected capability; None never initializes on read.
    pub successor_archive_transfer:
        Option<super::archive_transfer::BftSuccessorArchiveTransferLedgerV1>,
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
/// fenced. An existing archive-transfer capability and its nondecreasing
/// counter must survive load, acquire, release, read, State CAS, restart and
/// migration of this binding. Only proven legacy None-to-Some provisioning may
/// initialize the counter; an existing Some ledger must never be reset or removed.
/// This interface alone does not establish hardware properties.
pub trait BftSignerBackendV3: Send + Sync {
    fn read_private_retention_creation(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
    ) -> Result<
        super::private_retention_creation::ValidatedBftPrivateRetentionCreationV1,
        BftSignerErrorV3,
    > {
        Err(super::binding::invalid(
            "private retention original source creation unsupported",
        ))
    }
    fn load_private_retention_current(
        &mut self,
        _binding_digest: &str,
    ) -> Result<super::private_retention_backend::BftPrivateRetentionCurrentV1, BftSignerErrorV3>
    {
        Err(super::binding::invalid(
            "private retention current snapshot unsupported",
        ))
    }

    /// Load-only, same-backend atomic original sealed namespace. This does not
    /// acquire a lease, initialize material, or grant network read authority.
    fn load_private_retention_sealed_current(
        &mut self,
        _binding_digest: &str,
    ) -> Result<
        super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1,
        BftSignerErrorV3,
    > {
        Err(invalid("complete original retention load unsupported"))
    }

    fn read_private_retention_current(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
    ) -> Result<super::private_retention_backend::BftPrivateRetentionCurrentV1, BftSignerErrorV3>
    {
        Err(super::binding::invalid(
            "private retention current snapshot unsupported",
        ))
    }

    /// Local backend source only. Network release still requires the separate
    /// encrypted original-input read purpose and a fresh independent grant.
    /// Complete immutable original namespace, sampled atomically with its live
    /// State/lease. Public Current replies must not serialize this inventory.
    fn read_private_retention_sealed_current(
        &mut self,
        _generation: u64,
        _token: &str,
    ) -> Result<
        super::private_retention_sealed_current::BftPrivateRetentionSealedCurrentV1,
        BftSignerErrorV3,
    > {
        Err(invalid("complete original retention history unsupported"))
    }
    fn compare_and_swap_private_retention_seal(
        &mut self,
        _request: &super::private_retention_seal::BftPrivateRetentionSealCasV1,
    ) -> Result<super::private_retention_backend::BftPrivateRetentionSnapshotV1, BftSignerErrorV3>
    {
        Err(invalid("atomic original retention seal unsupported"))
    }

    fn load_private_retention_input(
        &mut self,
        _binding: &str,
        _reference: &super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<
        std::sync::Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>,
        BftSignerErrorV3,
    > {
        Err(super::binding::invalid(
            "private retention atomic input source unsupported",
        ))
    }

    /// Dedicated retention CAS. Must atomically compare the entire expected
    /// State/lease/sideledger, persist next State + full map + original signed
    /// receipt, and return those same three parts from one atomic snapshot.
    /// Ordinary CAS/acquire/release must preserve an existing retention ledger.
    /// This default never provisions a missing ledger or opens signing rights.
    fn compare_and_swap_private_retention(
        &mut self,
        _request: &super::private_retention_backend::BftPrivateRetentionCasV1,
    ) -> Result<super::private_retention_backend::BftPrivateRetentionSnapshotV1, BftSignerErrorV3>
    {
        Err(super::binding::invalid(
            "private retention atomic ledger unsupported",
        ))
    }

    /// Retrieves the exact original operation after an ambiguous result. The
    /// current fenced owner must be verified atomically. No new receipt may be
    /// signed here; unknown/missing operations remain errors, never empty maps.
    fn read_private_retention_operation(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
        _operation_digest: &str,
    ) -> Result<super::private_retention_backend::BftPrivateRetentionSnapshotV1, BftSignerErrorV3>
    {
        Err(super::binding::invalid(
            "private retention operation readback unsupported",
        ))
    }

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

    /// Atomically charges a complete encoded archive response before payload release.
    /// Must preserve State, generation/token and protected time exactly. Legacy
    /// backends are unsupported; provisioning/migration is an external operation.
    fn reserve_successor_archive_transfer(
        &mut self,
        _lease_generation: u64,
        _lease_token_digest: &str,
        _expected_revision: u64,
        _expected_state_digest: &str,
        _expected_reserved_response_bytes: u64,
        _reserved_response_bytes: u64,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        Err(super::binding::invalid(
            "successor archive transfer ledger unsupported",
        ))
    }

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
