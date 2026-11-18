//! Build the complete tail directories once from the authenticated GEN/cold
//! parent. Ordinary execution only borrows these owners or its accepted cache.
use super::*;
use crate::world::{
    incremental_factory_directory_records::{
        AcceptedFactoryRecipeDirectoryV1, FactoryRecipeDirectoryWorkV1,
        build_accepted_factory_recipe_directory_v1,
    },
    incremental_normal_host_work::NormalHostReadWorkV1,
    incremental_normal_tail_sources::{NormalTailBootstrapParentsV1, NormalTailDirectoryInputsV1},
};
use oasis7_world_state::{
    incremental_factory_records::{FactoryFamilyReadContextV1, FactoryWorkBudgetV1},
    incremental_industry_jobs::{IndustryFamilyReadContextV1, IndustryWorkBudgetV1},
    incremental_main_token_policy_records::{
        MainTokenPolicyKindV1, RestrictedClaimGrantDirectoryV1,
        build_restricted_claim_grant_directory_v1,
    },
    incremental_normal_gameplay_cycles_directory::{
        GameplayCycleDirectoryV1, GameplayDirectoryWorkV1, build_gameplay_cycle_directory_v1,
    },
    incremental_runtime_business::{
        ThreatKeyDirectoryV1, ThreatKeyDirectoryWorkV1, build_threat_key_directory_v1,
    },
    incremental_world_native::{
        IndustrialClaimDirectoryV1, IndustrialClaimDirectoryWorkV1, WorldNativeSelectedReadWorkV1,
        build_industrial_claim_directory_v1, industrial_claim_namespace_v1,
    },
};

#[path = "incremental_normal_host_resident_tail_meter.rs"]
mod meter;

/// Explicit remaining loans from the Host's original work. Factory/industry
/// work is shared with its action-cache bootstrap rather than copied here.
pub struct NormalHostBootstrapTailWorkV1<'a> {
    pub factory_read: &'a mut NormalHostReadWorkV1,
    pub recipe_read: &'a mut NormalHostReadWorkV1,
    pub factory_scan: &'a mut StateScanBudget,
    pub recipe_scan: &'a mut StateScanBudget,
    pub outer_scan: &'a mut StateScanBudget,
    pub inner_scan: &'a mut StateScanBudget,
    pub recipes: &'a mut FactoryRecipeDirectoryWorkV1,
    pub native_read: &'a mut WorldNativeSelectedReadWorkV1,
    pub claims: &'a mut IndustrialClaimDirectoryWorkV1,
    pub gameplay: &'a mut GameplayDirectoryWorkV1,
    pub grant_cells: &'a mut u64,
    pub threats: &'a mut ThreatKeyDirectoryWorkV1,
    pub max_grant_payload_bytes: u64,
}

pub(super) struct NormalHostBootstrapTailSourcesV1 {
    recipes: ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    claims: ReservedValue<IndustrialClaimDirectoryV1>,
    gameplay: ReservedValue<GameplayCycleDirectoryV1>,
    grants: ReservedValue<RestrictedClaimGrantDirectoryV1>,
    threats: ReservedValue<ThreatKeyDirectoryV1>,
    record: [u8; 32],
    fence: ExecutionFence,
    core: WorkIdentityV1,
}

impl NormalHostBootstrapTailSourcesV1 {
    fn validate(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        common.charge(8, 224).map_err(formal)?;
        if self.record != parent.refs().head.verified_record_id()
            || self.fence != parent.fence()
            || self.core != common.work().identity()
            || !self.recipes.uses_budget(common.memory())
            || !self.claims.uses_budget(common.memory())
            || !self.gameplay.uses_budget(common.memory())
            || !self.grants.uses_budget(common.memory())
            || !self.threats.uses_budget(common.memory())
        {
            return Err(bad());
        }
        self.recipes
            .validate_tail_source_v1(&parent.fence(), common.memory())?;
        self.claims.validate_binding(parent.refs().roots)?;
        self.gameplay
            .validate_binding(parent.refs().roots, common.memory())?;
        self.grants.validate_binding(parent.refs().roots)?;
        self.threats
            .validate_binding(parent.refs().roots, common.memory())?;
        Ok(())
    }
}

