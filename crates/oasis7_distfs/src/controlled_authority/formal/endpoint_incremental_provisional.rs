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
    ledger: Option<oasis7_world_state::incremental_cognition_proof_ledger::PreparedCognitionProofLedgerPromotionV1>,
    // The actual checked owner remains available to the bounded preaccept
    // consumer. Finish still promotes the separately prepared owner only after
    // the genuine Formal commit, then releases this original checked source.
    checked_ledger: Option<ReservedValue<oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1>>,
    retention: Option<oasis7_world_state::incremental_cognition_retention_ledger::PreparedCognitionRetentionLedgerPromotionV1>,
    complete_data: Option<ReservedValue<super::closure::CompleteDataClosureEvidenceV1>>,
    local_closure: Option<super::local_closure::PreparedLocalDataClosureV1>,
}
#[allow(
    clippy::large_enum_variant,
    reason = "Candidate ownership stays inline in the existing admitted cache; boxing would add a separate uncharged allocation."
)]
pub(super) enum AcceptedCurrentOwner {
    Candidate(ProvisionalCandidate),
    AuthenticatedCold(super::cold_install::AuthenticatedColdCandidateV1),
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
    pub(super) ledger: Option<
        ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
        >,
    >,
    pub(super) retention: Option<ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>>,
    pub(super) complete_data: Option<ReservedValue<super::closure::CompleteDataClosureEvidenceV1>>,
    pub(super) local_closure: Option<super::local_closure::PreparedLocalDataClosureV1>,
    pub(super) genesis_quorum:
        Option<oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedGenesisCommitV1>,
    pub(super) normal_quorum:
        Option<oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedNormalCommitV1>,
    pub(super) formal_parent_fee: Option<ReservedValue<()>>,
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
        ledger: Option<ReservedValue<oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1>>,
        retention: Option<ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::CheckedCognitionRetentionLedgerV1>>,
        memory: &MemoryBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Self, super::FormalError> {
        if provisional.source.results().proof_ledger().is_some() != ledger.is_some() {
            return Err(super::invalid("IWS complete routed ProofLedger selection"));
        }
        if let Some(value) = ledger.as_ref() {
            let occurrence = value.occurrence();
            let routed = provisional
                .source
                .results()
                .proof_ledger()
                .ok_or_else(|| super::invalid("IWS selected ProofLedger missing"))?
                .occurrence();
            if !value.uses_budget(memory)
                || occurrence.candidate_record_id() != provisional.core.candidate_record_id()
                || occurrence.body_id() != routed.body_id()
                || occurrence.roots_id() != provisional.core.body().roots
                || occurrence.location() != routed.location()
            {
                return Err(super::invalid("IWS ProofLedger exact candidate occurrence"));
            }
        }
        let prepared_ledger=ledger.as_ref().map(|value|oasis7_world_state::incremental_cognition_proof_ledger::prepare_cognition_proof_ledger_promotion_v1(value,memory,&mut cancelled).map_err(|e|super::invalid(format!("IWS prepare ProofLedger: {e:?}")))).transpose()?;
        if provisional.core.retention_ledger_artifact_id().is_some() != retention.is_some() {
            return Err(super::invalid("IWS signed retention role selection"));
        }
        if let Some(value) = retention.as_ref() {
            if !value.uses_budget(memory) {
                return Err(super::invalid("IWS retention promotion pool"));
            }
            value
                .validate_core_v1(&provisional.core)
                .map_err(|e| super::invalid(format!("IWS retention exact candidate: {e:?}")))?;
        }
        let retention = retention.as_ref().map(|value|
            oasis7_world_state::incremental_cognition_retention_ledger::prepare_cognition_retention_ledger_promotion_v1(value,memory,&mut cancelled)
                .map_err(|e|super::invalid(format!("IWS prepare retention: {e:?}")))
        ).transpose()?;
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
            ledger: prepared_ledger,
            checked_ledger: ledger,
            retention,
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
        let ledger=self.ledger.map(|value|oasis7_world_state::incremental_cognition_proof_ledger::finish_prepared_cognition_proof_ledger_promotion_v1(value,commit).map_err(|e|super::invalid(format!("IWS install ProofLedger: {e:?}")))).transpose()?;
        let retention = self.retention.map(|value|
            oasis7_world_state::incremental_cognition_retention_ledger::finish_prepared_cognition_retention_ledger_promotion_v1(value,commit)
                .map_err(|e|super::invalid(format!("IWS install retention: {e:?}")))
        ).transpose()?;
        Ok(AcceptedCandidateCache {
            commit_record: commit.verified_record_id(),
            current_owner: AcceptedCurrentOwner::Candidate(self.provisional),
            roots: self.roots,
            ledger,
            retention,
            cognition,
            runtime,
            complete_data: self.complete_data,
            local_closure: self.local_closure,
            recovered_pages: None,
            genesis_quorum: None,
            normal_quorum: None,
            formal_parent_fee: None,
        })
    }
}

