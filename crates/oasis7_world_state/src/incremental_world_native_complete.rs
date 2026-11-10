//! Mandatory conjunction of all 97 native WorldState fields.
use crate::{
    incremental_agent_authority_records as agent_authority, incremental_diplomacy as diplomacy,
    incremental_factory_authority_records as factory_authority,
    incremental_factory_records as factory,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_fixed_economy_records as fixed, incremental_governance_records as governance,
    incremental_industry_jobs as industry, incremental_inventory as inventory,
    incremental_logistics_records as logistics,
    incremental_main_token_policy_records as main_policy, incremental_material_records as material,
    incremental_module_marketplace as market, incremental_owner_closure as primitive,
    incremental_owner_closure::WorldStateOwnerError,
    incremental_reward_budget_records as reward,
    incremental_verified_world_transition::{
        VerifiedWorldTransitionV1, verify_world_transition_v1,
    },
    incremental_world_collection_controls as controls, incremental_world_native as native,
    incremental_world_rules_family as rules,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
/// All allowances are explicit, finite caller inputs. Each value is consumed
/// continuously across every owner; no default constructor resets a budget.
pub struct WorldNativeCompleteWorkV1 {
    pub legacy_cells_remaining: u64,
    pub max_payload_bytes: u64,
    pub primitive: primitive::WorldPrimitiveFamilyWorkV1,
    pub native: native::WorldNativeQualificationWorkV1,
    pub inventory: inventory::InventoryFamilyWorkV1,
    pub factory: factory::FactoryWorkBudgetV1,
    pub industry: industry::IndustryWorkBudgetV1,
    pub market: market::ModuleMarketplaceFamilyWorkV1,
    pub logistics: logistics::LogisticsWorkBudgetV1,
    pub rules: rules::WorldRulesFamilyWorkV1,
    pub controls: controls::WorldCollectionControlsWorkV1,
}
/// Complete structural evidence for the 97 native WorldState descriptors.
/// Required private slots cannot omit empty/None source families. Dedicated
/// Agent/module families, retained History and physical/original leases remain
/// independent obligations; this is not a WorldReady capability.
pub struct WorldNativeCompleteQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    primitive: ReservedValue<primitive::WorldPrimitiveQualificationV1>,
    native: ReservedValue<native::WorldNativeFieldsQualificationV1>,
    inventory: ReservedValue<inventory::InventoryFamilyQualificationV1>,
    factory: ReservedValue<factory::FactoryFamilyQualificationV1>,
    industry: ReservedValue<industry::IndustryFamilyQualificationV1>,
    lifecycle_index:
        ReservedValue<crate::incremental_factory_lifecycle_index::FactoryLifecycleIndexV1>,
    market: ReservedValue<market::ModuleMarketplaceFamilyQualificationV1>,
    fixed: ReservedValue<fixed::FixedEconomyFamilyQualificationV1>,
    reward: ReservedValue<reward::RewardFamilyQualificationV1>,
    main_policy: ReservedValue<main_policy::MainTokenPolicyFamilyQualificationV1>,
    factory_authority: ReservedValue<factory_authority::FactoryAuthorityFamilyQualificationV1>,
    agent_authority: ReservedValue<agent_authority::AgentAuthorityFamilyQualificationV1>,
    material: ReservedValue<material::MaterialFamilyQualificationV1>,
    governance: ReservedValue<governance::GovernanceFamilyQualificationV1>,
    logistics: ReservedValue<logistics::LogisticsFamilyQualificationV1>,
    diplomacy: ReservedValue<diplomacy::DiplomacyFamilyQualificationV1>,
    rules: ReservedValue<rules::WorldRulesQualificationV1>,
    controls: ReservedValue<controls::WorldCollectionControlsQualificationV1>,
}
impl WorldNativeCompleteQualificationV1 {
    pub fn primitive(&self) -> &ReservedValue<primitive::WorldPrimitiveQualificationV1> {
        &self.primitive
    }

    pub fn native(&self) -> &ReservedValue<native::WorldNativeFieldsQualificationV1> {
        &self.native
    }

    pub fn inventory(&self) -> &ReservedValue<inventory::InventoryFamilyQualificationV1> {
        &self.inventory
    }

    pub fn factory(&self) -> &ReservedValue<factory::FactoryFamilyQualificationV1> {
        &self.factory
    }

    pub fn industry(&self) -> &ReservedValue<industry::IndustryFamilyQualificationV1> {
        &self.industry
    }

    pub fn factory_lifecycle_index(
        &self,
    ) -> &ReservedValue<crate::incremental_factory_lifecycle_index::FactoryLifecycleIndexV1> {
        &self.lifecycle_index
    }

    pub fn market(&self) -> &ReservedValue<market::ModuleMarketplaceFamilyQualificationV1> {
        &self.market
    }

    pub fn fixed(&self) -> &ReservedValue<fixed::FixedEconomyFamilyQualificationV1> {
        &self.fixed
    }

    pub fn reward(&self) -> &ReservedValue<reward::RewardFamilyQualificationV1> {
        &self.reward
    }

