//! Original outer proposal identity and closed status, without opening manifest JSON.
use super::*;
pub(super) fn capture<M, R, E, C>(
    selected: Selection<'_>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    read.check()?;
    let parsed = WorldFieldKeyV1::from_bytes(selected.key()).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        crate::incremental_runtime_proposal::runtime_proposal_descriptor_v1(),
        parsed.kind,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != selected.key() || selected.namespace() != StateNamespaceV1::Authorization {
        return Err(bad());
    }
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    if parsed.kind != WorldFieldRecordKindV1::Entry {
        return Err(bad());
    }
    let mut text = member::Text {
        bytes: Vec::new(),
        fee: build.memory.reserve(0, MemoryClass::Ordinary)?,
        memory: &build.memory,
        p,
        first: None,
    };
    let max_text_bytes = read.record_bytes;
    let result = crate::incremental_runtime_proposal::projection_status_v1(
        &key,
        &selected.after().or(selected.before()).ok_or_else(bad)?,
        read,
        work.authorization_cells,
        max_text_bytes,
        &mut text,
    );
    if let Some(error) = text.first.take() {
        return Err(error);
    }
    let (original, approved) = result?;
    spend(p, 20, &mut read.cancelled)?;
    let _id_fee = build.memory.reserve(20, MemoryClass::Ordinary)?;
    let id = original.to_string();
    let (path, _path_fee) = owned_path(&[&id], p, &build.memory, &mut read.cancelled)?;
    // Keep status absence distinct from an absent outer row. Rejected/proposed/
    // shadowed rows are present null; only Approved/Applied carry the original hash.
    let hash = if approved {
        Some(std::str::from_utf8(&text.bytes).map_err(|_| bad())?)
    } else {
        None
    };
    let payload = if selected.after().is_some() {
        Some(json(&hash, &build.memory, p, &mut read.cancelled)?)
    } else {
        None
    };
    drop(text);
    build.push(
        "proposal_statuses",
        &path,
        Payload::Json(payload),
        p,
        &mut read.cancelled,
    )?;
    read.check()
}
