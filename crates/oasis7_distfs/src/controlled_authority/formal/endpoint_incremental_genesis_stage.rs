//! Explicit GEN source staging. This reuses the established source-validation
//! budget domain; it is not a CommonCore paid owner, receipt or serving permit.
use super::*;
impl FileFormalEndpoint {
    pub fn stage_genesis_source(
        &self,
        candidate: &FormalCandidateMaterialV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<VerifiedCandidateContextV1, FormalError> {
        if cancelled() {
            return Err(invalid("GEN source cancelled before verification"));
        }
        if self.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION || self.parent.is_some() || self.recovery.is_some() || candidate.candidate.record.schema_version!=7 || candidate.candidate.record.kind!=FormalRecordKindV1::InitialActivation || candidate.branch!=FormalCompletionBranchV1::Normal || candidate.frozen.is_some() || candidate.frozen_material.is_some(){return Err(invalid("GEN source requires fresh initial IWS authority/candidate"));}
        // Original ClosedRecord validation, writer signature/context and the
        // full CompleteEvidence/localwriter/jobs.stage pipeline remain intact.
        let context = self.verify_iws_candidate(candidate, &mut cancelled)?;
        if cancelled() {
            return Err(invalid("GEN source cancelled after verification"));
        }
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("GEN source page capability missing"))?;
        let id = context.record().record_id()?;
        let mut fixed = [0u8; 32];
        hex::decode_to_slice(id, &mut fixed).map_err(|_| invalid("GEN source record ID"))?;
        capability
            .jobs
            .genesis_job()?
            .receipt_binding(fixed, capability.pages.memory_budget())?;
        Ok(context)
    }
    /// Read the authenticated full proof from a retained Qualified record before
    /// fresh GEN source admission. This is public quorum-proof data, not local
    /// CompleteEvidence, namespace continuity or a serving permit. In particular
    /// it does not certify the local data_closure field or promote any cache.
    pub fn read_genesis_qualified_bounded(
        &self,
        candidate: &FormalCandidateMaterialV1,
        context: &VerifiedCandidateContextV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Option<PaidVerifiedGenesisCommitV1>, FormalError> {
        work.consume_steps(1, stop)?;
        if self.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION || candidate.candidate.record.schema_version!=7 || candidate.candidate.record.kind!=FormalRecordKindV1::InitialActivation || candidate.branch!=FormalCompletionBranchV1::Normal || candidate.frozen.is_some() || candidate.frozen_material.is_some(){return Err(invalid("GEN retained proof requires initial IWS request"));}
        if !same(
            context.finalizing_authority().definition(),
            self.authority.definition(),
            memory,
            work,
            stop,
        )? || !same(
            &context.finalizing_authority().activation_ref(),
            &self.authority.activation_ref(),
            memory,
            work,
            stop,
        )? {
            return Err(FormalError::FencingUnproven);
        }
        let Some(state) = self.store.read_request_bounded(
            &candidate.candidate.record.request_digest,
            memory,
            work,
            stop,
        )?
        else {
            return Ok(None);
        };
        let FormalRequestStateV1::Qualified {
            candidate: old,
            proof,
            data_closure,
        } = state.state()
        else {
            return Ok(None);
        };
        work.consume_steps(1, stop)?;
        let reference = data_closure
            .as_deref()
            .ok_or_else(|| invalid("GEN retained Qualified closure identifier missing"))?;
        work.consume_read_bytes(reference.len() as u64, stop)?;
        hash_hex(reference)?;
        if !same(old, candidate, memory, work, stop)? {
            return Err(FormalError::Conflict(
                "GEN retained exact material/candidate".into(),
            ));
        }
        let mut ctx = work.bounded_context(memory, stop);
        Ok(Some(verify_genesis_qualified_bounded(
            proof,
            &candidate.candidate,
            context,
            &mut ctx,
        )?))
    }
}

#[cfg(test)]
#[path = "endpoint_incremental_genesis_stage_tests.rs"]
mod tests;
