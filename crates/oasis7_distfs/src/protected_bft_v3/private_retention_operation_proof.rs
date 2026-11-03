//! Public original-operation replay metadata. Confidential input payloads are
//! deliberately absent and must use the independent OriginalAdmissionInput path.
use super::binding::BftSignerErrorV3;
use super::crypto::{BFT_SIGNER_MAX_FRAME_BYTES_V3, encode_bounded};
use super::private_retention_backend::BftPrivateRetentionLedgerV1;
use super::state::BftSignerStateV3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionOperationProofV1 {
    /// Exact original post-operation State, never a new current lease proof.
    pub state: BftSignerStateV3,
    /// Exact original map and receipt persisted with that State.
    pub ledger: BftPrivateRetentionLedgerV1,
}
impl BftPrivateRetentionOperationProofV1 {
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        super::crypto::decode_bounded(bytes, BFT_SIGNER_MAX_FRAME_BYTES_V3)
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, BftSignerErrorV3> {
        encode_bounded(self, BFT_SIGNER_MAX_FRAME_BYTES_V3)
    }
}

/// Checked metadata arithmetic, not an input authority or migration grant.
/// The independent Runtime supplies prices only after authorized actual replay.
pub struct BftPrivateRetentionOperationAccountingV1 {
    base: u64,
    expected: u64,
    added: Vec<super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1>,
}
impl BftPrivateRetentionOperationAccountingV1 {
    pub fn new_inputs(
        &self,
    ) -> &[super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1] {
        &self.added
    }
    pub fn verify_actual_prices(
        &self,
        prices: &std::collections::BTreeMap<String, u64>,
    ) -> Result<(), BftSignerErrorV3> {
        let mut actual = self.base;
        for reference in &self.added {
            let publication = prices.get(&reference.admission_digest).ok_or_else(|| {
                super::binding::invalid("archived original actual publication price missing")
            })?;
            actual = actual
                .checked_add(reference.object.bytes)
                .and_then(|n| n.checked_add(*publication))
                .ok_or_else(|| super::binding::invalid("archived original material overflow"))?;
        }
        if actual != self.expected {
            return Err(super::binding::invalid(
                "archived original exact material reservation differs",
            ));
        }
        Ok(())
    }
}
impl BftPrivateRetentionOperationProofV1 {
    /// Verify one contiguous original operation. The previous original State is
    /// not shortened into a synthetic prefix; only its signed pure-head update
    /// is reconstructed and checked against the original before-State digest.
    pub fn verify_original_step(
        &self,
        binding: &super::binding::BftSignerBindingV3,
        trusted: &super::private_retention::BftPrivateRetentionAttestorV1,
        previous: Option<&Self>,
        seal: Option<&super::private_retention_seal::BftPrivateRetentionSealV1>,
    ) -> Result<BftPrivateRetentionOperationAccountingV1, BftSignerErrorV3> {
        use super::binding::invalid;
        use super::private_retention::{BftPrivateRetentionFactV1, BftPrivateRetentionLifecycleV1};
        let ledger = &self.ledger;
        let body = &ledger.receipt.body;
        self.state.validate(binding, &trusted.provider_public_key)?;
        if self.state.private_retention.as_ref() != Some(&body.head)
            || self.state.revision != body.after_revision
            || self.state.state_digest != body.after_state_digest
            || body.original_lease_generation == 0
        {
            return Err(invalid("archived original operation State/head differs"));
        }
        let mut before = self.state.clone();
        before.revision = body.before_revision;
        before.state_digest = body.before_state_digest.clone();
        before.private_retention = previous.map(|old| old.ledger.receipt.body.head.clone());
        before.validate_digest()?;
        let mut added = Vec::new();
        let floor = match previous {
            None => {
                super::private_retention_selection::validate_extension(None, ledger)?;
                if body.head.sequence != 1
                    || body.head.previous_receipt_digest.is_some()
                    || ledger.map.previous_seal_receipt_digest.is_some()
                    || seal.is_some()
                    || before != BftSignerStateV3::from_handoff(binding)?
                {
                    return Err(invalid(
                        "archived original history lacks exact H first operation",
                    ));
                }
                trusted.coverage_start_height
            }
            Some(old) => {
                let digest = super::crypto::digest_hex(
                    "oasis7.bft.private-retention.v1.receipt",
                    &old.ledger.receipt,
                )?;
                if body.head.sequence
                    != old
                        .ledger
                        .receipt
                        .body
                        .head
                        .sequence
                        .checked_add(1)
                        .ok_or_else(|| invalid("archived original sequence overflow"))?
                    || body.head.previous_receipt_digest.as_ref() != Some(&digest)
                    || body.before_revision < old.state.revision
                    || body.original_lease_generation
                        < old.ledger.receipt.body.original_lease_generation
                    || (body.before_revision == old.state.revision
                        && body.before_state_digest != old.state.state_digest)
                {
                    return Err(invalid(
                        "archived original receipt/State/lease sequence gap",
                    ));
                }
                if let Some(seal) = seal {
                    if seal.body.original != old.ledger
                        || seal.body.carry_map()? != ledger.map
                        || seal.body.carry_selection_index()? != ledger.selection_index
                    {
                        return Err(invalid(
                            "archived original seal replaced complete old map/carry",
                        ));
                    }
                    seal.anchor()?.verify(trusted)?;
                    seal.body.validate_applied_cut(trusted, &before)?;
                    seal.body
                        .cut
                        .height
                        .checked_add(1)
                        .ok_or_else(|| invalid("archived original floor overflow"))?
                } else {
                    super::private_retention_selection::validate_extension(
                        Some(&old.ledger),
                        ledger,
                    )?;
                    if ledger.map.source != old.ledger.map.source
                        || ledger.map.coverage_start_height != old.ledger.map.coverage_start_height
                        || ledger.map.previous_seal_receipt_digest
                            != old.ledger.map.previous_seal_receipt_digest
                    {
                        return Err(invalid(
                            "archived ordinary operation changed source/floor/seal",
                        ));
                    }
                    for entry in &old.ledger.map.entries {
                        let replacement = ledger
                            .map
                            .entries
                            .binary_search_by(|next| next.key.cmp(&entry.key))
                            .ok()
                            .map(|i| &ledger.map.entries[i])
                            .ok_or_else(|| invalid("archived original operation omitted fact"))?;
                        if replacement.fact != entry.fact
                            || (entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready
                                && replacement.lifecycle != BftPrivateRetentionLifecycleV1::Ready)
                        {
                            return Err(invalid("archived original fact/Ready lifecycle changed"));
                        }
                    }
                    old.ledger.map.coverage_start_height
                }
            }
        };
        super::private_retention_seal::verify_map_at_floor(trusted, ledger, floor)?;
        if seal.is_none() {
            for entry in &ledger.map.entries {
                let existing = previous.is_some_and(|old| {
                    old.ledger
                        .map
                        .entries
                        .binary_search_by(|old| old.key.cmp(&entry.key))
                        .is_ok()
                });
                if !existing {
                    if entry.lifecycle != BftPrivateRetentionLifecycleV1::ReservedPending
                        || !matches!(entry.fact, BftPrivateRetentionFactV1::Some { .. })
                    {
                        return Err(invalid("archived fresh fact is not original Some Reserve"));
                    }
                    added.push(super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, entry)?);
                }
            }
        }
        let mut base = super::private_retention_backend::retention_material_bound_from_bytes(
            &before,
            previous.map(|old| &old.ledger),
            &ledger.map,
            &body.head,
            0,
        )?;
        if let Some(seal) = seal {
            base = base
                .checked_add(seal.canonical_bytes()?.len() as u64)
                .ok_or_else(|| invalid("archived seal metadata material overflow"))?;
        }
        let expected = body
            .head
            .retained_material_bytes
            .ok_or_else(|| invalid("archived original lacks head2 material reservation"))?;
        if expected < base {
            return Err(invalid("archived original material reservation regressed"));
        }
        let accounting = BftPrivateRetentionOperationAccountingV1 {
            base,
            expected,
            added,
        };
        if accounting.added.is_empty() {
            accounting.verify_actual_prices(&Default::default())?;
        }
        Ok(accounting)
    }
}

pub fn private_retention_receipt_digest(
    receipt: &super::private_retention::BftPrivateRetentionReceiptV1,
) -> Result<String, BftSignerErrorV3> {
    super::crypto::digest_hex("oasis7.bft.private-retention.v1.receipt", receipt)
}

#[cfg(test)]
#[path = "private_retention_operation_proof_tests.rs"]
mod tests;
