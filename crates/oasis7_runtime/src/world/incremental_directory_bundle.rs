//! Authentic final Current directory capture and real-seal binding.
//! No history candidate, durable acceptance or head promotion is minted here.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_factory_directory_orders::*,
    incremental_factory_directory_records::*,
    incremental_factory_phase::{
        FactoryPhaseAcceptedCachesV1, FactoryPhaseFamiliesV1, FactoryPhaseWorkV1,
    },
    incremental_module_invocation::{
        CurrentInvocationEnumerationErrorV1, FrozenInvocationDirectoryV1,
        RootModuleExecutionContextV1, collect_current_module_invocation_ids_v1,
        freeze_current_invocation_directory_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
        SealedExecutionBindingV1,
    },
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_runtime_business::{
    FrozenThreatKeyDirectoryV1, RuntimeBusinessQualificationV1, ThreatKeyDirectoryV1,
    ThreatKeyDirectoryWorkV1, freeze_current_threat_key_directory_v1,
};
use oasis7_world_state::{
    incremental_diplomacy::DiplomacyFamilyQualificationV1,
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_governance_records::GovernanceFamilyQualificationV1,
    incremental_industry_jobs::IndustryTransitionReadContextV1,
    incremental_inventory::{
        FrozenInventoryIdentityDirectoryV1, FrozenInventoryMaterialScopeV1,
        InventoryFamilyQualificationV1, InventoryIdentityDirectoryV1,
        InventoryIdentityDirectoryWorkV1, InventoryMaterialScopeWorkV1,
        ParentInventoryMaterialScopeV1, freeze_current_inventory_identity_directory_v1,
        freeze_current_inventory_material_scope_v1, open_current_inventory_material_scope_v1,
    },
    incremental_main_token_policy_records::{
        CurrentRestrictedClaimGrantSnapshotV1, MainTokenPolicyFamilyQualificationV1,
        RestrictedClaimGrantDirectoryV1, recover_current_restricted_claim_grant_snapshot_v1,
    },
    incremental_normal_gameplay_cycles_directory::{
        FrozenGameplayCycleDirectoryV1, GameplayCycleDirectoryV1, GameplayDirectoryWorkV1,
        freeze_current_gameplay_cycle_directory_v1,
    },
    incremental_world_native::{
        FrozenIndustrialClaimDirectoryV1, IndustrialClaimDirectoryV1,
        IndustrialClaimDirectoryWorkV1, WorldNativeFieldsQualificationV1,
        freeze_current_industrial_claim_directory_v1, industrial_claim_namespace_v1,
    },
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
};
use oasis7_world_state::{
    incremental_factory_schedule_index::{
        FactoryScheduleIndexV1, FactoryScheduleIndexWorkV1, FrozenFactoryScheduleBuildsV1,
        freeze_current_factory_schedule_builds_v1, observe_current_factory_schedule_builds_v1,
    },
    incremental_factory_schedule_receipts::{
        FactoryScheduleReceiptIndexV1, FactoryScheduleReceiptWorkV1,
        FrozenFactoryScheduleReceiptsV1, freeze_current_factory_schedule_receipts_v1,
        snapshot_current_factory_schedule_receipts_v1,
    },
};
#[derive(Debug)]
pub(in crate::world) enum RuntimeDirectoryFreezeErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Directory(FactoryOrderDirectoryErrorV1),
    Invocation(CurrentInvocationEnumerationErrorV1),
    Overlay(WorldFieldOverlayErrorV1),
}
macro_rules! from_error {
    ($ty:ty,$variant:ident) => {
        impl From<$ty> for RuntimeDirectoryFreezeErrorV1 {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(ExecutionError, Execution);
from_error!(ValueTreeError, Value);
from_error!(FactoryOrderDirectoryErrorV1, Directory);
from_error!(CurrentInvocationEnumerationErrorV1, Invocation);
from_error!(WorldFieldOverlayErrorV1, Overlay);
/// All members are genuine private typed snapshots. Readiness is not a commit
/// permission: only the actual runtime prepare/seal path may later bind this.
pub(in crate::world) struct FrozenRuntimeDirectoryBundleV1 {
    guard: PreparedGuard,
    memory: MemoryBudget,
    builds: ReservedValue<FrozenFactoryScheduleBuildsV1>,
    receipts: ReservedValue<FrozenFactoryScheduleReceiptsV1>,
    orders: ReservedValue<FrozenFactoryOrderDirectoryV1>,
    records: ReservedValue<FrozenFactoryRecipeDirectoryV1>,
    invocations: ReservedValue<FrozenInvocationDirectoryV1>,
    materials: ReservedValue<FrozenInventoryMaterialScopeV1>,
    inventory_identities: ReservedValue<FrozenInventoryIdentityDirectoryV1>,
    claims: ReservedValue<FrozenIndustrialClaimDirectoryV1>,
    grants: ReservedValue<CurrentRestrictedClaimGrantSnapshotV1>,
    gameplay: ReservedValue<FrozenGameplayCycleDirectoryV1>,
    threats: ReservedValue<FrozenThreatKeyDirectoryV1>,
}
impl FrozenRuntimeDirectoryBundleV1 {
    pub(in crate::world) fn iter_restricted_claim_grants(&self)->impl ExactSizeIterator<Item=&oasis7_world_state::incremental_main_token_policy_records::RestrictedClaimGrantIdentityV1>{
        self.grants.entries()
    }

    pub(in crate::world) fn iter_factory_ids(&self) -> impl ExactSizeIterator<Item = &str> {
        self.records.iter_factory_ids()
    }
    pub(in crate::world) fn iter_recipe_job_ids(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.records.iter_recipe_job_ids()
    }
    pub(in crate::world) fn iter_order_indices(
        &self,
    ) -> impl ExactSizeIterator<
        Item = (
            &str,
            &oasis7_world_state::incremental_world_rule_composite_family::CurrentAgentOrderIndexV1,
        ),
    > {
        self.orders.iter_order_indices()
    }

    pub(in crate::world) fn validate_terminal_guard<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), RuntimeDirectoryFreezeErrorV1> {
        let result = (|| {
            tx.validate_guard(&self.guard, live)?;
            if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
                return Err(ValueTreeError::Invalid("frozen directory bundle pool").into());
            }
            self.builds.validate_guard(tx, memory, live)?;
            self.receipts.validate_guard(tx, memory, live)?;
            self.orders.validate_guard(tx, memory, live)?;
            self.records.validate_guard(tx, memory, live)?;
            self.invocations.validate_guard(tx, memory, live)?;
            self.materials.validate_guard(tx, memory, live)?;
            self.inventory_identities.validate_guard(tx, memory, live)?;
            self.claims.validate_guard(tx, memory, live)?;
            self.grants.validate_guard(tx, memory, live)?;
            self.gameplay.validate_guard(tx, memory, live)?;
            self.threats.validate_guard(tx, memory, live)?;
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
    pub(in crate::world) fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), RuntimeDirectoryFreezeErrorV1> {
        if !self.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("sealed directory bundle pool").into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        self.builds.validate_sealed(binding, roots, memory)?;
        self.receipts.validate_sealed(binding, roots, memory)?;
        self.orders.validate_sealed(binding, roots, memory)?;
        self.records.validate_sealed(binding, roots, memory)?;
        self.invocations.validate_sealed(binding, roots, memory)?;
        self.materials.validate_sealed(binding, roots, memory)?;
        self.inventory_identities
            .validate_sealed(binding, roots, memory)?;
        self.claims.validate_sealed(binding, roots, memory)?;
        self.grants.validate_sealed(binding, roots, memory)?;
        self.gameplay.validate_sealed(binding, roots, memory)?;
        self.threats.validate_sealed(binding, roots, memory)?;
        Ok(())
    }
}
/// Owning structural output authenticated by the actual seal witness. It is
/// deliberately not an Accepted cache bundle; candidate/VerifiedFormalCommit
/// binding and the real durable Host publication remain separate obligations.
pub(in crate::world) struct SealedRuntimeDirectoryBundleV1 {
    frozen: std::sync::Arc<SharedFrozenRuntimeDirectoryBundleV1>,
    binding: ReservedValue<SealedExecutionBindingV1>,
    builds: ReservedValue<FactoryScheduleIndexV1>,
    receipts: ReservedValue<FactoryScheduleReceiptIndexV1>,
    materials: ReservedValue<ParentInventoryMaterialScopeV1>,
    inventory_identities: ReservedValue<InventoryIdentityDirectoryV1>,
    claims: ReservedValue<IndustrialClaimDirectoryV1>,
    grants: ReservedValue<RestrictedClaimGrantDirectoryV1>,
    gameplay: ReservedValue<GameplayCycleDirectoryV1>,
    threats: ReservedValue<ThreatKeyDirectoryV1>,
}
pub(in crate::world) struct SharedFrozenRuntimeDirectoryBundleV1 {
    frozen: ReservedValue<FrozenRuntimeDirectoryBundleV1>,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl SharedFrozenRuntimeDirectoryBundleV1 {
    pub(in crate::world) fn orders(&self) -> &FrozenFactoryOrderDirectoryV1 {
        &self.frozen.orders
    }
    pub(in crate::world) fn records(&self) -> &FrozenFactoryRecipeDirectoryV1 {
        &self.frozen.records
    }
    pub(in crate::world) fn invocations(&self) -> &FrozenInvocationDirectoryV1 {
        &self.frozen.invocations
    }
    pub(in crate::world) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.frozen.uses_budget(memory) && self._fee.uses_budget(memory)
    }
}
impl SealedRuntimeDirectoryBundleV1 {
    pub(in crate::world) fn build_parent(&self) -> &ReservedValue<FactoryScheduleIndexV1> {
        &self.builds
    }
    pub(in crate::world) fn receipt_parent(&self) -> &ReservedValue<FactoryScheduleReceiptIndexV1> {
        &self.receipts
    }
    pub(in crate::world) fn shared_frozen(
        &self,
    ) -> &std::sync::Arc<SharedFrozenRuntimeDirectoryBundleV1> {
        &self.frozen
    }
    pub(in crate::world) fn threat_key_parent(&self) -> &ReservedValue<ThreatKeyDirectoryV1> {
        &self.threats
    }

    pub(in crate::world) fn gameplay_cycle_parent(
        &self,
    ) -> &ReservedValue<GameplayCycleDirectoryV1> {
        &self.gameplay
    }

    pub(in crate::world) fn restricted_claim_grant_parent(
        &self,
    ) -> &ReservedValue<RestrictedClaimGrantDirectoryV1> {
        &self.grants
    }

    pub(in crate::world) fn industrial_claim_parent(
        &self,
    ) -> &ReservedValue<IndustrialClaimDirectoryV1> {
        &self.claims
    }
    pub(in crate::world) fn after_roots_digest(&self) -> [u8; 32] {
        self.binding.after_roots_digest()
    }

    pub(in crate::world) fn material_parent(
        &self,
    ) -> &ReservedValue<ParentInventoryMaterialScopeV1> {
        &self.materials
    }
    pub(in crate::world) fn inventory_identity_parent(
        &self,
    ) -> &ReservedValue<InventoryIdentityDirectoryV1> {
        &self.inventory_identities
    }
    pub(in crate::world) fn validate_after_roots(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), RuntimeDirectoryFreezeErrorV1> {
        self.frozen
            .frozen
            .validate_sealed(&self.binding, roots, memory)
    }
}
pub(in crate::world) fn bind_runtime_directories_to_actual_seal_v1(
    frozen: ReservedValue<FrozenRuntimeDirectoryBundleV1>,
    binding: ReservedValue<SealedExecutionBindingV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    work: &mut InventoryMaterialScopeWorkV1,
    identity_work: &mut InventoryIdentityDirectoryWorkV1,
    claim_work: &mut IndustrialClaimDirectoryWorkV1,
    grant_cells: &mut u64,
    gameplay_work: &mut GameplayDirectoryWorkV1,
    threat_work: &mut ThreatKeyDirectoryWorkV1,
    build_work: &mut FactoryScheduleIndexWorkV1,
    receipt_work: &mut FactoryScheduleReceiptWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<SealedRuntimeDirectoryBundleV1>, RuntimeDirectoryFreezeErrorV1> {
    if !frozen.uses_budget(memory) || !binding.uses_budget(memory) {
        return Err(ValueTreeError::Invalid("sealed directory owner pool").into());
    }
    frozen.validate_sealed(&binding, roots, memory)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let fee = memory
        .reserve(
            std::mem::size_of::<SealedRuntimeDirectoryBundleV1>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let builds =
        frozen
            .builds
            .rebind_after_seal_v1(&binding, roots, memory, build_work, &mut cancelled)?;
    let receipts = frozen.receipts.rebind_after_seal_v1(
        &binding,
        roots,
        memory,
        receipt_work,
        &mut cancelled,
    )?;
    let materials =
        frozen
            .materials
            .rebind_after_seal_v1(&binding, roots, memory, work, &mut cancelled)?;
    let inventory_identities = frozen.inventory_identities.rebind_after_seal_v1(
        &binding,
        roots,
        memory,
        identity_work,
        &mut cancelled,
    )?;
    let claims =
        frozen
            .claims
            .rebind_after_seal_v1(&binding, roots, memory, claim_work, &mut cancelled)?;
    let grants =
        frozen
            .grants
            .rebind_after_seal_v1(&binding, roots, memory, grant_cells, &mut cancelled)?;
    let gameplay = frozen.gameplay.rebind_after_seal_v1(
        &binding,
        roots,
        memory,
        gameplay_work,
        &mut cancelled,
    )?;
    let threats = frozen.threats.rebind_after_seal_v1(
        &binding,
        roots,
        memory,
        threat_work,
        &mut cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let shared_fee = memory
        .reserve(
            std::mem::size_of::<SharedFrozenRuntimeDirectoryBundleV1>()
                + 2 * std::mem::size_of::<usize>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let frozen = std::sync::Arc::new(SharedFrozenRuntimeDirectoryBundleV1 {
        frozen,
        _fee: shared_fee,
    });
    Ok(fee.retain(SealedRuntimeDirectoryBundleV1 {
        frozen,
        binding,
        builds,
        receipts,
        materials,
        inventory_identities,
        claims,
        grants,
        gameplay,
        threats,
    }))
}

#[allow(clippy::too_many_arguments)]
fn capture_claims<M, S, R, E, C>(
    directory: &ReservedValue<IndustrialClaimDirectoryV1>,
    native: &ReservedValue<WorldNativeFieldsQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    typed: &mut oasis7_world_state::incremental_world_rules_family::WorldRulesTypedReadWorkV1,
    work: &mut IndustrialClaimDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenIndustrialClaimDirectoryV1>, RuntimeDirectoryFreezeErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: roots.state_root(industrial_claim_namespace_v1()?),
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: context.values.source,
            memory: context.values.memory,
            class: context.values.class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: &mut context.cancelled,
    };
    Ok(freeze_current_industrial_claim_directory_v1(
        directory,
        native,
        roots,
        context.transaction,
        &mut read,
        scan,
        typed,
        work,
        guard,
        live,
    )?)
}

#[allow(clippy::too_many_arguments)]
fn capture_gameplay<M, S, R, E, C>(
    directory: &ReservedValue<GameplayCycleDirectoryV1>,
    diplomacy: &ReservedValue<DiplomacyFamilyQualificationV1>,
    governance: &ReservedValue<GovernanceFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenGameplayCycleDirectoryV1>, RuntimeDirectoryFreezeErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: context.values.source,
            memory: context.values.memory,
            class: context.values.class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: &mut context.cancelled,
    };
    Ok(freeze_current_gameplay_cycle_directory_v1(
        directory,
        diplomacy,
        governance,
        roots,
        context.transaction,
        &mut read,
        scan,
        work,
        guard,
        live,
    )?)
}

#[allow(clippy::too_many_arguments)]
fn capture_threats<M, S, R, E, C>(
    directory: &ReservedValue<ThreatKeyDirectoryV1>,
    business: &ReservedValue<RuntimeBusinessQualificationV1>,
    roots: &RootSetV1,
    state: &S,
    context: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    work: &mut ThreatKeyDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenThreatKeyDirectoryV1>, RuntimeDirectoryFreezeErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: context.values.source,
            memory: context.values.memory,
            class: context.values.class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: &mut context.cancelled,
    };
    Ok(freeze_current_threat_key_directory_v1(
        directory,
        business,
        roots,
        context.transaction,
        &mut read,
        scan,
        work,
        guard,
        live,
    )?)
}

impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn freeze_current_runtime_directories_v1<FR, FC, RR, RC>(
        &mut self,
        caches: &FactoryPhaseAcceptedCachesV1<'_>,
        families: &FactoryPhaseFamiliesV1<'_>,
        factory: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        work: &mut FactoryPhaseWorkV1,
        inventory_directory: &ReservedValue<InventoryIdentityDirectoryV1>,
        inventory_family: &ReservedValue<InventoryFamilyQualificationV1>,
        inventory_identity_work: &mut InventoryIdentityDirectoryWorkV1,
        claim_directory: &ReservedValue<IndustrialClaimDirectoryV1>,
        native: &ReservedValue<WorldNativeFieldsQualificationV1>,
        claim_work: &mut IndustrialClaimDirectoryWorkV1,
        grant_directory: &ReservedValue<RestrictedClaimGrantDirectoryV1>,
        main_policy: &ReservedValue<MainTokenPolicyFamilyQualificationV1>,
        grant_cells: &mut u64,
        grant_pending: &mut usize,
        max_grant_payload: u64,
        gameplay_directory: &ReservedValue<GameplayCycleDirectoryV1>,
        diplomacy: &ReservedValue<DiplomacyFamilyQualificationV1>,
        governance: &ReservedValue<GovernanceFamilyQualificationV1>,
        gameplay_work: &mut GameplayDirectoryWorkV1,
        threat_directory: &ReservedValue<ThreatKeyDirectoryV1>,
        business: &ReservedValue<RuntimeBusinessQualificationV1>,
        threat_work: &mut ThreatKeyDirectoryWorkV1,
        build_parent: &ReservedValue<FactoryScheduleIndexV1>,
        receipt_parent: &ReservedValue<FactoryScheduleReceiptIndexV1>,
        build_work: &mut FactoryScheduleIndexWorkV1,
        receipt_work: &mut FactoryScheduleReceiptWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<FrozenRuntimeDirectoryBundleV1>, RuntimeDirectoryFreezeErrorV1>
    where
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
    {
        let result = (|| {
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            self.ctx.transaction.validate_guard(&guard, &live)?;
            if self.ctx.values.class != MemoryClass::Ordinary
                || !self.ctx.transaction.uses_budget(self.ctx.values.memory)
            {
                return Err(ValueTreeError::Invalid("directory freeze pool/class").into());
            }
            let fee = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<FrozenRuntimeDirectoryBundleV1>() + 4096,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let build_snapshot = observe_current_factory_schedule_builds_v1(
                build_parent,
                families.factories,
                self.families.roots,
                self.ctx,
                &mut work.factory,
                build_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let receipt_snapshot = snapshot_current_factory_schedule_receipts_v1(
                receipt_parent,
                families.recipes,
                self.families.roots,
                self.ctx,
                &mut work.recipe,
                receipt_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            // Establish ALL actual read-set paths first. Later private snapshot guards
            // must agree with the terminal guard; an unexpected new read fails closed.
            let _selected = collect_current_module_invocation_ids_v1(
                caches.invocations,
                self.families.instances,
                self.families.config,
                self.state,
                self.ctx,
                &mut work.enumeration,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let source = bind_current_agent_orders_v1(
                families.orders,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                &mut work.typed,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let current_orders = open_current_factory_order_directory_v1(
                caches.orders,
                &source,
                self.state,
                self.ctx,
                &mut work.scan,
                &mut work.typed,
                &mut work.orders,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let current_records = open_current_factory_recipe_directory_v1(
                caches.records,
                families.factories,
                families.recipes,
                self.ctx.transaction,
                factory,
                recipe,
                &mut work.factory,
                &mut work.recipe,
                &mut work.records,
                &guard,
                &live,
            )?;
            // Establish claim read-set paths before capturing any terminal
            // directory guard. The final capture below repeats only pending
            // changes and selected leaf joins, never a cold all-claim scan.
            let guard = self.ctx.transaction.guard();
            let _claim_read_set = capture_claims(
                claim_directory,
                native,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                &mut work.typed,
                claim_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let _grant_read_set = recover_current_restricted_claim_grant_snapshot_v1(
                grant_directory,
                main_policy,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                grant_cells,
                max_grant_payload,
                grant_pending,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let _gameplay_read_set = capture_gameplay(
                gameplay_directory,
                diplomacy,
                governance,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                gameplay_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let _threat_read_set = capture_threats(
                threat_directory,
                business,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                threat_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let invocations = freeze_current_invocation_directory_v1(
                caches.invocations,
                self.families.instances,
                self.families.config,
                self.state,
                self.ctx,
                &mut work.enumeration,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let orders = freeze_current_factory_order_directory_v1(
                &current_orders,
                self.ctx,
                &mut work.orders,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let records = freeze_current_factory_recipe_directory_v1(
                &current_records,
                self.ctx.transaction,
                factory,
                recipe,
                &mut work.factory,
                &mut work.recipe,
                &mut work.records,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let current_materials = open_current_inventory_material_scope_v1(
                caches.materials,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.material_family,
                &mut work.material_scope,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let materials = freeze_current_inventory_material_scope_v1(
                current_materials,
                self.ctx,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let inventory_identities = freeze_current_inventory_identity_directory_v1(
                inventory_directory,
                inventory_family,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.material_family,
                inventory_identity_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let claims = capture_claims(
                claim_directory,
                native,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                &mut work.typed,
                claim_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let grants = recover_current_restricted_claim_grant_snapshot_v1(
                grant_directory,
                main_policy,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                grant_cells,
                max_grant_payload,
                grant_pending,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let gameplay = capture_gameplay(
                gameplay_directory,
                diplomacy,
                governance,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                gameplay_work,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let threats = capture_threats(
                threat_directory,
                business,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                threat_work,
                &guard,
                &live,
            )?;
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let builds = freeze_current_factory_schedule_builds_v1(
                build_snapshot,
                self.ctx.transaction,
                build_work,
                &live,
                &mut self.ctx.cancelled,
            )?;
            let receipts = freeze_current_factory_schedule_receipts_v1(
                receipt_snapshot,
                self.ctx.transaction,
                receipt_work,
                &live,
                &mut self.ctx.cancelled,
            )?;
            let value = fee.retain(FrozenRuntimeDirectoryBundleV1 {
                guard: self.ctx.transaction.guard(),
                memory: self.ctx.values.memory.clone(),
                builds,
                receipts,
                orders,
                records,
                invocations,
                materials,
                inventory_identities,
                claims,
                grants,
                gameplay,
                threats,
            });
            let terminal_live = live_owner();
            value.validate_terminal_guard(
                self.ctx.transaction,
                self.ctx.values.memory,
                &terminal_live,
            )?;
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(value)
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = self
                .ctx
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
