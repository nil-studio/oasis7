//! The first pre-signing original input. Later alternatives and Ready retain
//! their own protected operation contracts.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, BftSigningKeyHandleV3, invalid};
use super::crypto::{digest_hex, encode_bounded, sign_preimage, verify_signature};
use super::fresh_coverage::{
    MAP_DOMAIN, ORIGINAL_RECEIPT_RESERVE, RECEIPT_DOMAIN, activation_body, check_namespace_budget,
};
use super::{
    BftFreshCoverageSnapshotV1, BftPrivateRetentionActivationV1,
    BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionAdmissionInputV1,
    BftPrivateRetentionAttestorV1, BftPrivateRetentionCreationMarkerV1,
    BftPrivateRetentionInputObjectKindV1, BftPrivateRetentionKeyV1,
    BftPrivateRetentionReceiptBodyV1, BftPrivateRetentionReceiptV1, BftSignerStateV3,
    BftSuccessorWindowArchiveRefV1,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};

const MAX_PUBLICATION_BYTES: u64 = 512 * 1024 * 1024;

// Exact supported subset of the original Some and ReservedPending wire variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Fact {
    Some {
        original_data_ref: String,
        metadata_ref: String,
        admission_digest: String,
        input_ref: BftSuccessorWindowArchiveRefV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Lifecycle {
    ReservedPending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BftPrivateRetentionEntryV1 {
    key: BftPrivateRetentionKeyV1,
    fact: Fact,
    lifecycle: Lifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionMapV1 {
    schema_version: u32,
    source: super::BftPrivateRetentionSourceV1,
    coverage_start_height: u64,
    previous_seal_receipt_digest: Option<String>,
    entries: [BftPrivateRetentionEntryV1; 1],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionLedgerV1 {
    pub map: BftPrivateRetentionMapV1,
    pub receipt: BftPrivateRetentionReceiptV1,
}

/// Trusted Runtime output after verifying original H, candidate replay and
/// the retained initial dependency inventory for the independently pinned source.
pub struct BftFirstReservationPriceV1 {
    pub publication_bytes: u64,
    pub private_pin_bytes: u64,
    pub original_selection_bytes: u64,
    pub retained_h_dependencies: BTreeSet<String>,
}

impl BftFirstReservationPriceV1 {
    pub fn verify_input(
        &self,
        input: &BftPrivateRetentionAdmissionInputV1,
    ) -> Result<(), BftSignerErrorV3> {
        use BftPrivateRetentionInputObjectKindV1 as Kind;
        input.validate()?;
        if self.publication_bytes == 0
            || self.private_pin_bytes == 0
            || self.retained_h_dependencies.len() > 4096
        {
            return Err(invalid(
                "first reserve lacks original publication plan or H inventory",
            ));
        }
        for digest in &self.retained_h_dependencies {
            super::state::validate_digest(digest)?;
        }
        let mut total = self
            .publication_bytes
            .checked_add(self.private_pin_bytes)
            .and_then(|n| n.checked_add(self.original_selection_bytes))
            .ok_or_else(|| invalid("first reserve original price overflow"))?;
        for object in &input.objects {
            let added = match object.kind {
                Kind::OriginalData => object
                    .reference
                    .bytes
                    .checked_mul(2)
                    .ok_or_else(|| invalid("first reserve Data price overflow"))?,
                Kind::PublicationMetadata => object.reference.bytes,
                Kind::Dependency
                    if self
                        .retained_h_dependencies
                        .contains(&object.reference.digest) =>
                {
                    0
                }
                Kind::Dependency => object.reference.bytes,
            };
            total = total
                .checked_add(added)
                .ok_or_else(|| invalid("first reserve original price overflow"))?;
        }
        if total > MAX_PUBLICATION_BYTES || total != input.publication_reservation_bytes {
            return Err(invalid(
                "first reserve price differs from original authenticated inventory",
            ));
        }
        Ok(())
    }
}

/// The reader holds its authenticated transport allocation through the
/// original-byte verification and atomic publication/readback.
pub struct BftAuthorizedOriginalReserveInputV1 {
    pub input: Arc<BftPrivateRetentionAdmissionInputV1>,
    pub transport_hold: Box<dyn Send + Sync>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftFirstReservedOriginalV1 {
    pub ledger: BftPrivateRetentionLedgerV1,
    pub input: Arc<BftPrivateRetentionAdmissionInputV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftFirstReserveSnapshotV1 {
    pub current: BftFreshCoverageSnapshotV1,
    pub reservation: Option<BftFirstReservedOriginalV1>,
}

pub(super) fn original_activation_state(
    binding: &BftSignerBindingV3,
    attestor: &BftPrivateRetentionAttestorV1,
    marker: &BftPrivateRetentionCreationMarkerV1,
    activation: &BftPrivateRetentionActivationV1,
    current_generation: u64,
) -> Result<BftSignerStateV3, BftSignerErrorV3> {
    let initial = attestor.verify_marker(binding, marker)?;
    let (_, _, after) = activation_body(
        attestor,
        &initial,
        activation.receipt.body.original_lease_generation,
    )?;
    activation.verify(binding, attestor, marker, &after, current_generation)?;
    Ok(after)
}

fn reserve_body(
    before: &BftSignerStateV3,
    activation: &BftPrivateRetentionActivationV1,
    reference: &BftPrivateRetentionAdmissionInputRefV1,
    input: &BftPrivateRetentionAdmissionInputV1,
    generation: u64,
) -> Result<(BftPrivateRetentionLedgerV1, BftSignerStateV3), BftSignerErrorV3> {
    use BftPrivateRetentionInputObjectKindV1 as Kind;
    reference.validate_input(input)?;
    let original = &activation.receipt.body.head;
    if before.private_retention.as_ref() != Some(original)
        || reference.source != original.source
        || reference.key.height != activation.map.coverage_start_height
    {
        return Err(invalid(
            "first reserve input differs from original covered H",
        ));
    }
    let role = |kind| {
        input
            .objects
            .iter()
            .find(|object| object.kind == kind)
            .map(|object| object.reference.digest.clone())
            .ok_or_else(|| invalid("first reserve original role absent"))
    };
    let map = BftPrivateRetentionMapV1 {
        schema_version: activation.map.schema_version,
        source: activation.map.source.clone(),
        coverage_start_height: activation.map.coverage_start_height,
        previous_seal_receipt_digest: None,
        entries: [BftPrivateRetentionEntryV1 {
            key: reference.key.clone(),
            fact: Fact::Some {
                original_data_ref: role(Kind::OriginalData)?,
                metadata_ref: role(Kind::PublicationMetadata)?,
                admission_digest: reference.admission_digest.clone(),
                input_ref: reference.object.clone(),
            },
            lifecycle: Lifecycle::ReservedPending,
        }],
    };
    let bytes = encode_bounded(&map, 1024 * 1024)?;
    let mut head = original.clone();
    head.sequence = 2;
    head.entry_count = 1;
    head.previous_receipt_digest = Some(digest_hex(
        "oasis7.bft.private-retention.v1.receipt",
        &activation.receipt,
    )?);
    head.map_digest = digest_hex(MAP_DOMAIN, &map)?;
    head.map_byte_hash = blake3::hash(&bytes).to_hex().to_string();
    head.retained_material_bytes = Some(u64::MAX);
    let mut widest = before.clone();
    widest.private_retention = Some(head.clone());
    widest.revision = 2;
    let state_size = widest.encoded_len()? as u64;
    head.retained_material_bytes = Some(
        original
            .retained_material_bytes
            .and_then(|n| n.checked_add(reference.object.bytes))
            .and_then(|n| n.checked_add(input.publication_reservation_bytes))
            .and_then(|n| n.checked_add(state_size.checked_mul(2)?))
            .and_then(|n| n.checked_add(bytes.len() as u64))
            .and_then(|n| n.checked_add(ORIGINAL_RECEIPT_RESERVE))
            .ok_or_else(|| invalid("first reserve retained material overflow"))?,
    );
    let after = before.with_original_coverage_head(head.clone())?;
    let body = BftPrivateRetentionReceiptBodyV1 {
        head,
        operation_digest: digest_hex(
            "oasis7.bft.private-retention.v1.operation",
            &(
                Some(&reference.key),
                &map,
                Some(&activation.receipt.body.operation_digest),
            ),
        )?,
        original_lease_generation: generation,
        before_revision: before.revision,
        before_state_digest: before.state_digest.clone(),
        after_revision: after.revision,
        after_state_digest: after.state_digest.clone(),
    };
    Ok((
        BftPrivateRetentionLedgerV1 {
            map,
            receipt: BftPrivateRetentionReceiptV1 {
                body,
                signature_hex: String::new(),
            },
        },
        after,
    ))
}

pub(super) fn prepare_first_reservation(
    before: &BftSignerStateV3,
    activation: &BftPrivateRetentionActivationV1,
    reference: &BftPrivateRetentionAdmissionInputRefV1,
    input: Arc<BftPrivateRetentionAdmissionInputV1>,
    generation: u64,
    key: &dyn BftSigningKeyHandleV3,
) -> Result<(BftFirstReservedOriginalV1, BftSignerStateV3), BftSignerErrorV3> {
    let (mut ledger, after) = reserve_body(before, activation, reference, &input, generation)?;
    check_namespace_budget(&ledger.receipt.body, 0)?;
    ledger.receipt.signature_hex = sign_preimage(key, RECEIPT_DOMAIN, &ledger.receipt.body)?;
    Ok((BftFirstReservedOriginalV1 { ledger, input }, after))
}

pub(super) fn verify_first_reservation(
    before: &BftSignerStateV3,
    activation: &BftPrivateRetentionActivationV1,
    reference: &BftPrivateRetentionAdmissionInputRefV1,
    original: &BftFirstReservedOriginalV1,
    current: (&BftSignerStateV3, u64),
    attestor: &BftPrivateRetentionAttestorV1,
) -> Result<(), BftSignerErrorV3> {
    let generation = original.ledger.receipt.body.original_lease_generation;
    let (expected, after) =
        reserve_body(before, activation, reference, &original.input, generation)?;
    if generation < activation.receipt.body.original_lease_generation
        || generation > current.1
        || expected.map != original.ledger.map
        || expected.receipt.body != original.ledger.receipt.body
        || &after != current.0
    {
        return Err(invalid(
            "first reserve readback differs from original atomic input",
        ));
    }
    verify_signature(
        RECEIPT_DOMAIN,
        &original.ledger.receipt.body,
        &original.ledger.receipt.signature_hex,
        &attestor.provider_public_key,
    )
}
