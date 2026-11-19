//! Borrow the real accepted owner for a checked Frozen parent, without a future core.
//! This does not promote a staged candidate or supply endpoint live authority.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::CheckedIncrementalFrozenMaterialV1;
use oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactRole;
use oasis7_proto::memory_budget::MemoryClass;
impl AcceptedCandidateCache {
    pub(in super::super) fn frozen_parent_material<'a>(
        &'a self,
        checked: &ReservedValue<CheckedIncrementalFrozenMaterialV1<'_>>,
        memory: &MemoryBudget,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<
        &'a ReservedValue<super::super::closure::CandidateExecutionMaterialV1>,
        super::super::FormalError,
    > {
        use super::super::invalid;
        if cancelled() {
            return Err(invalid("IWS Frozen accepted parent cancelled"));
        }
        let _phase = memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(|_| invalid("IWS Frozen accepted parent admission"))?;
        let occurrence = self.runtime.occurrence();
        if !checked.uses_budget(memory)
            || checked.reserved_bytes() < std::mem::size_of_val(&**checked)
            || checked.parent_record_id() != self.commit_record
            || !self.current().uses_budget(memory)
            || self.current().allocation_class() != MemoryClass::Ordinary
            || !self.roots.uses_budget(memory)
            || !self.runtime.uses_budget(memory)
            || self.runtime.allocation_class() != MemoryClass::Ordinary
            || !occurrence.uses_budget(memory)
            || occurrence.allocation_class() != MemoryClass::Ordinary
            || occurrence.head_record_id() != self.commit_record
        {
            return Err(invalid("IWS Frozen exact accepted parent/pool/class"));
        }
        self.current()
            .validate_binding(&self.roots)
            .map_err(|_| invalid("IWS Frozen accepted Current binding"))?;
        let roots = self
            .roots
            .digest()
            .map_err(|_| invalid("IWS Frozen accepted RootSet digest"))?;
        let mut encoded_roots = [0; 64];
        hex::encode_to_slice(roots, &mut encoded_roots)
            .map_err(|_| invalid("IWS Frozen accepted RootSet encoding"))?;
        let mut encoded_body = [0; 64];
        hex::encode_to_slice(occurrence.head_body_id(), &mut encoded_body)
            .map_err(|_| invalid("IWS Frozen accepted body encoding"))?;
        let parent = checked.parent();
        if occurrence.roots_id() != roots
            || parent.verified_record_id() != self.commit_record
            || parent.record().after_state_root.as_bytes() != encoded_roots
            || parent
                .record()
                .artifact_roots
                .get(&ArtifactRole::IncrementalCommitBody)
                .map(|id| id.as_bytes())
                != Some(encoded_body.as_slice())
        {
            return Err(invalid("IWS Frozen accepted record/body/roots"));
        }
        let local = self
            .local_closure
            .as_ref()
            .ok_or_else(|| invalid("IWS Frozen accepted local closure missing"))?;
        local.reference_for(parent)?;
        let data = self
            .complete_data
            .as_ref()
            .ok_or_else(|| invalid("IWS Frozen accepted static material requires readmission"))?;
        if !data.uses_budget(memory)
            || data.candidate_record_id() != self.commit_record
            || data.body_id() != occurrence.head_body_id()
            || data.roots_id() != roots
        {
            return Err(invalid("IWS Frozen accepted complete-data binding"));
        }
        if cancelled() {
            return Err(invalid("IWS Frozen accepted parent final cancellation"));
        }
        Ok(data.material())
    }
}
