//! Data-only selected Current reads and CAS writes for Emergency lifecycle rows.
//!
//! The source is tied to the genuine WorldRules qualification and this
//! transaction's actual Current branch. Selected rows are original closed
//! codecs. These helpers grant no business authorization and publish no event.
use crate::{
    incremental_field_member::{MemberPayloadRefV1, MemberRecordWriteContext},
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_columns_read::{ColumnCellV1, ColumnReadProfileV1, read_column_row_v1},
    incremental_typed_record_codec::{
        Codec, Encoded, RecoveredTypedRecordV1, TypedRecordReadWorkV1, TypedRecordWriteWorkV1,
        recover_typed_record_body_v1,
    },
    incremental_typed_record_diff::{ChangeDecoder, accept_body},
    incremental_world_field_key::WorldFieldRecordKindV1,
    incremental_world_rule_composite::{
        WorldRuleCompositeBranchV1, WorldRuleCompositeReadWorkV1, WorldRuleCompositeWriteWorkV1,
    },
    incremental_world_rule_composite_family::{
        CompositeBranchCodecV1, WorldRuleCompositeQualificationV1, check, read_body,
    },
    incremental_world_rule_composite_overlay::{
        CompositePathPatchV1, WorldRuleCompositeOverlayV1, bind_current_world_rule_branch_v1,
        patch_world_rule_branch_v1, prepare_semantic_path_key_v1,
    },
    incremental_world_rule_parent::read_parent_overlay,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateNamespaceV1, StatePageWriter, StateRootV1,
        StateScanBudget,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;

pub type Emergency = crate::world_rules::EmergencySupplyStateV1;
pub type Authorization = crate::world_rules::EmergencyAuthorizationV1;
pub type Batch = crate::world_rules::EmergencyBatchV1;
pub type Need = crate::world_rules::EmergencyNeedLedgerV1;
pub type EmergencyOverlay = WorldRuleCompositeOverlayV1<Emergency>;

fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current Emergency lifecycle source")
}

/// Data-only proof that the Emergency branch in this transaction matches its
/// real accepted parent and that this branch has not changed since binding.
pub struct CurrentEmergencyLifecycleSourceV1 {
    parent: [u8; 32],
    overlay: ReservedValue<EmergencyOverlay>,
    memory: MemoryBudget,
    identity: PreparedGuard,
}

/// Current selector data derived from the actual transaction branch. The
/// retained source keeps the exact same-transaction guard alive; rows remain
/// data and selected records must still be reopened through the APIs below.
pub struct CurrentEmergencyLifecycleDirectoryV1 {
    parent: [u8; 32],
    guard: PreparedGuard,
    memory: MemoryBudget,
    current_body: ReservedValue<StateRootV1>,
    directory: crate::incremental_world_rule_composite_family::EmergencyLifecycleDirectoryV1,
}
impl CurrentEmergencyLifecycleDirectoryV1 {
    pub fn minimum_services(
        &self,
    ) -> impl ExactSizeIterator<Item = &crate::incremental_world_rules_family::LifecycleMinimumServiceV1>
    {
        self.directory.minimum_services()
    }
    pub fn authorizations(
        &self,
    ) -> impl ExactSizeIterator<
        Item = &crate::incremental_world_rules_family::LifecycleEmergencyAuthorizationV1,
    > {
        self.directory.emergency_authorizations()
    }
    pub fn batches(
        &self,
    ) -> impl ExactSizeIterator<Item = &crate::incremental_world_rules_family::LifecycleEmergencyBatchV1>
    {
        self.directory.emergency_batches()
    }
    pub fn observation_present(&self, key: &str) -> bool {
        self.directory.observation_key_present(key)
    }
    pub fn current_body(&self) -> &StateRootV1 {
        &self.current_body
    }
    pub fn active_inputs(
        &self,
    ) -> &crate::incremental_world_rule_composite_family::EmergencyActiveInputsV1 {
        self.directory.active_inputs()
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
            || self.parent != roots.digest().map_err(|_| bad())?
        {
            return Err(bad().into());
        }
        self.directory.check_budget(memory, MemoryClass::Ordinary)?;
        if !self.current_body.uses_budget(memory)
            || self.current_body.namespace != StateNamespaceV1::Authorization
        {
            return Err(bad().into());
        }
        Ok(())
    }
}