    pub fn main_policy(&self) -> &ReservedValue<main_policy::MainTokenPolicyFamilyQualificationV1> {
        &self.main_policy
    }

    pub fn factory_authority(
        &self,
    ) -> &ReservedValue<factory_authority::FactoryAuthorityFamilyQualificationV1> {
        &self.factory_authority
    }

    pub fn agent_authority(
        &self,
    ) -> &ReservedValue<agent_authority::AgentAuthorityFamilyQualificationV1> {
        &self.agent_authority
    }

    pub fn material(&self) -> &ReservedValue<material::MaterialFamilyQualificationV1> {
        &self.material
    }

    pub fn governance(&self) -> &ReservedValue<governance::GovernanceFamilyQualificationV1> {
        &self.governance
    }

    pub fn logistics(&self) -> &ReservedValue<logistics::LogisticsFamilyQualificationV1> {
        &self.logistics
    }

    pub fn diplomacy(&self) -> &ReservedValue<diplomacy::DiplomacyFamilyQualificationV1> {
        &self.diplomacy
    }

    pub fn rules(&self) -> &ReservedValue<rules::WorldRulesQualificationV1> {
        &self.rules
    }

    pub fn controls(&self) -> &ReservedValue<controls::WorldCollectionControlsQualificationV1> {
        &self.controls
    }

