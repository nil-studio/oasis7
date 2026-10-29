//! Separate protected accounting: never changes frozen signing State or evidence.
use super::backend::BftSignerBackendSnapshotV3;
use super::binding::{BftSignerErrorV3, invalid};
use super::state::BftSignerStateV3;
use super::successor_window::BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1;

pub const BFT_SUCCESSOR_ARCHIVE_TRANSFER_MAX_RESERVATION_BYTES_V1: u64 =
    super::wire::BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3 as u64 + 4;

/// This DTO is backend metadata, deliberately outside public State/checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BftSuccessorArchiveTransferLedgerV1 {
    pub schema_version: u32,
    pub reserved_response_bytes: u64,
}
impl BftSuccessorArchiveTransferLedgerV1 {
    /// Only explicit backend provisioning/migration may install this value.
    pub fn provisioned() -> Self {
        Self {
            schema_version: 1,
            reserved_response_bytes: 0,
        }
    }
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        if self.schema_version != 1
            || self.reserved_response_bytes > BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1
        {
            return Err(invalid(
                "successor archive transfer ledger version/capacity",
            ));
        }
        Ok(())
    }
}
/// Separate material and transfer totals share one namespace-wide capacity.
pub fn validate_successor_archive_budget(
    state: &BftSignerStateV3,
    ledger: Option<&BftSuccessorArchiveTransferLedgerV1>,
) -> Result<(), BftSignerErrorV3> {
    let material = state
        .successor_window
        .as_ref()
        .map_or(0, |w| w.usage.total_bytes);
    let transfer = match ledger {
        Some(value) => {
            value.validate()?;
            value.reserved_response_bytes
        }
        None => 0,
    };
    if material
        .checked_add(transfer)
        .is_none_or(|total| total > BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1)
    {
        return Err(invalid(
            "successor material plus reserved transfer capacity",
        ));
    }
    Ok(())
}
/// Backend implementations call under the same atomic lock/transaction as
/// lease and exact State checks, then persist only the returned ledger.
pub fn reserve_successor_archive_transfer_v1(
    snapshot: &BftSignerBackendSnapshotV3,
    lease_generation: u64,
    lease_token_digest: &str,
    expected_revision: u64,
    expected_state_digest: &str,
    expected_reserved_response_bytes: u64,
    reserved_response_bytes: u64,
) -> Result<BftSuccessorArchiveTransferLedgerV1, BftSignerErrorV3> {
    let ledger = snapshot
        .successor_archive_transfer
        .ok_or_else(|| invalid("successor archive transfer ledger unsupported"))?;
    validate_successor_archive_budget(&snapshot.state, Some(&ledger))?;
    if reserved_response_bytes == 0
        || reserved_response_bytes > BFT_SUCCESSOR_ARCHIVE_TRANSFER_MAX_RESERVATION_BYTES_V1
    {
        return Err(invalid("successor archive transfer reservation bounds"));
    }
    if lease_token_digest.is_empty()
        || snapshot.lease_generation != lease_generation
        || snapshot.lease_token_digest != lease_token_digest
        || snapshot.state.revision != expected_revision
        || snapshot.state.state_digest != expected_state_digest
        || ledger.reserved_response_bytes != expected_reserved_response_bytes
    {
        return Err(invalid(
            "successor archive transfer exact lease/state/counter conflict",
        ));
    }
    let next = BftSuccessorArchiveTransferLedgerV1 {
        schema_version: 1,
        reserved_response_bytes: ledger
            .reserved_response_bytes
            .checked_add(reserved_response_bytes)
            .ok_or_else(|| invalid("successor archive transfer counter overflow"))?,
    };
    validate_successor_archive_budget(&snapshot.state, Some(&next))?;
    Ok(next)
}
/// A live service rejects capability changes and counter rollback. For the
/// same binding, restart or custody migration must preserve an existing Some
/// capability and its nondecreasing spent bytes. Only proven legacy None-to-Some
/// provisioning may initialize a ledger; it is never a request-side reset.
pub fn validate_successor_archive_transfer_observation_v1(
    previous: Option<BftSuccessorArchiveTransferLedgerV1>,
    snapshot: &BftSignerBackendSnapshotV3,
) -> Result<(), BftSignerErrorV3> {
    validate_successor_archive_budget(
        &snapshot.state,
        snapshot.successor_archive_transfer.as_ref(),
    )?;
    match (previous, snapshot.successor_archive_transfer) {
        (None, None) => Ok(()),
        (Some(a), Some(b))
            if a.schema_version == b.schema_version
                && b.reserved_response_bytes >= a.reserved_response_bytes =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "successor archive transfer capability/counter regressed or changed live",
        )),
    }
}
pub fn validate_successor_archive_transfer_reservation_v1(
    before: &BftSignerBackendSnapshotV3,
    after: &BftSignerBackendSnapshotV3,
    reserved_response_bytes: u64,
) -> Result<(), BftSignerErrorV3> {
    let old = before
        .successor_archive_transfer
        .ok_or_else(|| invalid("successor archive transfer ledger unsupported"))?;
    let expected = reserve_successor_archive_transfer_v1(
        before,
        before.lease_generation,
        &before.lease_token_digest,
        before.state.revision,
        &before.state.state_digest,
        old.reserved_response_bytes,
        reserved_response_bytes,
    )?;
    let mut exact = before.clone();
    exact.successor_archive_transfer = Some(expected);
    if after != &exact {
        return Err(invalid(
            "successor archive reserve changed protected State/lease/floor or counter",
        ));
    }
    Ok(())
}
