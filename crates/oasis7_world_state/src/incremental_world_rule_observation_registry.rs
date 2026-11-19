//! Versioned observation references in the existing typed request map.
//! Shape and CAS are data checks; original/cut verification is still required.
use super::*;
use crate::world_rules::WorldRuleReceiptV1;
use oasis7_agent_api::world_action::{EmergencySupplyCommandV1, WorldRuleCommandV1};

const ORIGINAL_PREFIX: &str = "current-observation-original-v2:";

pub struct CurrentObservationRegistryEntryV2 {
    overlay: ReservedValue<WorldRuleCompositeOverlayV1<Requests>>,
    region: String,
    window: u64,
    key: String,
    length: u64,
    length_value: [u8; 32],
    record: Option<RecoveredTypedRecordV1<WorldRuleReceiptV1>>,
    memory: MemoryBudget,
    identity: PreparedGuard,
    _phase: MemoryReservation,
}
impl CurrentObservationRegistryEntryV2 {
    pub fn region(&self) -> &str {
        &self.region
    }
    pub fn window_start(&self) -> u64 {
        self.window
    }
    pub fn validate_current_scope<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.identity, live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.overlay.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }

    pub fn registered(&self) -> Option<&WorldRuleReceiptV1> {
        self.record.as_ref().map(RecoveredTypedRecordV1::value)
    }
    pub fn registry_key(&self) -> &str {
        &self.key
    }
    pub fn original_object_id(&self) -> Option<&str> {
        self.registered()?.result_id.strip_prefix(ORIGINAL_PREFIX)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Current observation V2 registration")
}
fn shape(value: &WorldRuleReceiptV1, region: &str, window: u64) -> Result<(), ValueTreeError> {
    let WorldRuleCommandV1::EmergencySupply {
        command: EmergencySupplyCommandV1::Observe { region_id },
    } = &value.command
    else {
        return Err(invalid());
    };
    let id = value
        .result_id
        .strip_prefix(ORIGINAL_PREFIX)
        .ok_or_else(invalid)?;
    if region_id != region
        || value.committed_at != window.checked_add(60).ok_or_else(invalid)?
        || !window.is_multiple_of(60)
        || id.len() != 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || crate::world_rules::validate_id(&value.actor_id).is_err()
        || crate::world_rules::validate_id(&value.request_id).is_err()
    {
        return Err(invalid());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn read_current_observation_registry_v2<M, S, R, E, C>(
    region: &str,
    window: u64,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Requests>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentObservationRegistryEntryV2>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if !window.is_multiple_of(60) || region.is_empty() || region.len() > 128 {
            return Err(invalid().into());
        }
        let bytes = region
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(4096 + size_of::<CurrentObservationRegistryEntryV2>()))
            .ok_or_else(invalid)?;
        let phase = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        debit_key(bytes, ctx.key_work)?;
        let key = crate::world_rules::current_observation_key_v2(region, window);
        let overlay =
            bind_current_world_rule_branch_v1(parent, roots, state, ctx, scan, work, guard, live)?;
        let body = overlay.body();
        let (length_value, length_row) =
            point(body, None, ctx, work.max_tree_heap)?.ok_or_else(invalid)?;
        let ColumnCellV1::U64(length) = &length_row.cell else {
            return Err(invalid().into());
        };
        let length = *length;
        let record = match point(body, Some(&key), ctx, work.max_tree_heap)? {
            None => None,
            Some((_, row)) => {
                let ColumnCellV1::Root(child) = &row.cell else {
                    return Err(invalid().into());
                };
                let mut read = WorldFieldReadContextV1 {
                    root: child,
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
                let value: RecoveredTypedRecordV1<WorldRuleReceiptV1> =
                    recover_typed_record_body_v1(
                        child,
                        &WorldRuleCompositeBranchV1::Requests.profile(),
                        &mut read,
                        scan,
                        work,
                    )?;
                let value_ref = value.value();
                debit_key(
                    value_ref
                        .actor_id
                        .len()
                        .checked_add(value_ref.request_id.len())
                        .and_then(|n| n.checked_add(value_ref.result_id.len()))
                        .and_then(|n| n.checked_add(region.len() + 128))
                        .ok_or_else(invalid)?,
                    ctx.key_work,
                )?;
                shape(value_ref, region, window)?;
                Some(value)
            }
        };
        let fee = ctx
            .values
            .memory
            .reserve(
                size_of::<CurrentObservationRegistryEntryV2>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        Ok(fee.retain(CurrentObservationRegistryEntryV2 {
            overlay,
            region: region.to_owned(),
            window,
            key,
            length,
            length_value,
            record,
            memory: ctx.values.memory.clone(),
            identity: ctx.transaction.guard(),
            _phase: phase,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

/// Insert only the exact observed absent key. The Runtime registrar must
/// consume the actual paid observation/cut owner before supplying this record.
#[allow(clippy::too_many_arguments)]
pub fn insert_current_observation_registry_v2<M, S, R, E, C, W, CW>(
    source: &ReservedValue<CurrentObservationRegistryEntryV2>,
    receipt: &WorldRuleReceiptV1,
    parent: &ReservedValue<WorldRuleCompositeQualificationV1<Requests>>,
    roots: &RootSetV1,
    state: &S,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    scan: &mut StateScanBudget,
    read_work: &mut WorldRuleCompositeReadWorkV1,
    write_work: &mut WorldRuleCompositeWriteWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<WorldRuleCompositeOverlayV1<Requests>>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        ctx.transaction
            .validate_transaction_identity(&source.identity)?;
        if !source.uses_budget(ctx.values.memory)
            || !source.memory.same_pool(ctx.values.memory)
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || source.record.is_some()
        {
            return Err(invalid().into());
        }
        debit_key(
            source
                .region
                .len()
                .checked_add(receipt.actor_id.len())
                .and_then(|n| n.checked_add(receipt.request_id.len()))
                .and_then(|n| n.checked_add(receipt.result_id.len()))
                .and_then(|n| n.checked_add(128))
                .ok_or_else(invalid)?,
            ctx.key_work,
        )?;
        shape(receipt, &source.region, source.window)?;
        let after = source.length.checked_add(1).ok_or_else(invalid)?;
        let edits = [
            WorldRuleRequestEditV1::Count {
                expected: source.length_value,
                after,
            },
            WorldRuleRequestEditV1::Put {
                key: &source.key,
                expected: None,
                receipt,
            },
        ];
        patch_world_rule_requests_v1(
            parent,
            roots,
            Some(&source.overlay),
            &edits,
            state,
            ctx,
            write,
            scan,
            read_work,
            write_work,
            guard,
            live,
        )
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
