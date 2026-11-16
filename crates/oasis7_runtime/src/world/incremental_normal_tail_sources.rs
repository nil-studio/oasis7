//! Complete fixed-entry tail sources from one actual accepted normal parent.
//! Warm and explicit GEN/cold typed parents share the same binding rules.
use crate::world::{
    incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1,
    incremental_body::BodyMutationError,
    incremental_commit::AcceptedRuntimeDirectoryCacheV1,
    incremental_factory_directory_records::AcceptedFactoryRecipeDirectoryV1,
    incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1,
    incremental_normal_cache_promotion::NormalWarmParentRefsV1,
    incremental_normal_cognition_service_evidence::AcceptedCognitionWakeEvidenceV2,
    incremental_normal_parent_caches::NormalParentCachesV1,
    incremental_normal_source_action_views::NormalActionParentCachesV1,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tick_driver::{NormalTickEnabledCognitionV2, NormalTickTailSourcesV1},
    incremental_normal_world_rule_lifecycle::NormalWorldRuleLifecycleSourcesV1,
    incremental_world_event_counter::WorldJournalRetentionV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, CurrentCognitionProofLedgerV1,
    },
    incremental_main_token_policy_records::RestrictedClaimGrantDirectoryV1,
    incremental_normal_gameplay_cycles_directory::GameplayCycleDirectoryV1,
    incremental_runtime_business::ThreatKeyDirectoryV1,
    incremental_world_native::IndustrialClaimDirectoryV1,
};
/// DATA inputs only. NoSources is accepted solely when the complete parent
/// router actually lacks this role and its real Cognition source is disabled.
pub(in crate::world) enum NormalTailCognitionInputsV1<'a> {
    NoSources,
    Present {
        ledger: &'a CurrentCognitionProofLedgerV1,
        accepted: &'a ReservedValue<AcceptedCognitionProofLedgerV1>,
        evidence: &'a ReservedValue<AcceptedCognitionWakeEvidenceV2>,
    },
}
/// Each field is a real original producer output. GEN/cold cannot substitute
/// an empty directory, caller root or allocation limit for a missing owner.
pub struct NormalTailBootstrapParentsV1<'a> {
    pub actions: &'a ReservedValue<NormalParentCachesV1>,
    pub recipes: &'a ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    pub claims: &'a ReservedValue<IndustrialClaimDirectoryV1>,
    pub gameplay: &'a ReservedValue<GameplayCycleDirectoryV1>,
    pub grants: &'a ReservedValue<RestrictedClaimGrantDirectoryV1>,
    pub threats: &'a ReservedValue<ThreatKeyDirectoryV1>,
}
pub enum NormalTailDirectoryInputsV1<'a> {
    Warm(&'a AcceptedRuntimeDirectoryCacheV1),
    GenesisOrCold(NormalTailBootstrapParentsV1<'a>),
}
enum BoundDirectoriesV1<'a> {
    Warm(ReservedValue<NormalWarmParentRefsV1<'a>>),
    GenesisOrCold(NormalTailBootstrapParentsV1<'a>),
}
pub(in crate::world) struct BoundNormalTickTailSourcesV1<'a, 'parent> {
    parent: &'a ReservedValue<BoundNormalTickParentV1<'parent>>,
    directories: BoundDirectoriesV1<'a>,
    retention: &'a ReservedValue<WorldJournalRetentionV1>,
    cognition: NormalTailCognitionInputsV1<'a>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("normal complete tail source parent")
}
#[derive(Debug)]
struct Cause<T: std::fmt::Debug>(T);
impl<T: std::fmt::Debug> std::fmt::Display for Cause<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl<T: std::fmt::Debug> std::error::Error for Cause<T> {}
fn original<T: std::fmt::Debug + Send + Sync + 'static>(e: T) -> BodyMutationError {
    BodyMutationError::Original(Box::new(Cause(e)))
}
struct View<'a> {
    actions: NormalActionParentCachesV1<'a>,
    recipes: &'a ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    claims: &'a ReservedValue<IndustrialClaimDirectoryV1>,
    gameplay: &'a ReservedValue<GameplayCycleDirectoryV1>,
    grants: &'a ReservedValue<RestrictedClaimGrantDirectoryV1>,
    threats: &'a ReservedValue<ThreatKeyDirectoryV1>,
}
impl BoundDirectoriesV1<'_> {
    fn view(&self) -> Result<View<'_>, BodyMutationError> {
        match self {
            Self::Warm(warm) => {
                let sealed = warm.sealed()?;
                Ok(View {
                    actions: warm.borrow(),
                    recipes: warm.records(),
                    claims: sealed.industrial_claim_parent(),
                    gameplay: sealed.gameplay_cycle_parent(),
                    grants: sealed.restricted_claim_grant_parent(),
                    threats: sealed.threat_key_parent(),
                })
            }
            Self::GenesisOrCold(s) => Ok(View {
                actions: s.actions.borrow(),
                recipes: s.recipes,
                claims: s.claims,
                gameplay: s.gameplay,
                grants: s.grants,
                threats: s.threats,
            }),
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn bind_normal_tick_tail_sources_v1<'a, 'parent, M: MerkleNodeReader>(
    parent: &'a ReservedValue<BoundNormalTickParentV1<'parent>>,
    input: NormalTailDirectoryInputsV1<'a>,
    retention: &'a ReservedValue<WorldJournalRetentionV1>,
    cognition: NormalTailCognitionInputsV1<'a>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<BoundNormalTickTailSourcesV1<'a, 'parent>>, BodyMutationError> {
    parent.validate_context(common).map_err(original)?;
    tx.validate_guard(&tx.guard(), live)?;
    if tx.parent_fence() != parent.fence()
        || !tx.uses_budget(common.memory())
        || !retention.uses_budget(common.memory())
        || retention.allocation_class() != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    let refs = parent.refs();
    let wire = crate::world::incremental_normal_sources::root_bytes(refs.roots, refs.roots)
        .map_err(original)?;
    common
        .charge(
            64,
            wire.checked_mul(16)
                .and_then(|n| n.checked_add(4096))
                .ok_or_else(bad)?,
        )
        .map_err(original)?;
    let directories = match input {
        NormalTailDirectoryInputsV1::Warm(cache) => {
            let warm = cache
                .bind_normal_warm_cache_v1(refs.runtime, refs.head, live, common)
                .map_err(original)?;
            warm.parent_caches().validate_parent_v1(parent, common)?;
            warm.sealed()?
                .validate_after_roots(refs.roots, common.memory())
                .map_err(original)?;
            BoundDirectoriesV1::Warm(warm)
        }
        NormalTailDirectoryInputsV1::GenesisOrCold(source) => {
            source.actions.validate_parent_v1(parent, common)?;
            BoundDirectoriesV1::GenesisOrCold(source)
        }
    };
    let view = directories.view()?;
    if !view.recipes.uses_budget(common.memory())
        || !view.claims.uses_budget(common.memory())
        || !view.gameplay.uses_budget(common.memory())
        || !view.grants.uses_budget(common.memory())
        || !view.threats.uses_budget(common.memory())
    {
        return Err(bad());
    }
    view.recipes
        .validate_tail_source_v1(&parent.fence(), common.memory())?;
    view.claims.validate_binding(refs.roots)?;
    view.gameplay
        .validate_binding(refs.roots, common.memory())?;
    view.grants.validate_binding(refs.roots)?;
    view.threats.validate_binding(refs.roots, common.memory())?;
    let route = refs.storage.results().proof_ledger();
    match (&cognition, route) {
        (NormalTailCognitionInputsV1::NoSources, None) => {
            if refs.storage.current().cognition().presence() != CognitionCurrentPresenceV3::Disabled
                || refs.cognition.selected().manifest().enabled
                || refs.cognition.current().enabled()
            {
                return Err(bad());
            }
        }
        (
            NormalTailCognitionInputsV1::Present {
                ledger,
                accepted,
                evidence,
            },
            Some(route),
        ) => {
            ledger.validate_context(tx, common, live)?;
            evidence.validate_parent(parent, accepted, common)?;
            let occurrence = accepted.occurrence();
            let selected = route.occurrence();
            if !accepted.uses_budget(common.memory())
                || !evidence.uses_budget(common.memory())
                || occurrence.head_record_id() != refs.head.verified_record_id()
                || occurrence.head_body_id() != refs.runtime.occurrence().head_body_id()
                || occurrence.fact_body_id() != occurrence.head_body_id()
                || occurrence.roots_id() != refs.runtime.manifest().roots
                || accepted.manifest().roots != refs.runtime.manifest().roots
                || selected.location().original != occurrence.location().original
                || selected.body_id() != occurrence.head_body_id()
                || selected.roots_id() != occurrence.roots_id()
            {
                return Err(bad());
            }
        }
        _ => return Err(bad()),
    }
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<BoundNormalTickTailSourcesV1<'_, '_>>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(BoundNormalTickTailSourcesV1 {
        parent,
        directories,
        retention,
        cognition,
        guard: tx.guard(),
        core: common.work().identity(),
        memory: common.memory().clone(),
    }))
}
impl BoundNormalTickTailSourcesV1<'_, '_> {
    pub(in crate::world) fn with_sources_v1<M: MerkleNodeReader, T>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        consume: impl FnOnce(
            &NormalTickTailSourcesV1<'_>,
            &mut ExecutionTransaction<'_, M>,
            &mut FormalBoundedContextV1<'_>,
        ) -> T,
    ) -> Result<T, BodyMutationError> {
        self.parent.validate_context(common).map_err(original)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || tx.parent_fence() != self.parent.fence()
        {
            return Err(bad());
        }
        if let NormalTailCognitionInputsV1::Present {
            ledger,
            accepted,
            evidence,
        } = &self.cognition
        {
            ledger.validate_context(tx, common, live)?;
            evidence.validate_parent(self.parent, accepted, common)?;
        }
        let bytes = std::mem::size_of::<FactoryScheduleSourcesV1<'_>>()
            + std::mem::size_of::<NormalWorldRuleLifecycleSourcesV1<'_>>()
            + std::mem::size_of::<NormalTickTailSourcesV1<'_>>();
        common.charge(24, bytes as u64).map_err(original)?;
        let _layout = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let view = self.directories.view()?;
        let refs = self.parent.refs();
        let current = refs.storage.current();
        let native = current.native();
        let raw = current.raw();
        let runtime = current.runtime();
        let factory = FactoryScheduleSourcesV1 {
            roots: refs.roots,
            lifecycle: FactoryLifecycleSourcesV1 {
                agents: raw.agents(),
                factories: native.factory(),
                industry: native.industry(),
                index: native.factory_lifecycle_index(),
                inventory: native.inventory(),
                primitive: native.primitive(),
                native: native.native(),
            },
            builds: view.actions.builds,
            receipts: view.actions.receipts,
            inventory_directory: view.actions.inventory_identity,
            factory_authority: native.factory_authority(),
            agent_authority: native.agent_authority(),
            logistics: native.logistics(),
            material_profiles: native.material(),
            artifact_config: runtime.config(),
            module_instances: raw.instances(),
            invocation_directory: view.actions.invocations,
        };
        let economic = NormalWorldRuleLifecycleSourcesV1 {
            rules: native.rules(),
            orders: view.actions.orders,
            factory: &factory,
            governance: runtime.governance(),
            revocation: runtime.revocation(),
            artifacts: runtime.resources(),
            claims: view.claims,
            fixed: native.fixed(),
            retention: self.retention,
        };
        let cognition = match &self.cognition {
            NormalTailCognitionInputsV1::NoSources => None,
            NormalTailCognitionInputsV1::Present {
                ledger,
                accepted,
                evidence,
            } => Some(NormalTickEnabledCognitionV2 {
                ledger,
                accepted_ledger: accepted,
                evidence,
            }),
        };
        let sources = NormalTickTailSourcesV1 {
            economic: &economic,
            recipes: view.recipes,
            gameplay: view.gameplay,
            grants: view.grants,
            threats: view.threats,
            cognition,
        };
        Ok(consume(&sources, tx, common))
    }
}
