//! One fixed Bootstrap batch. State, root and WorldEvent publication happen in
//! order in the original transaction. The batch is never an accepted commit.
use super::*;
use crate::world::{
    incremental_module_invocation::*,
    incremental_normal_bootstrap_input::BoundCurrentProviderBootstrapInputV1,
    incremental_normal_sources::BoundNormalTickParentV1, incremental_step::IwsStepContext,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use oasis7_world_state::{
    incremental_cognition_history::WrittenCurrentCognitionJournalV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
};

/// Only the complete successful fixed batch can construct this owner. It
/// retains the exact reserved input row, all actual authorization publications,
/// and both actual Economy frontiers through the normal candidate capture.
pub(in crate::world) struct CommittedCurrentBootstrapBatchV1<'a, 'f> {
    input: BoundCurrentProviderBootstrapInputV1<'a, 'f>,
    authorization: Vec<CommittedCurrentBootstrapAuthorizationV1>,
    frontiers: std::sync::Arc<ReservedValue<BootstrapEconomyFrontiersV1>>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _slots: MemoryReservation,
    _fee: MemoryReservation,
}
impl CommittedCurrentBootstrapBatchV1<'_, '_> {
    pub(in crate::world) fn validate_enqueued_source_v1<M: MerkleNodeReader>(
        &self,
        input: &PaidBoundNormalInputDataV1<'_>,
        index: usize,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<bool, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self._slots.uses_budget(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "Bootstrap completed batch enqueue binding",
            ));
        }
        self.input.matches_input_index_v1(input, index, common)
    }
    pub(in crate::world) fn frontier_source(
        &self,
    ) -> &std::sync::Arc<ReservedValue<BootstrapEconomyFrontiersV1>> {
        &self.frontiers
    }
}
/// Fixed retained dual-frontier DATA. Real Current/final source qualification
/// still comes from the original final-capture pipeline.
pub(in crate::world) struct BootstrapEconomyFrontiersV1 {
    journal:
        oasis7_world_state::incremental_cognition_history::RetainedWrittenCurrentCognitionJournalV1,
    provision:
        oasis7_world_state::incremental_cognition_history::RetainedWrittenCurrentCognitionJournalV1,
}
impl BootstrapEconomyFrontiersV1 {
    pub(in crate::world) fn journal(
        &self,
    ) -> &oasis7_world_state::incremental_cognition_history::RetainedWrittenCurrentCognitionJournalV1
    {
        &self.journal
    }
    pub(in crate::world) fn provision(
        &self,
    ) -> &oasis7_world_state::incremental_cognition_history::RetainedWrittenCurrentCognitionJournalV1
    {
        &self.provision
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), ValueTreeError> {
        use oasis7_world_state::incremental_cognition_history::CognitionHistoryFieldV1 as Field;
        if self.journal.field() != Field::EconomyJournal
            || self.provision.field() != Field::EconomyProvisionJournal
        {
            return Err(ValueTreeError::Invalid("Bootstrap fixed Economy frontiers"));
        }
        self.journal.validate_evidence_v1(tx, memory, live)?;
        self.provision.validate_evidence_v1(tx, memory, live)
    }
}
fn retain_economy_frontiers_v1<M: MerkleNodeReader>(
    journal: WrittenCurrentCognitionJournalV1<'_>,
    provision: WrittenCurrentCognitionJournalV1<'_>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<std::sync::Arc<ReservedValue<BootstrapEconomyFrontiersV1>>, BodyMutationError> {
    use oasis7_world_state::incremental_cognition_history::CognitionHistoryFieldV1 as Field;
    if journal.field() != Field::EconomyJournal
        || provision.field() != Field::EconomyProvisionJournal
    {
        return Err(BodyMutationError::Invalid(
            "Bootstrap fixed dual frontier source",
        ));
    }
    let bytes = std::mem::size_of::<ReservedValue<BootstrapEconomyFrontiersV1>>()
        .checked_add(2 * std::mem::size_of::<usize>())
        .and_then(|n| {
            n.checked_add(std::mem::align_of::<
                ReservedValue<BootstrapEconomyFrontiersV1>,
            >())
        })
        .ok_or(BodyMutationError::Invalid("Bootstrap frontier Arc layout"))?;
    common
        .charge(1, bytes as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let journal = journal.retain_owned_v1(tx, common.memory(), live)?;
    let provision = provision.retain_owned_v1(tx, common.memory(), live)?;
    Ok(std::sync::Arc::new(fee.retain(
        BootstrapEconomyFrontiersV1 { journal, provision },
    )))
}

fn source_error(
    error: crate::world::incremental_authorization_root_current::WarmError,
) -> BodyMutationError {
    use crate::world::incremental_authorization_root_current::{SourceError, WarmError};
    match error {
        WarmError::Body(e) => e,
        WarmError::Source(SourceError::Execution(e)) => e.into(),
        WarmError::Source(SourceError::Overlay(e)) => overlay_error(e),
        WarmError::Source(other) => {
            #[derive(Debug)]
            struct Cause(SourceError);
            impl std::fmt::Display for Cause {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self.0)
                }
            }
            impl std::error::Error for Cause {}
            BodyMutationError::Original(Box::new(Cause(other)))
        }
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// This method has no raw event/permission input. It consumes a private
    /// Current business plan, its real storage receipt and the real root CAS,
    /// then uses only Root's fixed Bootstrap original/fact publication gate.
    #[allow(clippy::too_many_arguments)]
    fn commit_bootstrap_authorization_plan_v1<PE, PC, ME>(
        &mut self,
        plan: super::write::PreparedBootstrapAuthorizationStateV1,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        authorization:&mut crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        revocation: &mut RuntimeRevocationCurrentV1,
        parent: &mut WorldFieldReadContextV1<'_, M, R, PE, PC>,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,ME>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CommittedCurrentBootstrapAuthorizationV1, RootModuleExecutionErrorV1<O::Error>>
    where
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let written = self.write_current_bootstrap_authorization_v1(
            plan,
            sources.lifecycle,
            revocation,
            parent,
            work,
            common,
            live_owner,
        )?;
        let root = authorization
            .publish_bootstrap_root_v1(
                &written,
                sources.lifecycle.typed,
                revocation,
                sources.lifecycle.families.roots,
                self.ctx,
                parent,
                self.members,
                work.module.capability_authorization,
                common,
                &live_owner(),
            )
            .map_err(source_error)?;
        let committed = written.into_committed_v1(root, self.ctx, common, &live_owner())?;
        let live = live_owner();
        let guard = self.ctx.transaction.guard();
        let mut step = IwsStepContext::borrow_counted_at(
            self.ctx.transaction,
            self.now,
            &guard,
            &live,
            self.events.values,
            &mut self.events.mutation,
            &mut self.events.original_work,
            self.events.world_events,
        )?;
        step.append_committed_bootstrap_authorization_event_v1(
            &committed,
            materials,
            common,
            &live,
            self.events.max_event_bytes,
            &mut self.events.persist_original,
        )?;
        Ok(committed)
    }
}