impl NormalHostResidentSourcesV1 {
    /// The caller cannot substitute a directory or choose warm versus cold.
    /// Its real resident owns that distinction and every retained source.
    pub(in crate::world) fn tail_directories_v1(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<NormalTailDirectoryInputsV1<'_>, BodyMutationError> {
        self.validate_live_binding_v1(
            parent.refs().head.verified_record_id(),
            &parent.fence(),
            common,
        )?;
        parent.validate_context(common).map_err(formal)?;
        match &self.storage {
            ResidentStorage::Built { caches, tail, .. } => {
                if !tail.uses_budget(common.memory()) {
                    return Err(bad());
                }
                caches.validate_parent_v1(parent, common)?;
                tail.validate(parent, common)?;
                Ok(NormalTailDirectoryInputsV1::GenesisOrCold(
                    NormalTailBootstrapParentsV1 {
                        actions: caches,
                        recipes: &tail.recipes,
                        claims: &tail.claims,
                        gameplay: &tail.gameplay,
                        grants: &tail.grants,
                        threats: &tail.threats,
                    },
                ))
            }
            ResidentStorage::Warm { directories, .. } => {
                // The fixed binder validates the actual accepted cache, parent,
                // roots and source pool before executing the first stage.
                Ok(NormalTailDirectoryInputsV1::Warm(directories))
            }
        }
    }
}

