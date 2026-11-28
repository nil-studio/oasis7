//! First Fresh Ready/OriginalLocal shape and original-receipt verification.
//! Pure preparation grants no material, signing, transport or storage authority.
use super::binding::invalid;
use super::crypto::{digest_hex, encode_bounded, verify_signature};
use super::fresh_coverage::{MAP_DOMAIN, RECEIPT_DOMAIN};
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSelectionCommitmentV1 {
    pub root: String,
    pub encoded_bytes: u64,
    pub count: u32,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BftPrivateRetentionSelectionOperationV1 {
    ReadyOriginalLocal {
        reference: BftPrivateRetentionAdmissionInputRefV1,
        expected_revision: u64,
        expected_state_digest: String,
        expected_head_digest: String,
    },
}
impl BftPrivateRetentionSelectionOperationV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        let Self::ReadyOriginalLocal {
            reference,
            expected_state_digest,
            expected_head_digest,
            ..
        } = self;
        reference.validate()?;
        state::validate_digest(expected_state_digest)?;
        state::validate_digest(expected_head_digest)?;
        digest_hex("oasis7.bft.private-retention.original-local-ready.v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BftPrivateRetentionSelectionKindV1 {
    OriginalLocal,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSelectionV1 {
    pub source: BftPrivateRetentionSourceV1,
    pub key: BftPrivateRetentionKeyV1,
    pub reference: BftPrivateRetentionAdmissionInputRefV1,
    pub kind: BftPrivateRetentionSelectionKindV1,
    pub local_identity_digest: String,
    pub local_identity_bytes: u64,
    pub sequence: u64,
    pub operation_digest: String,
    pub operation: BftPrivateRetentionSelectionOperationV1,
}
impl BftPrivateRetentionSelectionV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        digest_hex("oasis7.bft.private-retention.selection.v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSelectionIndexV1 {
    pub source: BftPrivateRetentionSourceV1,
    pub last_sequence: u64,
    pub selections: [BftPrivateRetentionSelectionV1; 1],
}
impl BftPrivateRetentionSelectionIndexV1 {
    pub fn commitment(&self) -> Result<BftPrivateRetentionSelectionCommitmentV1, BftSignerErrorV3> {
        Ok(BftPrivateRetentionSelectionCommitmentV1 {
            root: digest_hex("oasis7.bft.private-retention.selection-index.v1", self)?,
            encoded_bytes: encode_bounded(self, 1024 * 1024)?.len() as u64,
            count: 1,
            sequence: self.last_sequence,
        })
    }
}

/// Current Ready state; the first reservation remains a separate immutable value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftCurrentRetentionV1 {
    pub ledger: BftPrivateRetentionLedgerV1,
    pub selection_index: BftPrivateRetentionSelectionIndexV1,
}
impl BftCurrentRetentionV1 {
    /// Unsigned candidate only. A consumer must independently verify actual
    /// Local material and the complete new price before signing and one CAS.
    pub fn prepare_first(
        original: &BftFirstReservedOriginalV1,
        before: &BftSignerStateV3,
        local_identity_digest: &str,
        local_identity_bytes: u64,
        retained_material_bytes: u64,
        generation: u64,
    ) -> Result<(Self, BftSignerStateV3), BftSignerErrorV3> {
        let old = &original.ledger.receipt.body;
        let reference = original.input.descriptor()?;
        state::validate_digest(local_identity_digest)?;
        if generation < old.original_lease_generation
            || generation == 0
            || before.revision != old.after_revision
            || before.state_digest != old.after_state_digest
            || before.private_retention.as_ref() != Some(&old.head)
            || reference.source != old.head.source
            || !(1..=4 * 1024 * 1024).contains(&local_identity_bytes)
        {
            return Err(invalid(
                "first Ready original predecessor or local identity differs",
            ));
        }
        let operation = BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal {
            reference: reference.clone(),
            expected_revision: before.revision,
            expected_state_digest: before.state_digest.clone(),
            expected_head_digest: old.head.digest()?,
        };
        let operation_digest = operation.digest()?;
        let selection_index = BftPrivateRetentionSelectionIndexV1 {
            source: reference.source.clone(),
            last_sequence: 1,
            selections: [BftPrivateRetentionSelectionV1 {
                source: reference.source.clone(),
                key: reference.key.clone(),
                reference,
                kind: BftPrivateRetentionSelectionKindV1::OriginalLocal,
                local_identity_digest: local_identity_digest.into(),
                local_identity_bytes,
                sequence: 1,
                operation_digest: operation_digest.clone(),
                operation,
            }],
        };
        let map = original.ledger.map.first_ready()?;
        let bytes = encode_bounded(&map, 1024 * 1024)?;
        let mut head = old.head.clone();
        head.schema_version = 3;
        head.sequence = 3;
        head.selection_commitment = Some(selection_index.commitment()?);
        head.map_digest = digest_hex(MAP_DOMAIN, &map)?;
        head.map_byte_hash = blake3::hash(&bytes).to_hex().to_string();
        head.previous_receipt_digest = Some(digest_hex(
            "oasis7.bft.private-retention.v1.receipt",
            &original.ledger.receipt,
        )?);
        head.retained_material_bytes = Some(retained_material_bytes);
        let after = before.with_first_ready_head(head.clone())?;
        let receipt = BftPrivateRetentionReceiptV1 {
            body: BftPrivateRetentionReceiptBodyV1 {
                head,
                operation_digest,
                original_lease_generation: generation,
                before_revision: before.revision,
                before_state_digest: before.state_digest.clone(),
                after_revision: after.revision,
                after_state_digest: after.state_digest.clone(),
            },
            signature_hex: String::new(),
        };
        Ok((
            Self {
                ledger: BftPrivateRetentionLedgerV1 { map, receipt },
                selection_index,
            },
            after,
        ))
    }

    pub(super) fn validate_current(
        &self,
        snapshot: &BftFirstReserveSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        let original = snapshot
            .reservation
            .as_ref()
            .ok_or_else(|| invalid("Ready lacks original Reserve"))?;
        let current = &snapshot.current.signer;
        // Reconstruct the historical checkpoint without replacing current lease
        // or accepting that reconstruction as independent H authentication.
        let mut before = current.state.clone();
        before.revision = original.ledger.receipt.body.after_revision;
        before.state_digest = original.ledger.receipt.body.after_state_digest.clone();
        before.private_retention = Some(original.ledger.receipt.body.head.clone());
        let choice = &self.selection_index.selections[0];
        let body = &self.ledger.receipt.body;
        let (expected, after) = Self::prepare_first(
            original,
            &before,
            &choice.local_identity_digest,
            choice.local_identity_bytes,
            body.head
                .retained_material_bytes
                .ok_or_else(|| invalid("Ready material bound absent"))?,
            body.original_lease_generation,
        )?;
        if self.selection_index != expected.selection_index
            || self.ledger.map != expected.ledger.map
            || body != &expected.ledger.receipt.body
            || after != current.state
            || body.original_lease_generation > current.lease_generation
        {
            return Err(invalid("current Ready ledger/selection/State differs"));
        }
        fresh_coverage::check_namespace_budget(body, snapshot.current.reserved_response_bytes)
    }

    /// Verify original signatures at their original post-States, then current
    /// Ready independently. Current lease authorization remains the caller's job.
    pub fn verify(
        &self,
        binding: &BftSignerBindingV3,
        attestor: &BftPrivateRetentionAttestorV1,
        snapshot: &BftFirstReserveSnapshotV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate_current(snapshot)?;
        let activation = snapshot
            .current
            .original
            .as_ref()
            .ok_or_else(|| invalid("Ready lacks Fresh original"))?;
        let original = snapshot
            .reservation
            .as_ref()
            .ok_or_else(|| invalid("Ready lacks Reserve"))?;
        let generation = snapshot.current.signer.lease_generation;
        let covered = first_reservation::original_activation_state(
            binding,
            attestor,
            &snapshot.current.marker,
            activation,
            generation,
        )?;
        let reference = original.input.descriptor()?;
        let (_, reserved) = first_reservation::reserve_body(
            &covered,
            activation,
            &reference,
            &original.input,
            original.ledger.receipt.body.original_lease_generation,
        )?;
        first_reservation::verify_first_reservation(
            &covered,
            activation,
            &reference,
            original,
            (&reserved, generation),
            attestor,
        )?;
        let state = &snapshot.current.signer.state;
        if state != &reserved.with_first_ready_head(self.ledger.receipt.body.head.clone())? {
            return Err(invalid("Ready current State differs from independent H"));
        }
        state.validate(binding, &attestor.provider_public_key)?;
        verify_signature(
            RECEIPT_DOMAIN,
            &self.ledger.receipt.body,
            &self.ledger.receipt.signature_hex,
            &attestor.provider_public_key,
        )
    }
}
