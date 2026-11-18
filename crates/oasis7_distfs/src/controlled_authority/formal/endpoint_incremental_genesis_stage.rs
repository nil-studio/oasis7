//! Explicit GEN source staging. This reuses the established source-validation
//! budget domain; it is not a CommonCore paid owner, receipt or serving permit.
//! The bounded entry additionally charges catalog verification to the borrowed
//! Common. Other source-validation lanes retain their existing finite budgets.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{
        FormalBoundedChargeErrorV1, FormalBoundedContextV1, FormalBoundedWorkV1,
    },
    incremental_execution::ExecutionError,
};
impl FileFormalEndpoint {
    pub fn stage_genesis_source(
        &self,
        candidate: &FormalCandidateMaterialV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<VerifiedCandidateContextV1, FormalError> {
        self.stage_genesis_source_inner(candidate, None, None, &mut cancelled)
    }

    /// Validate and stage the same genuine GEN source while catalog metadata,
    /// point lookup and streaming traversal debit the caller's original Common.
    /// This borrows the existing pool and cancellation; it creates no budget or
    /// receipt/serving qualification and cannot run inside another Common child.
    pub fn stage_genesis_source_bounded(
        &self,
        candidate: &FormalCandidateMaterialV1,
        context: &mut FormalBoundedContextV1<'_>,
    ) -> Result<VerifiedCandidateContextV1, FormalError> {
        let (memory, common, stop) = context.parts();
        let stop = std::cell::RefCell::new(stop);
        let mut first = None;
        let mut charge = |steps, bytes| {
            common
                .charge(steps, bytes, &mut || (*stop.borrow_mut())())
                .map_err(|error| {
                    first.get_or_insert(error);
                    ExecutionError::ResourceLimited("execution catalog Common work")
                })
        };
        let result = self.stage_genesis_source_inner(
            candidate,
            Some((memory, common)),
            Some(&mut charge),
            &mut || (*stop.borrow_mut())(),
        );
        match first {
            Some(error) => Err(error),
            None => result,
        }
    }

    fn stage_genesis_source_inner(
        &self,
        candidate: &FormalCandidateMaterialV1,
        common: Option<(&MemoryBudget, &FormalBoundedWorkV1)>,
        catalog_work: Option<&mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<VerifiedCandidateContextV1, FormalError> {
        if cancelled() {
            return Err(invalid("GEN source cancelled before verification"));
        }
        if self.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION || self.parent.is_some() || self.recovery.is_some() || !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_initial_record_schema(candidate.candidate.record.schema_version) || candidate.candidate.record.kind!=FormalRecordKindV1::InitialActivation || candidate.branch!=FormalCompletionBranchV1::Normal || candidate.frozen.is_some() || candidate.frozen_material.is_some(){return Err(invalid("GEN source requires fresh initial IWS authority/candidate"));}
        if let Some((memory, common)) = common {
            let capability = self
                .iws_page_store
                .as_ref()
                .ok_or_else(|| invalid("GEN source page capability missing"))?;
            if !capability.pages.memory_budget().same_pool(memory) {
                return Err(invalid("GEN source Common memory pool"));
            }
            if common.is_child_active() {
                return Err(FormalBoundedChargeErrorV1::ChildReadActive.into());
            }
        }
        // Original ClosedRecord validation, writer signature/context and the
        // full CompleteEvidence/localwriter/jobs.stage pipeline remain intact.
        let context =
            self.verify_iws_candidate_with_artifact_work(candidate, catalog_work, cancelled)?;
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
        if self.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION || !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_initial_record_schema(candidate.candidate.record.schema_version) || candidate.candidate.record.kind!=FormalRecordKindV1::InitialActivation || candidate.branch!=FormalCompletionBranchV1::Normal || candidate.frozen.is_some() || candidate.frozen_material.is_some(){return Err(invalid("GEN retained proof requires initial IWS request"));}
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
