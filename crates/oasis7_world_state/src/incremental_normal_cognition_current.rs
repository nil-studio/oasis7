//! Actual Current selected Cognition JSON. These opaque DATA owners neither
//! qualify the complete Cognition family nor authorize a scheduler transition.
use crate::{
    incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    incremental_cognition_json::{CognitionJsonFieldV1, write_root},
    incremental_cognition_json_read as read,
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_json_records::{
        JsonReadWorkV1, JsonWriteWorkV1, materialize_records, prepare_json_subtree_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("selected Current Cognition JSON")
}
pub struct CurrentCognitionJsonRecordV1 {
    field: CognitionJsonFieldV1,
    key: WorldFieldKeyV1,
    before: [u8; 32],
    metadata: [u8; 32],
    value: serde_json::Value,
    qualification: ReservedValue<read::CognitionJsonEntryQualificationV1>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _value_fee: MemoryReservation,
}
impl CurrentCognitionJsonRecordV1 {
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }
    pub fn field(&self) -> CognitionJsonFieldV1 {
        self.field
    }
    /// Internal provenance check for the atomic multi-field publisher. Reading
    /// another selected field or appending Journal facts may advance the read-set
    /// revision, but does not authorize changing this record's expected rows.
    fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.memory.same_pool(memory)
            || !self.qualification.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self.qualification.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}
/// Absence is the actual pair of absent Current outer rows. It is not a claim
/// that other Cognition groups or side ProofLedger fields are empty/disabled.
#[allow(clippy::too_many_arguments)]
pub fn read_current_cognition_json_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionJsonFieldV1,
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
    max_heap: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<CurrentCognitionJsonRecordV1>>, WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        if roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        let phase = ctx
            .values
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let metadata =
            WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
        let key = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        let metadata_root = lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?;
        let outer = lookup(tx, &key.0, &mut records, &mut ctx.take_state_error)?;
        let Some(metadata_root) = metadata_root else {
            if outer.is_some() {
                return Err(invalid().into());
            }
            return Ok(None);
        };
        records.presence(&metadata_root, &metadata, 0)?;
        let outer = outer.ok_or_else(invalid)?;
        let child = read::outer(field, &key, &outer, &mut records)?;
        let proof = read::qualify_body_records(field, &child, &mut records, work)?;
        let (value, value_fee) =
            materialize_records(field.profile(), proof.json(), &mut records, work, max_heap)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(Some(phase.retain(CurrentCognitionJsonRecordV1 {
            field,
            key: key.0,
            before: outer,
            metadata: metadata_root,
            value,
            qualification: proof,
            guard,
            parent: tx.parent_fence().parent_roots,
            memory: ctx.values.memory.clone(),
            _value_fee: value_fee,
        })))
    })
}
/// Only the service's three original output fields. Fixed caller business
/// producers remain responsible for the actual scheduler/continuation rules.
#[allow(clippy::too_many_arguments)]
pub fn replace_current_cognition_service_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentCognitionJsonRecordV1>,
    after: &serde_json::Value,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        expected.validate(tx, ctx.values.memory, live)?;
        if !matches!(
            expected.field,
            CognitionJsonFieldV1::SchedulerState
                | CognitionJsonFieldV1::Continuations
                | CognitionJsonFieldV1::ContinuationEvaluations
        ) || !expected.uses_budget(ctx.values.memory)
            || ctx.values.class != MemoryClass::Ordinary
            || !ctx.values.memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        let child = prepare_json_subtree_v1(expected.field.profile(), after, write, work)?;
        let row = write_root(expected.field, child, write)?;
        if row.field_key != expected.key {
            return Err(invalid().into());
        }
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let metadata = WorldFieldKeyV1::new(
            expected.field.descriptor(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .map_err(ValueTreeError::Invalid)?;
        if lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?
            != Some(expected.metadata)
            || lookup(tx, &expected.key, &mut records, &mut ctx.take_state_error)?
                != Some(expected.before)
        {
            return Err(invalid().into());
        }
        if (records.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&expected.guard, live)?;
        let namespace = WorldFieldKeyV1::from_bytes(expected.key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .namespace;
        tx.write(namespace, expected.key.as_bytes(), Some(row.value_root))?;
        Ok(())
    })
}
/// Actual optional-evaluations creation. The two expected absent rows are
/// re-read on this transaction; no caller absence flag or raw root is used.
#[allow(clippy::too_many_arguments)]
pub fn create_current_cognition_evaluations_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    after: &serde_json::Value,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    create_current_cognition_service_map_v2(
        CognitionJsonFieldV1::ContinuationEvaluations,
        parent,
        roots,
        after,
        ctx,
        write,
        work,
        guard,
        live,
    )
}

