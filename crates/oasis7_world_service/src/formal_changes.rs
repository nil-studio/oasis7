//! Proven journal ranges and privately verified continuation cursors.
use super::*;

/// Formal Changes are computed from a complete already-authorized projection.
/// The global retention floor and high-watermark make an empty batch and sparse
/// Agent event IDs distinguishable from missing or expired history.
pub fn build_formal_changes_page(
    from_anchor: FormalReadAnchorRefV1,
    from_exclusive: FormalBusinessChangesCursorV1,
    target_request: ReadWorldViewRequest,
    target_view: ReadWorldViewResponse<WorldServiceProjection>,
    max_items: u32,
    max_bytes: u64,
) -> Result<FormalBusinessChangesPageV1<WorldServiceProjection>, String> {
    if max_items == 0
        || max_bytes == 0
        || max_items as usize > MAX_PAGE_ITEMS
        || max_bytes > MAX_PAGE_BYTES as u64
    {
        return Err("formal Changes page bounds".into());
    }
    target_view
        .validate(&target_request)
        .map_err(|error| error.to_string())?;
    let metadata = projection_metadata(&target_view.view)?;
    let expected_stream_id = authority::request_digest(
        "formal-events-v1",
        &(&target_request.world, &target_request.scope_id),
    )?;
    if target_request.scope_id != from_exclusive.scope_id
        || from_exclusive.stream_id != expected_stream_id
        || target_request.world.world_id != from_exclusive.commit.world.world_id
        || target_request.world.genesis_digest != from_exclusive.commit.world.genesis_digest
        || target_request.formal_anchor.as_ref() != Some(&from_anchor)
        || from_anchor.position != from_exclusive.commit.position
        || from_anchor.record_id != from_exclusive.commit.execution_block_hash
        || from_exclusive.event_id_era != metadata.event_id_era
        || from_exclusive.sequence > metadata.journal_high_watermark
        || from_exclusive.sequence.saturating_add(1) < metadata.journal_retention_floor
        || target_view.continuation.era != metadata.event_id_era
        || target_view.continuation.sequence != metadata.journal_high_watermark
        || target_view.continuation.stream_id != expected_stream_id
    {
        return Err(
            "HistoryUnavailable: formal Changes cursor is outside retained verified history".into(),
        );
    }
    let target_commit = target_view.version.commit.clone();
    if target_view.continuation.stream_id.trim().is_empty()
        || target_view.continuation.scope_id != from_exclusive.scope_id
        || target_view.continuation.commit != target_commit
    {
        return Err("formal Changes target continuation mismatch".into());
    }
    let mut visible = target_view
        .view
        .events
        .iter()
        .filter(|event| event.id > from_exclusive.sequence)
        .map(|event| {
            Ok((
                event.id,
                serde_json::to_value(event).map_err(|error| error.to_string())?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    visible.sort_by_key(|(sequence, _)| *sequence);
    if visible.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        || visible
            .iter()
            .any(|(sequence, _)| *sequence > metadata.journal_high_watermark)
    {
        return Err("formal Changes projection has invalid event sequence".into());
    }
    let item_limit = max_items as usize;
    let more_visible = visible.len() > item_limit;
    let visible_count = visible.len();
    visible.truncate(item_limit);
    let mut page = FormalBusinessChangesPageV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        from_anchor,
        from_exclusive: from_exclusive.clone(),
        to_inclusive: from_exclusive.clone(),
        next_cursor: from_exclusive.clone(),
        target_request,
        target_view,
        changes: Vec::new(),
    };
    for (sequence, change) in visible {
        let cursor = FormalBusinessChangesCursorV1 {
            stream_id: page.target_view.continuation.stream_id.clone(),
            scope_id: from_exclusive.scope_id.clone(),
            event_id_era: metadata.event_id_era,
            sequence,
            commit: target_commit.clone(),
        };
        page.changes.push(FormalBusinessChangeV1 { cursor, change });
        page.to_inclusive = page.changes.last().unwrap().cursor.clone();
        page.next_cursor = page.to_inclusive.clone();
        if serde_json::to_vec(&page)
            .map_err(|error| error.to_string())?
            .len()
            > (max_bytes as usize).min(MAX_PAGE_BYTES)
        {
            page.changes.pop();
            if let Some(last) = page.changes.last() {
                page.to_inclusive = last.cursor.clone();
                page.next_cursor = page.to_inclusive.clone();
            } else {
                return Err("formal Changes response bound cannot hold one event".into());
            }
            break;
        }
    }
    let selected_count = if more_visible {
        item_limit
    } else {
        visible_count
    };
    if page.changes.len() == item_limit && more_visible {
        // Stop at the last delivered visible event so the next page cannot
        // skip a scoped event hidden behind a numerical event-ID gap.
    } else if page.changes.len() == selected_count {
        // No authorized event remains in this retained interval. Advancing to
        // the global watermark proves that the page examined the whole range.
        page.to_inclusive = FormalBusinessChangesCursorV1 {
            stream_id: page.target_view.continuation.stream_id.clone(),
            scope_id: from_exclusive.scope_id.clone(),
            event_id_era: metadata.event_id_era,
            sequence: metadata.journal_high_watermark,
            commit: target_commit,
        };
        page.next_cursor = page.to_inclusive.clone();
    }
    page.validate().map_err(|error| error.to_string())?;
    if serde_json::to_vec(&page)
        .map_err(|error| error.to_string())?
        .len()
        > (max_bytes as usize).min(MAX_PAGE_BYTES)
    {
        return Err("formal Changes response exceeds its byte limit".into());
    }
    Ok(page)
}

#[derive(Debug, Clone, Copy)]
struct ProjectionRangeMetadata {
    event_id_era: u64,
    journal_retention_floor: u64,
    journal_high_watermark: u64,
}

fn projection_metadata(
    projection: &WorldServiceProjection,
) -> Result<ProjectionRangeMetadata, String> {
    let header = projection
        .formal_commit_header
        .as_ref()
        .ok_or("formal Changes projection lacks retention metadata")?;
    Ok(ProjectionRangeMetadata {
        event_id_era: header.event_id_era,
        journal_retention_floor: header.journal_retention_floor,
        journal_high_watermark: header.journal_high_watermark,
    })
}

pub type FormalBusinessChangesCursorV1 =
    oasis7_client_api::world_service::FormalBusinessChangesCursorV1;
pub use oasis7_game_client::formal_business::{
    VerifiedFormalChangesCursorV1, VerifiedFormalChangesPageV1,
};
pub fn verify_formal_changes_page(
    verifier: &mut FormalReadVerifier,
    source: &VerifiedFormalChangesCursorV1,
    read_authority_digest: &str,
    page: &FormalBusinessChangesPageV1<WorldServiceProjection>,
    reader_public_key: &str,
) -> Result<VerifiedFormalChangesPageV1, String> {
    oasis7_game_client::formal_business::verify_formal_changes_page(
        verifier,
        source,
        read_authority_digest,
        page,
        reader_public_key,
    )
}