struct BootstrapEnvironmentV1 {
    binding: oasis7_world_state::incremental_cognition_proof_ledger::ProofLedgerRecoveredV1<
        serde_json::Value,
    >,
    world: ReservedValue<String>,
    manifest: ReservedValue<
        crate::world::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1,
    >,
    tick: u64,
}
impl BootstrapEnvironmentV1 {
    fn scope(
        &self,
    ) -> Result<
        crate::world::incremental_safe_alternative_runtime_scope::SafeAlternativeRuntimeScopeV1<'_>,
        crate::WorldError,
    > {
        crate::world::incremental_safe_alternative_runtime_scope::runtime_scope_v1(
            self.binding.value(),
            &self.world,
            self.manifest.hash(),
        )
        .map_err(|code| crate::WorldError::DistributedValidationFailed {
            reason: code.to_owned(),
        })
    }
}
fn factory_error(
    e: crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1,
) -> BodyMutationError {
    match e{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e)=>e,crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(e)=>{
        struct Cause(crate::WorldError);impl std::fmt::Debug for Cause{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{:?}",self.0)}}impl std::fmt::Display for Cause{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{:?}",self.0)}}impl std::error::Error for Cause{}
        BodyMutationError::Original(Box::new(Cause(e)))
    }}
}
pub(super) fn admit_data<T: serde::Serialize + ?Sized>(
    value: &T,
    work: &mut CurrentBootstrapWorkV1<'_, '_>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, BodyMutationError> {
    use crate::world::cognition_economy::bounded_work::{
        CognitionEconomyWorkOpsV1, CurrentEconomyWorkV1,
    };
    let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
    let phase = ops.before_value(value);
    let reconciled = ops.finish();
    let _fees = reconciled?;
    phase.map_err(|_| BodyMutationError::Invalid("Bootstrap original graph admission"))
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    fn read_bootstrap_environment_v1<PE, PC>(
        &mut self,
        normal: &BoundNormalTickParentV1<'_>,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        parent: &mut WorldFieldReadContextV1<'_, M, R, PE, PC>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<BootstrapEnvironmentV1, RootModuleExecutionErrorV1<O::Error>>
    where
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        use crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1;
        use oasis7_world_state::incremental_runtime_artifact_resources::*;
        normal
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let tick = with_module_lifecycle_read_io_v1(
            sources.lifecycle.families.roots,
            self.ctx,
            parent,
            work.factory,
            common,
            &mut work.module,
            |ctx, _, _, _| {
                sources
                    .lifecycle
                    .primitive
                    .validate_binding(sources.lifecycle.families.roots)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(
                    oasis7_world_state::incremental_world_u64::WorldU64FieldV1::Time,
                    ctx,
                )
                .map_err(overlay_error)
                .map_err(Into::into)
            },
        )
        .map_err(factory_error)?;
        let binding=match sources.ledger.read_field(self.ctx.transaction,oasis7_world_state::incremental_cognition_proof_ledger::CognitionProofLedgerFieldV1::RuntimeBinding,parent,&mut work.cognition.ledger_scan,&mut work.cognition.ledger,common,live)?{Some(value)=>value,None=>return Err(RootModuleExecutionErrorV1::Business(paid_business_error_v1(crate::WorldError::DistributedValidationFailed{reason:"runtime_binding_missing".to_owned()},common)?))};
        let world=with_module_lifecycle_read_io_v1(sources.lifecycle.families.roots,self.ctx,parent,work.factory,common,&mut work.module,|ctx,parent,_,module|{
            let guard=ctx.transaction.guard();let child=open_current_chain_resource_manifest_v1(sources.artifacts,ctx,&guard,live).map_err(overlay_error)?;
            let mut read=WorldFieldReadContextV1{root:child.root(),state:parent.state,values:oasis7_formal_protocol::incremental_value::ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
            let first=read_runtime_artifact_member_v1(RuntimeArtifactRecordV1::ChainResourceManifest,&child,&"world_id",RuntimeArtifactMemberProfileV1::Text,&mut read,module.max_selected_heap as u64,&mut std::io::sink())?;
            let Some(RuntimeArtifactValueV1::Text{utf8_bytes})=first else{return Err(BodyMutationError::Invalid("Bootstrap actual manifest world").into())};
            let n=usize::try_from(utf8_bytes).map_err(|_|BodyMutationError::Invalid("Bootstrap manifest world size"))?;
            let fee=read.values.memory.reserve(n.checked_add(std::mem::size_of::<String>()).ok_or(BodyMutationError::Invalid("Bootstrap world allocation"))?,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            let mut raw=Vec::with_capacity(n);if raw.capacity()!=n{return Err(BodyMutationError::Invalid("Bootstrap world capacity").into())}
            let second=read_runtime_artifact_member_v1(RuntimeArtifactRecordV1::ChainResourceManifest,&child,&"world_id",RuntimeArtifactMemberProfileV1::Text,&mut read,utf8_bytes,&mut raw)?;
            if !matches!(second,Some(RuntimeArtifactValueV1::Text{utf8_bytes:actual})if actual==utf8_bytes)||raw.len()!=n{return Err(BodyMutationError::Invalid("Bootstrap actual manifest world changed").into())}
            Ok(fee.retain(String::from_utf8(raw).map_err(|e|BodyMutationError::Original(Box::new(e)))?))
        }).map_err(factory_error)?;
        let manifest=self.with_factory_prologue_external_meter_v1(common,&mut||*live,&mut super::economy::NoExternal,|root,_,_|{
            crate::world::incremental_module_execution_manifest_hash::prepare_current_world_manifest_hash_v1(normal.refs().runtime,normal.refs().head,sources.lifecycle.families.roots,root.manifest_cache,None,sources.lifecycle.families.config,root.state,root.ctx,root.manifest_work).map_err(RootModuleExecutionErrorV1::Manifest)
        })?;
        Ok(BootstrapEnvironmentV1 {
            binding,
            world,
            manifest,
            tick,
        })
    }
}

pub(super) fn paid_business_error_v1(
    error: crate::WorldError,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<crate::WorldError>, BodyMutationError> {
    struct Count<'a, 'b> {
        common: &'a mut FormalBoundedContextV1<'b>,
        bytes: usize,
        first: Option<oasis7_formal_protocol::controlled_authority::formal::FormalError>,
    }
    impl std::fmt::Write for Count<'_, '_> {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            if self.first.is_some() {
                return Err(std::fmt::Error);
            }
            if let Err(e) = self.common.charge(1, s.len() as u64) {
                self.first = Some(e);
                return Err(std::fmt::Error);
            }
            self.bytes = self.bytes.checked_add(s.len()).ok_or(std::fmt::Error)?;
            Ok(())
        }
    }
    let mut count = Count {
        common,
        bytes: 0,
        first: None,
    };
    let formatted = std::fmt::write(&mut count, format_args!("{:?}", error));
    if let Some(error) = count.first {
        return Err(BodyMutationError::Original(Box::new(error)));
    }
    formatted.map_err(|_| BodyMutationError::Invalid("Bootstrap original error layout"))?;
    let bytes = count
        .bytes
        .checked_mul(1024)
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::WorldError>()))
        .ok_or(ValueTreeError::ResourceLimited(
            "Bootstrap error graph layout",
        ))?;
    count
        .common
        .consume_steps(0)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(count
        .common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?
        .retain(error))
}
fn read_source_error(
    error: crate::world::incremental_authorization_root_current::SourceError,
) -> BodyMutationError {
    source_error(crate::world::incremental_authorization_root_current::WarmError::Source(error))
}
fn checked_proof_v1<M: MerkleNodeReader>(
    checked: crate::world::PaidModuleCapabilityAdmissionV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    Result<
        super::publication::VerifiedBootstrapCapabilityCheckV1,
        ReservedValue<crate::WorldError>,
    >,
    BodyMutationError,
> {
    checked.validate(tx, common, live)?;
    let (result, sources) = checked.into_parts();
    match result {
        Err(error) => Ok(Err(paid_business_error_v1(error, common)?)),
        Ok(()) => {
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<super::publication::VerifiedBootstrapCapabilityCheckV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            Ok(Ok(super::publication::VerifiedBootstrapCapabilityCheckV1 {
                guard: tx.guard(),
                core: common.work().identity(),
                fee,
                _sources: sources,
            }))
        }
    }
}
enum AuthorizationPlanQueryV1<'a> {
    Identity,
    Authority,
    Grant,
    Context(&'a oasis7_world_state::CapabilityInvocationContext),
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    fn prepare_bootstrap_authorization_plan_v1<PE, PC>(
        &mut self,
        input: &crate::ProviderBackedBootstrapAuthorityV1,
        query: AuthorizationPlanQueryV1<'_>,
        normal_parent: &BoundNormalTickParentV1<'_>,
        environment: &BootstrapEnvironmentV1,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        authorization:&crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        parent: &mut WorldFieldReadContextV1<'_, M, R, PE, PC>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<
        Option<super::write::PreparedBootstrapAuthorizationStateV1>,
        RootModuleExecutionErrorV1<O::Error>,
    >
    where
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        use crate::world::incremental_module_capability_admission_rules::{
            BootstrapCapabilityCheckV1 as Check, verify_current_bootstrap_capability_v1,
        };
        use crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1;
        use oasis7_world_state::incremental_runtime_revocation::{
            ModuleAdmissionRevocationRequestV1 as Request,
            ModuleAdmissionRevocationValueV1 as Selected,
        };
        let graph = admit_data(input, work, common)?;
        let mut proof = None;
        let event;
        let before;
        match query {
            AuthorizationPlanQueryV1::Identity => {
                let checked = sources
                    .lifecycle
                    .with_actual_sources(authorization, |actual| {
                        verify_current_bootstrap_capability_v1(
                            Check::AgentIdentity {
                                agent_id: &input.agent_id,
                                identity: &input.identity,
                            },
                            actual,
                            self.ctx,
                            parent,
                            work.factory,
                            &mut work.module,
                            common,
                            &mut work.cognition.hashes,
                            live,
                        )
                    })?;
                proof = Some(
                    checked_proof_v1(checked, self.ctx.transaction, common, live)?
                        .map_err(RootModuleExecutionErrorV1::Business)?,
                );
                let old=with_module_lifecycle_read_io_v1(sources.lifecycle.families.roots,self.ctx,parent,work.factory,common,&mut work.module,|ctx,parent,_,module|{let guard=ctx.transaction.guard();oasis7_world_state::incremental_runtime_revocation::read_current_revocation_agent_identity_v1(&input.agent_id,sources.lifecycle.typed.revocation(),ctx,parent,&mut module.capability_authorization.scan,&mut module.capability_authorization.typed,&guard,live).map_err(overlay_error).map_err(Into::into)}).map_err(factory_error)?;
                if old.identity() == Some(&input.identity) {
                    return Ok(None);
                }
                event = crate::CapabilityAuthorizationEvent::AgentIdentityInstalled {
                    agent_id: input.agent_id.clone(),
                    identity: input.identity.clone(),
                };
                let body = authorization
                    .read_module_admission_revocation_v1(
                        Request::Body,
                        sources.lifecycle.typed,
                        sources.lifecycle.families.roots,
                        self.ctx,
                        parent,
                        work.module.capability_authorization,
                        common,
                        live,
                    )
                    .map_err(read_source_error)?;
                let Selected::Body(body) = &*body else {
                    return Err(BodyMutationError::Invalid("Bootstrap identity actual body").into());
                };
                before = super::write::BootstrapBeforeV1::Revocation(
                    reserve_state_root_v1(body, common)?.retain(body.clone()),
                );
            }
            AuthorizationPlanQueryV1::Authority => {
                let old = authorization
                    .read_module_admission_revocation_v1(
                        Request::Authority(&input.authority_record.issuer_id),
                        sources.lifecycle.typed,
                        sources.lifecycle.families.roots,
                        self.ctx,
                        parent,
                        work.module.capability_authorization,
                        common,
                        live,
                    )
                    .map_err(read_source_error)?;
                let old_proof = authorization
                    .read_module_admission_revocation_v1(
                        Request::Proof(&input.authority_record.issuer_id),
                        sources.lifecycle.typed,
                        sources.lifecycle.families.roots,
                        self.ctx,
                        parent,
                        work.module.capability_authorization,
                        common,
                        live,
                    )
                    .map_err(read_source_error)?;
                let Selected::Authority(previous) = &*old else {
                    return Err(
                        BodyMutationError::Invalid("Bootstrap authority selected variant").into(),
                    );
                };
                let Selected::Proof(previous_proof) = &*old_proof else {
                    return Err(
                        BodyMutationError::Invalid("Bootstrap proof selected variant").into(),
                    );
                };
                if previous
                    .as_ref()
                    .is_some_and(|v| v.value() == &input.authority_record)
                    && previous_proof
                        .as_ref()
                        .is_some_and(|v| v.value() == &input.authority_finality_proof)
                {
                    return Ok(None);
                }
                let checked = sources
                    .lifecycle
                    .with_actual_sources(authorization, |actual| {
                        verify_current_bootstrap_capability_v1(
                            Check::AuthorityFinality {
                                record: &input.authority_record,
                                proof: &input.authority_finality_proof,
                            },
                            actual,
                            self.ctx,
                            parent,
                            work.factory,
                            &mut work.module,
                            common,
                            &mut work.cognition.hashes,
                            live,
                        )
                    })?;
                proof = Some(
                    checked_proof_v1(checked, self.ctx.transaction, common, live)?
                        .map_err(RootModuleExecutionErrorV1::Business)?,
                );
                if sources
                    .process
                    .is_devlocal_for_current_parent_v1(normal_parent, common)?
                {
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(
                            crate::world::capability_authorization::deny(
                                "finality authority conflicts with explicit DevLocal participant authority",
                            ),
                            common,
                        )?,
                    ));
                }
                if environment.world.as_str() != "unbound"
                    && environment.world.as_str() != input.authority_record.world_id
                {
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(
                            crate::world::capability_authorization::deny(
                                "authority record world does not match live world",
                            ),
                            common,
                        )?,
                    ));
                }
                if let Some(previous) = previous
                    .as_ref()
                    .filter(|v| v.value() != &input.authority_record)
                {
                    if let Err(error)=crate::world::capability_authorization_events::validate_authority_record_transition(previous.value(),&input.authority_record){return Err(RootModuleExecutionErrorV1::Business(paid_business_error_v1(error,common)?))}
                }
                if previous_proof
                    .as_ref()
                    .is_some_and(|v| v.value() != &input.authority_finality_proof)
                    && previous
                        .as_ref()
                        .is_some_and(|v| v.value() == &input.authority_record)
                {
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(
                            crate::world::capability_authorization::deny(
                                "authority finality proof is immutable",
                            ),
                            common,
                        )?,
                    ));
                }
                event = crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof {
                    record: input.authority_record.clone(),
                    proof: input.authority_finality_proof.clone(),
                };
                let body = authorization
                    .read_module_admission_revocation_v1(
                        Request::Body,
                        sources.lifecycle.typed,
                        sources.lifecycle.families.roots,
                        self.ctx,
                        parent,
                        work.module.capability_authorization,
                        common,
                        live,
                    )
                    .map_err(read_source_error)?;
                let Selected::Body(body) = &*body else {
                    return Err(
                        BodyMutationError::Invalid("Bootstrap authority actual body").into(),
                    );
                };
                before = super::write::BootstrapBeforeV1::Revocation(
                    reserve_state_root_v1(body, common)?.retain(body.clone()),
                );
            }
            AuthorizationPlanQueryV1::Grant => {
                let after_phase = admit_data(&input.grant, work, common)?;
                let after = after_phase.retain(
                    serde_json::to_value(&input.grant)
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?,
                );
                let old = authorization
                    .read_module_admission_grant_v1(
                        &input.grant.grant_id,
                        sources.lifecycle.typed,
                        sources.lifecycle.families.roots,
                        self.ctx,
                        parent,
                        work.module.capability_authorization,
                        common,
                        live,
                    )
                    .map_err(read_source_error)?;
                if let Some(old) = &old {
                    if **old == *after {
                        return Ok(None);
                    }
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(
                            crate::world::provider_backed_bootstrap::bootstrap_denied(
                                "provider bootstrap grant is immutable",
                            ),
                            common,
                        )?,
                    ));
                }
                if let Err(error) = authorization
                    .validate_module_admission_root_v1(
                        sources.lifecycle.typed,
                        sources.lifecycle.families.roots,
                        self.ctx,
                        parent,
                        work.module.capability_authorization,
                        common,
                        live,
                    )
                    .map_err(read_source_error)?
                {
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(error, common)?,
                    ));
                }
                // The subject was fixed to the actual Agent by input validation;
                // the original System-subject extra branch is unreachable here.
                if let Err(error) = super::hash::validate_registered_grant_v1(
                    &input.grant,
                    environment.tick,
                    common,
                    &mut work.cognition.hashes,
                )? {
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(error, common)?,
                    ));
                }
                let checked = sources
                    .lifecycle
                    .with_actual_sources(authorization, |actual| {
                        verify_current_bootstrap_capability_v1(
                            Check::IssuerRevocationAndParents(&input.grant),
                            actual,
                            self.ctx,
                            parent,
                            work.factory,
                            &mut work.module,
                            common,
                            &mut work.cognition.hashes,
                            live,
                        )
                    })?;
                proof = Some(
                    checked_proof_v1(checked, self.ctx.transaction, common, live)?
                        .map_err(RootModuleExecutionErrorV1::Business)?,
                );
                let key = admit_data(&input.grant.grant_id, work, common)?
                    .retain(input.grant.grant_id.clone());
                before = super::write::BootstrapBeforeV1::Grant {
                    key,
                    before: old,
                    after,
                };
                event = crate::CapabilityAuthorizationEvent::GrantRegistered {
                    grant: input.grant.clone(),
                };
            }
            AuthorizationPlanQueryV1::Context(context) => {
                if let Err(error) = authorization
                    .validate_module_admission_root_v1(
                        sources.lifecycle.typed,
                        sources.lifecycle.families.roots,
                        self.ctx,
                        parent,
                        work.module.capability_authorization,
                        common,
                        live,
                    )
                    .map_err(read_source_error)?
                {
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(error, common)?,
                    ));
                }
                if let Err(error) =
                    crate::world::capability_authorization::validate_invocation_context_with_work_v1(context,&mut|s,b|common.charge(s,b).map_err(|e|BodyMutationError::Original(Box::new(e))))?
                {
                    return Err(RootModuleExecutionErrorV1::Business(
                        paid_business_error_v1(error, common)?,
                    ));
                }
                common
                    .charge(
                        1,
                        (context.grant_id.len() + context.response_nonce.len()) as u64,
                    )
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                let key_input=match crate::world::capability_authorization_events::capability_invocation_context_key_input_v1(&context.grant_id,&context.response_nonce){Ok(v)=>v,Err(e)=>return Err(RootModuleExecutionErrorV1::Business(paid_business_error_v1(e,common)?))};
                let key =
                    super::hash::canonical_hash_v1(&key_input, common, &mut work.cognition.hashes)?;
                // The complete immutable grant/nonce selection is performed by
                // the catalogue adapter, before this fixed absent-row writer.
                before = super::write::BootstrapBeforeV1::Invocation { key, before: None };
                event = crate::CapabilityAuthorizationEvent::InvocationContextInstalled {
                    key: match &before {
                        super::write::BootstrapBeforeV1::Invocation { key, .. } => {
                            (*key).to_string()
                        }
                        _ => unreachable!(),
                    },
                    context: context.clone(),
                };
            }
        }
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<super::write::PreparedBootstrapAuthorizationStateV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        Ok(Some(super::write::PreparedBootstrapAuthorizationStateV1 {
            event,
            event_graph: graph,
            before,
            check: proof,
            guard: self.ctx.transaction.guard(),
            core: common.work().identity(),
            now: environment.tick,
            fee,
        }))
    }
}

impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    fn bootstrap_actual_owner_v1<PE,PC>(&mut self,agent:&str,normal:&BoundNormalTickParentV1<'_>,sources:&CurrentBootstrapSourcesV1<'_,'_>,revocation:&RuntimeRevocationCurrentV1,parent:&mut WorldFieldReadContextV1<'_,M,R,PE,PC>,work:&mut CurrentBootstrapWorkV1<'_,'_>,common:&mut FormalBoundedContextV1<'_>,live_owner:&mut dyn FnMut()->ExecutionFence)->Result<ReservedValue<Option<crate::world::incremental_industrial_eligibility::safe_alternative_owner::CurrentSafeAlternativeOwnerV1>>,RootModuleExecutionErrorV1<O::Error>>
    where PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool{
        struct Loan<'a,'r,M,R,E,C>{parent:&'a mut WorldFieldReadContextV1<'r,M,R,E,C>,eligibility:&'a mut crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,orders:&'a mut crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryWorkV1}
        impl<M, R, E, C> crate::world::incremental_factory_phase::io::FactoryPhaseMeterLanesV1
            for Loan<'_, '_, M, R, E, C>
        {
            fn visit_lanes(
                &mut self,
                v: &mut dyn for<'a> FnMut(
                    crate::world::incremental_factory_phase::io::FactoryPhaseMeterLaneV1<'a>,
                ),
            ) {
                use crate::world::incremental_factory_phase::io::FactoryPhaseMeterLaneV1::{
                    Bytes as B, Entries as N, Steps as S,
                };
                v(S(&mut self.parent.values.io.nodes_remaining));
                v(B(&mut self.parent.values.io.read_bytes_remaining));
                v(B(&mut self.parent.values.io.write_bytes_remaining));
                v(B(&mut self.parent.key_work.bytes_remaining));
                v(S(&mut self.eligibility.read.cells_remaining));
                v(B(&mut self.eligibility.read.comparison_bytes_remaining));
                v(S(&mut self.eligibility.typed.json.cells_remaining));
                v(S(&mut self.eligibility.typed.json.json_nodes_remaining));
                v(S(&mut self.eligibility.revocation.json.cells_remaining));
                v(S(&mut self
                    .eligibility
                    .revocation
                    .json
                    .json_nodes_remaining));
                v(S(&mut self.eligibility.scan.nodes_remaining));
                v(S(&mut self.eligibility.scan.leaves_remaining));
                v(B(&mut self.eligibility.scan.read_bytes_remaining));
                v(N(&mut self.eligibility.pending_entries_remaining));
                v(N(&mut self.orders.pending_entries_remaining));
                v(B(&mut self.orders.ids.comparison_bytes_remaining));
                v(B(&mut self.orders.merge_comparison_bytes_remaining));
            }
        }
        self.with_factory_prologue_external_meter_v1(common,live_owner,&mut Loan{parent,eligibility:work.eligibility,orders:work.orders},|root,live,loan|{
            let guard=root.ctx.transaction.guard();
            let current=oasis7_world_state::incremental_world_rule_composite_family::bind_current_agent_orders_v1(normal.refs().storage.current().native().rules().orders(),sources.lifecycle.families.roots,root.state,root.ctx,&mut loan.eligibility.scan,&mut loan.eligibility.typed,&guard,&live()).map_err(overlay_error)?;
            let guard=root.ctx.transaction.guard();
            let directory=crate::world::incremental_factory_directory_orders::open_current_factory_order_directory_v1(sources.accepted_orders,&current,root.state,root.ctx,&mut loan.eligibility.scan,&mut loan.eligibility.typed,loan.orders,&guard,&live()).map_err(|e|match e{crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1::Execution(e)=>BodyMutationError::from(e),crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1::Value(e)=>e.into(),crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1::Overlay(e)=>overlay_error(e),crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1::Body(e)=>e})?;
            let actual=crate::world::incremental_industrial_eligibility::IndustrialEligibilitySourcesV1{roots:sources.lifecycle.families.roots,orders:&directory,agents:sources.lifecycle.families.agents,native:sources.lifecycle.native,factories:sources.lifecycle.factory.lifecycle.factories,sites:sources.lifecycle.factory.factory_authority,anchors:sources.lifecycle.factory.agent_authority,revocation:sources.lifecycle.typed.revocation(),artifacts:sources.artifacts};
            let guard=root.ctx.transaction.guard();
            crate::world::incremental_industrial_eligibility::safe_alternative_owner::read_current_safe_alternative_owner_v1(&actual,revocation,agent,root.state,root.ctx,loan.parent,loan.eligibility,&guard,&live()).map_err(|e|RootModuleExecutionErrorV1::Body(match e{crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1::Execution(e)=>e.into(),crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1::Value(e)=>e.into(),crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1::Overlay(e)=>overlay_error(e),crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1::Directory(e)=>{
                #[derive(Debug)]struct Cause(crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1);impl std::fmt::Display for Cause{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{:?}",self.0)}}impl std::error::Error for Cause{}BodyMutationError::Original(Box::new(Cause(e)))
            }}))
        })
    }
}

