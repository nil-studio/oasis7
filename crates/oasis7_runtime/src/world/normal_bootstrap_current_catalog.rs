//! Complete Current catalogue adapter. The source and every query closure are
//! private and constructed here from actual owners; callers cannot inject a
//! registry, selected grants, verification result, or catalogue digest.
use super::*;
use crate::world::{
    capability_catalog::rules::*,
    incremental_authorization_root_current::CurrentAuthorizationProjectionV1,
    incremental_module_capability_admission_rules::{
        BootstrapCapabilityCheckV1, read_current_bootstrap_policy_v1,
        read_current_bootstrap_registry_v1, verify_current_bootstrap_capability_v1,
    },
    incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use crate::{
    WorldError,
    capability_authorization::{CapabilityAgentIdentity, CapabilityInvocationContext},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use oasis7_wasm_abi::{
    CapabilityCatalogSnapshot, CapabilityGrantV2, CapabilityPresenter, ModuleRegistry,
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::{
        ModuleAdmissionRevocationRequestV1 as Request, ModuleAdmissionRevocationValueV1 as Selected,
    },
};
use std::{cell::RefCell, collections::BTreeMap};
pub(super) enum CatalogErrorV1 {
    Body(BodyMutationError),
    World(WorldError),
    PaidWorld(ReservedValue<WorldError>),
    CheckedWorld {
        error: WorldError,
        proof: crate::world::ModuleCapabilityAdmissionFeesV1,
    },
}
impl std::fmt::Debug for CatalogErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Body(e) => f.debug_tuple("Body").field(e).finish(),
            Self::PaidWorld(e) => f.debug_tuple("World").field(&**e).finish(),
            Self::World(e) | Self::CheckedWorld { error: e, .. } => {
                f.debug_tuple("World").field(e).finish()
            }
        }
    }
}
impl From<ValueTreeError> for CatalogErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}
fn factory_error(
    e: crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1,
) -> CatalogErrorV1 {
    match e{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e)=>e.into(),crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(e)=>e.into()}
}
impl From<WorldError> for CatalogErrorV1 {
    fn from(e: WorldError) -> Self {
        Self::World(e)
    }
}
impl From<BodyMutationError> for CatalogErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
/// Retains output graph admission even on an original business refusal.
pub(super) struct CurrentBootstrapCatalogueV1 {
    result: CatalogueEvaluationV1,
    existing: ExistingContextV1,
    guard: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    _graph: MemoryReservation,
    _identity: Option<MemoryReservation>,
    _fee: MemoryReservation,
}
pub(super) enum ExistingContextV1 {
    Absent,
    Matching(CapabilityInvocationContext),
    KeyCollision,
}
pub(super) enum CatalogueEvaluationV1 {
    Fresh(Result<(CapabilityCatalogSnapshot, CapabilityInvocationContext), CatalogErrorV1>),
    Terminal(Result<CapabilityGrantV2, CatalogErrorV1>),
}
pub(super) enum CataloguePurposeV1<'a> {
    Fresh{presenter:CapabilityPresenter,nonce:String},Terminal{input:&'a crate::ProviderBackedBootstrapAuthorityV1,owner:ReservedValue<Option<crate::world::incremental_industrial_eligibility::safe_alternative_owner::CurrentSafeAlternativeOwnerV1>>,policy:&'a crate::ReleaseSecurityPolicy}}