    pub fn field_count(&self) -> usize {
        97
    }
    pub fn family_count(&self) -> usize {
        17
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), WorldNativeCompleteErrorV1> {
        let digest = roots.digest().map_err(WorldStateOwnerError::RootSet)?;
        if self.roots != digest {
            return Err(invalid().into());
        }
        self.primitive.validate_binding(roots)?;
        self.native.validate_binding(roots)?;
        self.inventory.validate_binding(roots)?;
        self.factory.validate_binding(&digest)?;
        self.industry.validate_binding(&digest)?;
        self.lifecycle_index.validate_aggregate_binding(
            &self.factory,
            &self.industry,
            &digest,
            self.class,
        )?;
        self.market.validate_binding(roots)?;
        self.fixed.validate_binding(&digest)?;
        self.reward.validate_binding(&digest)?;
        self.main_policy.validate_binding(&digest)?;
        self.factory_authority.validate_binding(&digest)?;
        self.agent_authority.validate_binding(&digest)?;
        self.material.validate_binding(&digest)?;
        self.governance.validate_binding(&digest)?;
        self.logistics.validate_binding(&digest)?;
        self.diplomacy.validate_binding(&digest)?;
        self.rules.validate_binding(roots)?;
        self.controls.validate_binding(roots)?;
        Ok(())
    }
    fn check_pool(&self, memory: &MemoryBudget) -> Result<(), ValueTreeError> {
        if !self.primitive.uses_budget(memory)
            || self.primitive.reserved_bytes()
                < std::mem::size_of::<primitive::WorldPrimitiveQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.native.uses_budget(memory)
            || self.native.reserved_bytes()
                < std::mem::size_of::<native::WorldNativeFieldsQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.inventory.uses_budget(memory)
            || self.inventory.reserved_bytes()
                < std::mem::size_of::<inventory::InventoryFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.factory.uses_budget(memory)
            || self.factory.reserved_bytes()
                < std::mem::size_of::<factory::FactoryFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.industry.uses_budget(memory)
            || self.industry.reserved_bytes()
                < std::mem::size_of::<industry::IndustryFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.lifecycle_index.uses_budget(memory)
            || self.lifecycle_index.allocation_class() != self.class
            || self.lifecycle_index.reserved_bytes()
                < std::mem::size_of::<
                    crate::incremental_factory_lifecycle_index::FactoryLifecycleIndexV1,
                >()
        {
            return Err(invalid());
        }
        if !self.market.uses_budget(memory)
            || self.market.reserved_bytes()
                < std::mem::size_of::<market::ModuleMarketplaceFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.fixed.uses_budget(memory)
            || self.fixed.reserved_bytes()
                < std::mem::size_of::<fixed::FixedEconomyFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.reward.uses_budget(memory)
            || self.reward.reserved_bytes()
                < std::mem::size_of::<reward::RewardFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.main_policy.uses_budget(memory)
            || self.main_policy.reserved_bytes()
                < std::mem::size_of::<main_policy::MainTokenPolicyFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.factory_authority.uses_budget(memory)
            || self.factory_authority.reserved_bytes()
                < std::mem::size_of::<factory_authority::FactoryAuthorityFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.agent_authority.uses_budget(memory)
            || self.agent_authority.reserved_bytes()
                < std::mem::size_of::<agent_authority::AgentAuthorityFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.material.uses_budget(memory)
            || self.material.reserved_bytes()
                < std::mem::size_of::<material::MaterialFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.governance.uses_budget(memory)
            || self.governance.reserved_bytes()
                < std::mem::size_of::<governance::GovernanceFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.logistics.uses_budget(memory)
            || self.logistics.reserved_bytes()
                < std::mem::size_of::<logistics::LogisticsFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.diplomacy.uses_budget(memory)
            || self.diplomacy.reserved_bytes()
                < std::mem::size_of::<diplomacy::DiplomacyFamilyQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.rules.uses_budget(memory)
            || self.rules.reserved_bytes() < std::mem::size_of::<rules::WorldRulesQualificationV1>()
        {
            return Err(invalid());
        }
        if !self.controls.uses_budget(memory)
            || self.controls.reserved_bytes()
                < std::mem::size_of::<controls::WorldCollectionControlsQualificationV1>()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
const CARRIER: usize = std::mem::size_of::<WorldNativeCompleteQualificationV1>() + 512;
#[derive(Debug)]
pub enum WorldNativeCompleteErrorV1 {
    Value(ValueTreeError),
    Owner(WorldStateOwnerError),
    Overlay(WorldFieldOverlayErrorV1),
}
impl std::fmt::Display for WorldNativeCompleteErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(e) => e.fmt(f),
            Self::Owner(e) => e.fmt(f),
            Self::Overlay(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for WorldNativeCompleteErrorV1 {}
impl From<ValueTreeError> for WorldNativeCompleteErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<WorldStateOwnerError> for WorldNativeCompleteErrorV1 {
    fn from(e: WorldStateOwnerError) -> Self {
        Self::Owner(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for WorldNativeCompleteErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete native WorldState97 qualification")
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
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error);
    }
    Ok(())
}
fn bind<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &ReservedValue<WorldNativeCompleteQualificationV1>,
    roots: &RootSetV1,
    class: MemoryClass,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<MemoryReservation, WorldNativeCompleteErrorV1> {
    check(ctx)?;
    if source.class != class
        || !source.uses_budget(ctx.values.memory)
        || source.reserved_bytes() < CARRIER
    {
        return Err(invalid().into());
    }
    source.check_pool(ctx.values.memory)?;
    let phase = ctx
        .values
        .memory
        .reserve(128 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    source.validate_binding(roots)?;
    coverage::check()?;
    check(ctx)?;
    Ok(phase)
}
// A lexical reborrow preserves the exact original readers, pool, class,
// callbacks, semantic budget and value-I/O allowance across owner calls.
macro_rules! local {
    ($ctx:expr,$root:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
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
pub fn qualify_world_native_complete_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<WorldNativeCompleteQualificationV1>, WorldNativeCompleteErrorV1> {
    let result = (|| {
        check(ctx)?;
        coverage::check()?;
        roots.validate().map_err(WorldStateOwnerError::RootSet)?;
        if !roots.state.iter().any(|r| r == ctx.root) {
            return Err(invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(128 << 10, ctx.values.class)
            .map_err(ValueTreeError::Memory)?;
        qualify::run(roots, ctx, outer_scan, inner_scan, work)
    })();
    check(ctx)?;
    result
}
pub fn accept_world_native_complete_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
>(
    parent: &ReservedValue<WorldNativeCompleteQualificationV1>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<WorldNativeCompleteQualificationV1>, WorldNativeCompleteErrorV1> {
    let result = (|| {
        check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary
            || verifier.class != MemoryClass::Ordinary
            || !verifier.memory.same_pool(ctx.values.memory)
            || !record.after.state.iter().any(|r| r == ctx.root)
        {
            return Err(invalid().into());
        }
        {
            let _phase = bind(parent, &record.before, MemoryClass::Ordinary, ctx)?;
        }
        let witness = verify_world_transition_v1(verifier, record, ctx)
            .map_err(WorldFieldOverlayErrorV1::from)?;
        accept_world_native_complete_verified_v1(parent, &witness, ctx, inner_scan, work)
    })();
    check(ctx)?;
    result
}
pub(crate) fn accept_world_native_complete_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeCompleteQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<WorldNativeCompleteQualificationV1>, WorldNativeCompleteErrorV1> {
    let result = (|| {
        check(ctx)?;
        witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
        if ctx.values.class != MemoryClass::Ordinary {
            return Err(invalid().into());
        }
        let _phase = bind(parent, &witness.record().before, MemoryClass::Ordinary, ctx)?;
        receiver::run(parent, witness, ctx, inner_scan, work)
    })();
    check(ctx)?;
    result
}
pub fn readmit_world_native_complete_ordinary_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &ReservedValue<WorldNativeCompleteQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<WorldNativeCompleteQualificationV1>, WorldNativeCompleteErrorV1> {
    let result = (|| {
        check(ctx)?;
        if ctx.values.class != MemoryClass::Ordinary || !roots.state.iter().any(|r| r == ctx.root) {
            return Err(invalid().into());
        }
        let _phase = bind(source, roots, MemoryClass::Recovery, ctx)?;
        readmission::run(source, roots, ctx, work)
    })();
    check(ctx)?;
    result
}
#[path = "incremental_world_native_complete_coverage.rs"]
mod coverage;
#[path = "incremental_world_native_complete_qualify.rs"]
mod qualify;
#[path = "incremental_world_native_complete_readmission.rs"]
mod readmission;
#[path = "incremental_world_native_complete_receiver.rs"]
mod receiver;

#[cfg(test)]
#[path = "incremental_world_native_complete_tests.rs"]
mod tests;

#[path = "incremental_world_native_factory_lifecycle_budget.rs"]
mod lifecycle_budget;
