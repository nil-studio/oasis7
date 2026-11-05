//! Endpoint-private provisional conjunction. This is never a receipt permit.
use oasis7_formal_protocol::{
    incremental_material::CheckedCommitCore, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::CheckedCognitionHistorySourceV3,
    incremental_runtime_history_source::CheckedRuntimeHistorySourceV1,
    incremental_world_result_router::CheckedWorldResultRoutingV1,
    incremental_world_storage_source::{
        CheckedWorldStorageSourceV1, qualify_checked_world_storage_source_v1,
    },
};
/// Own all logical evidence and its reservations through candidate persistence.
/// Physical sync and actual formal acceptance remain independent requirements.
pub(super) struct ProvisionalCandidate {
    pub(super) core: ReservedValue<CheckedCommitCore>,
    pub(super) source: ReservedValue<CheckedWorldStorageSourceV1>,
}
impl ProvisionalCandidate {
    pub(super) fn checked(
        core: ReservedValue<CheckedCommitCore>,
        current: ReservedValue<oasis7_world_state::incremental_world_current_complete::WorldCurrentStateQualificationV1>,
        cognition: ReservedValue<CheckedCognitionHistorySourceV3>,
        runtime: ReservedValue<CheckedRuntimeHistorySourceV1>,
        results: ReservedValue<CheckedWorldResultRoutingV1>,
        memory: &MemoryBudget,
        cancelled: impl FnMut() -> bool,
    ) -> Result<Self, ValueTreeError> {
        let source = qualify_checked_world_storage_source_v1(
            &core, current, cognition, runtime, results, memory, cancelled,
        )?;
        Ok(Self { core, source })
    }
}

use oasis7_world_state::{
    incremental_cognition_history::{
        CompleteCognitionHistorySourceV3, PreparedCompleteCognitionHistoryPromotionV3,
        finish_prepared_complete_cognition_history_promotion_v3,
        prepare_complete_cognition_history_promotion_v3,
    },
    incremental_runtime_history_index::RuntimeHistoryIndexQualificationV1,
    incremental_runtime_history_source::{
        AcceptedRuntimeHistorySourceV1, PreparedRuntimeHistorySourceV1,
        finish_prepared_runtime_history_source_promotion_v1,
        prepare_runtime_history_source_promotion_v1,
    },
};
/// All fallible output admission occurs before Formal accepts the head.
pub(super) struct PreparedCandidateInstall {
    provisional: ProvisionalCandidate,
    roots: ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1>,
    cognition: PreparedCompleteCognitionHistoryPromotionV3,
    runtime: PreparedRuntimeHistorySourceV1,
    complete_data: Option<ReservedValue<super::closure::CompleteDataClosureEvidenceV1>>,
    local_closure: Option<super::local_closure::PreparedLocalDataClosureV1>,
}
#[allow(
    clippy::large_enum_variant,
    reason = "Candidate ownership stays inline in the existing admitted cache; boxing would add a separate uncharged allocation."
)]
pub(super) enum AcceptedCurrentOwner {
    Candidate(ProvisionalCandidate),
    Cold {
        current: ReservedValue<oasis7_world_state::incremental_world_current_complete::WorldCurrentStateQualificationV1>,
        results: ReservedValue<oasis7_world_state::incremental_world_result_router_accepted::AcceptedWorldResultRoutingV1>,
    },
}
pub(super) struct AcceptedCandidateCache {
    pub(super) commit_record: [u8; 32],
    pub(super) current_owner: AcceptedCurrentOwner,
    pub(super) roots: ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1>,
    pub(super) cognition: ReservedValue<CompleteCognitionHistorySourceV3>,
    pub(super) runtime: ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub(super) complete_data: Option<ReservedValue<super::closure::CompleteDataClosureEvidenceV1>>,
    pub(super) local_closure: Option<super::local_closure::PreparedLocalDataClosureV1>,
    pub(super) recovered_pages: Option<ReservedValue<super::closure::StoreReadWitness>>,
}
impl PreparedCandidateInstall {
    pub(super) fn local_data_reference(
        &self,
        commit: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    ) -> Result<Option<String>, super::FormalError> {
        self.local_closure
            .as_ref()
            .map(|local| local.reference_for(commit).map(str::to_owned))
            .transpose()
    }
    pub(super) fn candidate_record_id(&self) -> [u8; 32] {
        self.provisional.core.candidate_record_id()
    }