/// Selected record proof. The embedded record ID is deliberately not used as
/// the map key: `outer_id` is the exact key observed in Current.
struct CurrentEmergencyRecordCoreV1 {
    parent: [u8; 32],
    identity: PreparedGuard,
    memory: MemoryBudget,
    collection: &'static str,
    outer_id: String,
    expected: Option<[u8; 32]>,
    _fee: MemoryReservation,
}

pub struct CurrentEmergencyAuthorizationV1 {
    core: CurrentEmergencyRecordCoreV1,
    value: Option<RecoveredTypedRecordV1<Authorization>>,
}
impl CurrentEmergencyAuthorizationV1 {
    pub fn outer_id(&self) -> &str {
        &self.core.outer_id
    }
    pub fn collection(&self) -> &'static str {
        self.core.collection
    }
    pub fn value(&self) -> Option<&Authorization> {
        self.value.as_ref().map(RecoveredTypedRecordV1::value)
    }
}
pub struct CurrentEmergencyBatchV1 {
    core: CurrentEmergencyRecordCoreV1,
    value: Option<RecoveredTypedRecordV1<Batch>>,
}
impl CurrentEmergencyBatchV1 {
    pub fn outer_id(&self) -> &str {
        &self.core.outer_id
    }
    pub fn collection(&self) -> &'static str {
        self.core.collection
    }
    pub fn value(&self) -> Option<&Batch> {
        self.value.as_ref().map(RecoveredTypedRecordV1::value)
    }
}
pub struct CurrentEmergencyNeedV1 {
    core: CurrentEmergencyRecordCoreV1,
    value: Option<RecoveredTypedRecordV1<Need>>,
}
impl CurrentEmergencyNeedV1 {
    pub fn outer_id(&self) -> &str {
        &self.core.outer_id
    }
    pub fn collection(&self) -> &'static str {
        self.core.collection
    }
    pub fn value(&self) -> Option<&Need> {
        self.value.as_ref().map(RecoveredTypedRecordV1::value)
    }
}

fn current_body<M, R, E, C>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<ReservedValue<StateRootV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let (field, present) =
        read_parent_overlay(ctx.transaction, &mut records, &mut ctx.take_state_error)?;
    let (key, semantic_bytes) = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &("emergency_supply", ()),
    )?;
    let value = lookup(
        ctx.transaction,
        &key,
        &mut records,
        &mut ctx.take_state_error,
    )?;
    match (present, value) {
        (true, Some(id)) => Ok(read_body::<Emergency, _, _>(
            &key,
            semantic_bytes,
            &id,
            &mut records,
        )?),
        _ => Err(bad().into()),
    }
}

fn validate_source<M, R, E, C>(
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_guard(guard, live)?;
    ctx.transaction
        .validate_transaction_identity(&source.identity)?;
    parent.check_budget(&ctx.values.memory, MemoryClass::Ordinary)?;
    if !source.uses_budget(&ctx.values.memory)
        || !source.memory.same_pool(&ctx.values.memory)
        || !source.overlay.uses_budget(&ctx.values.memory)
        || ctx.values.class != MemoryClass::Ordinary
        || !ctx.transaction.uses_budget(&ctx.values.memory)
        || source.parent != ctx.transaction.parent_fence().parent_roots
        || source.parent != roots.digest().map_err(|_| bad())?
    {
        return Err(bad().into());
    }
    parent.validate_binding(roots)?;
    let actual = current_body(ctx)?;
    if &*actual != source.overlay.body() {
        return Err(bad().into());
    }
    Ok(())
}

