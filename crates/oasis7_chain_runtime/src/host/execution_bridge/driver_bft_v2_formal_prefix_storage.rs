//! A separate H-anchored multiheight format. Neither an engineering genesis
//! nor the old fixed H+1 ledger is accepted as this ledger's origin.
use super::super::super::storage as common_storage;
use super::*;
use oasis7_formal_protocol::bft::{FormalBftCertifiedEntryV1, FormalBftCertifiedPrefixV1};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

pub(super) const DIRECTORY: &str = "formal-bft-prefix-v1";
const MARKER: &str = "formal-bft-prefix-required-v1";
const MARKER_BYTES: &[u8] = b"oasis7.formal.bft.prefix.required/v1\n";
#[cfg(test)]
const LEGACY_MARKER: &str = "formal-bft-read-required-v1";
const DOWNGRADE_FENCE: &[u8] = common_storage::FORMAL_PREFIX_DOWNGRADE_FENCE;
const SCOPE: &str = "formal-bft-prefix-v1";
#[cfg(test)]
pub(super) use super::formal_ledger::PublicationFault;
use super::formal_ledger::{Format, GenericLedger, OriginContract, RecordContract};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OriginPin {
    schema_version: u32,
    domain: String,
    ledger_id: String,
    material_pin_digest: String,
    first_height: u64,
    format_lock: common_storage::LockCarrierIdentity,
    writer_lock: common_storage::LockCarrierIdentity,
}
impl OriginPin {
    fn validate(&self) -> Result<(), String> {
        hash(&self.ledger_id)?;
        hash(&self.material_pin_digest)?;
        hash(&self.format_lock.token)?;
        hash(&self.writer_lock.token)?;
        if self.schema_version != 1
            || self.domain != "oasis7.formal.bft.prefix.origin.v1"
            || self.format_lock.inode == 0
            || self.writer_lock.inode == 0
            || self.first_height == 0
            || self.first_height.checked_add(63).is_none()
        {
            return Err("formal prefix origin schema or height capacity".into());
        }
        Ok(())
    }
    fn digest(&self) -> Result<String, String> {
        self.validate()?;
        digest(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    schema_version: u32,
    origin_digest: String,
    pub(super) height: u64,
    parent_record_digest: String,
    pub(super) entry: FormalBftCertifiedEntryV1,
    snapshot_ref: String,
    journal_ref: String,
    business_index_refs: Vec<String>,
    new_results: Vec<CanonicalIntentResultV1>,
}
impl Record {
    pub(super) fn digest(&self) -> Result<String, String> {
        digest(self)
    }
    fn references(&self) -> BTreeSet<String> {
        let mut refs: BTreeSet<_> = self.business_index_refs.iter().cloned().collect();
        refs.insert(self.snapshot_ref.clone());
        refs.insert(self.journal_ref.clone());
        refs
    }
    fn validate(&self, origin: &OriginPin) -> Result<(), String> {
        for value in [
            &self.origin_digest,
            &self.parent_record_digest,
            &self.snapshot_ref,
            &self.journal_ref,
        ] {
            hash(value)?;
        }
        self.entry.certificate.validate_wire_bounds()?;
        self.entry.commitment.validate()?;
        if self.schema_version != 1
            || self.origin_digest != origin.digest()?
            || self.height != self.entry.certificate.proposal.block.height
            || self.height < origin.first_height
            || self.height - origin.first_height >= 64
            || self.new_results.len() > 1024
            || serde_cbor::to_vec(&self.new_results)
                .map_err(|e| e.to_string())?
                .len()
                > 4 * 1024 * 1024
            || self.business_index_refs.len() > formal_dependencies::MAX_RETAINED_INDEX_NODES
            || !self
                .business_index_refs
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        {
            return Err("formal prefix record header or dependency/result capacity".into());
        }
        for reference in &self.business_index_refs {
            hash(reference)?;
        }
        self.entry.commitment.verify_block(
            &self.entry.certificate.proposal.block,
            self.entry.commitment.activation_digest(),
        )?;
        Ok(())
    }
}

pub(super) struct V1Format;
impl Format for V1Format {
    type Origin = OriginPin;
    type Record = Record;
    const DIRECTORY: &'static str = DIRECTORY;
    const MARKER: &'static str = MARKER;
    const MARKER_BYTES: &'static [u8] = MARKER_BYTES;
    const DOWNGRADE_FENCE: &'static [u8] = DOWNGRADE_FENCE;
    const SCOPE: &'static str = SCOPE;
    const OTHER_DIRECTORY: &'static str = "formal-bft-header-prefix-v1";
    const OTHER_MARKER: &'static str = "formal-bft-header-prefix-required-v1";
    fn new_origin(
        material_pin_digest: &str,
        first_height: u64,
        ledger_id: String,
        format_lock: common_storage::LockCarrierIdentity,
        writer_lock: common_storage::LockCarrierIdentity,
    ) -> OriginPin {
        OriginPin {
            schema_version: 1,
            domain: "oasis7.formal.bft.prefix.origin.v1".into(),
            ledger_id,
            material_pin_digest: material_pin_digest.into(),
            first_height,
            format_lock,
            writer_lock,
        }
    }
}
impl OriginContract for OriginPin {
    fn validate(&self) -> Result<(), String> {
        OriginPin::validate(self)
    }
    fn digest(&self) -> Result<String, String> {
        OriginPin::digest(self)
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
impl RecordContract<OriginPin> for Record {
    fn validate(&self, origin: &OriginPin) -> Result<(), String> {
        Record::validate(self, origin)
    }
    fn digest(&self) -> Result<String, String> {
        Record::digest(self)
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
        Record::references(self)
    }
}
pub(super) struct Ledger(GenericLedger<V1Format>);
impl std::ops::Deref for Ledger {
    type Target = GenericLedger<V1Format>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Ledger {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Ledger {
    pub(super) fn exists(root: &Path) -> Result<bool, String> {
        GenericLedger::<V1Format>::exists(root)
    }
    pub(super) fn open(root: &Path, initialize: bool, origin_bytes: usize) -> Result<Self, String> {
        GenericLedger::open(root, initialize, origin_bytes).map(Self)
    }
    pub(super) fn prefix_through(&self, height: u64) -> Result<FormalBftCertifiedPrefixV1, String> {
        Ok(FormalBftCertifiedPrefixV1 {
            schema_version: 1,
            entries: self
                .records_through(height)?
                .map(|record| record.entry.clone())
                .collect(),
        })
    }
    pub(super) fn check_input(&self, prefix: &FormalBftCertifiedPrefixV1) -> Result<(), String> {
        for (index, record) in self.records().enumerate() {
            if prefix.entries.get(index) != Some(&record.entry) {
                return Err(
                    "formal prefix input conflicts with installed history or original pending QC"
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub(super) fn accept(
        &mut self,
        entry: &FormalBftCertifiedEntryV1,
        executed: &ExecutedFormalCandidateV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        previous: &str,
        store: &LocalCasStore,
        append: bool,
    ) -> Result<String, String> {
        self.verify_carriers()?;
        if entry.certificate.proposal.block != executed.block
            || entry.commitment != executed.commitment
        {
            return Err("formal prefix candidate was not independently reproduced".into());
        }
        formal_dependencies::validate_nodes(nodes)?;
        let record = Record {
            schema_version: 1,
            origin_digest: self.origin_digest()?,
            height: executed.block.height,
            parent_record_digest: previous.into(),
            entry: entry.clone(),
            snapshot_ref: blake3_hex(&executed.snapshot_bytes),
            journal_ref: blake3_hex(&executed.journal_bytes),
            business_index_refs: self.retained_business_refs(executed.block.height, nodes)?,
            new_results: executed.new_results.clone(),
        };
        self.accept_prepared(
            record,
            &executed.snapshot_bytes,
            &executed.journal_bytes,
            nodes,
            store,
            append,
        )
    }
}

fn digest(value: &impl Serialize) -> Result<String, String> {
    Ok(blake3_hex(
        &serde_cbor::to_vec(value).map_err(|e| e.to_string())?,
    ))
}
fn hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("formal prefix digest is not canonical lowercase BLAKE3 hex".into());
    }
    Ok(())
}

#[cfg(test)]
mod capacity_tests {
    use super::*;
    #[test]
    fn completed_origin_rejects_replaced_os_lock_carriers_on_fresh_open() {
        use std::os::unix::fs::OpenOptionsExt;
        struct Root(PathBuf);
        impl Drop for Root {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let root = Root(
            std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("oasis7-prefix-carrier-{}", hex::encode(random))),
        );
        let mut ledger = Ledger::open(&root.0, true, 0).unwrap();
        ledger.bind(&"a".repeat(64), 3).unwrap();
        drop(ledger);
        for relative in ["bft-format.lock", "formal-bft-prefix-v1/writer.lock"] {
            let carrier = root.0.join(relative);
            let token = fs::read(&carrier).unwrap();
            for changed in [vec![0x7a; 32], Vec::new()] {
                fs::write(&carrier, &changed).unwrap();
                assert!(
                    Ledger::open(&root.0, true, 0).is_err(),
                    "fresh open accepted changed/empty token on same inode: {relative}"
                );
                assert_eq!(
                    fs::read(&carrier).unwrap(),
                    changed,
                    "failed reopen must preserve original corrupted carrier bytes"
                );
                fs::write(&carrier, &token).unwrap();
                drop(Ledger::open(&root.0, false, 0).unwrap());
            }
            let original = carrier.with_extension("preserved-test-lock");
            fs::rename(&carrier, &original).unwrap();
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&carrier)
                .unwrap();
            assert!(
                Ledger::open(&root.0, true, 0).is_err(),
                "fresh open accepted replaced {relative}"
            );
            fs::remove_file(&carrier).unwrap();
            fs::rename(&original, &carrier).unwrap();
            drop(Ledger::open(&root.0, false, 0).unwrap());
        }
        // Even if this fence is the only remaining durable prefix identity,
        // a current engineering-mode attempt must preserve the damaged lock.
        fs::remove_file(root.0.join(DIRECTORY).join("origin.cbor")).unwrap();
        fs::remove_file(root.0.join(MARKER)).unwrap();
        assert_eq!(
            fs::read(root.0.join(LEGACY_MARKER)).unwrap(),
            DOWNGRADE_FENCE
        );
        let carrier = root.0.join("bft-format.lock");
        fs::write(&carrier, []).unwrap();
        assert!(common_storage::lock_format(&root.0).is_err());
        assert!(fs::read(&carrier).unwrap().is_empty());
        assert!(Ledger::open(&root.0, true, 0).is_err());
        assert!(fs::read(&carrier).unwrap().is_empty());
    }
}
