//! Original signed retention operation history, separate from public Current.
//! Receipts/maps are immutable original records, never caller enumerations.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::crypto::digest_hex;
use super::private_retention::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionFactV1, BftPrivateRetentionLifecycleV1,
};
use super::private_retention_backend::{
    BftPrivateRetentionLedgerV1, BftPrivateRetentionSnapshotV1, validate_input_inventory,
};
use super::private_retention_seal::BftPrivateRetentionSealV1;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub type BftPrivateRetentionHistoryV1 = BTreeMap<String, Arc<BftPrivateRetentionSnapshotV1>>;

pub(crate) fn validate_original_history(
    binding: &BftSignerBindingV3,
    trusted: &BftPrivateRetentionAttestorV1,
    current: &BftPrivateRetentionLedgerV1,
    operations: &BftPrivateRetentionHistoryV1,
    seals: &BTreeMap<String, Arc<BftPrivateRetentionSealV1>>,
) -> Result<(), BftSignerErrorV3> {
    validate_original_history_with_active_context(
        binding, trusted, current, operations, seals, None,
    )
}

pub(crate) fn validate_original_history_with_active_context(
    binding: &BftSignerBindingV3,
    trusted: &BftPrivateRetentionAttestorV1,
    current: &BftPrivateRetentionLedgerV1,
    operations: &BftPrivateRetentionHistoryV1,
    seals: &BTreeMap<String, Arc<BftPrivateRetentionSealV1>>,
    active_context: Option<Arc<super::active_window::VerifiedBftActiveWindowContextV1>>,
) -> Result<(), BftSignerErrorV3> {
    let mut id = Some(digest_hex(
        "oasis7.bft.private-retention.v1.receipt",
        &current.receipt,
    )?);
    let mut visited = BTreeSet::new();
    let mut chain = Vec::new();
    while let Some(digest) = id {
        if !visited.insert(digest.clone()) {
            return Err(invalid("original retention receipt history cycle"));
        }
        let operation = operations
            .get(&digest)
            .ok_or_else(|| invalid("original retention receipt history gap"))?;
        if digest_hex(
            "oasis7.bft.private-retention.v1.receipt",
            &operation.retention.receipt,
        )? != digest
        {
            return Err(invalid("original retention receipt index identity changed"));
        }
        id = operation
            .retention
            .receipt
            .body
            .head
            .previous_receipt_digest
            .clone();
        chain.push(operation.as_ref());
    }
    if visited.len() != operations.len() {
        return Err(invalid(
            "original retention history contains unenumerated operation",
        ));
    }
    chain.reverse();
    let mut previous: Option<&BftPrivateRetentionSnapshotV1> = None;
    let mut operation_ids = BTreeSet::new();
    for operation in chain {
        let ledger = &operation.retention;
        let body = &ledger.receipt.body;
        let state = &operation.signer.state;
        if !operation_ids.insert(body.operation_digest.clone()) {
            return Err(invalid("original retention operation digest reused"));
        }
        super::private_retention_seal::verify_map_at_floor(
            trusted,
            ledger,
            ledger.map.coverage_start_height,
        )?;
        super::state::validate_state_with_active_context(
            binding,
            Some(&trusted.provider_public_key),
            state,
            active_context.clone(),
        )?;
        validate_input_inventory(ledger, &operation.inputs)?;
        if state.binding_digest != trusted.source.binding_digest
            || state.private_retention.as_ref() != Some(&body.head)
            || state.revision != body.after_revision
            || state.state_digest != body.after_state_digest
            || operation.signer.lease_generation != body.original_lease_generation
            || operation.signer.lease_token_digest.is_empty()
        {
            return Err(invalid(
                "original retention operation State/receipt differs",
            ));
        }
        let mut before = state.clone();
        before.revision = body.before_revision;
        before.state_digest = body.before_state_digest.clone();
        match previous {
            Some(old) => {
                if body.head.sequence
                    != old
                        .retention
                        .receipt
                        .body
                        .head
                        .sequence
                        .checked_add(1)
                        .ok_or_else(|| invalid("original retention sequence overflow"))?
                {
                    return Err(invalid("original retention sequence gap"));
                }
                before.private_retention = Some(old.retention.receipt.body.head.clone());
                if body.before_revision < old.retention.receipt.body.after_revision
                    || operation.signer.lease_generation < old.signer.lease_generation
                    || (body.before_revision == old.retention.receipt.body.after_revision
                        && body.before_state_digest
                            != old.retention.receipt.body.after_state_digest)
                {
                    return Err(invalid(
                        "original retention State/lease operation order regressed",
                    ));
                }
                if ledger.map.coverage_start_height == old.retention.map.coverage_start_height {
                    validate_ordinary_extension(&old.retention, ledger)?;
                } else {
                    let digest = ledger
                        .map
                        .previous_seal_receipt_digest
                        .as_ref()
                        .ok_or_else(|| invalid("original floor changed without seal"))?;
                    let seal = seals
                        .get(digest)
                        .ok_or_else(|| invalid("original floor seal missing"))?;
                    if seal.body.original != old.retention
                        || seal.body.carry_map()? != ledger.map
                        || seal.body.carry_selection_index()? != ledger.selection_index
                    {
                        return Err(invalid("original floor seal/carry differs"));
                    }
                    seal.body.validate_applied_cut(trusted, &before)?;
                }
            }
            None => {
                super::private_retention_selection::validate_extension(None, ledger)?;
                before.private_retention = None;
                if body.head.sequence != 1
                    || ledger.map.previous_seal_receipt_digest.is_some()
                    || ledger.map.coverage_start_height != trusted.coverage_start_height
                    || before != super::state::BftSignerStateV3::from_handoff(binding)?
                {
                    return Err(invalid("original retention history lacks first H coverage"));
                }
            }
        }
        before.validate_digest()?;
        let mut before_snapshot = operation.signer.clone();
        before_snapshot.state = before;
        let mut material = super::private_retention_backend::retention_material_bound(
            &before_snapshot,
            previous.map(|old| &old.retention),
            &ledger.map,
            &body.head,
            &operation.inputs,
        )?;
        if previous.is_some_and(|old| {
            old.retention.map.coverage_start_height != ledger.map.coverage_start_height
        }) {
            let id = ledger
                .map
                .previous_seal_receipt_digest
                .as_ref()
                .ok_or_else(|| invalid("original material seal missing"))?;
            let seal = seals
                .get(id)
                .ok_or_else(|| invalid("original material seal unavailable"))?;
            material = material
                .checked_add(seal.canonical_bytes()?.len() as u64)
                .ok_or_else(|| invalid("original sealed material overflow"))?;
        }
        if body.head.retained_material_bytes != Some(material) {
            return Err(invalid(
                "original retention operation material reservation differs",
            ));
        }
        previous = Some(operation);
    }
    if previous.is_none_or(|last| last.retention != *current) {
        return Err(invalid(
            "original retention history does not end at current map",
        ));
    }
    Ok(())
}
fn validate_ordinary_extension(
    old: &BftPrivateRetentionLedgerV1,
    next: &BftPrivateRetentionLedgerV1,
) -> Result<(), BftSignerErrorV3> {
    super::private_retention_selection::validate_extension(Some(old), next)?;
    if old.map.source != next.map.source
        || old.map.previous_seal_receipt_digest != next.map.previous_seal_receipt_digest
    {
        return Err(invalid("ordinary retention operation changed source/seal"));
    }
    for entry in &old.map.entries {
        let replacement = next
            .map
            .entries
            .binary_search_by(|candidate| candidate.key.cmp(&entry.key))
            .ok()
            .map(|index| &next.map.entries[index])
            .ok_or_else(|| invalid("original retention history omitted entry"))?;
        if replacement.fact != entry.fact
            || (entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready
                && replacement.lifecycle != BftPrivateRetentionLifecycleV1::Ready)
        {
            return Err(invalid(
                "original retention history changed original fact/lifecycle",
            ));
        }
    }
    for entry in &next.map.entries {
        if old
            .map
            .entries
            .binary_search_by(|old| old.key.cmp(&entry.key))
            .is_err()
            && (!matches!(entry.fact, BftPrivateRetentionFactV1::Some { .. })
                || entry.lifecycle != BftPrivateRetentionLifecycleV1::ReservedPending)
        {
            return Err(invalid(
                "original retention history added non-Some reservation",
            ));
        }
    }
    Ok(())
}
