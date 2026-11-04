//! Required conjunction of all 46 typed Runtime descriptors. Raw allocators,
//! schedule, agent/module bodies, Cognition and History belong to the enclosing
//! World verifier. This token alone grants no accepted head or physical lease.
use crate::incremental_pending_action_family as pending;
use crate::incremental_rollback_family as rollback;
use crate::incremental_runtime_artifact_config as config;
use crate::incremental_runtime_artifact_content as content;
use crate::incremental_runtime_artifact_resources as resources;
use crate::incremental_runtime_authorization_family as authorization_records;
use crate::incremental_runtime_authorization_grants_family as grants;
use crate::incremental_runtime_authorization_root as authorization_root;
use crate::incremental_runtime_business as business;
use crate::incremental_runtime_capability_records as capabilities;
use crate::incremental_runtime_controls as controls;
use crate::incremental_runtime_effect_records as effects;
use crate::incremental_runtime_governance as governance;
use crate::incremental_runtime_module_authorization as module_authorization;
use crate::incremental_runtime_proposal as proposals;
use crate::incremental_runtime_revocation as revocation;
use crate::incremental_runtime_scheduler_controls as scheduler;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub struct RuntimeTypedCompleteQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    config: ReservedValue<config::ArtifactConfigFamilyQualificationV1>,
    controls: ReservedValue<controls::RuntimeControlsQualificationV1>,
    business: ReservedValue<business::RuntimeBusinessQualificationV1>,
    effects: ReservedValue<effects::RuntimeEffectFamilyQualificationV1>,
    scheduler: ReservedValue<scheduler::RuntimeSchedulerControlsQualificationV1>,
    capabilities: ReservedValue<capabilities::RuntimeCapabilityFamilyQualificationV1>,
    module_authorization:
        ReservedValue<module_authorization::ModuleAuthorizationFamilyQualificationV1>,
    grants: ReservedValue<grants::AuthorizationGrantsQualificationV1>,
    authorization_records:
        ReservedValue<authorization_records::AuthorizationRecordFamiliesQualificationV1>,
    authorization_root: ReservedValue<authorization_root::RuntimeAuthorizationRootQualificationV1>,
    revocation: ReservedValue<revocation::RuntimeRevocationQualificationV1>,
    proposals: ReservedValue<proposals::RuntimeProposalsQualificationV1>,
    governance: ReservedValue<governance::RuntimeGovernanceQualificationV1>,
    resources: ReservedValue<resources::RuntimeArtifactResourcesQualificationV1>,
    content: ReservedValue<content::RuntimeArtifactContentQualificationV1>,
    rollback: ReservedValue<rollback::RollbackFamilyQualificationV1>,
    pending: ReservedValue<pending::PendingActionQualificationV1>,
}
impl RuntimeTypedCompleteQualificationV1 {
    pub fn config(&self) -> &ReservedValue<config::ArtifactConfigFamilyQualificationV1> { &self.config }
    pub fn controls(&self) -> &ReservedValue<controls::RuntimeControlsQualificationV1> { &self.controls }
    pub fn business(&self) -> &ReservedValue<business::RuntimeBusinessQualificationV1> { &self.business }
    pub fn effects(&self) -> &ReservedValue<effects::RuntimeEffectFamilyQualificationV1> { &self.effects }
    pub fn scheduler(&self) -> &ReservedValue<scheduler::RuntimeSchedulerControlsQualificationV1> { &self.scheduler }
    pub fn capabilities(&self) -> &ReservedValue<capabilities::RuntimeCapabilityFamilyQualificationV1> { &self.capabilities }
    pub fn module_authorization(&self) -> &ReservedValue<module_authorization::ModuleAuthorizationFamilyQualificationV1> { &self.module_authorization }
    pub fn grants(&self) -> &ReservedValue<grants::AuthorizationGrantsQualificationV1> { &self.grants }
    pub fn authorization_records(&self) -> &ReservedValue<authorization_records::AuthorizationRecordFamiliesQualificationV1> { &self.authorization_records }
    pub fn authorization_root(&self) -> &ReservedValue<authorization_root::RuntimeAuthorizationRootQualificationV1> { &self.authorization_root }
    pub fn revocation(&self) -> &ReservedValue<revocation::RuntimeRevocationQualificationV1> { &self.revocation }
    pub fn proposals(&self) -> &ReservedValue<proposals::RuntimeProposalsQualificationV1> { &self.proposals }
    pub fn governance(&self) -> &ReservedValue<governance::RuntimeGovernanceQualificationV1> { &self.governance }
    pub fn resources(&self) -> &ReservedValue<resources::RuntimeArtifactResourcesQualificationV1> { &self.resources }
    pub fn content(&self) -> &ReservedValue<content::RuntimeArtifactContentQualificationV1> { &self.content }
    pub fn rollback(&self) -> &ReservedValue<rollback::RollbackFamilyQualificationV1> { &self.rollback }
    pub fn pending(&self) -> &ReservedValue<pending::PendingActionQualificationV1> { &self.pending }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
}
const CARRIER: usize = std::mem::size_of::<RuntimeTypedCompleteQualificationV1>() + 512;
pub struct RuntimeTypedCompleteWorkV1 {
    pub outer_scan: StateScanBudget,
    pub body_scan: StateScanBudget,
    pub resource_scans: [StateScanBudget; resources::RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1],
    pub cells_remaining: u64,
    pub max_text_bytes: u64,
    pub max_record_bytes: u64,
    pub max_selected_heap: usize,
    pub config: config::ArtifactConfigReadWorkV1,
    pub resources: resources::RuntimeArtifactResourcesWorkV1,
    pub content: content::RuntimeArtifactOriginalWorkV1,
    pub effects: effects::RuntimeEffectJsonWorkV1,
    pub module_authorization: module_authorization::ModuleAuthorizationReadWorkV1,
    pub json: crate::incremental_json_records::JsonReadWorkV1,
    pub revocation: revocation::RevocationReadWorkV1,
    pub proposals: proposals::RuntimeProposalReadWorkV1,
    pub governance: governance::RuntimeGovernanceWorkV1,
    pub rollback: crate::incremental_rollback_genesis::RollbackWorkV1,
    pub pending: pending::PendingActionFamilyWorkV1,
}
#[derive(Debug)]
pub enum RuntimeTypedCompleteErrorV1<O> {
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Content(content::RuntimeArtifactOriginalErrorV1<O>),
    Pending(pending::PendingActionFamilyErrorV1<O>),
}
impl<O> From<ValueTreeError> for RuntimeTypedCompleteErrorV1<O> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl<O> From<WorldFieldOverlayErrorV1> for RuntimeTypedCompleteErrorV1<O> {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete Runtime46 binding/pool/class")
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    Ok(())
}
macro_rules! local {
    ($roots:expr,$ns:expr,$ctx:expr) => {
        WorldFieldReadContextV1 {
            root: $roots.state_root($ns),
            state: $ctx.state,
            values: ValueReadContext {
                source: $ctx.values.source,
                memory: $ctx.values.memory,
                class: $ctx.values.class,
                io: &mut *$ctx.values.io,
            },
            key_work: &mut *$ctx.key_work,
            semantic_bytes: $ctx.semantic_bytes,
            record_bytes: $ctx.record_bytes,
            take_state_error: &mut $ctx.take_state_error,
            cancelled: &mut $ctx.cancelled,
        }
    };
}
pub fn qualify_runtime_typed_complete_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    objects: &mut O,
    work: &mut RuntimeTypedCompleteWorkV1,
) -> Result<ReservedValue<RuntimeTypedCompleteQualificationV1>, RuntimeTypedCompleteErrorV1<O::Error>>
{
    let result = (|| {
        check(ctx)?;
        roots.validate().map_err(|_| invalid())?;
        if !roots.state.iter().any(|r| r == ctx.root) {
            return Err(invalid().into());
        }
        let retained = ctx
            .values
            .memory
            .reserve(CARRIER, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        let config = {
            let mut local = local!(roots, Ns::Artifacts, ctx);
            config::qualify_artifact_config_family_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.body_scan,
                &mut work.config,
            )?
        };
        let controls = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            controls::qualify_runtime_controls_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                work.max_text_bytes,
            )?
        };
        let business = {
            let mut local = local!(roots, Ns::Business, ctx);
            business::qualify_runtime_business_v1(roots, &mut local, &mut work.outer_scan)?
        };
        let effects = {
            let mut local = local!(roots, Ns::Effects, ctx);
            effects::qualify_runtime_effect_records_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.effects,
            )?
        };
        let scheduler = {
            let mut local = local!(roots, Ns::Scheduler, ctx);
            scheduler::qualify_runtime_scheduler_controls_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.body_scan,
                &mut work.cells_remaining,
                work.max_text_bytes,
            )?
        };
        let capabilities = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            capabilities::qualify_runtime_capability_family_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.body_scan,
                &mut work.cells_remaining,
                work.max_record_bytes,
            )?
        };
        let module_authorization = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            module_authorization::qualify_module_authorization_family_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.body_scan,
                &mut work.module_authorization,
            )?
        };
        let grants = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            grants::qualify_authorization_grants_family_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.json,
            )?
        };
        let authorization_records = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            authorization_records::qualify_authorization_record_families_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.body_scan,
                &mut work.json,
                work.max_selected_heap,
            )?
        };
        let authorization_root = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            authorization_root::qualify_runtime_authorization_root_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                work.max_text_bytes,
            )?
        };
        let revocation = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            revocation::qualify_runtime_revocation_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.revocation,
            )?
        };
        let proposals = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            proposals::qualify_runtime_proposals_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                &mut work.proposals,
            )?
        };
        let governance = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            governance::qualify_runtime_governance_v1(
                roots,
                &mut local,
                &mut work.outer_scan,
                work.max_selected_heap,
            )?
        };
        let resources = resources::qualify_runtime_artifact_resources_v1(
            roots,
            &mut resources::RuntimeArtifactResourcesReadContextV1 {
                state: ctx.state,
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                max_payload_bytes: work.max_record_bytes,
                scans: &mut work.resource_scans,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            },
            &mut work.resources,
        )?;
        let content = content::qualify_runtime_artifact_content_v1(
            roots,
            &mut local!(roots, Ns::Artifacts, ctx),
            &mut work.outer_scan,
            objects,
            &mut work.content,
        )
        .map_err(RuntimeTypedCompleteErrorV1::Content)?;
        let pending = pending::qualify_pending_actions_v1(
            roots,
            &mut local!(roots, Ns::Scheduler, ctx),
            &mut work.outer_scan,
            &mut work.pending,
            objects,
        )
        .map_err(RuntimeTypedCompleteErrorV1::Pending)?;
        let rollback = rollback::qualify_rollback_family_v1(
            roots,
            ctx.state,
            &mut work.outer_scan,
            &mut crate::incremental_rollback_recovery::RollbackDecodeContextV1 {
                values: &mut ctx.values,
                keys: &mut *ctx.key_work,
                work: &mut work.rollback,
                limits: crate::incremental_rollback_recovery::RollbackDecodeLimitsV1 {
                    max_output_bytes: work.max_selected_heap,
                    max_text_bytes: work.max_text_bytes,
                    max_record_bytes: work.max_record_bytes,
                },
                cancelled: &mut ctx.cancelled,
            },
            &mut ctx.take_state_error,
        )?;
        check(ctx)?;
        Ok(retained.retain(RuntimeTypedCompleteQualificationV1 {
            roots: roots.digest().map_err(|_| invalid())?,
            class: ctx.values.class,
            config,
            controls,
            business,
            effects,
            scheduler,
            capabilities,
            module_authorization,
            grants,
            authorization_records,
            authorization_root,
            revocation,
            proposals,
            governance,
            resources,
            content,
            rollback,
            pending,
        }))
    })();
    check(ctx)?;
    result
}
pub(crate) fn accept_runtime_typed_complete_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    objects: &mut O,
    work: &mut RuntimeTypedCompleteWorkV1,
) -> Result<ReservedValue<RuntimeTypedCompleteQualificationV1>, RuntimeTypedCompleteErrorV1<O::Error>>
{
    let result = (|| {
        check(ctx)?;
        witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
        let _phase = preflight(parent, witness.record(), ctx)?;
        let roots = &witness.record().after;
        let retained = ctx
            .values
            .memory
            .reserve(CARRIER, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let config = {
            let mut local = local!(roots, Ns::Artifacts, ctx);
            config::accept_artifact_config_verified_v1(
                &parent.config,
                witness,
                &mut local,
                &mut work.body_scan,
                &mut work.config,
                None,
            )?
        };
        let controls = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            controls::accept_runtime_controls_verified_v1(
                &parent.controls,
                witness,
                &mut local,
                work.max_text_bytes,
            )?
        };
        let business = {
            let mut local = local!(roots, Ns::Business, ctx);
            business::accept_runtime_business_verified_v1(&parent.business, witness, &mut local)?
        };
        let effects = {
            let mut local = local!(roots, Ns::Effects, ctx);
            effects::accept_runtime_effect_verified_v1(
                &parent.effects,
                witness,
                &mut local,
                &mut work.effects,
            )?
        };
        let scheduler = {
            let mut local = local!(roots, Ns::Scheduler, ctx);
            scheduler::accept_runtime_scheduler_controls_verified_v1(
                &parent.scheduler,
                witness,
                &mut local,
                &mut work.cells_remaining,
                work.max_text_bytes,
            )?
        };
        let capabilities = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            capabilities::accept_runtime_capability_verified_v1(
                &parent.capabilities,
                witness,
                &mut local,
                &mut work.outer_scan,
                &mut work.cells_remaining,
                work.max_record_bytes,
            )?
        };
        let module_authorization = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            module_authorization::accept_module_authorization_verified_v1(
                &parent.module_authorization,
                witness,
                &mut local,
                &mut work.body_scan,
                &mut work.module_authorization,
            )?
        };
        let grants = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            grants::accept_authorization_grants_verified_v1(
                &parent.grants,
                witness,
                &mut local,
                &mut work.json,
            )?
        };
        let authorization_records = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            authorization_records::accept_authorization_record_families_verified_v1(
                &parent.authorization_records,
                witness,
                &mut local,
                &mut work.body_scan,
                &mut work.json,
                work.max_selected_heap,
            )?
        };
        let authorization_root = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            authorization_root::accept_runtime_authorization_root_verified_v1(
                &parent.authorization_root,
                witness,
                &mut local,
                work.max_text_bytes,
            )?
        };
        let revocation = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            revocation::accept_runtime_revocation_verified_v1(
                &parent.revocation,
                witness,
                &mut local,
                &mut work.outer_scan,
                &mut work.revocation,
            )?
        };
        let proposals = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            proposals::accept_runtime_proposals_verified_v1(
                &parent.proposals,
                witness,
                &mut local,
                &mut work.outer_scan,
                &mut work.proposals,
            )?
        };
        let governance = {
            let mut local = local!(roots, Ns::Authorization, ctx);
            governance::accept_runtime_governance_verified_v1(
                &parent.governance,
                witness,
                &mut local,
                &mut work.body_scan,
                &mut work.governance,
                work.max_selected_heap,
            )?
        };
        let resources = resources::accept_runtime_artifact_resources_verified_v1(
            &parent.resources,
            witness,
            &mut local!(roots, Ns::Artifacts, ctx),
            &mut work.resource_scans,
            work.max_record_bytes,
            &mut work.resources,
        )?;
        let content = content::accept_runtime_artifact_content_verified_v1(
            &parent.content,
            witness,
            &mut local!(roots, Ns::Artifacts, ctx),
            objects,
            &mut work.content,
        )
        .map_err(RuntimeTypedCompleteErrorV1::Content)?;
        let pending = pending::accept_pending_actions_verified_v1(
            &parent.pending,
            witness,
            &mut local!(roots, Ns::Scheduler, ctx),
            &mut work.pending,
            objects,
        )
        .map_err(RuntimeTypedCompleteErrorV1::Pending)?;
        let rollback = rollback::accept_rollback_family_verified_v1(
            &parent.rollback,
            witness,
            ctx.state,
            &mut work.body_scan,
            &mut crate::incremental_rollback_recovery::RollbackDecodeContextV1 {
                values: &mut ctx.values,
                keys: &mut *ctx.key_work,
                work: &mut work.rollback,
                limits: crate::incremental_rollback_recovery::RollbackDecodeLimitsV1 {
                    max_output_bytes: work.max_selected_heap,
                    max_text_bytes: work.max_text_bytes,
                    max_record_bytes: work.max_record_bytes,
                },
                cancelled: &mut ctx.cancelled,
            },
            &mut work.cells_remaining,
            &mut ctx.take_state_error,
        )?;
        check(ctx)?;
        Ok(retained.retain(RuntimeTypedCompleteQualificationV1 {
            roots: *witness.after_digest(),
            class: MemoryClass::Ordinary,
            config,
            controls,
            business,
            effects,
            scheduler,
            capabilities,
            module_authorization,
            grants,
            authorization_records,
            authorization_root,
            revocation,
            proposals,
            governance,
            resources,
            content,
            rollback,
            pending,
        }))
    })();
    check(ctx)?;
    result
}
fn preflight<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, ValueTreeError> {
    check(ctx)?;
    if parent.class != MemoryClass::Ordinary
        || ctx.values.class != MemoryClass::Ordinary
        || !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < CARRIER
        || !record.after.state.iter().any(|r| r == ctx.root)
    {
        return Err(invalid());
    }
    let phase = ctx
        .values
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)?;
    parent.validate_binding(&record.before)?;
    check(ctx)?;
    Ok(phase)
}
pub fn accept_runtime_typed_complete_transition_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
>(
    parent: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    objects: &mut O,
    work: &mut RuntimeTypedCompleteWorkV1,
) -> Result<ReservedValue<RuntimeTypedCompleteQualificationV1>, RuntimeTypedCompleteErrorV1<O::Error>>
{
    let result = (|| {
        check(ctx)?;
        if verifier.class != MemoryClass::Ordinary || !verifier.memory.same_pool(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        {
            let _phase = preflight(parent, record, ctx)?;
        }
        let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
            verifier, record, ctx,
        )
        .map_err(WorldFieldOverlayErrorV1::from)?;
        accept_runtime_typed_complete_verified_v1(parent, &witness, ctx, objects, work)
    })();
    check(ctx)?;
    result
}
pub fn readmit_runtime_typed_complete_ordinary_v1(
    source: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    work: &mut pending::PendingActionIndexWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeTypedCompleteQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if source.class != MemoryClass::Recovery
        || !source.uses_budget(memory)
        || source.reserved_bytes() < CARRIER
    {
        return Err(invalid());
    }
    let retained = memory.reserve(CARRIER, MemoryClass::Ordinary)?;
    let _phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let config = config::readmit_artifact_config_family_ordinary_v1(
        &source.config,
        roots,
        memory,
        &mut cancelled,
    )?;
    let controls = controls::readmit_runtime_controls_ordinary_v1(
        &source.controls,
        roots,
        memory,
        &mut cancelled,
    )?;
    let business = business::readmit_runtime_business_ordinary_v1(
        &source.business,
        roots,
        memory,
        &mut cancelled,
    )?;
    let effects = effects::readmit_runtime_effect_family_ordinary_v1(
        &source.effects,
        roots,
        memory,
        &mut cancelled,
    )?;
    let scheduler = scheduler::readmit_runtime_scheduler_controls_ordinary_v1(
        &source.scheduler,
        roots,
        memory,
        &mut cancelled,
    )?;
    let capabilities = capabilities::readmit_runtime_capability_family_ordinary_v1(
        &source.capabilities,
        roots,
        memory,
        &mut cancelled,
    )?;
    let module_authorization =
        module_authorization::readmit_module_authorization_family_ordinary_v1(
            &source.module_authorization,
            roots,
            memory,
            &mut cancelled,
        )?;
    let grants = grants::readmit_authorization_grants_ordinary_v1(
        &source.grants,
        roots,
        memory,
        &mut cancelled,
    )?;
    let authorization_records =
        authorization_records::readmit_authorization_record_families_ordinary_v1(
            &source.authorization_records,
            roots,
            memory,
            &mut cancelled,
        )?;
    let authorization_root = authorization_root::readmit_runtime_authorization_root_ordinary_v1(
        &source.authorization_root,
        roots,
        memory,
        &mut cancelled,
    )?;
    let revocation = revocation::readmit_runtime_revocation_ordinary_v1(
        &source.revocation,
        roots,
        memory,
        &mut cancelled,
    )?;
    let proposals = proposals::readmit_runtime_proposals_ordinary_v1(
        &source.proposals,
        roots,
        memory,
        &mut cancelled,
    )?;
    let governance = governance::readmit_runtime_governance_ordinary_v1(
        &source.governance,
        roots,
        memory,
        &mut cancelled,
    )?;
    let resources = resources::readmit_runtime_artifact_resources_ordinary_v1(
        &source.resources,
        roots,
        memory,
        &mut cancelled,
    )?;
    let content = content::readmit_runtime_artifact_content_ordinary_v1(
        &source.content,
        roots,
        memory,
        &mut cancelled,
    )?;
    let rollback = rollback::readmit_rollback_family_ordinary_v1(
        &source.rollback,
        roots,
        memory,
        &mut cancelled,
    )?;
    let pending =
        pending::readmit_pending_actions_v1(&source.pending, roots, memory, work, &mut cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(RuntimeTypedCompleteQualificationV1 {
        roots: source.roots,
        class: MemoryClass::Ordinary,
        config,
        controls,
        business,
        effects,
        scheduler,
        capabilities,
        module_authorization,
        grants,
        authorization_records,
        authorization_root,
        revocation,
        proposals,
        governance,
        resources,
        content,
        rollback,
        pending,
    }))
}