/// Bind a genuine Emergency branch in Current. Rebind after each successful
/// patch; selected leaf proofs from the same transaction remain usable for
/// unrelated rows, while every write rechecks its exact leaf against latest
/// Current.
#[allow(clippy::too_many_arguments)]
pub fn bind_current_emergency_lifecycle_v1<M, S, R, E, C>(
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentEmergencyLifecycleSourceV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(&ctx.values.memory)
        {
            return Err(bad().into());
        }
        let retained = ctx
            .values
            .memory
            .reserve(
                size_of::<CurrentEmergencyLifecycleSourceV1>() + 4096,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let overlay = bind_current_world_rule_branch_v1::<Emergency, _, _, _, _, _>(
            parent, roots, state, ctx, scan, work, guard, live,
        )?;
        let digest = roots.digest().map_err(|_| bad())?;
        Ok(retained.retain(CurrentEmergencyLifecycleSourceV1 {
            parent: digest,
            overlay,
            memory: ctx.values.memory.clone(),
            identity: ctx.transaction.guard(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

/// Build the full Emergency selector projection from this transaction's
/// authenticated Current branch, folding only real changed rows over the
/// accepted parent projection. This owner grants no action authority.
#[allow(clippy::too_many_arguments)]
pub fn project_current_emergency_lifecycle_v1<M, S, R, E, C>(
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentEmergencyLifecycleDirectoryV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        validate_source(source, parent, roots, ctx, guard, live)?;
        let parent_directory = parent.emergency_lifecycle_directory().ok_or_else(bad)?;
        let mut read = WorldFieldReadContextV1 {
            root: source.overlay.body(),
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let directory = crate::incremental_world_rule_composite_family::project_current_emergency_lifecycle_directory_v1(
            parent_directory,
            parent.body(),
            source.overlay.body(),
            &mut read,
            scan,
            work,
        )?;
        read.check()?;
        drop(read);
        // The projection read updates this transaction's read set. Validate
        // against the tail guard captured after those reads while retaining
        // the strict entry validation above.
        let tail_guard = ctx.transaction.guard();
        validate_source(source, parent, roots, ctx, &tail_guard, live)?;
        let body = source.overlay.body();
        if body.namespace != StateNamespaceV1::Authorization {
            return Err(bad().into());
        }
        let body_fee = ctx
            .values
            .memory
            .reserve(
                size_of::<StateRootV1>() + body.node_root.len() + 512,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let current_body = body_fee.retain(body.clone());
        if current_body.node_root.capacity() > body.node_root.len() {
            return Err(bad().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<CurrentEmergencyLifecycleDirectoryV1>() + 512,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(CurrentEmergencyLifecycleDirectoryV1 {
            parent: source.parent,
            guard: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
            current_body,
            directory,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

fn collection_root<M, R, E, C>(
    body: &StateRootV1,
    collection: &'static str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &TypedRecordReadWorkV1,
) -> Result<ReservedValue<StateRootV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let profile = WorldRuleCompositeBranchV1::EmergencySupply.profile();
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let (key, _) = records.key(profile.field, WorldFieldRecordKindV1::Entry, collection)?;
    let Some(id) =
        crate::incremental_record_tree::lookup(body, &key, &mut records, profile.labels)?
    else {
        return Err(bad().into());
    };
    drop(records);
    let _entry = ctx
        .values
        .memory
        .reserve(
            key.as_bytes().len() + size_of::<StateEntryV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let row = read_column_row_v1(
        ColumnReadProfileV1 {
            field: profile.field,
            schema: profile.schema,
            groups: &[],
            reference_schema: profile.reference_schema,
        },
        &StateEntryV1 {
            namespace: StateNamespaceV1::Authorization,
            key: key.as_bytes().to_vec(),
            value: id,
        },
        &mut records,
        work.max_tree_heap,
    )?;
    let ColumnCellV1::Root(root) = &row.cell else {
        return Err(bad().into());
    };
    drop(records);
    let fee = ctx
        .values
        .memory
        .reserve(size_of::<StateRootV1>() + 512, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(root.clone()))
}

fn read_selected<T, M, S, R, E, C>(
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    state: &S,
    outer_id: &str,
    collection: &'static str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<
    (
        CurrentEmergencyRecordCoreV1,
        Option<RecoveredTypedRecordV1<T>>,
    ),
    WorldFieldOverlayErrorV1,
>
where
    T: Codec,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    validate_source(source, parent, roots, ctx, guard, live)?;
    let profile = WorldRuleCompositeBranchV1::EmergencySupply.profile();
    let nested = collection_root(source.overlay.body(), collection, ctx, work)?;
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let (key, _) = records.key(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        &("entry", outer_id),
    )?;
    let leaf = crate::incremental_record_tree::lookup(&nested, &key, &mut records, profile.labels)?;
    let value = if let Some(id) = leaf {
        let _entry = records
            .values
            .memory
            .reserve(
                key.as_bytes().len() + size_of::<StateEntryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let row = read_column_row_v1(
            ColumnReadProfileV1 {
                field: profile.field,
                schema: profile.schema,
                groups: &[],
                reference_schema: profile.reference_schema,
            },
            &StateEntryV1 {
                namespace: StateNamespaceV1::Authorization,
                key: key.as_bytes().to_vec(),
                value: id,
            },
            &mut records,
            work.max_tree_heap,
        )?;
        let ColumnCellV1::Root(record) = &row.cell else {
            return Err(bad().into());
        };
        drop(records);
        let mut read = WorldFieldReadContextV1 {
            root: record,
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let decoded =
            recover_typed_record_body_v1::<T, _, _, _, _>(record, &profile, &mut read, scan, work)?;
        check(&mut read)?;
        Some(decoded)
    } else {
        drop(records);
        None
    };
    let fee_bytes = size_of::<CurrentEmergencyRecordCoreV1>()
        .checked_add(outer_id.len().checked_mul(2).ok_or_else(bad)?)
        .and_then(|n| n.checked_add(512))
        .ok_or_else(bad)?;
    let fee = ctx
        .values
        .memory
        .reserve(fee_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let tail_guard = ctx.transaction.guard();
    validate_source(source, parent, roots, ctx, &tail_guard, live)?;
    Ok((
        CurrentEmergencyRecordCoreV1 {
            parent: source.parent,
            identity: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
            collection,
            outer_id: outer_id.to_owned(),
            expected: leaf,
            _fee: fee,
        },
        value,
    ))
}

macro_rules! read_api {
    ($name:ident,$wrapper:ident,$dto:ty,$collection:literal) => {
        #[allow(clippy::too_many_arguments)]
        pub fn $name<M, S, R, E, C>(
            source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
            parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
            roots: &RootSetV1,
            state: &S,
            outer_id: &str,
            ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            scan: &mut StateScanBudget,
            work: &mut TypedRecordReadWorkV1,
            guard: &PreparedGuard,
            live: &ExecutionFence,
        ) -> Result<ReservedValue<$wrapper>, WorldFieldOverlayErrorV1>
        where
            M: MerkleNodeReader,
            S: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
        {
            let result = (|| {
                let (core, value) = read_selected::<$dto, _, _, _, _, _>(
                    source,
                    parent,
                    roots,
                    state,
                    outer_id,
                    $collection,
                    ctx,
                    scan,
                    work,
                    guard,
                    live,
                )?;
                let fee = ctx
                    .values
                    .memory
                    .reserve(size_of::<$wrapper>(), MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                Ok(fee.retain($wrapper { core, value }))
            })();
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
    };
}
read_api!(
    read_current_emergency_authorization_v1,
    CurrentEmergencyAuthorizationV1,
    Authorization,
    "authorizations"
);
read_api!(
    read_current_emergency_batch_v1,
    CurrentEmergencyBatchV1,
    Batch,
    "batches"
);
read_api!(
    read_current_emergency_need_v1,
    CurrentEmergencyNeedV1,
    Need,
    "needs"
);

#[path = "incremental_world_rule_emergency_close_records.rs"]
mod close_records;
pub use close_records::{
    CurrentEmergencyEvidenceV1, CurrentEmergencyMinimumServiceV1, CurrentEmergencyServiceV1,
    read_current_emergency_evidence_v1, read_current_emergency_minimum_service_v1,
    read_current_emergency_service_v1, replace_current_emergency_service_v1,
    validate_current_emergency_minimum_selection_v1,
    validate_current_emergency_service_selection_v1,
};

/// Bind a Current selector projection to the exact branch source used by
/// selected-record reads. The caller supplies the same continuous selector
/// work lane because the root equality check can inspect both retained roots.
#[allow(clippy::too_many_arguments)]
pub fn validate_current_emergency_directory_source_v1<M, R, E, C>(
    directory: &ReservedValue<CurrentEmergencyLifecycleDirectoryV1>,
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut super::WorldRulesFamilyWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    directory.validate(ctx.transaction, roots, ctx.values.memory, live)?;
    if !source.uses_budget(ctx.values.memory)
        || !directory.uses_budget(ctx.values.memory)
        || directory.parent != source.parent
    {
        return Err(bad().into());
    }
    validate_source(source, parent, roots, ctx, guard, live)?;
    let actual = source.overlay.body();
    let expected = &*directory.current_body;
    let comparison_bytes = actual
        .node_root
        .len()
        .checked_add(expected.node_root.len())
        .ok_or_else(bad)?;
    work.outer_records_remaining = work
        .outer_records_remaining
        .checked_sub(1)
        .ok_or_else(|| ValueTreeError::ResourceLimited("Current Emergency source join"))?;
    work.remainder.cells_remaining = work
        .remainder
        .cells_remaining
        .checked_sub(
            u64::try_from(comparison_bytes)
                .map_err(|_| bad())?
                .checked_add(1)
                .ok_or_else(bad)?,
        )
        .ok_or_else(|| ValueTreeError::ResourceLimited("Current Emergency source join"))?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if actual != expected {
        return Err(bad().into());
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let tail_guard = ctx.transaction.guard();
    ctx.transaction.validate_guard(&tail_guard, live)?;
    Ok(())
}

#[path = "incremental_world_rule_emergency_close_write.rs"]
mod close_write;
pub use close_write::{
    EmergencyBatchReleaseV1, EmergencyNeedReservationChangeV1,
    commit_current_emergency_close_columns_v1,
};

fn patch_selected<T, M, S, R, E, C, W, CW>(
    source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
    selected: &CurrentEmergencyRecordCoreV1,
    after: Option<&T>,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<EmergencyOverlay>>, WorldFieldOverlayErrorV1>
where
    T: Codec,
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
{
    ctx.transaction.validate_guard(guard, live)?;
    ctx.transaction
        .validate_transaction_identity(&source.identity)?;
    ctx.transaction
        .validate_transaction_identity(&selected.identity)?;
    if selected.parent != source.parent
        || selected.collection.is_empty()
        || !selected.memory.same_pool(&ctx.values.memory)
        || !source.memory.same_pool(&ctx.values.memory)
        || !write.memory.same_pool(&ctx.values.memory)
        || !source.uses_budget(&ctx.values.memory)
        || !selected._fee.bytes().gt(&0)
        || ctx.values.class != MemoryClass::Ordinary
    {
        return Err(bad().into());
    }
    validate_source(source, parent, roots, ctx, guard, live)?;
    let profile = WorldRuleCompositeBranchV1::EmergencySupply.profile();
    let nested = collection_root(source.overlay.body(), selected.collection, ctx, read_work)?;
    let (actual, count_hash, count) = {
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let (item_key, _) = records.key(
            profile.field,
            WorldFieldRecordKindV1::Entry,
            &("entry", selected.outer_id.as_str()),
        )?;
        let item = crate::incremental_record_tree::lookup(
            &nested,
            &item_key,
            &mut records,
            profile.labels,
        )?;
        let (len_key, _) = records.key(profile.field, WorldFieldRecordKindV1::Entry, "_length")?;
        let len = crate::incremental_record_tree::lookup(
            &nested,
            &len_key,
            &mut records,
            profile.labels,
        )?
        .ok_or_else(bad)?;
        let _entry = records
            .values
            .memory
            .reserve(
                len_key.as_bytes().len() + size_of::<StateEntryV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let row = read_column_row_v1(
            ColumnReadProfileV1 {
                field: profile.field,
                schema: profile.schema,
                groups: &[],
                reference_schema: profile.reference_schema,
            },
            &StateEntryV1 {
                namespace: StateNamespaceV1::Authorization,
                key: len_key.as_bytes().to_vec(),
                value: len,
            },
            &mut records,
            read_work.max_tree_heap,
        )?;
        let ColumnCellV1::U64(count) = &row.cell else {
            return Err(bad().into());
        };
        (item, Some(len), *count)
    };
    if actual != selected.expected {
        return Err(bad().into());
    }
    let count_after = match (actual.is_some(), after.is_some()) {
        (false, true) => Some(count.checked_add(1).ok_or_else(bad)?),
        (true, false) => Some(count.checked_sub(1).ok_or_else(bad)?),
        _ => None,
    };
    let encoded_after = if let Some(value) = after {
        let encoded = {
            let mut context_cancel = std::cell::RefCell::new(&mut ctx.cancelled);
            let mut write_cancel = std::cell::RefCell::new(&mut write.cancelled);
            let mut cancelled =
                || (*context_cancel.borrow_mut())() || (*write_cancel.borrow_mut())();
            let mut output = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: &mut cancelled,
            };
            value.encode(&profile, &mut output, write_work)?
        };
        Some(encoded)
    } else {
        None
    };
    let _paths = write
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let field = prepare_semantic_path_key_v1(selected.collection, write)?;
    let item = prepare_semantic_path_key_v1(&("entry", selected.outer_id.as_str()), write)?;
    let item_path = [field.as_slice(), item.as_slice()];
    let item_patch = CompositePathPatchV1 {
        path: &item_path,
        expected: selected.expected,
        after: encoded_after,
    };
    // The selected-record lookup above extends this transaction's read set.
    // Keep the strict entry guard check, then bind the actual sparse write to
    // the fresh guard after all point reads and encoding have completed.
    let tail_guard = ctx.transaction.guard();
    ctx.transaction.validate_guard(&tail_guard, live)?;
    if let Some(value) = count_after {
        let key = prepare_semantic_path_key_v1("_length", write)?;
        let path = [field.as_slice(), key.as_slice()];
        patch_world_rule_branch_v1(
            parent,
            roots,
            Some(&source.overlay),
            [
                item_patch,
                CompositePathPatchV1 {
                    path: &path,
                    expected: count_hash,
                    after: Some(Encoded::Scalar(MemberPayloadRefV1::U64(value))),
                },
            ],
            state,
            ctx,
            write,
            scan,
            read_work,
            write_work,
            &tail_guard,
            live,
        )
    } else {
        patch_world_rule_branch_v1(
            parent,
            roots,
            Some(&source.overlay),
            [item_patch],
            state,
            ctx,
            write,
            scan,
            read_work,
            write_work,
            &tail_guard,
            live,
        )
    }
}

macro_rules! write_api {
    ($name:ident,$wrapper:ty,$dto:ty,$collection:literal) => {
        #[allow(clippy::too_many_arguments)]
        pub fn $name<M, S, R, E, C, W, CW>(
            source: &ReservedValue<CurrentEmergencyLifecycleSourceV1>,
            selected: &ReservedValue<$wrapper>,
            after: Option<&$dto>,
            parent: &ReservedValue<WorldRuleCompositeQualificationV1<Emergency>>,
            roots: &RootSetV1,
            state: &S,
            ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
            write: &mut MemberRecordWriteContext<'_, W, CW>,
            scan: &mut StateScanBudget,
            read_work: &mut WorldRuleCompositeReadWorkV1,
            write_work: &mut WorldRuleCompositeWriteWorkV1,
            guard: &PreparedGuard,
            live: &ExecutionFence,
        ) -> Result<Option<ReservedValue<EmergencyOverlay>>, WorldFieldOverlayErrorV1>
        where
            M: MerkleNodeReader,
            S: MerkleNodeReader,
            R: ValueNodeReader,
            E: FnMut() -> Option<ValueTreeError>,
            C: FnMut() -> bool,
            W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
            CW: FnMut() -> bool,
        {
            let result = if selected.core.collection != $collection
                || !selected.uses_budget(&ctx.values.memory)
                || selected.reserved_bytes() < size_of::<$wrapper>()
            {
                Err(bad().into())
            } else {
                patch_selected::<$dto, _, _, _, _, _, _, _>(
                    source,
                    &selected.core,
                    after,
                    parent,
                    roots,
                    state,
                    ctx,
                    write,
                    scan,
                    read_work,
                    write_work,
                    guard,
                    live,
                )
            };
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
    };
}
write_api!(
    replace_current_emergency_authorization_v1,
    CurrentEmergencyAuthorizationV1,
    Authorization,
    "authorizations"
);
write_api!(
    replace_current_emergency_batch_v1,
    CurrentEmergencyBatchV1,
    Batch,
    "batches"
);
write_api!(
    replace_current_emergency_need_v1,
    CurrentEmergencyNeedV1,
    Need,
    "needs"
);