impl CurrentBootstrapCatalogueV1 {
    pub(super) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self._graph.uses_budget(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "Current Bootstrap catalogue evidence",
            ));
        }
        Ok(())
    }
    pub(super) fn with_result<T>(
        self,
        common: &mut FormalBoundedContextV1<'_>,
        consume: impl FnOnce(
            CatalogueEvaluationV1,
            ExistingContextV1,
            &mut FormalBoundedContextV1<'_>,
        ) -> T,
    ) -> Result<T, BodyMutationError> {
        if self.core != common.work().identity() || !self._graph.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "Bootstrap catalogue consuming context",
            ));
        }
        let Self {
            result,
            existing,
            _graph,
            _identity,
            _fee,
            ..
        } = self;
        let output = consume(result, existing, common);
        drop((_graph, _identity, _fee));
        Ok(output)
    }
}
fn source_error(
    e: crate::world::incremental_authorization_root_current::SourceError,
) -> BodyMutationError {
    use crate::world::incremental_authorization_root_current::SourceError;
    match e {
        SourceError::Execution(e) => e.into(),
        SourceError::Overlay(e) => overlay_error(e),
        other => {
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
fn admit<T: serde::Serialize + ?Sized>(
    value: &T,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
    canonical: &RefCell<&mut oasis7_wasm_abi::CanonicalCborWorkV1>,
) -> Result<MemoryReservation, BodyMutationError> {
    use crate::world::cognition_economy::bounded_work::{
        CognitionEconomyWorkOpsV1, CurrentEconomyWorkV1,
    };
    let mut c = common.borrow_mut();
    let mut k = canonical.borrow_mut();
    let mut ops = CurrentEconomyWorkV1::new(&mut c, &mut k)?;
    let admitted = ops.before_value(value);
    let settled = ops.finish();
    let _fees = settled?;
    admitted.map_err(|_| BodyMutationError::Invalid("closed catalogue graph admission"))
}
struct Ops<'a, 'b, 'c, 'd> {
    common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    canonical: &'a RefCell<&'d mut oasis7_wasm_abi::CanonicalCborWorkV1>,
    hash: &'a super::hash::BootstrapCanonicalMeterV1<'a, 'b, 'c, 'd>,
}
impl CapabilityCatalogOpsV1<CatalogErrorV1> for Ops<'_, '_, '_, '_> {
    fn work(&mut self, steps: usize, bytes: usize) -> Result<(), CatalogErrorV1> {
        self.hash.check()?;
        self.common
            .borrow_mut()
            .charge(steps as u64, bytes as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)).into())
    }
    fn before_value<T: serde::Serialize>(&mut self, value: &T) -> Result<(), CatalogErrorV1> {
        // A single whole-source phase below covers the complete cloned grant
        // vector, catalog snapshot and transient hashes. This visit accounts
        // actual work without accumulating one giant phase per failed filter.
        let _scratch = admit(value, self.common, self.canonical)?;
        Ok(())
    }
    fn before_slots<T>(&mut self, count: usize) -> Result<(), CatalogErrorV1> {
        let bytes = count
            .max(4)
            .checked_mul(std::mem::size_of::<T>())
            .and_then(|n| n.checked_mul(4))
            .ok_or(BodyMutationError::Invalid("catalogue vector layout"))?;
        self.work(count, bytes)
    }
    fn before_command_catalog(&mut self, registry: &ModuleRegistry) -> Result<(), CatalogErrorV1> {
        self.before_value(registry)
    }
}
#[derive(Clone, Copy)]
enum Query<'a> {
    Issuer(&'a CapabilityGrantV2),
    Revocation(&'a CapabilityGrantV2),
    Parents(&'a CapabilityGrantV2),
    ParentsAt(&'a CapabilityGrantV2, u64),
    Audience(&'a CapabilityGrantV2),
}
// These closures are not input capabilities: only the concrete function below
// constructs them, and their bodies call the original actual Current kernels.
struct Source<'a> {
    grants: &'a BTreeMap<String, serde_json::Value>,
    registry: &'a ModuleRegistry,
    policies: &'a oasis7_world_state::policy::PolicySet,
    tick: u64,
    head: u64,
    epoch: u64,
    agent: &'a dyn Fn(&str) -> Result<bool, CatalogErrorV1>,
    identity: &'a dyn Fn(&str) -> Result<Option<CapabilityAgentIdentity>, CatalogErrorV1>,
    verify: &'a dyn Fn(Query<'_>) -> Result<(), CatalogErrorV1>,
}
impl CapabilityCatalogSourcesV1 for Source<'_> {
    type Error = CatalogErrorV1;
    fn agent_live(&self, id: &str) -> Result<bool, Self::Error> {
        (self.agent)(id)
    }
    fn identity(&self, id: &str) -> Result<Option<CapabilityAgentIdentity>, Self::Error> {
        (self.identity)(id)
    }
    fn grants(&self) -> Result<&BTreeMap<String, serde_json::Value>, Self::Error> {
        Ok(self.grants)
    }
    fn registry(&self) -> Result<&ModuleRegistry, Self::Error> {
        Ok(self.registry)
    }
    fn policies(&self) -> Result<&oasis7_world_state::policy::PolicySet, Self::Error> {
        Ok(self.policies)
    }
    fn tick(&self) -> u64 {
        self.tick
    }
    fn world_journal_head(&self)->Result<u64,Self::Error>{Ok(self.head)}
    fn revocation_epoch(&self)->Result<u64,Self::Error>{Ok(self.epoch)}
    fn verify_issuer(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error> {
        (self.verify)(Query::Issuer(g))
    }
    fn verify_live_revocation(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error> {
        (self.verify)(Query::Revocation(g))
    }
    fn verify_parent_chain(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error> {
        (self.verify)(Query::Parents(g))
    }
    fn verify_audience(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error> {
        (self.verify)(Query::Audience(g))
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_current_bootstrap_catalogue_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    agent: &str,
    purpose: CataloguePurposeV1<'_>,
    world: &str,
    binding: &serde_json::Value,
    tick: u64,
    world_head: u64,
    normal_parent: &BoundNormalTickParentV1<'_>,
    finality: &crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    current: &CurrentAuthorizationProjectionV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, M, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    limits:&oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentBootstrapCatalogueV1, CatalogErrorV1> {
    let registry = read_current_bootstrap_registry_v1(
        sources,
        sources.factory.invocation_directory,
        ctx,
        parent,
        factory,
        module,
        common,
        live,
    )
    .map_err(factory_error)?;
    let policy=read_current_bootstrap_policy_v1(sources,ctx,parent,factory,module,common,live).map_err(|e|match e{crate::world::incremental_module_capability_admission_rules::CurrentBootstrapPolicyErrorV1::Body(e)=>CatalogErrorV1::Body(e),crate::world::incremental_module_capability_admission_rules::CurrentBootstrapPolicyErrorV1::World{error,fees}=>CatalogErrorV1::CheckedWorld{error,proof:fees}})?;
    policy.validate_current(ctx.transaction, common, live)?;
    let snapshot = current
        .read_bootstrap_auth_snapshot_v1(ctx.transaction, module.pending_entries, common, live)
        .map_err(source_error)?;
    let inventory = match snapshot
        .decode_inventory(limits, common)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?
    {
        Ok(v) => v,
        Err(e) => {
            return Err(BodyMutationError::Original(Box::new(CatalogCodecRefusalV1(e))).into());
        }
    };
    let common = RefCell::new(common);
    let canonical = RefCell::new(canonical);
    let hash = super::hash::BootstrapCanonicalMeterV1::new(&common, &canonical);
    let graph = admit(
        &(
            inventory.grants(),
            inventory.invocation_contexts(),
            registry.value(),
            policy.value(),
            agent,
            match &purpose {
                CataloguePurposeV1::Fresh { presenter, nonce } => Some((presenter, nonce)),
                CataloguePurposeV1::Terminal { .. } => None,
            },
        ),
        &common,
        &canonical,
    )?;
    let fee = common
        .borrow()
        .memory()
        .reserve(
            std::mem::size_of::<CurrentBootstrapCatalogueV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let io = RefCell::new((ctx, parent, factory, module));
    let identity_fee = RefCell::new(None);
    let agent_live = |id: &str| -> Result<bool, CatalogErrorV1> {
        let mut io = io.borrow_mut();
        let (ctx, parent, factory, module) = &mut *io;
        let mut c = common.borrow_mut();
        with_module_lifecycle_read_io_v1(sources.families.roots,*ctx,*parent,factory,&mut c,module,|ctx,parent,factory,_|{
            let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(id,sources.factory.lifecycle.agents,sources.families.roots,ctx,parent,&mut factory.lifecycle.pending_entries_remaining,&guard,live).map_err(overlay_error)?==oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Existing)
        }).map_err(factory_error)
    };
    let identity = |id: &str| -> Result<Option<CapabilityAgentIdentity>, CatalogErrorV1> {
        let value = {
            let mut io = io.borrow_mut();
            let (ctx, parent, factory, module) = &mut *io;
            let mut c = common.borrow_mut();
            with_module_lifecycle_read_io_v1(sources.families.roots,*ctx,*parent,factory,&mut c,module,|ctx,parent,_,module|{let guard=ctx.transaction.guard();oasis7_world_state::incremental_runtime_revocation::read_current_revocation_agent_identity_v1(id,sources.capability.typed.revocation(),ctx,parent,&mut module.capability_authorization.scan,&mut module.capability_authorization.typed,&guard,live).map_err(overlay_error).map_err(Into::into)}).map_err(factory_error)?
        };
        match value.identity() {
            None => Ok(None),
            Some(value) => {
                let phase = admit(value, &common, &canonical)?;
                let copied = value.clone();
                *identity_fee.borrow_mut() = Some(phase);
                Ok(Some(copied))
            }
        }
    };
    let verify = |query: Query<'_>| -> Result<(), CatalogErrorV1> {
        let grant = match query {
            Query::Issuer(g)
            | Query::Revocation(g)
            | Query::Parents(g)
            | Query::ParentsAt(g, _)
            | Query::Audience(g) => g,
        };
        let mut io = io.borrow_mut();
        let (ctx, parent, factory, module) = &mut *io;
        let mut c = common.borrow_mut();
        if let Query::Audience(_) = query {
            let audience = &grant.audience;
            match audience.target_kind.as_str() {
                "world" if audience.target_id.is_none() => {}
                "module_instance" => {
                    let id = audience.target_id.as_deref().ok_or_else(|| {
                        WorldError::CapabilityAuthorizationDenied {
                            reason: "capability module-instance target id is required".into(),
                        }
                    })?;
                    let valid=with_module_lifecycle_read_io_v1(sources.families.roots,*ctx,*parent,factory,&mut c,module,|ctx,_,_,module|{
                        use oasis7_world_state::incremental_module_instance_fields::{ModuleInstanceFieldV1 as F,ModuleInstanceScalarRefV1 as S};
                        let mut read=crate::world::incremental_module_read::ModuleReadContext{qualification:sources.families.instances,values:ctx.values.source,memory:ctx.values.memory,io:&mut*ctx.values.io,max_value_bytes:module.max_selected_heap as u64,cancelled:&mut ctx.cancelled};read.check(ctx.transaction)?;
                        let Some(row)=read.read_instance(id,ctx.transaction)?else{return Ok(false)};
                        Ok(matches!(row.field(F::Active),S::Bool(true))&&matches!(row.field(F::InstanceId),S::Text(v)if v==id)&&matches!(row.field(F::ModuleId),S::Text(v)if v==grant.scope.module_id)&&matches!(row.field(F::Version),S::Text(v)if v==grant.scope.module_version))
                    }).map_err(factory_error)?;
                    if !valid {
                        return Err(WorldError::CapabilityAuthorizationDenied {
                            reason: "capability module-instance target is not live".into(),
                        }
                        .into());
                    }
                }
                _ => {
                    return Err(WorldError::CapabilityAuthorizationDenied {
                        reason: "capability audience target is not live".into(),
                    }
                    .into());
                }
            }
            let authority = current
                .read_module_admission_revocation_v1(
                    Request::Authority(&grant.issuer.issuer_id),
                    sources.capability.typed,
                    sources.families.roots,
                    *ctx,
                    *parent,
                    module.capability_authorization,
                    &mut c,
                    live,
                )
                .map_err(source_error)?;
            let Selected::Authority(Some(authority)) = &*authority else {
                return Err(WorldError::CapabilityAuthorizationDenied {
                    reason: "capability audience authority is not installed".into(),
                }
                .into());
            };
            let authority = authority.value();
            c.charge(
                1,
                (authority.branch_id.len()
                    + audience.branch_id.len()
                    + world.len()
                    + audience.world_id.len()) as u64,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            if authority.finality_status != "finalized"
                || authority.branch_id != audience.branch_id
                || authority.finality_epoch != audience.finality_epoch
            {
                return Err(WorldError::CapabilityAuthorizationDenied {
                    reason: "capability audience is not bound to live finality".into(),
                }
                .into());
            }
            if world != "unbound" && world != audience.world_id {
                return Err(WorldError::CapabilityAuthorizationDenied {
                    reason: "capability audience world does not match live world".into(),
                }
                .into());
            }
            if binding.get("world_id").and_then(serde_json::Value::as_str)
                != Some(audience.world_id.as_str())
                || binding.get("branch_id").and_then(serde_json::Value::as_str)
                    != Some(audience.branch_id.as_str())
                || binding
                    .get("finality_epoch")
                    .and_then(serde_json::Value::as_u64)
                    != Some(audience.finality_epoch)
            {
                return Err(WorldError::CapabilityAuthorizationDenied {
                    reason: "capability audience conflicts with Runtime binding".into(),
                }
                .into());
            }
            // The verified Formal parent is mandatory. Its authority epoch is
            // deliberately not substituted for the Node chain epoch below.
            normal_parent
                .validate_context(&mut c)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            return finality.validate_audience_v1(normal_parent,audience.finality_epoch,&mut c,live).map_err(|e|match e{crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityErrorV1::Source(e)=>CatalogErrorV1::Body(e),crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityErrorV1::World(e)=>CatalogErrorV1::PaidWorld(e)});
        }
        let check = match query {
            Query::Issuer(g) => BootstrapCapabilityCheckV1::CatalogueIssuer(g),
            Query::Revocation(g) => BootstrapCapabilityCheckV1::CatalogueRevocation(g),
            Query::Parents(g) => BootstrapCapabilityCheckV1::CatalogueParents(g),
            Query::ParentsAt(g, tick) => {
                BootstrapCapabilityCheckV1::CatalogueParentsAt { grant: g, tick }
            }
            Query::Audience(_) => unreachable!(),
        };
        let checked = verify_current_bootstrap_capability_v1(
            check,
            sources,
            *ctx,
            *parent,
            factory,
            module,
            &mut c,
            &mut canonical.borrow_mut(),
            live,
        )?;
        let (result, proof) = checked.into_parts();
        match result {
            Ok(()) => Ok(()),
            Err(error) => Err(CatalogErrorV1::CheckedWorld { error, proof }),
        }
    };
    let epoch = {
        let mut io = io.borrow_mut();
        let (ctx, parent, _, module) = &mut *io;
        let mut c = common.borrow_mut();
        let value = current
            .read_module_admission_revocation_v1(
                Request::Epoch,
                sources.capability.typed,
                sources.families.roots,
                *ctx,
                *parent,
                module.capability_authorization,
                &mut c,
                live,
            )
            .map_err(source_error)?;
        let Selected::Epoch(value) = &*value else {
            return Err(BodyMutationError::Invalid("Current catalogue epoch").into());
        };
        *value
    };
    let source = Source {
        grants: inventory.grants(),
        registry: registry.value(),
        policies: policy.value(),
        tick,
        head: world_head,
        epoch,
        agent: &agent_live,
        identity: &identity,
        verify: &verify,
    };
    let mut existing = ExistingContextV1::Absent;
    let result = match purpose {
        CataloguePurposeV1::Fresh { presenter, nonce } => {
            let result = capability_context_from_sources_v1(
                &source,
                &mut Ops {
                    common: &common,
                    canonical: &canonical,
                    hash: &hash,
                },
                agent,
                presenter,
                nonce,
            );
            if let Ok((_, derived)) = &result {
                for context in inventory.invocation_contexts().values() {
                    common
                        .borrow_mut()
                        .charge(
                            1,
                            (context.grant_id.len()
                                + derived.grant_id.len()
                                + context.response_nonce.len()
                                + derived.response_nonce.len()) as u64,
                        )
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    if context.grant_id == derived.grant_id
                        && context.response_nonce == derived.response_nonce
                    {
                        existing = ExistingContextV1::Matching(context.clone());
                        break;
                    }
                }
                if matches!(existing, ExistingContextV1::Absent) {
                    common
                        .borrow_mut()
                        .charge(
                            1,
                            (derived.grant_id.len() + derived.response_nonce.len()) as u64,
                        )
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    let input=crate::world::capability_authorization_events::capability_invocation_context_key_input_v1(&derived.grant_id,&derived.response_nonce)?;
                    let key = oasis7_wasm_abi::canonical_hash_with_work_v1(&input, &hash);
                    hash.check()?;
                    let key = key.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    common
                        .borrow_mut()
                        .charge(
                            1 + inventory.invocation_contexts().len().max(1).ilog2() as u64,
                            (key.as_str().len() * 64) as u64,
                        )
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    if inventory.invocation_contexts().contains_key(key.as_str()) {
                        existing = ExistingContextV1::KeyCollision
                    }
                }
            }
            CatalogueEvaluationV1::Fresh(result)
        }
        CataloguePurposeV1::Terminal {
            input,
            owner,
            policy,
        } => {
            let refusals = RefCell::new(Vec::new());
            let live_context = |subject: &str| -> Result<
                Result<CapabilityInvocationContext, WorldError>,
                CatalogErrorV1,
            > {
                let mut selected = None;
                for context in inventory.invocation_contexts().values() {
                    common
                        .borrow_mut()
                        .charge(
                            1,
                            (context.module_id.len() + context.grant_id.len() + subject.len())
                                as u64,
                        )
                        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                    if matches!(&context.subject,oasis7_wasm_abi::CapabilitySubject::Agent{agent_id,..}if agent_id==subject)
                        && context.presenter.presenter_kind == "provider"
                        && binding.get("world_id").and_then(serde_json::Value::as_str)
                            == Some(context.audience.world_id.as_str())
                        && binding.get("branch_id").and_then(serde_json::Value::as_str)
                            == Some(context.audience.branch_id.as_str())
                        && binding
                            .get("finality_epoch")
                            .and_then(serde_json::Value::as_u64)
                            == Some(context.audience.finality_epoch)
                    {
                        selected = Some(context);
                        break;
                    }
                }
                let Some(selected) = selected else {
                    return Ok(Err(WorldError::CapabilityAuthorizationDenied {
                        reason: "canonical provider invocation unavailable".into(),
                    }));
                };
                let nonce = format!("runtime-live:{subject}:{tick}:0");
                let result = capability_context_from_sources_v1(
                    &source,
                    &mut Ops {
                        common: &common,
                        canonical: &canonical,
                        hash: &hash,
                    },
                    subject,
                    selected.presenter.clone(),
                    nonce,
                );
                match result {
                    Ok((catalog, context)) => {
                        if catalog.world_id
                            != binding
                                .get("world_id")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                            || catalog.branch_id
                                != binding
                                    .get("branch_id")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("")
                            || Some(catalog.finality_epoch)
                                != binding
                                    .get("finality_epoch")
                                    .and_then(serde_json::Value::as_u64)
                            || catalog.logical_tick != tick
                            || context
                                .presenter
                                .session_id
                                .as_ref()
                                .is_none_or(|s| s.trim().is_empty())
                        {
                            return Ok(Err(WorldError::CapabilityAuthorizationDenied {
                                reason: "canonical provider current context binding invalid".into(),
                            }));
                        }
                        Ok(Ok(context))
                    }
                    Err(CatalogErrorV1::World(e)) => Ok(Err(e)),
                    Err(CatalogErrorV1::CheckedWorld { error, proof }) => {
                        refusals
                            .borrow_mut()
                            .push(RetainedRefusalV1::Checked(proof));
                        Ok(Err(error))
                    }
                    Err(CatalogErrorV1::PaidWorld(error)) => {
                        let WorldError::CapabilityAuthorizationDenied { reason } = &*error else {
                            return Err(CatalogErrorV1::PaidWorld(error));
                        };
                        common
                            .borrow_mut()
                            .charge(1, reason.len() as u64)
                            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                        let result = WorldError::CapabilityAuthorizationDenied {
                            reason: reason.clone(),
                        };
                        refusals.borrow_mut().push(RetainedRefusalV1::Paid(error));
                        Ok(Err(result))
                    }
                    Err(e) => Err(e),
                }
            };
            let root = || -> Result<(), CatalogErrorV1> {
                let mut io = io.borrow_mut();
                let (ctx, parent, _, module) = &mut *io;
                let mut c = common.borrow_mut();
                current
                    .validate_module_admission_root_v1(
                        sources.capability.typed,
                        sources.families.roots,
                        *ctx,
                        *parent,
                        module.capability_authorization,
                        &mut c,
                        live,
                    )
                    .map_err(source_error)??;
                Ok(())
            };
            let terminal = TerminalSource {
                catalogue: &source,
                contexts: inventory.invocation_contexts(),
                owner: RefCell::new(Some(owner)),
                policy,
                root: &root,
                live: &live_context,
            };
            let mut result=crate::world::provider_terminal_authority_rules::validate_provider_grant_for_terminal_from_sources_v1(&terminal,&mut Ops{common:&common,canonical:&canonical,hash:&hash},agent,&input.grant.grant_id,input.grant.issued_at_tick);
            if result.is_ok() {
                // Historical replay is exact ALL-context evidence, never a
                // fresh installation or a nonce-point substitute.
                let _input_phase = admit(input, &common, &canonical)?;
                let encoded = serde_json::to_value(&input.grant).map_err(|_| {
                    WorldError::CapabilityAuthorizationDenied {
                        reason: "expired bootstrap grant cannot be encoded".into(),
                    }
                })?;
                let exact_authority = (|| -> Result<bool, CatalogErrorV1> {
                    if identity(&input.agent_id)?.as_ref() != Some(&input.identity) {
                        return Ok(false);
                    }
                    if inventory.grants().get(&input.grant.grant_id) != Some(&encoded) {
                        return Ok(false);
                    }
                    {
                        let mut io = io.borrow_mut();
                        let (ctx, parent, _, module) = &mut *io;
                        let mut c = common.borrow_mut();
                        let record = current
                            .read_module_admission_revocation_v1(
                                Request::Authority(&input.authority_record.issuer_id),
                                sources.capability.typed,
                                sources.families.roots,
                                *ctx,
                                *parent,
                                module.capability_authorization,
                                &mut c,
                                live,
                            )
                            .map_err(source_error)?;
                        if !matches!(&*record,Selected::Authority(Some(v))if v.value()==&input.authority_record)
                        {
                            return Ok(false);
                        }
                        let proof = current
                            .read_module_admission_revocation_v1(
                                Request::Proof(&input.authority_record.issuer_id),
                                sources.capability.typed,
                                sources.families.roots,
                                *ctx,
                                *parent,
                                module.capability_authorization,
                                &mut c,
                                live,
                            )
                            .map_err(source_error)?;
                        if !matches!(&*proof,Selected::Proof(Some(v))if v.value()==&input.authority_finality_proof)
                        {
                            return Ok(false);
                        }
                    }
                    let mut count = 0usize;
                    for context in inventory.invocation_contexts().values() {
                        common
                            .borrow_mut()
                            .charge(
                                1,
                                (context.grant_id.len() + input.grant.grant_id.len()) as u64,
                            )
                            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                        if context.grant_id == input.grant.grant_id {
                            count = count
                                .checked_add(1)
                                .ok_or(BodyMutationError::Invalid("expired context count"))?
                        }
                    }
                    if count != 1 {
                        return Ok(false);
                    }
                    for context in inventory.invocation_contexts().values() {
                        let _phase = admit(context, &common, &canonical)?;
                        if context == &input.invocation_context {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                })()?;
                if !exact_authority {
                    result = Err(WorldError::CapabilityAuthorizationDenied {
                        reason: "expired bootstrap is not an exact installed authority replay"
                            .into(),
                    }
                    .into())
                }
            }
            // Every ignored business error's charged source stays live until
            // the original historical fallback and all of its reads finish.
            drop(terminal);
            drop(live_context);
            drop(refusals);
            CatalogueEvaluationV1::Terminal(result)
        }
    };
    let mut io = io.borrow_mut();
    let (ctx, _, _, module) = &mut *io;
    let mut c = common.borrow_mut();
    inventory
        .validate_read_stability_v1(ctx.transaction, module.pending_entries, &mut c, live)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    registry.validate_current(ctx.transaction, module.pending_entries, &mut c, live)?;
    Ok(CurrentBootstrapCatalogueV1 {
        result,
        existing,
        guard: ctx.transaction.guard(),
        core: c.work().identity(),
        _graph: graph,
        _identity: identity_fee.into_inner(),
        _fee: fee,
    })
}

enum RetainedRefusalV1 {
    Checked(crate::world::ModuleCapabilityAdmissionFeesV1),
    Paid(ReservedValue<WorldError>),
}
struct TerminalSource<'a>{catalogue:&'a Source<'a>,contexts:&'a BTreeMap<String,CapabilityInvocationContext>,owner:RefCell<Option<ReservedValue<Option<crate::world::incremental_industrial_eligibility::safe_alternative_owner::CurrentSafeAlternativeOwnerV1>>>>,policy:&'a crate::ReleaseSecurityPolicy,root:&'a dyn Fn()->Result<(),CatalogErrorV1>,live:&'a dyn Fn(&str)->Result<Result<CapabilityInvocationContext,WorldError>,CatalogErrorV1>}
impl crate::world::provider_terminal_authority_rules::sealed::Sources for TerminalSource<'_> {}
impl crate::world::provider_terminal_authority_rules::sealed::Ops for Ops<'_, '_, '_, '_> {}
impl crate::world::provider_terminal_authority_rules::ProviderTerminalOpsV1<CatalogErrorV1>
    for Ops<'_, '_, '_, '_>
{
    fn work(&mut self, n: usize, b: usize) -> Result<(), CatalogErrorV1> {
        CapabilityCatalogOpsV1::work(self, n, b)
    }
    fn before_value<T: serde::Serialize>(&mut self, v: &T) -> Result<(), CatalogErrorV1> {
        CapabilityCatalogOpsV1::before_value(self, v)
    }
    fn before_slots<T>(&mut self, n: usize) -> Result<(), CatalogErrorV1> {
        CapabilityCatalogOpsV1::before_slots::<T>(self, n)
    }
    fn before_map<K, V>(&mut self, n: usize) -> Result<(), CatalogErrorV1> {
        CapabilityCatalogOpsV1::before_slots::<(K, V)>(self, n.saturating_mul(16))
    }
    fn before_command_catalog(&mut self, r: &ModuleRegistry) -> Result<(), CatalogErrorV1> {
        CapabilityCatalogOpsV1::before_command_catalog(self, r)
    }
}
impl crate::world::provider_terminal_authority_rules::ProviderTerminalSourcesV1
    for TerminalSource<'_>
{
    type Error = CatalogErrorV1;
    type LiveContextError = WorldError;
    fn production_hardened(&self) -> bool {
        self.policy.is_production_hardened()
    }
    fn allow_local_finality_signing(&self) -> bool {
        self.policy.allow_local_finality_signing
    }
    fn agent_live(&self, id: &str) -> Result<bool, Self::Error> {
        self.catalogue.agent_live(id)
    }
    fn verify_authorization_root(&self) -> Result<(), Self::Error> {
        (self.root)()
    }
    fn identity(&self, id: &str) -> Result<Option<CapabilityAgentIdentity>, Self::Error> {
        self.catalogue.identity(id)
    }
    fn owner(
        &self,
        _: &str,
    ) -> Result<
        Option<crate::world::provider_terminal_authority_rules::ProviderTerminalOwnerV1>,
        Self::Error,
    > {
        Ok(Some(
            crate::world::provider_terminal_authority_rules::ProviderTerminalOwnerV1::Current(
                self.owner
                    .borrow_mut()
                    .take()
                    .ok_or(BodyMutationError::Invalid(
                        "actual terminal Owner used twice",
                    ))?,
            ),
        ))
    }
    fn live_context(
        &self,
        id: &str,
    ) -> Result<Result<CapabilityInvocationContext, WorldError>, Self::Error> {
        (self.live)(id)
    }
    fn grants(&self) -> Result<&BTreeMap<String, serde_json::Value>, Self::Error> {
        Ok(self.catalogue.grants)
    }
    fn contexts(&self) -> Result<&BTreeMap<String, CapabilityInvocationContext>, Self::Error> {
        Ok(self.contexts)
    }
    fn tick(&self) -> u64 {
        self.catalogue.tick
    }
    fn registry(&self) -> Result<&ModuleRegistry, Self::Error> {
        Ok(self.catalogue.registry)
    }
    fn verify_issuer(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error> {
        self.catalogue.verify_issuer(g)
    }
    fn verify_live_revocation(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error> {
        self.catalogue.verify_live_revocation(g)
    }
    fn verify_parent_chain_at_tick(
        &self,
        g: &CapabilityGrantV2,
        t: u64,
    ) -> Result<(), Self::Error> {
        (self.catalogue.verify)(Query::ParentsAt(g, t))
    }
    fn verify_audience(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error> {
        self.catalogue.verify_audience(g)
    }
}

struct CatalogCodecRefusalV1(
    oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::PaidCborRefusalV1,
);
impl std::fmt::Debug for CatalogCodecRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Bootstrap catalogue codec")
            .field(&self.0.message())
            .finish()
    }
}
impl std::fmt::Display for CatalogCodecRefusalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.message())
    }
}
impl std::error::Error for CatalogCodecRefusalV1 {}

impl oasis7_wasm_abi::CanonicalCborAllocator for Ops<'_, '_, '_, '_> {
    type Charge = MemoryReservation;
    fn charge_work(&self, s: u64, b: u64) -> Result<(), oasis7_wasm_abi::CanonicalCborStreamError> {
        oasis7_wasm_abi::CanonicalCborAllocator::charge_work(self.hash, s, b)
    }
    fn reserve(
        &self,
        n: usize,
    ) -> Result<MemoryReservation, oasis7_wasm_abi::CanonicalCborStreamError> {
        oasis7_wasm_abi::CanonicalCborAllocator::reserve(self.hash, n)
    }
}
impl oasis7_wasm_abi::CapabilityCanonicalWorkV1 for Ops<'_, '_, '_, '_> {
    type Loan<'a>
        = std::cell::RefMut<'a, oasis7_wasm_abi::CanonicalCborWorkV1>
    where
        Self: 'a;
    fn record_stream_error(
        &self,
        error: oasis7_wasm_abi::CanonicalCborStreamError,
    ) -> oasis7_wasm_abi::CanonicalCborStreamError {
        oasis7_wasm_abi::CapabilityCanonicalWorkV1::record_stream_error(self.hash, error)
    }
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        oasis7_wasm_abi::CapabilityCanonicalWorkV1::loan_canonical_work(self.hash)
    }
}