fn catalogue_error_v1<O>(
    error: super::catalog::CatalogErrorV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<RootModuleExecutionErrorV1<O>, BodyMutationError> {
    use super::catalog::CatalogErrorV1;
    Ok(match error {
        CatalogErrorV1::Body(e) => RootModuleExecutionErrorV1::Body(e),
        CatalogErrorV1::PaidWorld(e) => RootModuleExecutionErrorV1::Business(e),
        CatalogErrorV1::World(e) => {
            RootModuleExecutionErrorV1::Business(paid_business_error_v1(e, common)?)
        }
        CatalogErrorV1::CheckedWorld { error, proof } => {
            let paid = paid_business_error_v1(error, common)?;
            drop(proof);
            RootModuleExecutionErrorV1::Business(paid)
        }
    })
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + StatePageWriter
        + oasis7_formal_protocol::incremental_schedule::SchedulePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Fixed per-row prefix. The returned bool is private control flow (the
    /// original expiry branch), never a permission to skip an input row.
    #[allow(clippy::too_many_arguments)]
    fn prepare_bootstrap_input_prefix_v1<PE, PC, ME>(
        &mut self,
        input: &crate::ProviderBackedBootstrapAuthorityV1,
        normal: &BoundNormalTickParentV1<'_>,
        environment: &BootstrapEnvironmentV1,
        sources: &CurrentBootstrapSourcesV1<'_, '_>,
        authorization:&mut crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        revocation: &mut RuntimeRevocationCurrentV1,
        parent: &mut WorldFieldReadContextV1<'_, M, R, PE, PC>,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,ME>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        publications: &mut Vec<CommittedCurrentBootstrapAuthorizationV1>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<bool, RootModuleExecutionErrorV1<O::Error>>
    where
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        use crate::world::cognition_economy::{
            CognitionProvisioningRequestV1,
            bounded_work::{CognitionEconomyWorkOpsV1, CurrentEconomyWorkV1},
        };
        use crate::world::provider_backed_bootstrap::input_rules;
        // All ordinary comparisons below visit only the actually admitted
        // input graph; source reads remain on the continuous original IO bank.
        let _phase = admit_data(input, work, common)?;
        let scope = match environment.scope() {
            Ok(v) => v,
            Err(e) => {
                return Err(RootModuleExecutionErrorV1::Business(
                    paid_business_error_v1(e, common)?,
                ));
            }
        };
        if let Err(e) =
            input_rules::validate_binding_scope_v1(input, scope.world, scope.branch, scope.reorg)
        {
            return Err(RootModuleExecutionErrorV1::Business(
                paid_business_error_v1(e, common)?,
            ));
        }
        let present=crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(sources.lifecycle.families.roots,self.ctx,parent,work.factory,common,&mut work.module,|ctx,parent,factory,_|{
            let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(&input.agent_id,sources.lifecycle.factory.lifecycle.agents,sources.lifecycle.families.roots,ctx,parent,&mut factory.lifecycle.pending_entries_remaining,&guard,&live_owner()).map_err(overlay_error)?==oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing)
        }).map_err(factory_error)?;
        if !present {
            return Err(RootModuleExecutionErrorV1::Business(
                paid_business_error_v1(
                    crate::world::provider_backed_bootstrap::bootstrap_denied(
                        "provider bootstrap requires a live agent",
                    ),
                    common,
                )?,
            ));
        }
        if let Err(e) = input_rules::validate_authority_scope_v1(input, scope.finality_epoch()) {
            return Err(RootModuleExecutionErrorV1::Business(
                paid_business_error_v1(e, common)?,
            ));
        }
        let mut ops = CurrentEconomyWorkV1::new(common, &mut work.cognition.hashes)?;
        let request = (|| {
            let request = CognitionProvisioningRequestV1::new_with_work_v1(
                &input.provision_id,
                &input.owner_binding,
                &input.owner_binding,
                input.owner_generation,
                &input.world_id,
                &input.branch_id,
                input.reorg_epoch,
                input.allowance,
                &input.authority_context,
                &mut ops,
            )?;
            request.value().validate_with_work_v1(&mut ops)?;
            Ok::<_, crate::world::cognition_economy::CognitionEconomyError>(request)
        })();
        let settled = ops.finish();
        let _request_fees = settled?;
        let request = match request {
            Ok(value) => value,
            Err(error) => {
                common
                    .charge(1, 256)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                return Err(RootModuleExecutionErrorV1::Business(
                    paid_business_error_v1(
                        crate::world::provider_backed_bootstrap::bootstrap_denied(format!(
                            "provider bootstrap request: {error}"
                        )),
                        common,
                    )?,
                ));
            }
        };
        if let Err(e) = input_rules::validate_request_digest_fields_v1(input, request.value()) {
            return Err(RootModuleExecutionErrorV1::Business(
                paid_business_error_v1(e, common)?,
            ));
        }
        drop(request);
        let expired = input
            .grant
            .expires_at_tick
            .is_some_and(|expiry| environment.tick > expiry);
        if !expired {
            for query in [
                AuthorizationPlanQueryV1::Identity,
                AuthorizationPlanQueryV1::Authority,
                AuthorizationPlanQueryV1::Grant,
            ] {
                if let Some(plan) = self.prepare_bootstrap_authorization_plan_v1(
                    input,
                    query,
                    normal,
                    environment,
                    sources,
                    authorization,
                    parent,
                    work,
                    common,
                    &live_owner(),
                )? {
                    if publications.len() == publications.capacity() {
                        return Err(BodyMutationError::Invalid(
                            "Bootstrap preadmitted publication slots",
                        )
                        .into());
                    }
                    publications.push(self.commit_bootstrap_authorization_plan_v1(
                        plan,
                        sources,
                        authorization,
                        revocation,
                        parent,
                        materials,
                        work,
                        common,
                        live_owner,
                    )?);
                }
            }
        }
        let owner = if expired {
            Some(self.bootstrap_actual_owner_v1(
                &input.agent_id,
                normal,
                sources,
                revocation,
                parent,
                work,
                common,
                live_owner,
            )?)
        } else {
            None
        };
        let policy = if expired {
            Some(
                sources
                    .process
                    .release_security_policy_for_parent_v1(normal, common)?,
            )
        } else {
            None
        };
        let purpose = match (owner, policy) {
            (Some(owner), Some(policy)) => super::catalog::CataloguePurposeV1::Terminal {
                input,
                owner,
                policy,
            },
            (None, None) => super::catalog::CataloguePurposeV1::Fresh {
                presenter: input.invocation_context.presenter.clone(),
                nonce: input.invocation_context.response_nonce.clone(),
            },
            _ => return Err(BodyMutationError::Invalid("Bootstrap fixed branch sources").into()),
        };
        let head = self
            .organization_activity_last_journal_v1(
                normal.refs().runtime,
                parent,
                work.journal,
                common,
            )?
            .map_or(0, |v| v.event().id);
        let catalogue = sources
            .lifecycle
            .with_actual_sources(authorization, |actual| {
                super::catalog::prepare_current_bootstrap_catalogue_v1(
                    &input.agent_id,
                    purpose,
                    &environment.world,
                    environment.binding.value(),
                    environment.tick,
                    head,
                    normal,
                    sources.finality,
                    actual,
                    authorization,
                    self.ctx,
                    parent,
                    work.factory,
                    &mut work.module,
                    &mut work.cognition.hashes,
                    work.data_limits,
                    common,
                    &live_owner(),
                )
            });
        let catalogue = match catalogue {
            Ok(v) => v,
            Err(e) => return Err(catalogue_error_v1(e, common)?),
        };
        catalogue.validate_evidence(self.ctx.transaction, common, &live_owner())?;
        catalogue.with_result(common,|result,existing,common|->Result<(),RootModuleExecutionErrorV1<O::Error>>{
            use super::catalog::{CatalogueEvaluationV1 as ResultV,ExistingContextV1};
            match result{
                ResultV::Terminal(result)=>{result.map_err(|e|catalogue_error_v1(e,common).unwrap_or_else(RootModuleExecutionErrorV1::Body))?;},
                ResultV::Fresh(result)=>{
                    let (_,derived)=result.map_err(|e|catalogue_error_v1(e,common).unwrap_or_else(RootModuleExecutionErrorV1::Body))?;
                    if derived.grant_id!=input.grant.grant_id{return Err(RootModuleExecutionErrorV1::Business(paid_business_error_v1(crate::world::provider_backed_bootstrap::bootstrap_denied("provider bootstrap derived invocation context grant does not match supplied grant"),common)?))}
                    match existing{
                        ExistingContextV1::Matching(old)=>{
                            if old.subject!=derived.subject||old.presenter!=derived.presenter||old.audience!=derived.audience||old.module_id!=derived.module_id||old.module_version!=derived.module_version{return Err(RootModuleExecutionErrorV1::Business(paid_business_error_v1(crate::world::provider_backed_bootstrap::bootstrap_denied("provider bootstrap invocation context is bound to different authority"),common)?))}
                        },
                        ExistingContextV1::KeyCollision=>return Err(RootModuleExecutionErrorV1::Business(paid_business_error_v1(crate::world::capability_authorization::deny("invocation context is immutable"),common)?)),
                        ExistingContextV1::Absent=>{
                            if let Some(plan)=self.prepare_bootstrap_authorization_plan_v1(input,AuthorizationPlanQueryV1::Context(&derived),normal,environment,sources,authorization,parent,work,common,&live_owner())?{
                                if publications.len()==publications.capacity(){return Err(BodyMutationError::Invalid("Bootstrap preadmitted publication slots").into())}
                                publications.push(self.commit_bootstrap_authorization_plan_v1(plan,sources,authorization,revocation,parent,materials,work,common,live_owner)?);
                            }
                        },
                    }
                },
            }Ok(())
        })??;
        Ok(expired)
    }
}

#[path = "normal_bootstrap_current_batch.rs"]
mod batch;

/// StateRoot is not serde DATA. Its only child is the actual root string;
/// prepay that exact fixed layout before cloning the original descriptor.
fn reserve_state_root_v1(
    root: &oasis7_formal_protocol::incremental_state::StateRootV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, BodyMutationError> {
    let n = std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>()
        .checked_add(root.node_root.len())
        .ok_or(ValueTreeError::ResourceLimited("Bootstrap StateRoot copy"))?;
    common
        .charge(1, n as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|e| ValueTreeError::Memory(e).into())
}
