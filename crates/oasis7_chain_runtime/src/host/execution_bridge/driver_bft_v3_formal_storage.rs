//! Distinct private wire-3 records. The common engine sees only typed material
//! references; all authority and actual-execution checks remain in the adapter.
use super::super::super::super::storage as common_storage;
use super::super::formal_ledger::{Format, GenericLedger, OriginContract, RecordContract};
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(super) const DIRECTORY: &str = common_storage::FORMAL_HEADER_PREFIX_DIRECTORY;
pub(super) const MARKER: &str = common_storage::FORMAL_HEADER_PREFIX_MARKER;
pub(super) const DOWNGRADE_FENCE: &[u8] = common_storage::FORMAL_HEADER_PREFIX_DOWNGRADE_FENCE;
const ORIGIN_DOMAIN: &str = "oasis7.formal.bft.header_prefix.origin.v1";
const RECORD_DOMAIN: &str = "oasis7.formal.bft.header_prefix.record.v1";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Origin {
    schema_version: u32,
    domain: String,
    ledger_id: String,
    material_pin_digest: String,
    first_height: u64,
    format_lock: common_storage::LockCarrierIdentity,
    writer_lock: common_storage::LockCarrierIdentity,
}
impl OriginContract for Origin {
    fn validate(&self) -> Result<(), String> {
        for hash in [
            &self.ledger_id,
            &self.material_pin_digest,
            &self.format_lock.token,
            &self.writer_lock.token,
        ] {
            validate_hash(hash)?;
        }
        if self.schema_version != 1
            || self.domain != ORIGIN_DOMAIN
            || self.first_height == 0
            || self.first_height.checked_add(63).is_none()
            || self.format_lock.inode == 0
            || self.writer_lock.inode == 0
        {
            return Err("wire 3 ledger origin schema or carrier identity".into());
        }
        Ok(())
    }
    fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest(self)
    }
    fn material_pin_digest(&self) -> &str {
        &self.material_pin_digest
    }
    fn ledger_id(&self) -> &str {
        &self.ledger_id
    }
    fn first_height(&self) -> u64 {
        self.first_height
    }
    fn format_lock(&self) -> &common_storage::LockCarrierIdentity {
        &self.format_lock
    }
    fn writer_lock(&self) -> &common_storage::LockCarrierIdentity {
        &self.writer_lock
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    schema_version: u32,
    domain: String,
    origin_digest: String,
    parent_record_digest: String,
    height: u64,
    certificate: FormalBftCertifiedHeaderV3,
    #[serde(deserialize_with = "super::private::deserialize_actions")]
    actions_cbor: Vec<u8>,
    snapshot_ref: String,
    journal_ref: String,
    business_index_refs: Vec<String>,
}
impl Record {
    pub(super) fn from_actual(
        origin: &Origin,
        parent_record_digest: &str,
        input: &PrivateHeaderEntryV3,
        executed: &ExecutedFormalHeaderV3,
        business_index_refs: Vec<String>,
    ) -> Result<Self, String> {
        if input.certificate.header != executed.candidate.header
            || input.certificate.execution_commitment != executed.candidate.commitment
        {
            return Err("wire 3 private record was not actually reproduced".into());
        }
        input.validate_binding()?;
        let record = Self {
            schema_version: 1,
            domain: RECORD_DOMAIN.into(),
            origin_digest: origin.digest()?,
            parent_record_digest: parent_record_digest.into(),
            height: input.certificate.execution_commitment.height,
            certificate: input.certificate.clone(),
            actions_cbor: input.actions_cbor.clone(),
            snapshot_ref: blake3_hex(&executed.snapshot_bytes),
            journal_ref: blake3_hex(&executed.journal_bytes),
            business_index_refs,
        };
        record.validate(origin)?;
        Ok(record)
    }
    pub(super) fn input(&self) -> PrivateHeaderEntryV3 {
        PrivateHeaderEntryV3 {
            certificate: self.certificate.clone(),
            actions_cbor: self.actions_cbor.clone(),
        }
    }
}
impl RecordContract<Origin> for Record {
    fn validate(&self, origin: &Origin) -> Result<(), String> {
        for value in [
            &self.origin_digest,
            &self.parent_record_digest,
            &self.snapshot_ref,
            &self.journal_ref,
        ] {
            validate_hash(value)?;
        }
        let bound = self.certificate.validate_binding()?;
        ValidatedBftPrivatePayloadV3::decode(bound, &self.actions_cbor)?;
        if self.schema_version != 1
            || self.domain != RECORD_DOMAIN
            || self.origin_digest != origin.digest()?
            || self.height != self.certificate.execution_commitment.height
            || self.height < origin.first_height
            || self.height - origin.first_height >= 64
            || self.snapshot_ref != self.certificate.execution_commitment.execution_state_root
            || self.business_index_refs.len()
                > super::super::formal_dependencies::MAX_RETAINED_INDEX_NODES
            || !self
                .business_index_refs
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        {
            return Err("wire 3 private ledger record binding or capacity".into());
        }
        for reference in &self.business_index_refs {
            validate_hash(reference)?;
        }
        Ok(())
    }
    fn digest(&self) -> Result<String, String> {
        digest(self)
    }
    fn height(&self) -> u64 {
        self.height
    }
    fn origin_digest(&self) -> &str {
        &self.origin_digest
    }
    fn parent_record_digest(&self) -> &str {
        &self.parent_record_digest
    }
    fn snapshot_ref(&self) -> &str {
        &self.snapshot_ref
    }
    fn journal_ref(&self) -> &str {
        &self.journal_ref
    }
    fn business_index_refs(&self) -> &[String] {
        &self.business_index_refs
    }
    fn references(&self) -> BTreeSet<String> {
        self.business_index_refs
            .iter()
            .cloned()
            .chain([self.snapshot_ref.clone(), self.journal_ref.clone()])
            .collect()
    }
}

pub(super) struct HeaderFormat;
impl Format for HeaderFormat {
    type Origin = Origin;
    type Record = Record;
    const DIRECTORY: &'static str = DIRECTORY;
    const MARKER: &'static str = MARKER;
    const MARKER_BYTES: &'static [u8] = b"oasis7.formal.bft.header_prefix.required/v1\n";
    const DOWNGRADE_FENCE: &'static [u8] = DOWNGRADE_FENCE;
    const SCOPE: &'static str = DIRECTORY;
    const OTHER_DIRECTORY: &'static str = "formal-bft-prefix-v1";
    const OTHER_MARKER: &'static str = "formal-bft-prefix-required-v1";
    fn new_origin(
        material_pin_digest: &str,
        first_height: u64,
        ledger_id: String,
        format_lock: common_storage::LockCarrierIdentity,
        writer_lock: common_storage::LockCarrierIdentity,
    ) -> Origin {
        Origin {
            schema_version: 1,
            domain: ORIGIN_DOMAIN.into(),
            ledger_id,
            material_pin_digest: material_pin_digest.into(),
            first_height,
            format_lock,
            writer_lock,
        }
    }
}
pub(super) type Ledger = GenericLedger<HeaderFormat>;

fn digest(value: &impl Serialize) -> Result<String, String> {
    Ok(blake3_hex(
        &serde_cbor::to_vec(value).map_err(|_| "encode private header record".to_string())?,
    ))
}
fn validate_hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("private header ledger requires canonical BLAKE3 references".into());
    }
    Ok(())
}
