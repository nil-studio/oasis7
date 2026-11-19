//! First original private-retention transition, before any candidate admission.
use super::binding::{
    BftSignerBindingV3, BftSignerErrorV3, BftSigningKeyHandleV3, decode_key, invalid,
};
use super::crypto::{digest_hex, encode_bounded, sign_preimage, verify_signature};
use super::state::{BftSignerStateV3, validate_digest};
use super::{BftPrivateRetentionSourceV1, BftSignerBackendSnapshotV3};
use serde::{Deserialize, Serialize};

const MAP_DOMAIN: &str = "oasis7.bft.private-retention.v1.map";
const RECEIPT_DOMAIN: &str = "oasis7.bft.private-retention.v1.head-receipt";
const SOURCE_INITIALIZATION_RESERVE: u64 =
    64 * 1024 * 1024 + 4 * 4 * 1024 * 1024 + 4 * 64 * 1024 + 2 * 4 * 1024 * 1024 + 1024 * 1024;
const ORIGINAL_RECEIPT_RESERVE: u64 = 16 * 1024;
const NAMESPACE_LIMIT: u64 = super::BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionCreationMarkerV1 {
    pub schema_version: u32,
    pub creation_identity: String,
    pub source: BftPrivateRetentionSourceV1,
    pub ledger_origin_digest: String,
    pub activation_digest: String,
    pub initial_state_digest: String,
    pub initial_height: u64,
}

impl BftPrivateRetentionCreationMarkerV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        if self.schema_version != 1 || self.initial_height == 0 {
            return Err(invalid("private retention creation schema/height"));
        }
        self.source.validate()?;
        for value in [
            &self.creation_identity,
            &self.ledger_origin_digest,
            &self.activation_digest,
            &self.initial_state_digest,
        ] {
            validate_digest(value)?;
        }
        digest_hex("oasis7.bft.private-retention.v1.source-creation", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionAttestorV1 {
    pub schema_version: u32,
    pub provider_public_key: String,
    pub source: BftPrivateRetentionSourceV1,
    pub creation_marker_digest: String,
    pub coverage_start_height: u64,
    pub allow_authenticated_successor_origins: bool,
}

impl BftPrivateRetentionAttestorV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        if self.schema_version != 1 || self.coverage_start_height == 0 {
            return Err(invalid("private retention capability schema/coverage"));
        }
        self.source.validate()?;
        validate_digest(&self.creation_marker_digest)?;
        decode_key(&self.provider_public_key)?;
        digest_hex("oasis7.bft.private-retention.v1.attestor", self)
    }

    pub(super) fn verify_marker(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
    ) -> Result<BftSignerStateV3, BftSignerErrorV3> {
        self.digest()?;
        let initial = BftSignerStateV3::from_handoff(binding)?;
        if marker.digest()? != self.creation_marker_digest
            || marker.source != self.source
            || self.source.binding_digest != initial.binding_digest
            || self.source.handoff_digest != binding.handoff_record_id
            || self.source.material_source_digest != binding.material_identity
            || marker.initial_height != self.coverage_start_height
            || marker.initial_height != binding.initial_schedule().height()
            || marker.activation_digest != binding.activation_digest
            || marker.initial_state_digest != initial.state_digest
        {
            return Err(invalid(
                "original Fresh source differs from independent binding",
            ));
        }
        Ok(initial)
    }
}

/// The empty-only subset of the original V1 map. A nonempty map is outside
/// this activation protocol, and cannot be accepted as an initial map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionEmptyMapV1 {
    schema_version: u32,
    source: BftPrivateRetentionSourceV1,
    coverage_start_height: u64,
    previous_seal_receipt_digest: Option<String>,
    entries: [(); 0],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionHeadV1 {
    pub schema_version: u32,
    pub source: BftPrivateRetentionSourceV1,
    pub capability_digest: String,
    pub sequence: u64,
    pub map_digest: String,
    pub map_byte_hash: String,
    pub entry_count: u32,
    pub previous_receipt_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_material_bytes: Option<u64>,
}

