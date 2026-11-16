//! Optional receipts branch lifecycle. Existing maps are edited through the
//! ordinary fixed ledger path writer; only actual absence/presence changes use
//! this producer. A legacy V1 source never supplies absence for this operation.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use crate::incremental_json_records::{JsonChangeWorkV1, JsonWriteWorkV1, prepare_json_subtree_v1};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
};
/// Fixed CAS for introduction/removal of the original nested receipts object.
/// `expected` is compared with the current owner's actual subtree. It is DATA,
/// never a permit to execute a delegation action. Existing nonempty maps must
/// use the original path writer, not replacement of their complete history.
#[allow(clippy::too_many_arguments)]
pub fn change_current_cognition_delegation_receipts_presence_v2<M, P, R, E, C, W, D>(
    current: &mut CurrentCognitionProofLedgerV1,
    expected: Option<&StateRootV1>,
    after: Option<&serde_json::Value>,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    producer: &mut JsonWriteWorkV1,
    changed: &mut JsonChangeWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    change_current_proof_field_presence(
        current,
        CognitionProofLedgerFieldV1::DelegationReceipts,
        expected,
        after,
        tx,
        read,
        write,
        producer,
        changed,
        common,
        live,
    )
}
/// Presence CAS for the two original optional service fields. Their values are
/// preserved verbatim; only the original business operation requires a map.
pub fn change_current_cognition_service_proof_field_presence_v3<M, P, R, E, C, W, D>(
    current: &mut CurrentCognitionProofLedgerV1,
    field: CognitionProofLedgerFieldV1,
    expected: Option<&StateRootV1>,
    after: Option<&serde_json::Value>,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    producer: &mut JsonWriteWorkV1,
    changed: &mut JsonChangeWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    if !matches!(
        field,
        CognitionProofLedgerFieldV1::ServiceCausalProposals
            | CognitionProofLedgerFieldV1::ActionDigests
    ) {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        return Err(invalid().into());
    }
    change_current_proof_field_presence(
        current, field, expected, after, tx, read, write, producer, changed, common, live,
    )
}
fn change_current_proof_field_presence<M, P, R, E, C, W, D>(
    current: &mut CurrentCognitionProofLedgerV1,
    field: CognitionProofLedgerFieldV1,
    expected: Option<&StateRootV1>,
    after: Option<&serde_json::Value>,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    producer: &mut JsonWriteWorkV1,
    changed: &mut JsonChangeWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        current.validate_context(tx, common, live)?;
        if field.index() >= current.wire_version.field_count() {
            return Err(ValueTreeError::Invalid(if field.index() >= 6 {
                "legacy ProofLedger cognition service field source missing"
            } else {
                "legacy ProofLedger delegation receipts source missing"
            })
            .into());
        }
        if !read.values.memory.same_pool(write.memory)
            || !common.memory().same_pool(write.memory)
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(invalid().into());
        }
        let actual = current.fields[field.index()].as_ref().map(|q| q.root());
        if actual != expected {
            return Err(invalid().into());
        }
        if actual.is_some() && after.is_some() {
            return Err(
                ValueTreeError::Invalid("existing delegation receipts require path edit").into(),
            );
        }
        let Some(after) = after else {
            current.fields[field.index()] = None;
            return Ok(());
        };
        if field == CognitionProofLedgerFieldV1::DelegationReceipts && !after.is_object() {
            return Err(invalid().into());
        }
        let p = profile(field);
        let jp = JsonRecordProfileV1 {
            field: p.field,
            schema: p.schema,
        };
        let (_, meter, stop) = common.parts();
        let child = meter.begin_child().map_err(|_| invalid())?;
        let steps = child.remaining_steps() / 7;
        let bytes = child.remaining_read_bytes() / 5;
        producer.cells_remaining = producer.cells_remaining.min(steps);
        producer.json_nodes_remaining = producer.json_nodes_remaining.min(steps);
        changed.json.cells_remaining = changed.json.cells_remaining.min(steps);
        changed.json.json_nodes_remaining = changed.json.json_nodes_remaining.min(steps);
        changed.paths_remaining = changed.paths_remaining.min(steps);
        read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(steps);
        write.io.nodes_remaining = write.io.nodes_remaining.min(steps);
        read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(bytes);
        write.io.read_bytes_remaining = write.io.read_bytes_remaining.min(bytes);
        write.io.write_bytes_remaining = write.io.write_bytes_remaining.min(bytes);
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(bytes);
        write.key_work.bytes_remaining = write.key_work.bytes_remaining.min(bytes);
        let before_s = [
            producer.cells_remaining,
            producer.json_nodes_remaining,
            changed.json.cells_remaining,
            changed.json.json_nodes_remaining,
            changed.paths_remaining,
            read.values.io.nodes_remaining,
            write.io.nodes_remaining,
        ];
        let before_b = [
            read.values.io.read_bytes_remaining,
            write.io.read_bytes_remaining,
            write.io.write_bytes_remaining,
            read.key_work.bytes_remaining,
            write.key_work.bytes_remaining,
        ];
        let produced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<_, WorldFieldOverlayErrorV1> {
                let stop = std::cell::RefCell::new(stop);
                let read_stop = &mut read.cancelled;
                let write_stop = &mut write.cancelled;
                let mut output = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || write_stop() || (stop.borrow_mut())(),
                };
                let root = prepare_json_subtree_v1(jp, after, &mut output, producer)?;
                let mut source = WorldFieldReadContextV1 {
                    root: &root,
                    state: read.state,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: read.values.source,
                        memory: read.values.memory,
                        class: read.values.class,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: || read_stop() || (stop.borrow_mut())(),
                };
                let next = qualify_json_subtree_v1(jp, &root, &mut source, &mut changed.json)?;
                if field == CognitionProofLedgerFieldV1::DelegationReceipts {
                    crate::incremental_json_records::require_json_container_records_v1(
                        jp,
                        &next,
                        false,
                        &mut source.records(),
                        &mut changed.json,
                    )?;
                }
                if (output.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                Ok(next)
            },
        ));
        // A panic abandons the live child. It must not be converted into a
        // successful reconciliation: ChildLease Drop forfeits its admitted
        // balance, and no caller lane may retain that abandoned allowance.
        let produced = match produced {
            Ok(result) => result,
            Err(panic) => {
                producer.cells_remaining = 0;
                producer.json_nodes_remaining = 0;
                changed.json.cells_remaining = 0;
                changed.json.json_nodes_remaining = 0;
                changed.paths_remaining = 0;
                read.values.io.nodes_remaining = 0;
                write.io.nodes_remaining = 0;
                read.values.io.read_bytes_remaining = 0;
                write.io.read_bytes_remaining = 0;
                write.io.write_bytes_remaining = 0;
                read.key_work.bytes_remaining = 0;
                write.key_work.bytes_remaining = 0;
                drop(child);
                std::panic::resume_unwind(panic)
            }
        };
        let after_s = [
            producer.cells_remaining,
            producer.json_nodes_remaining,
            changed.json.cells_remaining,
            changed.json.json_nodes_remaining,
            changed.paths_remaining,
            read.values.io.nodes_remaining,
            write.io.nodes_remaining,
        ];
        let after_b = [
            read.values.io.read_bytes_remaining,
            write.io.read_bytes_remaining,
            write.io.write_bytes_remaining,
            read.key_work.bytes_remaining,
            write.key_work.bytes_remaining,
        ];
        let delta = |b: &[u64], a: &[u64]| {
            b.iter()
                .zip(a)
                .try_fold(0u64, |sum, (b, a)| sum.checked_add(b.checked_sub(*a)?))
        };
        let paid = match (delta(&before_s, &after_s), delta(&before_b, &after_b)) {
            (Some(s), Some(b)) => child
                .consume(s, b)
                .map_err(|_| ValueTreeError::ResourceLimited("ProofLedger Common producer")),
            _ => Err(invalid()),
        };
        child.finish();
        let next = produced?;
        paid?;
        common.consume_steps(0).map_err(common_error)?;
        current.fields[field.index()] = Some(next);
        Ok(())
    }));
    match outcome {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
