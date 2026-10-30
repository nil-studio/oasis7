//! Independent bounded V3 successor segment; the original H ledger is immutable.
use super::super::super::super::super::storage as common_storage;
use super::super::super::formal_ledger::{Format, GenericLedger};
use super::*;
use serde::{Deserialize, Serialize};

const ORIGIN_DOMAIN: &str = "oasis7.formal.bft.private-window.origin.v1";
pub(super) const RECORD_DOMAIN: &str = "oasis7.formal.bft.private-window.record.v1";
pub(super) const DIRECTORY: &str = "formal-bft-header-window-v1";

#[path = "driver_bft_v3_window_directories.rs"]
pub(super) mod directories;
use directories::DirectoryIdentity;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WindowOrigin {
    schema_version: u32,
    domain: String,
    ledger_id: String,
    base_digest: String,
    first_height: u64,
    format_lock: common_storage::LockCarrierIdentity,
    writer_lock: common_storage::LockCarrierIdentity,
    directories: [DirectoryIdentity; 5],
}
impl WindowOrigin {
    pub(super) fn directory_identities(&self) -> &[DirectoryIdentity; 5] {
        &self.directories
    }
}
impl OriginContract for WindowOrigin {
    fn validate(&self) -> Result<(), String> {
        for h in [
            &self.ledger_id,
            &self.base_digest,
            &self.format_lock.token,
            &self.writer_lock.token,
        ] {
            hash(h)?;
        }
        if self.schema_version != 1
            || self.domain != ORIGIN_DOMAIN
            || self.first_height == 0
            || self.first_height.checked_add(63).is_none()
            || self.format_lock.inode == 0
            || self.writer_lock.inode == 0
            || self.directories.iter().any(|d| d.inode == 0)
        {
            return Err("private window origin version/range/carrier".into());
        }
        Ok(())
    }
    fn digest(&self) -> Result<String, String> {
        self.validate()?;
        Ok(blake3_hex(&successor_archive::encode(
            self,
            successor_archive::MAX_METADATA,
        )?))
    }
    fn material_pin_digest(&self) -> &str {
        &self.base_digest
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
pub(crate) struct WindowRecord {
    schema_version: u32,
    domain: String,
    origin_digest: String,
    parent_record_digest: String,
    height: u64,
    certificate: FormalBftCertifiedHeaderV3,
    #[serde(deserialize_with = "super::super::private::deserialize_actions")]
    actions_cbor: Vec<u8>,
    snapshot_ref: String,
    journal_ref: String,
    business_index_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    publication_reservation_ref: Option<String>,
}
impl WindowRecord {
    pub(super) fn from_actual(
        origin: &WindowOrigin,
        previous: &str,
        input: &PrivateHeaderEntryV3,
        executed: &ExecutedFormalHeaderV3,
        references: Vec<String>,
    ) -> Result<Self, String> {
        if input.certificate.header != executed.candidate.header
            || input.certificate.execution_commitment != executed.candidate.commitment
        {
            return Err("private window record was not actually reproduced".into());
        }
        let record = Self {
            schema_version: 1,
            domain: RECORD_DOMAIN.into(),
            origin_digest: origin.digest()?,
            parent_record_digest: previous.into(),
            height: input.certificate.execution_commitment.height,
            certificate: input.certificate.clone(),
            actions_cbor: input.actions_cbor.clone(),
            snapshot_ref: blake3_hex(&executed.snapshot_bytes),
            journal_ref: blake3_hex(&executed.journal_bytes),
            business_index_refs: references,
            publication_reservation_ref: None,
        };
        record.validate(origin)?;
        Ok(record)
    }
    pub(super) fn with_publication_reservation(
        mut self,
        reference: Option<String>,
    ) -> Result<Self, String> {
        if let Some(reference) = &reference {
            hash(reference)?;
        }
        self.publication_reservation_ref = reference;
        Ok(self)
    }
    pub(super) fn input(&self) -> PrivateHeaderEntryV3 {
        PrivateHeaderEntryV3 {
            certificate: self.certificate.clone(),
            actions_cbor: self.actions_cbor.clone(),
        }
    }
}
impl RecordContract<WindowOrigin> for WindowRecord {
    fn validate(&self, origin: &WindowOrigin) -> Result<(), String> {
        origin.validate()?;
        for h in [
            &self.origin_digest,
            &self.parent_record_digest,
            &self.snapshot_ref,
            &self.journal_ref,
        ] {
            hash(h)?;
        }
        if let Some(reference) = &self.publication_reservation_ref {
            hash(reference)?;
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
                > super::super::super::formal_dependencies::MAX_RETAINED_INDEX_NODES
            || !self.business_index_refs.windows(2).all(|p| p[0] < p[1])
        {
            return Err("private window record original/range/dependency binding".into());
        }
        for h in &self.business_index_refs {
            hash(h)?;
        }
        Ok(())
    }
    fn digest(&self) -> Result<String, String> {
        Ok(blake3_hex(&successor_archive::encode(
            self,
            super::super::super::formal_ledger::MAX_RECORD,
        )?))
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
    fn publication_reservation_ref(&self) -> Option<&str> {
        self.publication_reservation_ref.as_deref()
    }
    fn publication_binding(
        &self,
    ) -> Result<Option<super::super::super::formal_ledger::PublicationRecordBindingV1>, String>
    {
        self.publication_reservation_ref
            .as_ref()
            .map(|reference| {
                self.certificate.header.block_id().map(|header_id| {
                    super::super::super::formal_ledger::PublicationRecordBindingV1 {
                        reservation_ref: reference.clone(),
                        header_id,
                        actions_digest: blake3_hex(&self.actions_cbor),
                    }
                })
            })
            .transpose()
    }
}
pub(crate) struct WindowFormat;
impl Format for WindowFormat {
    type Origin = WindowOrigin;
    type Record = WindowRecord;
    fn publication_proof_bytes(records: &[Self::Record]) -> Result<Option<usize>, String> {
        Ok(Some(
            oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3 {
                schema_version: 1,
                entries: records
                    .iter()
                    .map(|record| record.input().certificate)
                    .collect(),
            }
            .canonical_bytes()?
            .len(),
        ))
    }
    const DIRECTORY: &'static str = DIRECTORY;
    const MARKER: &'static str = "formal-bft-header-window-required-v1";
    const MARKER_BYTES: &'static [u8] = b"oasis7.formal.bft.private-window.required/v1\n";
    const DOWNGRADE_FENCE: &'static [u8] =
        b"oasis7.formal.bft.private-window.downgrade-denied/v1\n";
    const SCOPE: &'static str = DIRECTORY;
    const OTHER_DIRECTORY: &'static str = "formal-bft-prefix-v1";
    const OTHER_MARKER: &'static str = "formal-bft-prefix-required-v1";
    const EXTRA_PRIVATE_DIRECTORY: Option<&'static str> = Some(storage::NODE_PRIVATE_DIRECTORY);
    const EMPTY_PRIVATE_DIRECTORY_DURING_INIT: bool = true;
    #[allow(clippy::too_many_arguments)]
    fn new_origin_at(
        base_digest: &str,
        first_height: u64,
        ledger_id: String,
        format_lock: common_storage::LockCarrierIdentity,
        writer_lock: common_storage::LockCarrierIdentity,
        root: &Path,
        _dir: &Path,
    ) -> Result<WindowOrigin, String> {
        let mut origin = Self::new_origin(
            base_digest,
            first_height,
            ledger_id,
            format_lock,
            writer_lock,
        );
        let paths = directories::paths(root)?;
        for (identity, path) in origin.directories.iter_mut().zip(paths) {
            *identity = DirectoryIdentity::read(&path)?;
        }
        origin.validate()?;
        Ok(origin)
    }
    fn validate_origin_directories(
        origin: &WindowOrigin,
        root: &Path,
        _dir: &Path,
    ) -> Result<(), String> {
        for (identity, path) in origin.directories.iter().zip(directories::paths(root)?) {
            if identity != &DirectoryIdentity::read(&path)? {
                return Err("active private parent directory was replaced".into());
            }
        }
        Ok(())
    }
    fn new_origin(
        base_digest: &str,
        first_height: u64,
        ledger_id: String,
        format_lock: common_storage::LockCarrierIdentity,
        writer_lock: common_storage::LockCarrierIdentity,
    ) -> WindowOrigin {
        WindowOrigin {
            schema_version: 1,
            domain: ORIGIN_DOMAIN.into(),
            ledger_id,
            base_digest: base_digest.into(),
            first_height,
            format_lock,
            writer_lock,
            directories: [DirectoryIdentity::default(); 5],
        }
    }
}
pub(crate) type WindowLedger = GenericLedger<WindowFormat>;

fn hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("private window hash encoding".into());
    }
    Ok(())
}
