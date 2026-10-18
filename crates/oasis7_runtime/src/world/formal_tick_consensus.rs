//! Bounded qualified Tick prefix plus a hot tail. Archived records retain their
//! original canonical bytes; the qualified Snapshot anchors count and index.
use super::{World, WorldError};
use crate::formal_retained_history::{
    VerifiedFormalRetainedHistoryParentV1, retained_tick_consensus_leaves,
};
use crate::{TickConsensusRecord, WorldTime};
use oasis7_distfs::controlled_authority::formal::business_index::{
    BusinessIndexLeafV1, BusinessIndexNodeReader,
};
const HOT_FORMAL_TICK_RECORDS: usize = 32;
const MAX_TICK_RECORDS_TO_ARCHIVE_PER_PLAN: usize = 64;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormalTickConsensusArchivePlanV1 {
    pub base_archived_count: u64,
    pub proposed_archived_count: u64,
    pub records_to_prune: Vec<TickConsensusRecord>,
    pub immutable_leaves: Vec<BusinessIndexLeafV1>,
}
fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: reason.into(),
    }
}
impl World {
    /// Hot certificates only when a qualified prefix exists. Exact owned
    /// historical queries retain the original archive certificate bytes.
    pub fn tick_consensus_records(&self) -> &[TickConsensusRecord] {
        self.tick_consensus_records.as_slice()
    }

