//! The original subject is one closed logical field, not independent branch flags.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub const RUNTIME_CAPABILITY_SUBJECT_COLUMNS_V1: [RuntimeCapabilityColumnV1; 9] = [
    RuntimeCapabilityColumnV1::SubjectKind,
    RuntimeCapabilityColumnV1::SubjectAgentId,
    RuntimeCapabilityColumnV1::SubjectOwnerBinding,
    RuntimeCapabilityColumnV1::SubjectGeneration,
    RuntimeCapabilityColumnV1::SubjectModuleId,
    RuntimeCapabilityColumnV1::SubjectModuleVersion,
    RuntimeCapabilityColumnV1::SubjectInstanceId,
    RuntimeCapabilityColumnV1::SubjectSystemId,
    RuntimeCapabilityColumnV1::SubjectEpoch,
];
pub(super) fn values(subject: &CapabilitySubject) -> [RuntimeCapabilityValueRefV1<'_>; 9] {
    use RuntimeCapabilityValueRefV1 as V;
    match subject {
        CapabilitySubject::Agent {
            agent_id,
            owner_binding,
            generation,
        } => [
            V::Text("agent"),
            V::Text(agent_id),
            V::Text(owner_binding),
            V::Unsigned(*generation),
            V::Unit,
            V::Unit,
            V::Unit,
            V::Unit,
            V::Unit,
        ],
        CapabilitySubject::Module {
            module_id,
            module_version,
            instance_id,
        } => [
            V::Text("module"),
            V::Unit,
            V::Unit,
            V::Unit,
            V::Text(module_id),
            V::Text(module_version),
            V::Text(instance_id),
            V::Unit,
            V::Unit,
        ],
        CapabilitySubject::System { system_id, epoch } => [
            V::Text("system"),
            V::Unit,
            V::Unit,
            V::Unit,
            V::Unit,
            V::Unit,
            V::Unit,
            V::Text(system_id),
            V::Unsigned(*epoch),
        ],
    }
}
/// Shared borrowed projection used by GEN and grouped normal subject changes.
pub fn visit_runtime_capability_subject_v1<'a, E>(
    subject: &'a CapabilitySubject,
    mut emit: impl FnMut(Option<RuntimeCapabilityColumnV1>, MemberPayloadRefV1<'a>) -> Result<(), E>,
) -> Result<(), E> {
    for (column, value) in RUNTIME_CAPABILITY_SUBJECT_COLUMNS_V1
        .into_iter()
        .zip(values(subject))
    {
        emit(Some(column), value.payload())?;
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub struct RuntimeCapabilitySubjectRequestV1<'a> {
    pub key: RuntimeCapabilityRecordKeyV1<'a>,
    pub qualification: &'a ReservedValue<RuntimeCapabilityRecordQualificationV1>,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeCapabilitySubjectRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<(point::Resolved, [RuntimeCapabilityValueV1; 9]), WorldFieldOverlayErrorV1> {
    if request.key.kind != RuntimeCapabilityRecordKindV1::Budget {
        return Err(read::invalid().into());
    }
    let resolved = point::resolve(request.key, request.qualification, tx, records, error)?;
    let mut result = [RuntimeCapabilityValueV1::Unit; 9];
    for (index, column) in RUNTIME_CAPABILITY_SUBJECT_COLUMNS_V1
        .into_iter()
        .enumerate()
    {
        let (key, id) = point::entry(request.key.kind, column, &resolved.child, records)?;
        result[index] = read::value(Some(column), &id, &key, records, max, output)?;
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((resolved, result))
}
/// Streams the original subject's Text cells in SUBJECT_COLUMNS order. The
/// returned fixed array supplies each exact UTF8 length and non-Text value;
/// output remains provisional until the final cancellation check succeeds.
pub fn read_qualified_runtime_capability_subject_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeCapabilitySubjectRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<[RuntimeCapabilityValueV1; 9], WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _admission = ctx
            .values
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        Ok(resolve(
            request,
            tx,
            &mut records,
            &mut ctx.take_state_error,
            max,
            output,
        )?
        .1)
    })
}
struct Compare<'a> {
    segments: [&'a str; 9],
    index: usize,
    offset: usize,
    equal: bool,
}
impl Compare<'_> {
    fn advance(&mut self) {
        while self.index < 9 && self.offset == self.segments[self.index].len() {
            self.index += 1;
            self.offset = 0;
        }
    }
    fn complete(&mut self) -> bool {
        self.advance();
        self.equal && self.index == 9
    }
}
impl std::io::Write for Compare<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut rest = bytes;
        while !rest.is_empty() {
            self.advance();
            if self.index == 9 {
                self.equal = false;
                break;
            }
            let expected = self.segments[self.index].as_bytes();
            let n = rest.len().min(expected.len() - self.offset);
            self.equal &= rest[..n] == expected[self.offset..self.offset + n];
            self.offset += n;
            rest = &rest[n..];
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub struct RuntimeCapabilitySubjectChangeV1<'a> {
    pub request: RuntimeCapabilitySubjectRequestV1<'a>,
    pub expected: &'a CapabilitySubject,
    pub after: &'a CapabilitySubject,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_runtime_capability_subject_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RuntimeCapabilitySubjectChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<RuntimeCapabilityRecordQualificationV1>>, WorldFieldOverlayErrorV1>
{
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(read::invalid().into());
        }
        let _admission = write
            .memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let before = values(change.expected);
        let after = values(change.after);
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            let mut compare = Compare {
                segments: before.map(RuntimeCapabilityValueRefV1::text),
                index: 0,
                offset: 0,
                equal: true,
            };
            let (resolved, current) = resolve(
                change.request,
                tx,
                &mut records,
                error,
                change.max_current_payload_bytes,
                &mut compare,
            )?;
            if !before.into_iter().zip(current).all(|(a, b)| a.matches(b)) || !compare.complete() {
                return Err(read::invalid().into());
            }
            resolved
        };
        let equal = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if equal {
            return Ok(None);
        }
        let mut child = resolved.child;
        for (index, column) in RUNTIME_CAPABILITY_SUBJECT_COLUMNS_V1
            .into_iter()
            .enumerate()
        {
            let unchanged = before[index] == after[index];
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if unchanged {
                continue;
            }
            let (key, old) = {
                let mut combined = || cancel() || (write.cancelled)();
                let mut records = InventoryValueReader {
                    values: &mut ctx.values,
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    cancelled: &mut combined,
                };
                point::entry(change.request.key.kind, column, &child, &mut records)?
            };
            let row = {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                write_cell(
                    change.request.key.kind,
                    Some(column),
                    after[index].payload(),
                    &mut combined,
                )?
            };
            if row.field_key != key.0 {
                return Err(read::invalid().into());
            }
            child = tree::edit(
                child,
                &key.0,
                (Some(old), Some(row.value_root)),
                &mut ctx.values,
                write,
                cancel,
                RecordTreeWritePolicyV1 {
                    max_pending_node_bytes: change.max_pending_node_bytes,
                    labels: genesis::LABELS,
                },
            )?;
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::write_root(change.request.key, child.clone(), &mut combined)?
        };
        if row.field_key != resolved.outer {
            return Err(read::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            change.request.key.kind.namespace(),
            resolved.outer.as_bytes(),
            Some(row.value_root),
        )?;
        let token = closure::mint(
            change.request.key.kind,
            resolved.owner,
            child,
            tx.parent_fence().parent_roots,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(token))
    })
}
