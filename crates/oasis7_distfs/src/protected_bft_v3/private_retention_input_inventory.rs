//! Backend-only input inventory and the existing namespace material budget.
use super::*;
use crate::protected_bft_v3::private_retention::{
    BftPrivateRetentionFactV1, BftPrivateRetentionHeadV1, BftPrivateRetentionMapV1,
};
// Original H business-index retention (64MiB), four bounded original Data
// publications (4MiB each), four activation capsules (64KiB each), two bounded
// pin receipts and original role/marker. This is a permanent conservative
// reservation in the original 4GiB namespace, never an additional allowance.
const SOURCE_INITIALIZATION_RESERVE: u64 =
    64 * 1024 * 1024 + 4 * 4 * 1024 * 1024 + 4 * 64 * 1024 + 2 * 4 * 1024 * 1024 + 1024 * 1024;
const ORIGINAL_RECEIPT_RESERVE: u64 = 16 * 1024;

pub(crate) fn validate_inventory(
    ledger: &BftPrivateRetentionLedgerV1,
    inputs: &BftPrivateRetentionInputInventoryV1,
) -> Result<(), BftSignerErrorV3> {
    crate::protected_bft_v3::private_retention_selection::validate_ledger(ledger)?;
    let mut matched = 0usize;
    for entry in &ledger.map.entries {
        if let BftPrivateRetentionFactV1::Some {
            admission_digest, ..
        } = &entry.fact
        {
            let input = inputs
                .get(admission_digest)
                .ok_or_else(|| invalid("retention original admission input missing"))?;
            if input.source != ledger.map.source
                || input.key != entry.key
                || input.fact()? != entry.fact
            {
                return Err(invalid("retention original input source/key/fact mismatch"));
            }
            matched = matched
                .checked_add(1)
                .ok_or_else(|| invalid("retention input count overflow"))?;
        }
    }
    if matched != inputs.len() {
        return Err(invalid(
            "retention input inventory has unenumerated originals",
        ));
    }
    Ok(())
}

pub(crate) fn retention_material_bound(
    before: &BftSignerBackendSnapshotV3,
    previous: Option<&BftPrivateRetentionLedgerV1>,
    map: &BftPrivateRetentionMapV1,
    head: &BftPrivateRetentionHeadV1,
    inputs: &BftPrivateRetentionInputInventoryV1,
) -> Result<u64, BftSignerErrorV3> {
    let mut new_input_bytes = 0u64;
    for (digest, input) in inputs {
        let already = previous.is_some_and(|ledger| ledger.map.entries.iter().any(|entry| matches!(&entry.fact, BftPrivateRetentionFactV1::Some { admission_digest, .. } if admission_digest == digest)));
        if !already {
            let bytes = input
                .descriptor()?
                .object
                .bytes
                .checked_add(input.publication_reservation_bytes)
                .ok_or_else(|| invalid("retention input publication reservation overflow"))?;
            new_input_bytes = new_input_bytes
                .checked_add(bytes)
                .ok_or_else(|| invalid("retention input material overflow"))?;
        }
    }
    retention_material_bound_from_bytes(&before.state, previous, map, head, new_input_bytes)
}

/// Arithmetic only; a caller must independently establish original inventory
/// and exact input publication pricing before using this value as evidence.
pub(crate) fn retention_material_bound_from_bytes(
    before: &crate::protected_bft_v3::state::BftSignerStateV3,
    previous: Option<&BftPrivateRetentionLedgerV1>,
    map: &BftPrivateRetentionMapV1,
    head: &BftPrivateRetentionHeadV1,
    new_input_bytes: u64,
) -> Result<u64, BftSignerErrorV3> {
    let old = before
        .private_retention
        .as_ref()
        .and_then(|head| head.retained_material_bytes);
    if old.is_none() && previous.is_some_and(|ledger| !ledger.map.entries.is_empty()) {
        return Err(invalid(
            "unaccounted legacy retention map cannot migrate by inference",
        ));
    }
    let material = old
        .unwrap_or(SOURCE_INITIALIZATION_RESERVE)
        .checked_add(new_input_bytes)
        .ok_or_else(|| invalid("retention input material overflow"))?;
    // The unsigned usage field is maximally wide when measuring the immutable
    // readback snapshot. Two complete State copies cover original readback and
    // the current write image; overwriting a write image never refunds this
    // conservative reservation. Receipt uses its existing bounded full encoding.
    let mut placeholder = before.clone();
    let mut maximum = head.clone();
    maximum.retained_material_bytes = Some(u64::MAX);
    placeholder.private_retention = Some(maximum);
    placeholder.revision = before
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("retention revision overflow"))?;
    let metadata = (placeholder.encoded_len()? as u64)
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(map.canonical_bytes().ok()?.len() as u64))
        .and_then(|bytes| {
            bytes.checked_add(
                head.selection_commitment
                    .as_ref()
                    .map_or(0, |s| s.encoded_bytes),
            )
        })
        .and_then(|bytes| bytes.checked_add(ORIGINAL_RECEIPT_RESERVE))
        .ok_or_else(|| invalid("retention complete metadata material bound"))?;
    material
        .checked_add(metadata)
        .ok_or_else(|| invalid("retention cumulative material overflow"))
}
