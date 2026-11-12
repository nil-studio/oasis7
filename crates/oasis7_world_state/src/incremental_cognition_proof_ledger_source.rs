//! Cold independent ProofLedger reconstruction from genuine accepted Result bytes.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFactKindV1,
    incremental_material::{
        CoreError, FactReadContextV1, FactRequestV1, OriginalObjectStreamReader,
        verify_fact_occurrence_v1,
    },
};
use std::io::{self, Write};
#[derive(Debug)]
pub enum ProofLedgerSourceErrorV1<E> {
    Value(ValueTreeError),
    Formal(CoreError<E>),
    Original(E),
}
impl<E> From<ValueTreeError> for ProofLedgerSourceErrorV1<E> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
struct ManifestSink<'a, C> {
    bytes: [u8; COGNITION_PROOF_LEDGER_BYTES_V1],
    at: usize,
    stopped: bool,
    cancelled: &'a mut C,
}
impl<C: FnMut() -> bool> Write for ManifestSink<'_, C> {
    fn write(&mut self, raw: &[u8]) -> io::Result<usize> {
        if self.stopped {
            return Err(io::Error::other("ProofLedger stream stopped"));
        }
        self.stopped = true;
        if (self.cancelled)() {
            return Err(io::Error::other("ProofLedger cancelled"));
        }
        let n = raw.len().min(8192);
        let end = self
            .at
            .checked_add(n)
            .filter(|n| *n <= self.bytes.len())
            .ok_or_else(|| io::Error::other("ProofLedger length"))?;
        self.bytes[self.at..end].copy_from_slice(&raw[..n]);
        self.at = end;
        self.stopped = false;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.stopped {
            Err(io::Error::other("ProofLedger stream stopped"))
        } else {
            Ok(())
        }
    }
}
/// A historical capsule cannot stand in for this parent's exact ledger.
/// The mandatory genuine Result router independently enforces profile uniqueness.
pub fn recover_cognition_proof_ledger_v1<O, M, C, S, R, E, D>(
    request: &FactRequestV1<'_>,
    routing: &ReservedValue<
        crate::incremental_world_result_router_accepted::AcceptedWorldResultRoutingV1,
    >,
    context: &mut FactReadContextV1<'_, O, M, C>,
    read: &mut WorldFieldReadContextV1<'_, S, R, E, D>,
    work: &mut JsonReadWorkV1,
) -> Result<ReservedValue<AcceptedCognitionProofLedgerV1>, ProofLedgerSourceErrorV1<O::Error>>
where
    O: OriginalObjectStreamReader,
    M: MerkleNodeReader,
    C: FnMut() -> bool,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
{
    let _routing_phase = context
        .memory
        .reserve(4096, context.class)
        .map_err(ValueTreeError::Memory)?;
    let routed = routing.proof_ledger().ok_or_else(invalid)?;
    let selected = routed.occurrence();
    if !routing.uses_budget(context.memory)
        || routing.allocation_class() != context.class
        || routing.record_id() != request.head.verified_record_id()
        || routing.roots_id() != request.roots.digest().map_err(|_| invalid())?
        || !selected.uses_budget(context.memory)
        || selected.allocation_class() != context.class
        || selected.kind() != ExecutionFactKindV1::Result
        || selected.location() != request.location
        || selected.head_record_id() != routing.record_id()
        || selected.head_body_id() != routing.body_id()
        || selected.fact_body_id() != routing.body_id()
        || selected.roots_id() != routing.roots_id()
    {
        return Err(invalid().into());
    }
    if request.kind != ExecutionFactKindV1::Result
        || !context.memory.same_pool(read.values.memory)
        || context.class != read.values.class
    {
        return Err(invalid().into());
    }
    let outer = context
        .memory
        .reserve(
            std::mem::size_of::<AcceptedCognitionProofLedgerV1>() + 4096,
            context.class,
        )
        .map_err(ValueTreeError::Memory)?;
    let manifest_fee = context
        .memory
        .reserve(
            std::mem::size_of::<CognitionProofLedgerManifestV1>() + 5 * 512,
            context.class,
        )
        .map_err(ValueTreeError::Memory)?;
    let occurrence =
        verify_fact_occurrence_v1(request, context).map_err(ProofLedgerSourceErrorV1::Formal)?;
    if occurrence.fact_body_id() != occurrence.head_body_id()
        || occurrence.original_bytes() != COGNITION_PROOF_LEDGER_BYTES_V1 as u64
    {
        return Err(invalid().into());
    }
    context.fact_budget.facts_remaining = context
        .fact_budget
        .facts_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("ProofLedger originals"))?;
    if context.fact_budget.max_fact_bytes < COGNITION_PROOF_LEDGER_BYTES_V1 as u64 {
        return Err(ValueTreeError::ResourceLimited("ProofLedger original bound").into());
    }
    context.fact_budget.original_bytes_remaining = context
        .fact_budget
        .original_bytes_remaining
        .checked_sub(COGNITION_PROOF_LEDGER_BYTES_V1 as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "ProofLedger original bytes",
        ))?;
    let mut sink = ManifestSink {
        bytes: [0; COGNITION_PROOF_LEDGER_BYTES_V1],
        at: 0,
        stopped: false,
        cancelled: &mut context.cancelled,
    };
    let result = context.originals.stream_original(
        &request.location.original,
        COGNITION_PROOF_LEDGER_BYTES_V1 as u64,
        context.memory,
        &mut sink,
    );
    if (sink.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if sink.stopped || sink.at != COGNITION_PROOF_LEDGER_BYTES_V1 {
        return Err(invalid().into());
    }
    let n = result.map_err(ProofLedgerSourceErrorV1::Original)?;
    if n != sink.at as u64 || blake3::hash(&sink.bytes).as_bytes() != &request.location.original {
        return Err(invalid().into());
    }
    let manifest = CognitionProofLedgerManifestV1::from_canonical_bytes(&sink.bytes)?;
    if manifest.roots != occurrence.roots_id()
        || occurrence.head_record_id() != request.head.verified_record_id()
    {
        return Err(invalid().into());
    }
    let parent_id=match request.head.record().kind {
 oasis7_formal_protocol::controlled_authority::formal::FormalRecordKindV1::InitialActivation=>None,
 _=>{let mut id=[0;32];hex::decode_to_slice(&request.head.record().parent_record_id,&mut id).map_err(|_|invalid())?;Some(id)}
 };
    if manifest.previous_record != parent_id {
        return Err(invalid().into());
    }
    let fields = qualify_fields(&manifest, read, work)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(outer.retain(AcceptedCognitionProofLedgerV1 {
        manifest: manifest_fee.retain(manifest),
        occurrence,
        fields,
    }))
}
