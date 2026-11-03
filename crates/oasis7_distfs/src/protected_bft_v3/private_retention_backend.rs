//! Atomic retention backend envelopes; generic State CAS must not emulate this operation.
use super::backend::BftSignerBackendSnapshotV3;
use super::binding::{BftSignerErrorV3, invalid};
use super::private_retention_input::BftPrivateRetentionAdmissionInputV1;
pub type BftPrivateRetentionInputInventoryV1 =
    std::collections::BTreeMap<String, std::sync::Arc<BftPrivateRetentionAdmissionInputV1>>;
#[path = "private_retention_input_inventory.rs"]
mod input_inventory;
use super::private_retention::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionMapV1, BftPrivateRetentionReceiptV1,
};
pub(crate) use input_inventory::validate_inventory as validate_input_inventory;
pub(crate) use input_inventory::{retention_material_bound, retention_material_bound_from_bytes};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionLedgerV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_index:
        Option<super::private_retention_selection::BftPrivateRetentionSelectionIndexV1>,
    pub map: BftPrivateRetentionMapV1,
    /// Exactly the receipt persisted by the original atomic operation.
    pub receipt: BftPrivateRetentionReceiptV1,
}

/// All three parts are sampled under one backend atomic read. A separately
/// loaded map or a newly signed response is not an original operation readback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftPrivateRetentionSnapshotV1 {
    pub signer: BftSignerBackendSnapshotV3,
    pub retention: BftPrivateRetentionLedgerV1,
    /// Backend-only confidential originals; never serialized into public replies.
    pub inputs: BftPrivateRetentionInputInventoryV1,
}

/// Current lease/State observed atomically with its retained historical receipt.
/// The receipt's original lease and post-State must never replace `signer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftPrivateRetentionCurrentV1 {
    pub signer: BftSignerBackendSnapshotV3,
    pub retention: Option<BftPrivateRetentionLedgerV1>,
    pub inputs: BftPrivateRetentionInputInventoryV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftPrivateRetentionCasV1 {
    pub before: BftSignerBackendSnapshotV3,
    pub expected_retention: Option<BftPrivateRetentionLedgerV1>,
    /// Includes next State, full map and the already constructed original receipt.
    /// Never expose these receipt bytes until exact atomic readback succeeds.
    pub after: BftPrivateRetentionSnapshotV1,
}

impl BftPrivateRetentionCasV1 {
    /// Envelope checks only. Admission, lifecycle/seal transition and inclusion
    /// of the unsigned head in State must also pass before invoking the backend.
    pub fn validate_envelope(
        &self,
        trusted: &BftPrivateRetentionAttestorV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate_envelope_with_anchor(trusted, None)
    }