impl BftPrivateRetentionHeadV1 {
    pub(super) fn validate_initial(&self, binding_digest: &str) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        for hash in [
            &self.capability_digest,
            &self.map_digest,
            &self.map_byte_hash,
        ] {
            validate_digest(hash)?;
        }
        if self.schema_version != 2
            || self.source.binding_digest != binding_digest
            || self.sequence != 1
            || self.entry_count != 0
            || self.previous_receipt_digest.is_some()
            || self
                .retained_material_bytes
                .is_none_or(|bytes| bytes > NAMESPACE_LIMIT || bytes == 0)
        {
            return Err(invalid(
                "private retention first head shape or binding differs",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionReceiptBodyV1 {
    pub head: BftPrivateRetentionHeadV1,
    pub operation_digest: String,
    pub original_lease_generation: u64,
    pub before_revision: u64,
    pub before_state_digest: String,
    pub after_revision: u64,
    pub after_state_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionReceiptV1 {
    pub body: BftPrivateRetentionReceiptBodyV1,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionActivationV1 {
    pub map: BftPrivateRetentionEmptyMapV1,
    pub receipt: BftPrivateRetentionReceiptV1,
}

impl BftPrivateRetentionActivationV1 {
    pub(super) fn prepare(
        attestor: &BftPrivateRetentionAttestorV1,
        before: &BftSignerBackendSnapshotV3,
        transfer_reserved_bytes: u64,
        provider: &dyn BftSigningKeyHandleV3,
    ) -> Result<(Self, BftSignerStateV3), BftSignerErrorV3> {
        let (map, body, after) = activation_body(attestor, &before.state, before.lease_generation)?;
        check_namespace_budget(&body, transfer_reserved_bytes)?;
        let signature_hex = sign_preimage(provider, RECEIPT_DOMAIN, &body)?;
        Ok((
            Self {
                map,
                receipt: BftPrivateRetentionReceiptV1 {
                    body,
                    signature_hex,
                },
            },
            after,
        ))
    }

    /// Verify against the independently pinned source and a separately
    /// authenticated current checkpoint. This does not authorize signing.
    pub fn verify(
        &self,
        binding: &BftSignerBindingV3,
        attestor: &BftPrivateRetentionAttestorV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
        current: &BftSignerStateV3,
        current_generation: u64,
    ) -> Result<(), BftSignerErrorV3> {
        let initial = attestor.verify_marker(binding, marker)?;
        let generation = self.receipt.body.original_lease_generation;
        let (map, expected, after) = activation_body(attestor, &initial, generation)?;
        if generation == 0
            || generation > current_generation
            || self.map != map
            || self.receipt.body != expected
            || *current != after
        {
            return Err(invalid(
                "private retention original activation readback differs",
            ));
        }
        verify_signature(
            RECEIPT_DOMAIN,
            &self.receipt.body,
            &self.receipt.signature_hex,
            &attestor.provider_public_key,
        )
    }
}

fn activation_body(
    attestor: &BftPrivateRetentionAttestorV1,
    before: &BftSignerStateV3,
    generation: u64,
) -> Result<
    (
        BftPrivateRetentionEmptyMapV1,
        BftPrivateRetentionReceiptBodyV1,
        BftSignerStateV3,
    ),
    BftSignerErrorV3,
> {
    let map = BftPrivateRetentionEmptyMapV1 {
        schema_version: 1,
        source: attestor.source.clone(),
        coverage_start_height: attestor.coverage_start_height,
        previous_seal_receipt_digest: None,
        entries: [],
    };
    let bytes = encode_bounded(&map, 1024 * 1024)?;
    let mut head = BftPrivateRetentionHeadV1 {
        schema_version: 2,
        source: map.source.clone(),
        capability_digest: attestor.digest()?,
        sequence: 1,
        map_digest: digest_hex(MAP_DOMAIN, &map)?,
        map_byte_hash: blake3::hash(&bytes).to_hex().to_string(),
        entry_count: 0,
        previous_receipt_digest: None,
        retained_material_bytes: Some(u64::MAX),
    };
    let mut placeholder = before.clone();
    placeholder.private_retention = Some(head.clone());
    placeholder.revision = before
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("retention revision overflow"))?;
    // Exact original initialization charge: two widest encoded States, map,
    // original receipt envelope, and initial source-material reservation.
    head.retained_material_bytes = Some(
        (placeholder.encoded_len()? as u64)
            .checked_mul(2)
            .and_then(|n| n.checked_add(bytes.len() as u64))
            .and_then(|n| n.checked_add(ORIGINAL_RECEIPT_RESERVE))
            .and_then(|n| n.checked_add(SOURCE_INITIALIZATION_RESERVE))
            .ok_or_else(|| invalid("retention initialization material overflow"))?,
    );
    let after = before.with_original_coverage_head(head.clone())?;
    let operation_digest = digest_hex(
        "oasis7.bft.private-retention.v1.operation",
        &(
            Option::<&super::BftPrivateRetentionKeyV1>::None,
            &map,
            Option::<&String>::None,
        ),
    )?;
    let body = BftPrivateRetentionReceiptBodyV1 {
        head,
        operation_digest,
        original_lease_generation: generation,
        before_revision: before.revision,
        before_state_digest: before.state_digest.clone(),
        after_revision: after.revision,
        after_state_digest: after.state_digest.clone(),
    };
    Ok((map, body, after))
}

pub(super) fn check_namespace_budget(
    body: &BftPrivateRetentionReceiptBodyV1,
    transfer_reserved_bytes: u64,
) -> Result<(), BftSignerErrorV3> {
    if body
        .head
        .retained_material_bytes
        .and_then(|n| n.checked_add(transfer_reserved_bytes))
        .is_none_or(|total| total > NAMESPACE_LIMIT)
    {
        return Err(invalid(
            "private retention initialization exceeds namespace budget",
        ));
    }
    Ok(())
}