    /// `tick_consensus_records()` exposes only this hot tail when a formal
    /// prefix exists. Use exact indexed queries for older certificates.
    pub fn tick_consensus_record_count(&self) -> u64 {
        self.formal_archived_tick_consensus_count
            .checked_add(self.tick_consensus_records.len() as u64)
            .expect("validated Tick count invariant")
    }
    pub fn read_tick_consensus_record(
        &self,
        height: u64,
    ) -> Result<Option<TickConsensusRecord>, WorldError> {
        if height == 0 {
            return Err(invalid("Tick consensus height is zero"));
        }
        if let Some(record) = self
            .tick_consensus_records
            .iter()
            .find(|r| r.certificate.consensus_height == height)
        {
            self.validate_tick_consensus_record_integrity(record)?;
            return Ok(Some(record.clone()));
        }
        if height > self.formal_archived_tick_consensus_count {
            return Ok(None);
        }
        let (view, reader) = self.attached_formal_history()?;
        let record = view
            .read_tick_consensus_record(height, reader)?
            .ok_or_else(|| invalid("qualified Tick prefix references missing archived record"))?;
        self.validate_tick_consensus_record_integrity(&record)?;
        Ok(Some(record))
    }
    pub fn read_tick_consensus_at_tick(
        &self,
        tick: WorldTime,
    ) -> Result<Option<TickConsensusRecord>, WorldError> {
        if tick > self.state.time {
            return Err(invalid("Tick lookup is beyond current World time"));
        }
        if let Some(record) = self
            .tick_consensus_records
            .iter()
            .find(|r| r.block.header.tick == tick)
        {
            self.validate_tick_consensus_record_integrity(record)?;
            return Ok(Some(record.clone()));
        }
        if self.formal_archived_tick_consensus_count == 0 {
            return Ok(None);
        }
        let (view, reader) = self.attached_formal_history()?;
        let Some(record) = view.read_tick_consensus_at_tick(tick, reader)? else {
            return Ok(None);
        };
        if record.certificate.consensus_height > self.formal_archived_tick_consensus_count {
            return Err(invalid(
                "archived Tick locator exceeds qualified prefix count",
            ));
        }
        self.validate_tick_consensus_record_integrity(&record)?;
        Ok(Some(record))
    }
    pub(super) fn formal_tick_consensus_boundary(
        &self,
    ) -> Result<Option<TickConsensusRecord>, WorldError> {
        if self.formal_archived_tick_consensus_count == 0 {
            return Ok(None);
        }
        self.read_tick_consensus_record(self.formal_archived_tick_consensus_count)
    }
    pub(super) fn validate_tick_consensus_record_integrity(
        &self,
        record: &TickConsensusRecord,
    ) -> Result<(), WorldError> {
        let source = record.certificate.authority_source.trim();
        if source.is_empty()
            || record.certificate.consensus_height == 0
            || record.certificate.block_hash != record.block.block_hash()
            || record.certificate.threshold == 0
            || record.certificate.threshold as usize > record.certificate.signatures.len()
        {
            return Err(invalid(
                "retained Tick certificate identity/hash/threshold mismatch",
            ));
        }
        let expected = format!(
            "sha256:{}",
            crate::util::sha256_hex(
                format!("tickcert:v1|{source}|{}", record.certificate.block_hash).as_bytes()
            )
        );
        if record.certificate.signatures.get(source) != Some(&expected) {
            return Err(invalid("retained Tick source audit signature mismatch"));
        }
        Ok(())
    }
    pub(super) fn prepare_formal_tick_consensus_archive(
        &self,
        parent: &VerifiedFormalRetainedHistoryParentV1,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<FormalTickConsensusArchivePlanV1, String> {
        if self.formal_archived_tick_consensus_count != parent.tick_consensus_archived_count() {
            return Err("Tick archive count differs from original qualified Snapshot".into());
        }
        let qualified = parent.tick_consensus_records();
        if self.tick_consensus_records.len() < qualified.len()
            || self.tick_consensus_records[..qualified.len()] != *qualified
        {
            return Err("candidate rewrote an original qualified Tick record".into());
        }
        self.verify_tick_consensus_chain()
            .map_err(|e| format!("{e:?}"))?;
        if self.formal_archived_tick_consensus_count > 0 {
            let boundary = parent
                .index_view()
                .read_tick_consensus_record(self.formal_archived_tick_consensus_count, reader)
                .map_err(|e| e.to_string())?
                .ok_or("qualified Tick archive boundary missing")?;
            self.validate_tick_consensus_record_integrity(&boundary)
                .map_err(|e| format!("{e:?}"))?;
        }
        // Approved legacy initial snapshots may contain a larger bounded
        // vector. Migrate at most one frame per candidate, then converge on32.
        let count = self
            .tick_consensus_records
            .len()
            .saturating_sub(HOT_FORMAL_TICK_RECORDS)
            .min(MAX_TICK_RECORDS_TO_ARCHIVE_PER_PLAN);
        if count > qualified.len() {
            return Err("cannot archive a Tick outside the original qualified tail".into());
        }
        let records_to_prune = self.tick_consensus_records[..count].to_vec();
        let mut immutable_leaves = Vec::new();
        for record in &records_to_prune {
            if parent
                .logical_tick()
                .is_some_and(|tick| record.block.header.tick > tick)
            {
                return Err("cannot archive an unqualified candidate Tick".into());
            }
            self.validate_tick_consensus_record_integrity(record)
                .map_err(|e| format!("{e:?}"))?;
            immutable_leaves.extend(retained_tick_consensus_leaves(record)?);
        }
        let proposed_archived_count = self
            .formal_archived_tick_consensus_count
            .checked_add(count as u64)
            .ok_or("Tick archive count overflow")?;
        Ok(FormalTickConsensusArchivePlanV1 {
            base_archived_count: self.formal_archived_tick_consensus_count,
            proposed_archived_count,
            records_to_prune,
            immutable_leaves,
        })
    }
    /// Called after the retention delta/view has been installed in the staged
    /// candidate. Every pruned certificate must be readable at that exact root.
    pub(super) fn apply_formal_tick_consensus_archive(
        &mut self,
        plan: &FormalTickConsensusArchivePlanV1,
    ) -> Result<(), String> {
        let count = plan.records_to_prune.len();
        if count > MAX_TICK_RECORDS_TO_ARCHIVE_PER_PLAN
            || count > self.tick_consensus_records.len()
            || self.formal_archived_tick_consensus_count != plan.base_archived_count
            || plan.base_archived_count.checked_add(count as u64)
                != Some(plan.proposed_archived_count)
            || self.tick_consensus_records[..count] != *plan.records_to_prune
        {
            return Err("Tick archive plan differs from exact staged prefix".into());
        }
        let (view, reader) = self.attached_formal_history().map_err(|e| e.to_string())?;
        for expected in &plan.records_to_prune {
            if view
                .read_tick_consensus_record(expected.certificate.consensus_height, reader)
                .map_err(|e| e.to_string())?
                .as_ref()
                != Some(expected)
                || view
                    .read_tick_consensus_at_tick(expected.block.header.tick, reader)
                    .map_err(|e| e.to_string())?
                    .as_ref()
                    != Some(expected)
            {
                return Err("Tick archive delta lost or rewrote a pruned certificate".into());
            }
        }
        self.tick_consensus_records.drain(..count);
        self.formal_archived_tick_consensus_count = plan.proposed_archived_count;
        self.verify_tick_consensus_chain()
            .map_err(|e| format!("{e:?}"))
    }
}

impl World {
    pub(super) fn consensus_height_for_tick(&self, tick: WorldTime) -> Result<u64, WorldError> {
        if let Some(record) = self.read_tick_consensus_at_tick(tick)? {
            return Ok(record.certificate.consensus_height);
        }
        self.tick_consensus_record_count()
            .checked_add(1)
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: "Tick height overflow".into(),
            })
    }
    pub(super) fn parent_hash_for_tick(&self, tick: WorldTime) -> Result<String, WorldError> {
        if let Some(record) = self
            .tick_consensus_records
            .iter()
            .rev()
            .find(|r| r.block.header.tick < tick)
        {
            return Ok(record.certificate.block_hash.clone());
        }
        if let Some(boundary) = self.formal_tick_consensus_boundary()? {
            if boundary.block.header.tick >= tick {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "cannot append/rewrite an archived Tick".into(),
                });
            }
            return Ok(boundary.certificate.block_hash);
        }
        Ok(super::tick_consensus::TICK_CONSENSUS_GENESIS_PARENT_HASH.to_string())
    }
}