    /// Explicit authenticated latest anchor permits the new floor only for
    /// this original source. Ordinary callers retain fixed-floor verification.
    pub(crate) fn validate_envelope_with_anchor(
        &self,
        trusted: &BftPrivateRetentionAttestorV1,
        latest: Option<&super::private_retention_current_anchor::BftPrivateRetentionSealAnchorV1>,
    ) -> Result<(), BftSignerErrorV3> {
        input_inventory::validate_inventory(&self.after.retention, &self.after.inputs)?;
        super::private_retention_selection::validate_extension(
            self.expected_retention.as_ref(),
            &self.after.retention,
        )?;
        let before = &self.before;
        let after = &self.after.signer;
        let receipt = &self.after.retention.receipt;
        super::private_retention_current_anchor::verify_current_anchor(
            trusted,
            &self.after.retention,
            latest,
        )?;
        let mut unchanged = after.state.clone();
        unchanged.private_retention = before.state.private_retention.clone();
        unchanged.revision = before.state.revision;
        unchanged.state_digest = before.state.state_digest.clone();
        if before.lease_generation != after.lease_generation
            || before.lease_token_digest != after.lease_token_digest
            || before.lease_token_digest.is_empty()
            || before.protected_time_floor_ms != after.protected_time_floor_ms
            || before.successor_archive_transfer != after.successor_archive_transfer
            || before.state.binding_digest != after.state.binding_digest
            || before.state.binding_digest != trusted.source.binding_digest
            || receipt.body.original_lease_generation != before.lease_generation
            || receipt.body.before_revision != before.state.revision
            || receipt.body.before_state_digest != before.state.state_digest
            || receipt.body.after_revision != after.state.revision
            || receipt.body.after_state_digest != after.state.state_digest
            || after.state.private_retention.as_ref() != Some(&receipt.body.head)
            || unchanged != before.state
            || before
                .state
                .with_private_retention_head(receipt.body.head.clone())?
                != after.state
        {
            return Err(invalid("private retention atomic envelope mismatch"));
        }
        if let Some(previous) = &self.expected_retention {
            super::private_retention_current_anchor::verify_current_anchor(
                trusted, previous, latest,
            )?;
            if before.state.private_retention.as_ref() != Some(&previous.receipt.body.head) {
                return Err(invalid("private retention expected State/head mismatch"));
            }
            validate_map_extension(&previous.map, &self.after.retention.map)?;
            let previous_digest = super::crypto::digest_hex(
                "oasis7.bft.private-retention.v1.receipt",
                &previous.receipt,
            )?;
            if receipt.body.head.sequence
                != previous
                    .receipt
                    .body
                    .head
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| invalid("private retention sequence overflow"))?
                || receipt.body.head.previous_receipt_digest.as_ref() != Some(&previous_digest)
            {
                return Err(invalid("private retention predecessor mismatch"));
            }
        } else if before.state.private_retention.is_some()
            || !before.state.history.is_empty()
            || !before.state.completed_journals.is_empty()
            || before.state.pending_driver_apply.is_some()
            || !before.state.applied_installs.is_empty()
            || before
                .state
                .journal
                .as_ref()
                .is_some_and(|journal| !journal.events.is_empty())
            || receipt.body.head.sequence != 1
            || receipt.body.head.previous_receipt_digest.is_some()
            || self
                .after
                .retention
                .map
                .previous_seal_receipt_digest
                .is_some()
        {
            return Err(invalid("private retention first head mismatch"));
        } else {
            let mut empty = self.after.retention.map.clone();
            empty.entries.clear();
            validate_map_extension(&empty, &self.after.retention.map)?;
        }
        super::archive_transfer::validate_successor_archive_budget(
            &after.state,
            after.successor_archive_transfer.as_ref(),
        )?;
        let expected_material = retention_material_bound(
            &self.before,
            self.expected_retention.as_ref(),
            &self.after.retention.map,
            &receipt.body.head,
            &self.after.inputs,
        )?;
        if !matches!(receipt.body.head.schema_version, 2 | 3)
            || receipt.body.head.retained_material_bytes != Some(expected_material)
        {
            return Err(invalid(
                "private retention exact cumulative material bound differs",
            ));
        }
        Ok(())
    }

    /// Called on the snapshot returned by CAS or an original-operation read.
    /// A newer head is not equivalent success. Failure must poison the owner;
    /// retry may only retrieve this same operation, never sign a second receipt.
    pub fn validate_exact_readback(
        &self,
        actual: &BftPrivateRetentionSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        if actual != &self.after
            || actual.retention.map.canonical_bytes()?
                != self.after.retention.map.canonical_bytes()?
            || super::crypto::encode_bounded(&actual.retention.receipt, 16 * 1024)?
                != super::crypto::encode_bounded(&self.after.retention.receipt, 16 * 1024)?
            || super::crypto::encode_bounded(
                &actual.signer.state,
                super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
            )? != super::crypto::encode_bounded(
                &self.after.signer.state,
                super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
            )?
        {
            return Err(invalid("private retention atomic readback differs"));
        }
        Ok(())
    }
}

fn validate_map_extension(
    old: &BftPrivateRetentionMapV1,
    next: &BftPrivateRetentionMapV1,
) -> Result<(), BftSignerErrorV3> {
    use super::private_retention::{BftPrivateRetentionFactV1, BftPrivateRetentionLifecycleV1};
    if old.source != next.source
        || old.coverage_start_height != next.coverage_start_height
        || old.previous_seal_receipt_digest != next.previous_seal_receipt_digest
    {
        return Err(invalid(
            "private retention extension changed source/coverage/seal",
        ));
    }
    for entry in &old.entries {
        let replacement = next
            .entries
            .binary_search_by(|new| new.key.cmp(&entry.key))
            .ok()
            .map(|index| &next.entries[index])
            .ok_or_else(|| invalid("private retention extension removed original entry"))?;
        if entry.fact != replacement.fact
            || (entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready
                && replacement.lifecycle != BftPrivateRetentionLifecycleV1::Ready)
        {
            return Err(invalid(
                "private retention extension changed original fact/lifecycle",
            ));
        }
    }
    for entry in &next.entries {
        if old
            .entries
            .binary_search_by(|old| old.key.cmp(&entry.key))
            .is_err()
            && (!matches!(entry.fact, BftPrivateRetentionFactV1::Some { .. })
                || entry.lifecycle != BftPrivateRetentionLifecycleV1::ReservedPending)
        {
            return Err(invalid(
                "fresh retention entry must be original Some reservation",
            ));
        }
    }
    Ok(())
}