    pub(super) fn prepare(
        provisional: ProvisionalCandidate,
        indexes: [ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, super::FormalError> {
        let cognition = prepare_complete_cognition_history_promotion_v3(
            provisional.source.cognition(),
            memory,
            &mut cancelled,
        )
        .map_err(|e| super::invalid(format!("IWS prepare Cognition: {e}")))?;
        let runtime = prepare_runtime_history_source_promotion_v1(
            provisional.source.runtime(),
            indexes,
            memory,
            &mut cancelled,
        )
        .map_err(|e| super::invalid(format!("IWS prepare Runtime History: {e:?}")))?;
        let roots = provisional.core.roots();
        let bytes = roots.state.iter().try_fold(
            std::mem::size_of::<oasis7_formal_protocol::incremental_execution::ExecutionParent>()
                .checked_add(512)
                .and_then(|n| n.checked_add(roots.world_id.len()))
                .ok_or_else(|| super::invalid("IWS parent root accounting"))?,
            |n, r| {
                n.checked_add(r.node_root.len())
                    .and_then(|n| n.checked_add(128))
                    .ok_or_else(|| super::invalid("IWS parent root accounting"))
            },
        )?;
        let parent_charge = memory
            .reserve(bytes, oasis7_proto::memory_budget::MemoryClass::Ordinary)
            .map_err(|_| super::invalid("IWS parent root admission"))?;
        let roots = parent_charge.retain(roots.clone());
        Ok(Self {
            provisional,
            roots,
            cognition,
            runtime,
            complete_data: None,
            local_closure: None,
        })
    }
    /// Only the private full consumer can supply this opaque byte closure.
    pub(super) fn with_complete_data(
        mut self,
        data: ReservedValue<super::closure::CompleteDataClosureEvidenceV1>,
        local: super::local_closure::PreparedLocalDataClosureV1,
    ) -> Result<Self, super::FormalError> {
        if local.candidate_record_id() != data.candidate_record_id()
            || data.candidate_record_id() != self.provisional.core.candidate_record_id()
            || data.body_id() != self.provisional.source.runtime().occurrence().body_id()
            || data.roots_id() != self.provisional.source.runtime().occurrence().roots_id()
        {
            return Err(super::invalid("IWS prepared complete data binding"));
        }
        self.complete_data = Some(data);
        self.local_closure = Some(local);
        Ok(self)
    }
    /// Consumes an already genuinely verified commit; no output admission or IO.
    pub(super) fn finish(
        self,
        commit: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    ) -> Result<AcceptedCandidateCache, super::FormalError> {
        let cognition =
            finish_prepared_complete_cognition_history_promotion_v3(self.cognition, commit)
                .map_err(|_| super::invalid("IWS install Cognition binding"))?;
        let runtime = finish_prepared_runtime_history_source_promotion_v1(self.runtime, commit)
            .map_err(|_| super::invalid("IWS install Runtime History binding"))?;
        Ok(AcceptedCandidateCache {
            commit_record: commit.verified_record_id(),
            current_owner: AcceptedCurrentOwner::Candidate(self.provisional),
            roots: self.roots,
            cognition,
            runtime,
            complete_data: self.complete_data,
            local_closure: self.local_closure,
            recovered_pages: None,
        })
    }
}

impl AcceptedCandidateCache {
    pub(super) fn page_witness(&self) -> Option<&ReservedValue<super::closure::StoreReadWitness>> {
        self.complete_data
            .as_ref()
            .map(|data| data.pages())
            .or(self.recovered_pages.as_ref())
    }

    pub(super) fn complete_data(
        &self,
    ) -> Option<&ReservedValue<super::closure::CompleteDataClosureEvidenceV1>> {
        self.complete_data.as_ref()
    }

    pub(super) fn current(
        &self,
    ) -> &ReservedValue<
        oasis7_world_state::incremental_world_current_complete::WorldCurrentStateQualificationV1,
    > {
        match &self.current_owner {
            AcceptedCurrentOwner::Candidate(p) => p.source.current(),
            AcceptedCurrentOwner::Cold { current, .. } => current,
        }
    }

    /// Exact private accepted parent admission before any ordinary replay IO.
    pub(super) fn check_parent(
        &self,
        next: &ReservedValue<CheckedCommitCore>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if self
            .local_closure
            .as_ref()
            .is_some_and(|local| local.candidate_record_id() != self.commit_record)
        {
            return Err(ValueTreeError::Invalid(
                "IWS accepted local data record binding",
            ));
        }
        if !self.current().uses_budget(memory)
            || !self.roots.uses_budget(memory)
            || !next.uses_budget(memory)
            || next.parent_record_id() != Some(self.commit_record)
            || next.parent_roots() != Some(&*self.roots)
        {
            return Err(ValueTreeError::Invalid("IWS private exact accepted parent"));
        }
        if let AcceptedCurrentOwner::Cold { results, .. } = &self.current_owner
            && (!results.uses_budget(memory)
                || results.allocation_class() != oasis7_proto::memory_budget::MemoryClass::Ordinary
                || results.record_id() != self.commit_record
                || results.body_id() != self.runtime.occurrence().head_body_id())
        {
            return Err(ValueTreeError::Invalid("IWS accepted cold Result binding"));
        }
        let _phase = memory.reserve(8192, oasis7_proto::memory_budget::MemoryClass::Ordinary)?;
        let body = next
            .parent_body()
            .ok_or(ValueTreeError::Invalid("IWS accepted parent body missing"))?;
        if body
            .digest()
            .map_err(|_| ValueTreeError::Invalid("IWS parent body digest"))?
            != self.runtime.occurrence().head_body_id()
        {
            return Err(ValueTreeError::Invalid("IWS exact accepted parent body"));
        }
        Ok(())
    }
}
