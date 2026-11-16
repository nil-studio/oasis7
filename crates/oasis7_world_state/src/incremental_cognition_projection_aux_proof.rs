//! Independent Versioned ProofLedger original JSON paths. This DATA component neither
//! replaces the actual Result role nor qualifies a retained receipt/history.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_json_records::{
        CognitionAuxJsonPatchV1, JsonAuxBuildV1, JsonChangeWorkV1, project_json_aux_paths_v1,
    },
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_world_rule_projection_delta::{Processing, spend},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
use std::{cell::RefCell, mem::size_of, sync::Arc};
struct Row {
    field: CognitionProofLedgerFieldV1,
    present: bool,
    patches: JsonAuxBuildV1,
}
pub struct CognitionProofLedgerProjectionAuxDataV1 {
    rows: [Option<Row>; 8],
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl CognitionProofLedgerProjectionAuxDataV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.parent != tx.parent_fence().parent_roots
            || !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn visit_presence_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, bool) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in self.rows.iter().flatten() {
            work(1, 0)?;
            emit(row.field.name(), row.present)?
        }
        Ok(())
    }
    pub fn visit_paths_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &CognitionAuxJsonPatchV1) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in self.rows.iter().flatten() {
            for patch in &row.patches.patches {
                work(1, 0)?;
                emit(row.field.name(), patch)?
            }
        }
        Ok(())
    }
}
struct PaidValues<'a, 'b, R> {
    source: &'a R,
    p: &'a Processing<'b>,
}
impl<R: ValueNodeReader> ValueNodeReader for PaidValues<'_, '_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        spend(self.p, 0, &mut || false)?;
        let raw = self.source.read_node(id, max, memory, class)?;
        let n = raw
            .len()
            .checked_mul(16)
            .and_then(|n| n.checked_add(128))
            .ok_or(ValueTreeError::ResourceLimited(
                "ProofLedger JSON projection bytes",
            ))?;
        spend(self.p, n, &mut || false)?;
        Ok(raw)
    }
}
#[derive(Clone, Copy)]
enum Scope<'a> {
    Current(&'a ReservedValue<FrozenCognitionProofLedgerManifestSourceV1>),
    Qualified,
}
fn capture_inner<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    json: &mut JsonChangeWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
    scope: Scope<'_>,
) -> Result<CognitionProofLedgerProjectionAuxDataV1, WorldFieldOverlayErrorV1> {
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    if let Scope::Qualified = scope {
        ctx.transaction.validate_bootstrap_read_only_v1(live)?
    }
    let memory = ctx.values.memory.clone();
    before.check_pool(&memory, MemoryClass::Ordinary)?;
    let occurrence = before.occurrence();
    for field in CognitionProofLedgerFieldV1::ALL {
        before.manifest().require_field_source(field)?;
    }
    if !before.uses_budget(&memory)
        || ctx.values.class != MemoryClass::Ordinary
        || !ctx.transaction.uses_budget(&memory)
        || occurrence.roots_id() != ctx.transaction.parent_fence().parent_roots
        || occurrence.head_body_id() != ctx.transaction.parent_fence().parent_commit
    {
        return Err(invalid().into());
    }
    if let Scope::Current(after) = scope {
        if after.wire_version_data_v1() != before.manifest().wire_version
            || !after.uses_budget(&memory)
            || after.parent_record_data_v1() != occurrence.head_record_id()
        {
            return Err(invalid().into());
        }
    }
    let p = Processing {
        bank: RefCell::new(processing),
    };
    spend(
        &p,
        size_of::<CognitionProofLedgerProjectionAuxDataV1>() + 4096,
        &mut ctx.cancelled,
    )?;
    let fee = memory
        .reserve(
            size_of::<CognitionProofLedgerProjectionAuxDataV1>() + 4096,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let source = PaidValues {
        source: ctx.values.source,
        p: &p,
    };
    let mut values = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut *ctx.values.io,
    };
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let mut rows = std::array::from_fn(|_| None);
    for field in CognitionProofLedgerFieldV1::ALL {
        spend(&p, 0, &mut *records.cancelled)?;
        let old = before.manifest().fields[field.index()].as_ref();
        let new = match scope {
            Scope::Current(after) => after.field_root_data_v1(field),
            Scope::Qualified => old,
        };
        if matches!(scope, Scope::Current(_)) && old == new {
            continue;
        }
        let codec = profile(field);
        let json_profile = JsonRecordProfileV1 {
            field: codec.field,
            schema: codec.schema,
        };
        let mut patches = JsonAuxBuildV1::new(&memory)?;
        project_json_aux_paths_v1(
            json_profile,
            if matches!(scope, Scope::Current(_)) {
                old
            } else {
                None
            },
            new,
            &mut records,
            &mut json.json,
            &mut json.paths_remaining,
            &p,
            &mut patches,
        )?;
        rows[field.index()] = Some(Row {
            field,
            present: new.is_some(),
            patches,
        });
    }
    drop(records);
    drop(values);
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error.into());
    }
    if let Scope::Qualified = scope {
        ctx.transaction.validate_bootstrap_read_only_v1(live)?
    }
    Ok(CognitionProofLedgerProjectionAuxDataV1 {
        rows,
        guard: ctx.transaction.guard(),
        parent: ctx.transaction.parent_fence().parent_roots,
        memory,
        _fee: fee,
    })
}
fn capture<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    json: &mut JsonChangeWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
    scope: Scope<'_>,
) -> Result<CognitionProofLedgerProjectionAuxDataV1, WorldFieldOverlayErrorV1> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        capture_inner(before, ctx, json, processing, live, scope)
    }));
    match outcome {
        Ok(result) => {
            let result = if let Some(error) = (ctx.take_state_error)() {
                Err(error.into())
            } else {
                result
            };
            if result.is_err() {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
            }
            result
        }
        Err(panic) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(panic)
        }
    }
}
/// The fixed Runtime consumer first validates its actual final-capture input
/// including Core and TX identity, then lends that input's exact frozen owner.
pub fn capture_current_proof_ledger_projection_aux_data_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    after: &ReservedValue<FrozenCognitionProofLedgerManifestSourceV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    json: &mut JsonChangeWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<CognitionProofLedgerProjectionAuxDataV1, WorldFieldOverlayErrorV1> {
    capture(before, ctx, json, processing, live, Scope::Current(after))
}
pub fn capture_qualified_proof_ledger_projection_aux_data_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &ReservedValue<AcceptedCognitionProofLedgerV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    json: &mut JsonChangeWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    live: &ExecutionFence,
) -> Result<CognitionProofLedgerProjectionAuxDataV1, WorldFieldOverlayErrorV1> {
    capture(before, ctx, json, processing, live, Scope::Qualified)
}
