//! Scoped qualifications for one of the six fixed original rule branches.
//! Unknown outer keys and all other branches remain the global router's duty.
use crate::{
    incremental_typed_record_codec::*,
    incremental_typed_record_diff::{ChangeDecoder, accept_body},
    incremental_world_rule_composite::WorldRuleCompositeBranchV1,
};
use oasis7_formal_protocol::incremental_execution::{
    RootSetV1, TransitionRecordV1, TransitionVerifier,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::marker::PhantomData;
pub(crate) trait CompositeBranchCodecV1: Codec {
    const BRANCH: WorldRuleCompositeBranchV1;
}
macro_rules! branch {
    ($ty:ty,$branch:ident) => {
        impl CompositeBranchCodecV1 for $ty {
            const BRANCH: WorldRuleCompositeBranchV1 = WorldRuleCompositeBranchV1::$branch;
        }
    };
}
branch!(
    crate::world_rules::OrganizationContinuityStateV1,
    OrganizationContinuity
);
branch!(crate::world_rules::EmergencySupplyStateV1, EmergencySupply);
branch!(
    oasis7_agent_api::world_action::IndustrialWorkStateV1,
    IndustrialWork
);
branch!(
    oasis7_agent_api::world_action::SubstitutionStateV1,
    Substitution
);
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("world rule composite family")
}
pub struct WorldRuleCompositeQualificationV1<T> {
    roots: [u8; 32],
    memory: MemoryBudget,
    present: bool,
    body: Option<ReservedValue<StateRootV1>>,
    class: MemoryClass,
    _type: PhantomData<fn() -> T>,
}
impl<T> WorldRuleCompositeQualificationV1<T> {
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        let _metadata = self.memory.reserve(4096, self.class)?;
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn body(&self) -> Option<&StateRootV1> {
        self.body.as_deref()
    }
    pub fn parent_present(&self) -> bool {
        self.present
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub(crate) fn check_budget(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<(), ValueTreeError> {
        if !self.memory.same_pool(memory)
            || self.class != class
            || self.present != self.body.is_some()
            || self
                .body
                .as_ref()
                .is_some_and(|r| !r.uses_budget(memory) || r.reserved_bytes() < 512)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(crate) fn check<
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
pub(crate) fn read_body<T: CompositeBranchCodecV1, R: ValueNodeReader, C: FnMut() -> bool>(
    key: &WorldFieldKeyV1,
    semantic_bytes: u64,
    id: &[u8; 32],
    records: &mut crate::incremental_inventory_value_read::InventoryValueReader<'_, '_, R, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    let charge = records.values.memory.reserve(512, records.values.class)?;
    let bytes = records
        .fixed::<{ oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES }>(
            id,
            key,
            semantic_bytes,
            MemberPayloadCodecV1::RecordRoot,
        )?;
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let body = reference
        .validate_binding(
            &T::BRANCH.profile().schema,
            &parsed.semantic_key.ok_or_else(invalid)?,
            StateNamespaceV1::Authorization,
        )
        .map_err(|_| invalid())?;
    Ok(charge.retain(body.clone()))
}
fn fixed<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(bool, Option<ReservedValue<StateRootV1>>), ValueTreeError> {
    check(ctx)?;
    let (field, present) = crate::incremental_world_rule_parent::read_parent_fixed(ctx)?;
    let (key, semantic_bytes) = ctx.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(T::BRANCH.name(), ()),
    )?;
    let id = ctx.lookup(&key)?;
    let body = match (present, id) {
        (false, None) => None,
        (true, Some(id)) => Some(read_body::<T, _, _>(
            &key,
            semantic_bytes,
            &id,
            &mut ctx.records(),
        )?),
        _ => return Err(invalid()),
    };
    check(ctx)?;
    Ok((present, body))
}
pub(crate) fn qualify_world_rule_branch_v1<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<ReservedValue<WorldRuleCompositeQualificationV1<T>>, ValueTreeError> {
    roots.validate().map_err(|_| invalid())?;
    check(ctx)?;
    if ctx.root != roots.state_root(StateNamespaceV1::Authorization) {
        return Err(invalid());
    }
    let _phase = ctx.values.memory.reserve(128 << 10, ctx.values.class)?;
    let carrier = ctx.values.memory.reserve(512, ctx.values.class)?;
    let (present, body) = fixed::<T, _, _, _, _>(ctx)?;
    let trees_remaining = work.max_retained_trees;
    accept_body::<T, _, _, _, _>(
        &T::BRANCH.profile(),
        None,
        body.as_deref(),
        &mut ChangeDecoder {
            ctx,
            scan,
            work,
            profile: T::BRANCH.profile(),
            trees_remaining,
            tree_depth: 0,
        },
    )?;
    check(ctx)?;
    Ok(carrier.retain(WorldRuleCompositeQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        memory: ctx.values.memory.clone(),
        present,
        body,
        class: ctx.values.class,
        _type: PhantomData,
    }))
}
pub fn readmit_world_rule_branch_ordinary_v1<T>(
    source: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<WorldRuleCompositeQualificationV1<T>>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    source.check_budget(memory, MemoryClass::Recovery)?;
    if !source.uses_budget(memory) || source.reserved_bytes() < 512 {
        return Err(invalid());
    }
    let mut phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    if source.roots != roots.digest().map_err(|_| invalid())? {
        return Err(invalid());
    }
    let carrier = phase.split(512).map_err(ValueTreeError::Invalid)?;
    let body = match source.body() {
        Some(root) => Some(
            phase
                .split(512)
                .map_err(ValueTreeError::Invalid)?
                .retain(root.clone()),
        ),
        None => None,
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(carrier.retain(WorldRuleCompositeQualificationV1 {
        roots: source.roots,
        memory: memory.clone(),
        present: source.present,
        body,
        class: MemoryClass::Ordinary,
        _type: PhantomData,
    }))
}
fn branch_transition_preflight_v1<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<
    oasis7_proto::memory_budget::MemoryReservation,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
    check(ctx)?;
    if ctx.values.class != MemoryClass::Ordinary {
        return Err(invalid().into());
    }
    parent.check_budget(ctx.values.memory, MemoryClass::Ordinary)?;
    if !parent.uses_budget(ctx.values.memory)
        || parent.reserved_bytes() < 512
        || ctx.root != record.after.state_root(StateNamespaceV1::Authorization)
    {
        return Err(invalid().into());
    }
    let phase = ctx
        .values
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    parent.validate_binding(&record.before)?;
    check(ctx)?;
    Ok(phase)
}
pub(crate) fn accept_world_rule_branch_transition_v1<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    verifier: &TransitionVerifier<'_, P>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<
    ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
    check(ctx)?;
    if verifier.class != MemoryClass::Ordinary || !verifier.memory.same_pool(ctx.values.memory) {
        return Err(invalid().into());
    }
    {
        let _phase = branch_transition_preflight_v1(parent, record, ctx)?;
    }
    let witness = crate::incremental_verified_world_transition::verify_world_transition_v1(
        verifier, record, ctx,
    )?;
    let result = accept_world_rule_branch_verified_v1(parent, &witness, ctx, scan, work);
    check(ctx)?;
    result
}
pub(crate) fn accept_world_rule_branch_verified_v1<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<
    ReservedValue<WorldRuleCompositeQualificationV1<T>>,
    crate::incremental_field_overlay::WorldFieldOverlayErrorV1,
> {
    check(ctx)?;
    witness.check_context(ctx.values.memory, ctx.values.class, ctx.root)?;
    let record = witness.record();
    let _phase = branch_transition_preflight_v1(parent, record, ctx)?;
    let carrier = ctx
        .values
        .memory
        .reserve(512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let (present, body) = fixed::<T, _, _, _, _>(ctx)?;
    let trees_remaining = work.max_retained_trees;
    accept_body::<T, _, _, _, _>(
        &T::BRANCH.profile(),
        parent.body(),
        body.as_deref(),
        &mut ChangeDecoder {
            ctx,
            scan,
            work,
            profile: T::BRANCH.profile(),
            trees_remaining,
            tree_depth: 0,
        },
    )?;
    check(ctx)?;
    Ok(carrier.retain(WorldRuleCompositeQualificationV1 {
        roots: *witness.after_digest(),
        memory: ctx.values.memory.clone(),
        present,
        body,
        class: ctx.values.class,
        _type: PhantomData,
    }))
}

#[cfg(test)]
#[path = "incremental_world_rule_composite_test_support.rs"]
pub(crate) mod test_support;
#[cfg(test)]
#[path = "incremental_world_rule_composite_tests.rs"]
mod tests;

#[path = "incremental_world_rule_composite_orders_current.rs"]
mod orders_current;
pub use orders_current::{
    AgentOrderDirectoryWorkV1, CurrentAgentOrderIndexV1, CurrentAgentOrdersV1,
    bind_current_agent_orders_v1, collect_changed_agent_order_ids_v1,
    collect_parent_agent_order_ids_v1, read_current_agent_order_index_v1,
    read_current_agent_order_v1,
};