/// Create only the original optional service maps after authenticating actual
/// absent Current metadata and entry rows. No caller absence token is accepted.
#[allow(clippy::too_many_arguments)]
pub fn create_current_cognition_service_map_v2<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    field: CognitionJsonFieldV1,
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    after: &serde_json::Value,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    if !matches!(
        field,
        CognitionJsonFieldV1::ContinuationEvaluations | CognitionJsonFieldV1::ContinuationContexts
    ) {
        return ctx.transaction.contribute(|_| Err(invalid().into()));
    }
    create_current_cognition_service_value_v2(
        field, parent, roots, after, ctx, write, work, guard, live,
    )
}

/// First creation of an original commit map. Actual Current metadata/entry
/// absence is checked by the same kernel as the original service maps. Only
/// the newly created field is qualified; no complete Cognition value is read.
#[allow(clippy::too_many_arguments)]
pub fn create_current_cognition_commit_map_v1<M, R, E, C, W, CW>(
    field: CognitionJsonFieldV1,
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    after: &serde_json::Value,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    qualify: &mut JsonReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<read::CognitionJsonEntryQualificationV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !matches!(
            field,
            CognitionJsonFieldV1::StagedActions | CognitionJsonFieldV1::IdempotencyIndex
        ) {
            return ctx.transaction.contribute(|_| Err(invalid().into()));
        }
        create_current_cognition_service_value_v2(
            field, parent, roots, after, ctx, write, work, guard, live,
        )?;
        ctx.transaction.contribute(|tx| {
            tx.validate_guard(&tx.guard(), live)?;
            let mut stop = || (ctx.cancelled)() || (write.cancelled)();
            // Read our actual fresh pages through the original writer's reader, rather
            // than trusting a caller-provided root or requiring a cold-store refresh.
            let mut values = oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: &*write.sink,
                memory: write.memory,
                class: MemoryClass::Ordinary,
                io: &mut *ctx.values.io,
            };
            let mut records = InventoryValueReader {
                values: &mut values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut stop,
            };
            let metadata =
                WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let marker = lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?
                .ok_or_else(invalid)?;
            records.presence(&marker, &metadata, 0)?;
            let key = records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
            let outer =
                lookup(tx, &key.0, &mut records, &mut ctx.take_state_error)?.ok_or_else(invalid)?;
            let root = read::outer(field, &key, &outer, &mut records)?;
            let result = read::qualify_body_records(field, &root, &mut records, qualify)?;
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(result)
        })
    }));
    let result = match attempt {
        Ok(result) => result,
        Err(panic) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(panic)
        }
    };
    result.map_err(|error| match error {
        WorldFieldOverlayErrorV1::Member(error) => {
            WorldFieldOverlayErrorV1::Value(error.into_value_error())
        }
        other => other,
    })
}

/// The original first-admission array creation, using the same actual double
/// absence and fixed codec kernel as the optional service maps.
#[allow(clippy::too_many_arguments)]
pub fn create_current_cognition_continuations_v2<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    after: &serde_json::Value,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    create_current_cognition_service_value_v2(
        CognitionJsonFieldV1::Continuations,
        parent,
        roots,
        after,
        ctx,
        write,
        work,
        guard,
        live,
    )
}

#[allow(clippy::too_many_arguments)]
fn create_current_cognition_service_value_v2<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    field: CognitionJsonFieldV1,
    parent: &ReservedValue<CompleteCognitionCurrentQualificationV3>,
    roots: &RootSetV1,
    after: &serde_json::Value,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        use crate::incremental_field_presence::*;
        tx.validate_guard(guard, live)?;
        if !match field {
            CognitionJsonFieldV1::ContinuationEvaluations
            | CognitionJsonFieldV1::ContinuationContexts
            | CognitionJsonFieldV1::StagedActions
            | CognitionJsonFieldV1::IdempotencyIndex => after.is_object(),
            CognitionJsonFieldV1::Continuations => after.is_array(),
            _ => false,
        } || ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !ctx.values.memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        parent.validate_binding(roots)?;
        if roots.digest().map_err(|_| invalid())? != tx.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let metadata =
            WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
        let key = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            let key = records
                .key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?
                .0;
            if lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?.is_some()
                || lookup(tx, &key, &mut records, &mut ctx.take_state_error)?.is_some()
            {
                return Err(invalid().into());
            }
            key
        };
        let child = prepare_json_subtree_v1(field.profile(), after, write, work)?;
        let entry = write_root(field, child, write)?;
        let marker = write_field_metadata_v1(
            field.descriptor(),
            &mut PresenceRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: write.limits.semantic_bytes,
                    record_bytes: write.limits.record_bytes,
                },
                cancelled: &mut write.cancelled,
            },
        )
        .map_err(WorldFieldOverlayErrorV1::Member)?;
        if entry.field_key != key || marker.field_key != metadata {
            return Err(invalid().into());
        }
        if (ctx.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        let namespace = WorldFieldKeyV1::from_bytes(key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .namespace;
        tx.write(namespace, metadata.as_bytes(), Some(marker.value_root))?;
        tx.write(namespace, key.as_bytes(), Some(entry.value_root))?;
        Ok(())
    })
}

