//! Atomic archive-transfer reservation and monotonic service observation.
use super::*;
use crate::protected_bft_v3::archive_transfer;

impl BftSignerServiceRuntimeV3 {
    pub(in crate::protected_bft_v3) fn observe_archive_transfer(
        &mut self,
        snapshot: &BftSignerBackendSnapshotV3,
    ) -> Result<(), BftSignerErrorV3> {
        if let Err(error) = archive_transfer::validate_successor_archive_transfer_observation_v1(
            self.observed_archive_transfer,
            snapshot,
        ) {
            self.poisoned = true;
            return Err(error);
        }
        self.observed_archive_transfer = snapshot.successor_archive_transfer;
        Ok(())
    }
}
impl BftSignerServiceV3 {
    /// Query modules call after authenticating the lease and validating the
    /// exact original object, before returning any archive payload. Never refund.
    pub(in crate::protected_bft_v3) fn reserve_archive_transfer(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        before: &BftSignerBackendSnapshotV3,
        reserved_response_bytes: u64,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        if runtime.poisoned {
            return Err(invalid("BFT V3 provider is poisoned"));
        }
        if runtime
            .verified
            .as_ref()
            .is_none_or(|v| v.state != before.state)
        {
            runtime.poisoned = true;
            return Err(invalid(
                "successor archive reserve differs from verified current State",
            ));
        }
        runtime.observe_archive_transfer(before)?;
        let counter = before
            .successor_archive_transfer
            .ok_or_else(|| invalid("successor archive transfer ledger unsupported"))?
            .reserved_response_bytes;
        // Reject unsupported/capacity/shape before invoking a potentially ambiguous backend mutation.
        archive_transfer::reserve_successor_archive_transfer_v1(
            before,
            before.lease_generation,
            &before.lease_token_digest,
            before.state.revision,
            &before.state.state_digest,
            counter,
            reserved_response_bytes,
        )?;
        let after = match runtime.backend.reserve_successor_archive_transfer(
            before.lease_generation,
            &before.lease_token_digest,
            before.state.revision,
            &before.state.state_digest,
            counter,
            reserved_response_bytes,
        ) {
            Ok(value) => value,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "successor archive transfer outcome unknown; provider poisoned: {error}"
                )));
            }
        };
        if let Err(error) = archive_transfer::validate_successor_archive_transfer_reservation_v1(
            before,
            &after,
            reserved_response_bytes,
        ) {
            runtime.poisoned = true;
            return Err(error);
        }
        runtime.observe_archive_transfer(&after)?;
        Ok(after)
    }
}