impl AcceptedCandidateCache {
    pub(super) fn accepted_cold_results(&self) -> Option<&ReservedValue<
    oasis7_world_state::incremental_world_result_router_accepted::AcceptedWorldResultRoutingV1>>{
        match &self.current_owner {
            AcceptedCurrentOwner::Cold { results, .. } => Some(results),
            AcceptedCurrentOwner::AuthenticatedCold(p) => Some(p.source().results()),
            AcceptedCurrentOwner::Candidate(_) => None,
        }
    }
    pub(super) fn authenticated_cold(
        &self,
    ) -> Option<&super::cold_install::AuthenticatedColdCandidateV1> {
        match &self.current_owner {
            AcceptedCurrentOwner::AuthenticatedCold(p) => Some(p),
            _ => None,
        }
    }
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
            AcceptedCurrentOwner::AuthenticatedCold(p) => p.source().current(),
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
        if let Some(results) = self.accepted_cold_results()
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
        if let Some(retention) = self.retention.as_ref()
            && (!retention.uses_budget(memory)
                || retention.allocation_class()
                    != oasis7_proto::memory_budget::MemoryClass::Ordinary
                || retention.head_record_id() != self.commit_record
                || retention.head_body_id() != self.runtime.occurrence().head_body_id()
                || retention.manifest().roots != body.roots)
        {
            return Err(ValueTreeError::Invalid(
                "IWS accepted retention exact parent",
            ));
        }
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

#[path = "endpoint_incremental_frozen_parent_cache.rs"]
mod frozen_parent_cache;

impl PreparedCandidateInstall {
    pub(super) fn genesis_reference<'a>(
        &'a self,
        commit: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        memory: &MemoryBudget,
    ) -> Result<&'a str, super::FormalError> {
        let core = &self.provisional.core;
        if !core.uses_budget(memory)
            || core.body().height != 0
            || core.transition().is_some()
            || core.parent_record_id().is_some()
            || core.body().parent_commit.is_some()
            || self.candidate_record_id() != commit.verified_record_id()
        {
            return Err(super::invalid("GEN requires exact complete genesis source"));
        }
        let data = self
            .complete_data
            .as_ref()
            .ok_or_else(|| super::invalid("GEN complete data evidence missing"))?;
        if !data.uses_budget(memory)
            || data.candidate_record_id() != commit.verified_record_id()
            || data.body_id() != self.provisional.source.runtime().occurrence().body_id()
            || data.roots_id() != self.provisional.source.runtime().occurrence().roots_id()
        {
            return Err(super::invalid("GEN complete data source binding"));
        }
        self.local_closure
            .as_ref()
            .ok_or_else(|| super::invalid("GEN genuine local closure missing"))?
            .reference_for(commit)
    }
}

impl PreparedCandidateInstall {
    pub(super) fn genesis_receipt_binding(
        &self,
        record: [u8; 32],
        memory: &MemoryBudget,
    ) -> Result<(), super::FormalError> {
        let core = &self.provisional.core;
        if !core.uses_budget(memory)
            || core.body().height != 0
            || core.transition().is_some()
            || core.parent_record_id().is_some()
            || core.body().parent_commit.is_some()
            || self.candidate_record_id() != record
        {
            return Err(super::invalid("GEN receipt exact complete source"));
        }
        let data = self
            .complete_data
            .as_ref()
            .ok_or_else(|| super::invalid("GEN receipt complete data missing"))?;
        let local = self
            .local_closure
            .as_ref()
            .ok_or_else(|| super::invalid("GEN receipt local closure missing"))?;
        if !data.uses_budget(memory)
            || data.candidate_record_id() != record
            || local.candidate_record_id() != record
            || data.body_id() != self.provisional.source.runtime().occurrence().body_id()
            || data.roots_id() != self.provisional.source.runtime().occurrence().roots_id()
        {
            return Err(super::invalid("GEN receipt complete source binding"));
        }
        Ok(())
    }
}

impl PreparedCandidateInstall {
    pub(super) fn normal_reference<'a>(
        &'a self,
        commit: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        parent: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        memory: &MemoryBudget,
    ) -> Result<&'a str, super::FormalError> {
        let core = &self.provisional.core;
        if !core.uses_budget(memory)
            || core.body().height == 0
            || core.transition().is_none()
            || core.parent_record_id() != Some(parent.verified_record_id())
            || core.body().parent_commit.is_none()
            || self.candidate_record_id() != commit.verified_record_id()
        {
            return Err(super::invalid("normal exact complete staged child/parent"));
        }
        let data = self
            .complete_data
            .as_ref()
            .ok_or_else(|| super::invalid("normal complete data missing"))?;
        if !data.uses_budget(memory)
            || data.candidate_record_id() != commit.verified_record_id()
            || data.body_id() != self.provisional.source.runtime().occurrence().body_id()
            || data.roots_id() != self.provisional.source.runtime().occurrence().roots_id()
        {
            return Err(super::invalid("normal complete source binding"));
        }
        self.local_closure
            .as_ref()
            .ok_or_else(|| super::invalid("normal genuine local closure missing"))?
            .reference_for(commit)
    }
}

impl PreparedCandidateInstall {
    pub(super) fn provisional_parent_record(&self) -> Option<[u8; 32]> {
        self.provisional.core.parent_record_id()
    }
}

impl PreparedCandidateInstall {
    pub(super) fn normal_candidate_read_sources_v1(
        &self,
    ) -> super::jobs::NormalCandidateReadSources<'_> {
        (
            &self.provisional.core,
            &self.provisional.source,
            &self.roots,
            self.checked_ledger.as_ref(),
        )
    }
}
