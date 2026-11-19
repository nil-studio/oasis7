//! One actual Current Responses point and its original bytes. No accepted
//! occurrence or complete request/receipt validity is inferred from this DATA.
use super::*;
use crate::world::{
    incremental_factory_phase::io::{FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1},
    incremental_module_invocation::*,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    incremental_material::{FactBudget, OriginalObjectReader, OriginalObjectStreamReader},
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    cognition_recovery::CognitionResponseRecordV1,
    incremental_cognition_history::{
        CognitionHistoryFieldV1, CognitionHistoryIndexPointV1, CognitionHistoryIndexWorkV1,
        PendingCurrentCognitionResponsesV1,
    },
    incremental_cognition_history_cbor::{
        CognitionHistoryCborWorkV1, CognitionHistoryRecordSchemaV1, CognitionOriginalCborRequestV1,
        materialize_cognition_history_original_json_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
};
use std::{
    io::{self, Read},
    sync::Arc,
};
pub(in crate::world) struct CurrentCognitionResponseReadWorkV1<'a> {
    pub indexes: &'a mut CognitionHistoryIndexWorkV1,
    pub cbor: &'a mut CognitionHistoryCborWorkV1,
    pub processing: &'a mut BusinessDiffProcessingWorkV1,
    pub originals: &'a mut FactBudget,
    pub max_selected_heap: usize,
}
impl FactoryPhaseMeterLanesV1 for CurrentCognitionResponseReadWorkV1<'_> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        use FactoryPhaseMeterLaneV1::{Bytes as B, Steps as S};
        v(S(&mut self.cbor.nodes_remaining));
        v(S(&mut self.cbor.source_reads_remaining));
        v(B(&mut self.cbor.bytes_remaining));
        v(B(&mut self.cbor.key_bytes_remaining));
        v(S(&mut self.processing.steps_remaining));
        v(B(&mut self.processing.bytes_remaining));
        v(S(&mut self.originals.facts_remaining));
        v(B(&mut self.originals.original_bytes_remaining));
    }
}
pub(super) struct CurrentCognitionResponseOriginalV1 {
    pub(super) point: ReservedValue<CognitionHistoryIndexPointV1>,
    pub(super) raw: Arc<ReservedValue<Vec<u8>>>,
    pub(super) schema: ReservedValue<CognitionHistoryRecordSchemaV1>,
    pub(super) json: ReservedValue<serde_json::Value>,
    pub(super) response: ReservedValue<CognitionResponseRecordV1>,
    _fee: MemoryReservation,
}
struct Input<'a, 'b, 'c> {
    bytes: &'a [u8],
    at: usize,
    common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    first: Option<BodyMutationError>,
}
impl Read for Input<'_, '_, '_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        let n = out.len().min(self.bytes.len() - self.at).min(8192);
        if let Err(e) = self.common.borrow_mut().charge(1, n as u64) {
            self.first = Some(original(e));
            return Err(io::ErrorKind::Other.into());
        }
        out[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}
#[derive(Debug)]
struct CodecError {
    error: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for CodecError {}
fn response(
    value: &serde_json::Value,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<CognitionResponseRecordV1>, BodyMutationError> {
    let bytes =
        crate::world::incremental_normal_projection_commitment::capture_projection_json_data_v1(
            value, common,
        )?;
    // Closed graph: the exact original four fields, String/Option<Value> and
    // empty defaults. No caller-provided layout or custom object decoder enters.
    let width = size_of::<CognitionResponseRecordV1>()
        .max(size_of::<serde_json::Value>())
        .max(size_of::<(String, serde_json::Value)>());
    let n = bytes
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(width))
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(bytes.len().checked_mul(2)?))
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(|| bad("response typed JSON layout"))?;
    if n > max {
        return Err(ValueTreeError::ResourceLimited("response typed JSON maximum").into());
    }
    common.charge(1, n as u64).map_err(original)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let common = RefCell::new(common);
    let mut input = Input {
        bytes: &bytes,
        at: 0,
        common: &common,
        first: None,
    };
    let decoded = serde_json::from_reader(&mut input);
    if let Some(first) = input.first {
        return Err(first);
    }
    common.borrow_mut().consume_steps(0).map_err(original)?;
    match decoded {
        Ok(value) => Ok(fee.retain(value)),
        Err(error) => Err(original(CodecError { error, _fee: fee })),
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(super) fn read_current_cognition_response_original_v1<P, V, PE, PC>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        responses: &mut PendingCurrentCognitionResponsesV1<'_>,
        ordinal: u64,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut CurrentCognitionResponseReadWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionResponseOriginalV1, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let n = size_of::<CurrentCognitionResponseOriginalV1>();
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        parent.validate_context(common).map_err(original)?;
        responses.validate_context(self.ctx.transaction, common, &live())?;
        let point = responses.read_index_point(
            ordinal,
            self.ctx.transaction,
            read,
            work.indexes,
            common,
            &live(),
        )?;
        let row = point.row();
        let (raw, schema, json): (
            Arc<ReservedValue<Vec<u8>>>,
            ReservedValue<CognitionHistoryRecordSchemaV1>,
            ReservedValue<serde_json::Value>,
        ) = self.with_factory_prologue_external_meter_v1(
            common,
            live,
            work,
            |root, _live, work| {
                work.originals.facts_remaining =
                    work.originals.facts_remaining.checked_sub(1).ok_or(
                        ValueTreeError::ResourceLimited("Cognition response Original count"),
                    )?;
                let max = usize::try_from(row.encoded_bytes)
                    .map_err(|_| bad("response Original length"))?;
                if row.encoded_bytes > work.originals.max_fact_bytes
                    || row.encoded_bytes > work.originals.original_bytes_remaining
                {
                    return Err(ValueTreeError::ResourceLimited(
                        "Cognition response Original bytes",
                    )
                    .into());
                }
                work.originals.original_bytes_remaining -= row.encoded_bytes;
                if root.originals.allocation_class() != MemoryClass::Ordinary
                    || !root
                        .originals
                        .allocation_budget()
                        .same_pool(root.ctx.values.memory)
                {
                    return Err(bad("response actual Original reader pool/class").into());
                }
                let raw = root
                    .originals
                    .read_original(&row.location.original, max, root.ctx.values.memory)
                    .map_err(RootModuleExecutionErrorV1::ArtifactOriginalStorage)?;
                if !raw.uses_budget(root.ctx.values.memory) || raw.len() != max {
                    return Err(bad("response actual Original buffer binding").into());
                }
                let (schema, json) = materialize_cognition_history_original_json_v1(
                    CognitionOriginalCborRequestV1 {
                        field: CognitionHistoryFieldV1::Responses,
                        id: row.location.original,
                        bytes: row.encoded_bytes,
                    },
                    &mut raw.as_slice(),
                    root.ctx.values.memory,
                    MemoryClass::Ordinary,
                    work.cbor,
                    work.processing,
                    &mut root.ctx.cancelled,
                )?;
                Ok((raw, schema, json))
            },
        )?;
        let response = response(&json, work.max_selected_heap, common)?;
        responses.validate_context(self.ctx.transaction, common, &live())?;
        Ok(CurrentCognitionResponseOriginalV1 {
            point,
            raw,
            schema,
            json,
            response,
            _fee: fee,
        })
    }
}