struct Loan<'a, 'tx, 's, 'v, 'r, 'w, M, R, E, C> {
    ctx: &'a mut WorldFieldOverlayContextV1<'tx, 's, 'v, M, R, E, C>,
    read: &'a mut WorldFieldReadContextV1<'r, M, R, E, C>,
    factory: &'a mut FactoryWorkBudgetV1,
    industry: &'a mut IndustryWorkBudgetV1,
    work: &'a mut NormalHostBootstrapTailWorkV1<'w>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build<M, R, E, C>(
    parent: &BoundNormalTickParentV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    factory: &mut FactoryWorkBudgetV1,
    industry: &mut IndustryWorkBudgetV1,
    work: &mut NormalHostBootstrapTailWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<NormalHostBootstrapTailSourcesV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
{
    parent.validate_context(common).map_err(formal)?;
    ctx.transaction.validate_bootstrap_read_only_v1(live)?;
    if ctx.transaction.parent_fence() != parent.fence()
        || !ctx.transaction.uses_budget(common.memory())
        || !ctx.values.memory.same_pool(common.memory())
        || !read.values.memory.same_pool(common.memory())
        || ctx.values.class != MemoryClass::Ordinary
        || read.values.class != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    let bytes = std::mem::size_of::<NormalHostBootstrapTailSourcesV1>();
    common.charge(1, bytes as u64).map_err(formal)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let core = common.work().identity();
    let mut loan = Loan {
        ctx,
        read,
        factory,
        industry,
        work,
    };
    let sources = meter::run(&mut loan, common, |loan, stop| {
        build_in_child(parent, loan, core, stop, live)
    })?;
    loan.ctx.transaction.validate_bootstrap_read_only_v1(live)?;
    sources.validate(parent, common)?;
    Ok(fee.retain(sources))
}

fn build_in_child<M, R, E, C>(
    parent: &BoundNormalTickParentV1<'_>,
    loan: &mut Loan<'_, '_, '_, '_, '_, '_, M, R, E, C>,
    core: WorkIdentityV1,
    stop: &mut dyn FnMut() -> bool,
    live: &ExecutionFence,
) -> Result<NormalHostBootstrapTailSourcesV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
{
    let refs = parent.refs();
    let native = refs.storage.current().native();
    let runtime = refs.storage.current().runtime();
    let cancelled =
        std::cell::RefCell::new((stop, &mut loan.ctx.cancelled, &mut loan.read.cancelled));
    let poll = || {
        let mut callbacks = cancelled.borrow_mut();
        callbacks.0() || callbacks.1() || callbacks.2()
    };
    if poll() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let recipes = {
        let mut factory = FactoryFamilyReadContextV1 {
            state: loan.read.state,
            values: ValueReadContext {
                source: loan.read.values.source,
                memory: loan.read.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut loan.work.factory_read.io,
            },
            key_work: &mut loan.work.factory_read.keys,
            semantic_bytes: loan.work.factory_read.semantic_bytes,
            record_bytes: loan.work.factory_read.record_bytes,
            max_payload_bytes: loan.work.factory_read.max_payload_bytes,
            outer_scan: loan.work.factory_scan,
            take_state_error: loan.read.take_state_error,
            cancelled: &poll,
        };
        let mut recipe = IndustryFamilyReadContextV1 {
            state: loan.read.state,
            values: ValueReadContext {
                source: loan.read.values.source,
                memory: loan.read.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut loan.work.recipe_read.io,
            },
            key_work: &mut loan.work.recipe_read.keys,
            semantic_bytes: loan.work.recipe_read.semantic_bytes,
            record_bytes: loan.work.recipe_read.record_bytes,
            max_payload_bytes: loan.work.recipe_read.max_payload_bytes,
            outer_scan: loan.work.recipe_scan,
            take_state_error: loan.read.take_state_error,
            cancelled: &poll,
        };
        let guard = loan.ctx.transaction.guard();
        build_accepted_factory_recipe_directory_v1(
            refs.runtime,
            refs.head,
            refs.roots,
            native.factory(),
            native.industry(),
            loan.ctx.transaction,
            &mut factory,
            &mut recipe,
            loan.factory,
            loan.industry,
            loan.work.recipes,
            &guard,
            live,
        )
        .map_err(original)?
    };
    let mut read = WorldFieldReadContextV1 {
        root: refs.roots.state_root(industrial_claim_namespace_v1()?),
        state: loan.read.state,
        values: ValueReadContext {
            source: loan.read.values.source,
            memory: loan.read.values.memory,
            class: MemoryClass::Ordinary,
            io: &mut *loan.read.values.io,
        },
        key_work: &mut *loan.read.key_work,
        semantic_bytes: loan.read.semantic_bytes,
        record_bytes: loan.read.record_bytes,
        take_state_error: loan.read.take_state_error,
        cancelled: &poll,
    };
    let claims = build_industrial_claim_directory_v1(
        native.native(),
        refs.roots,
        &mut read,
        loan.work.outer_scan,
        loan.work.native_read,
        loan.work.claims,
    )?;
    read.root = refs
        .roots
        .state_root(MainTokenPolicyKindV1::Grant.namespace());
    let grants = build_restricted_claim_grant_directory_v1(
        native.main_policy(),
        refs.roots,
        &mut read,
        loan.work.outer_scan,
        loan.work.grant_cells,
        loan.work.max_grant_payload_bytes,
    )?;
    read.root = refs.roots.state_root(StateNamespaceV1::Authorization);
    let gameplay = build_gameplay_cycle_directory_v1(
        native.diplomacy(),
        native.governance(),
        refs.roots,
        &mut read,
        loan.work.outer_scan,
        loan.work.inner_scan,
        loan.work.gameplay,
    )?;
    read.root = refs.roots.state_root(StateNamespaceV1::Business);
    let threats = build_threat_key_directory_v1(
        runtime.business(),
        refs.roots,
        &mut read,
        loan.work.outer_scan,
        loan.work.threats,
    )?;
    read.check()?;
    Ok(NormalHostBootstrapTailSourcesV1 {
        recipes,
        claims,
        gameplay,
        grants,
        threats,
        record: refs.head.verified_record_id(),
        fence: parent.fence(),
        core,
    })
}
