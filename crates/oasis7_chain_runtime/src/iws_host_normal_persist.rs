//! Persist the fixed Runtime's actual sealed transition and History material.
//! This is provisional content, never a receipt or an accepted-head constructor.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_material::OriginalObjectStreamWriter,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_runtime::PreparedNormalHostRuntimeV1;

#[derive(Debug)]
pub enum NormalCommitPersistErrorV1<E> {
    Context(FormalError),
    Execution(ExecutionError),
    Value(ValueTreeError),
    Original(E),
}
fn header<E>(
    common: &mut FormalBoundedContextV1<'_>,
    encode: impl FnOnce(
        &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Vec<u8>, ExecutionError>,
) -> Result<ReservedValue<Vec<u8>>, NormalCommitPersistErrorV1<E>> {
    // Each selected original header has a 4096-byte codec limit. The source is
    // the already checked sealed preparation. Pay growable scratch + final Vec.
    let fee = common
        .memory()
        .reserve(
            4 * oasis7_formal_protocol::incremental_history::HISTORY_MAX_HEADER_BYTES + 1024,
            MemoryClass::Ordinary,
        )
        .map_err(|e| NormalCommitPersistErrorV1::Value(ValueTreeError::Memory(e)))?;
    let mut first = None;
    let raw = encode(&mut |steps, bytes| {
        common.charge(steps, bytes).map_err(|e| {
            first = Some(e);
            ExecutionError::Aborted
        })
    });
    if let Some(error) = first {
        return Err(NormalCommitPersistErrorV1::Context(error));
    }
    let raw = raw.map_err(NormalCommitPersistErrorV1::Execution)?;
    if raw.capacity() > fee.bytes() {
        return Err(NormalCommitPersistErrorV1::Execution(
            ExecutionError::ResourceLimited("normal commit header capacity"),
        ));
    }
    Ok(fee.retain(raw))
}
fn put_original<O: OriginalObjectStreamWriter>(
    bytes: &ReservedValue<Vec<u8>>,
    original: &mut O,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), NormalCommitPersistErrorV1<O::Error>> {
    if !bytes.uses_budget(common.memory()) {
        return Err(NormalCommitPersistErrorV1::Execution(
            ExecutionError::Invalid("normal commit original pool"),
        ));
    }
    put_original_bytes(bytes.as_slice(), original, common)
}
fn put_original_bytes<O: OriginalObjectStreamWriter>(
    bytes: &[u8],
    original: &mut O,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), NormalCommitPersistErrorV1<O::Error>> {
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.chunks(4096) {
        common
            .charge(1, chunk.len() as u64)
            .map_err(NormalCommitPersistErrorV1::Context)?;
        hash.update(chunk);
    }
    let id = *hash.finalize().as_bytes();
    let actual = original
        .write_original(
            &mut std::io::Cursor::new(bytes),
            bytes.len() as u64,
            id,
            common.memory(),
        )
        .map_err(NormalCommitPersistErrorV1::Original)?;
    common
        .charge(1, 32)
        .map_err(NormalCommitPersistErrorV1::Context)?;
    if actual != id {
        return Err(NormalCommitPersistErrorV1::Execution(
            ExecutionError::Invalid("normal commit persisted original identity"),
        ));
    }
    Ok(())
}
/// The caller is the fixed Chain IO scope. Both physical namespaces and the
/// original Node/Frozen holders remain borrowed across every write/readback.
pub(super) fn persist_sealed_normal_commit_v1<W: ValueNodeWriter, O: OriginalObjectStreamWriter>(
    prepared: &PreparedNormalHostRuntimeV1,
    input: &crate::iws_normal_input::PaidVerifiedNormalTickInputV1<'_>,
    pages: &mut W,
    originals: &mut O,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), NormalCommitPersistErrorV1<O::Error>> {
    input
        .validate_context(common)
        .map_err(NormalCommitPersistErrorV1::Context)?;
    // Frozen already authenticated this exact graph. Reuse its originals,
    // including the real execution-mode manifest/input/policy/continuation;
    // never copy a GEN manifest into an ordinary candidate or rerun a producer.
    for object in &input.data().checked().material().objects {
        common
            .charge(1, object.content_hash.len() as u64)
            .map_err(NormalCommitPersistErrorV1::Context)?;
        put_original_bytes(&object.bytes, originals, common)?;
    }
    let candidate = prepared.candidate();
    candidate
        .validate_live_fence(live)
        .map_err(NormalCommitPersistErrorV1::Execution)?;
    if !candidate.transition().uses_budget(common.memory())
        || !candidate.history().uses_budget(common.memory())
        || !candidate.catalog().uses_budget(common.memory())
    {
        return Err(NormalCommitPersistErrorV1::Execution(
            ExecutionError::Invalid("normal sealed commit pool"),
        ));
    }
    // These are the actual post-seal tree nodes, not a historical full scan.
    for (id, bytes) in candidate
        .transition()
        .new_nodes
        .iter()
        .map(|n| (&n.digest, &n.bytes))
        .chain(
            candidate
                .catalog()
                .new_nodes
                .iter()
                .map(|n| (&n.digest, &n.bytes)),
        )
    {
        common
            .charge(1, id.len() as u64)
            .map_err(NormalCommitPersistErrorV1::Context)?;
        let mut digest = [0u8; 32];
        hex::decode_to_slice(id, &mut digest).map_err(|_| {
            NormalCommitPersistErrorV1::Execution(ExecutionError::Invalid(
                "normal sealed node digest",
            ))
        })?;
        let fee = common
            .memory()
            .reserve(
                bytes.len() + std::mem::size_of::<Vec<u8>>(),
                MemoryClass::Ordinary,
            )
            .map_err(|e| NormalCommitPersistErrorV1::Value(ValueTreeError::Memory(e)))?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(bytes.len()).map_err(|_| {
            NormalCommitPersistErrorV1::Execution(ExecutionError::ResourceLimited(
                "normal sealed node allocation",
            ))
        })?;
        if copy.capacity() > bytes.len() {
            return Err(NormalCommitPersistErrorV1::Execution(
                ExecutionError::ResourceLimited("normal sealed node capacity"),
            ));
        }
        for chunk in bytes.chunks(4096) {
            common
                .charge(1, chunk.len() as u64)
                .map_err(NormalCommitPersistErrorV1::Context)?;
            copy.extend_from_slice(chunk);
        }
        pages
            .put_node(digest, fee.retain(copy))
            .map_err(NormalCommitPersistErrorV1::Value)?;
    }
    put_original(candidate.root_set_bytes(), originals, common)?;
    put_original(candidate.catalog_bytes(), originals, common)?;
    if let Some(transition) = candidate.history().transition_bytes.as_ref() {
        put_original(transition, originals, common)?;
    }
    let memory = common.memory().clone();
    let mut first = None;
    let facts = candidate
        .history()
        .facts
        .canonical_bytes_with_work(&memory, &mut |s, b| {
            common.charge(s, b).map_err(|e| {
                first = Some(e);
                ExecutionError::Aborted
            })
        });
    if let Some(error) = first {
        return Err(NormalCommitPersistErrorV1::Context(error));
    }
    put_original(
        &facts.map_err(NormalCommitPersistErrorV1::Execution)?,
        originals,
        common,
    )?;
    let history = candidate.history();
    macro_rules! put_header {
        ($value:expr) => {{
            let bytes = header::<O::Error>(common, |work| $value.canonical_bytes_with_work(work))?;
            put_original(&bytes, originals, common)?;
        }};
    }
    put_header!(history.segment);
    put_header!(history.prefix);
    put_header!(history.body);
    if let Some(checkpoint) = history.checkpoint.as_ref() {
        let bytes = header(common, |w| checkpoint.canonical_bytes_with_work(w))?;
        put_original(&bytes, originals, common)?;
    }
    candidate
        .validate_live_fence(live)
        .map_err(NormalCommitPersistErrorV1::Execution)
}