/// Atomic fixed service-map publication, including actual V2 admission contexts. All original rows and guards are
/// compared against actual Current before the first write: a Scheduler write must not stale the
/// Continuations guard from the same selected Current snapshot.
#[allow(clippy::too_many_arguments)]
pub fn replace_current_cognition_service_batch_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    changes: &[(
        &ReservedValue<CurrentCognitionJsonRecordV1>,
        &serde_json::Value,
    )],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    replace_current_cognition_fixed_batch(
        changes,
        ctx,
        write,
        work,
        guard,
        live,
        BatchProfile::Service,
    )
}

/// Fixed nine-map Economy CAS. This preserves the service-only entry point;
/// only the original Economy profiles can enter this sibling publication.
#[allow(clippy::too_many_arguments)]
pub fn replace_current_cognition_economy_batch_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    changes: &[(
        &ReservedValue<CurrentCognitionJsonRecordV1>,
        &serde_json::Value,
    )],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    replace_current_cognition_fixed_batch(
        changes,
        ctx,
        write,
        work,
        guard,
        live,
        BatchProfile::Economy,
    )
}

#[derive(Clone, Copy)]
enum BatchProfile {
    Service,
    Economy,
}
impl BatchProfile {
    fn max(self) -> usize {
        match self {
            Self::Service => 4,
            Self::Economy => 9,
        }
    }
    fn allows(self, field: CognitionJsonFieldV1) -> bool {
        match self {
            Self::Service => matches!(
                field,
                CognitionJsonFieldV1::SchedulerState
                    | CognitionJsonFieldV1::Continuations
                    | CognitionJsonFieldV1::ContinuationEvaluations
                    | CognitionJsonFieldV1::ContinuationContexts
            ),
            Self::Economy => matches!(
                field,
                CognitionJsonFieldV1::EconomyBalances
                    | CognitionJsonFieldV1::EconomyProvisionedBalances
                    | CognitionJsonFieldV1::EconomyLeaseBindingKeys
                    | CognitionJsonFieldV1::EconomyProvisions
                    | CognitionJsonFieldV1::EconomyProvisionReceipts
                    | CognitionJsonFieldV1::EconomyLeases
                    | CognitionJsonFieldV1::EconomyReceipts
                    | CognitionJsonFieldV1::EconomyIdempotency
                    | CognitionJsonFieldV1::EconomyOperations
            ),
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn replace_current_cognition_fixed_batch<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + StatePageWriter,
    CW: FnMut() -> bool,
>(
    changes: &[(
        &ReservedValue<CurrentCognitionJsonRecordV1>,
        &serde_json::Value,
    )],
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut JsonWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    profile: BatchProfile,
) -> Result<(), WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if changes.is_empty()
            || changes.len() > profile.max()
            || ctx.values.class != MemoryClass::Ordinary
            || !ctx.values.memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut roots = [None; 9];
        for (index, (expected, after)) in changes.iter().enumerate() {
            if (ctx.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            expected.validate_provenance(tx, ctx.values.memory, live)?;
            if !expected.uses_budget(ctx.values.memory) || !profile.allows(expected.field) {
                return Err(invalid().into());
            }
            for (previous, _) in &changes[..index] {
                if previous.key == expected.key {
                    return Err(invalid().into());
                }
            }
            let child = prepare_json_subtree_v1(expected.field.profile(), after, write, work)?;
            let row = write_root(expected.field, child, write)?;
            if row.field_key != expected.key {
                return Err(invalid().into());
            }
            roots[index] = Some(row.value_root);
        }
        {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            for (expected, _) in changes {
                let metadata = WorldFieldKeyV1::new(
                    expected.field.descriptor(),
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .map_err(ValueTreeError::Invalid)?;
                if lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?
                    != Some(expected.metadata)
                    || lookup(tx, &expected.key, &mut records, &mut ctx.take_state_error)?
                        != Some(expected.before)
                {
                    return Err(invalid().into());
                }
            }
        }
        // The actual row comparisons above, rather than a refreshed old owner,
        // establish this write cut after all selected reads and Journal appends.
        let write_guard = tx.guard();
        tx.validate_guard(&write_guard, live)?;
        for (expected, _) in changes {
            expected.validate_provenance(tx, ctx.values.memory, live)?;
        }
        for (index, (expected, _)) in changes.iter().enumerate() {
            if (ctx.cancelled)() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let namespace = WorldFieldKeyV1::from_bytes(expected.key.as_bytes())
                .map_err(ValueTreeError::Invalid)?
                .namespace;
            tx.write(
                namespace,
                expected.key.as_bytes(),
                Some(roots[index].ok_or_else(invalid)?),
            )?;
        }
        Ok(())
    })
}
