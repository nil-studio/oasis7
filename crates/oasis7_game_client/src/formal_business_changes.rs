use super::*;
#[derive(Debug, Clone, Copy)]
struct ProjectionRangeMetadata {
    event_id_era: u64,
    journal_retention_floor: u64,
    journal_high_watermark: u64,
}

fn projection_metadata<P: crate::formal_read::FormalReadProjection>(
    projection: &P,
) -> Result<ProjectionRangeMetadata, String> {
    let projection = projection.read_metadata()?;
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

/// The new cursor has a dedicated era field: it is the committed journal ID
/// era, not a reorg generation or the legacy route's cursor era.
pub type FormalBusinessChangesCursorV1 =
    oasis7_client_api::world_service::FormalBusinessChangesCursorV1;

/// Private cursor minted only from a verified full audience projection.
#[derive(Debug, Clone)]
pub struct VerifiedFormalChangesCursorV1 {
    cursor: FormalBusinessChangesCursorV1,
    anchor: FormalReadAnchorRefV1,
    read_authority_digest: String,
    reader_public_key: String,
}
impl VerifiedFormalChangesCursorV1 {
    /// Mint a cursor from an ordinary view which already holds qualified proof
    /// and exact configured read-authority lineage.
    pub(crate) fn from_world_view<P: crate::formal_read::FormalReadProjection>(
        view: &crate::verified_world_view::VerifiedWorldView<P>,
        reader_public_key: &str,
        read_authority_digest: &str,
    ) -> Result<Self, String> {
        if !view.read_authority_matches(read_authority_digest) {
            return Err("formal Changes view belongs to another read authority".into());
        }
        let anchor = view
            .formal_read_anchor()
            .ok_or("formal Changes view lacks qualified anchor")?;
        let commit = view
            .formal_commit()
            .ok_or("formal Changes view lacks qualified commit")?;
        let metadata = metadata_from_projection::<P>(view.projection(), commit.record())?;
        if committed_reference::<P>(commit.record(), &metadata, view.projection())?
            != view.version().commit
        {
            return Err("formal Changes view aliases differ from qualified proof".into());
        }
        let cursor = FormalBusinessChangesCursorV1 {
            stream_id: view.continuation().stream_id.clone(),
            scope_id: view.version().visibility_scope.clone(),
            event_id_era: metadata.event_id_era,
            sequence: metadata.journal_high_watermark,
            commit: view.version().commit.clone(),
        };
        let expected_stream = authority::request_digest(
            "formal-events-v1",
            &(&cursor.commit.world, &cursor.scope_id),
        )?;
        if cursor.stream_id != expected_stream
            || view.continuation().sequence != cursor.sequence
            || view.continuation().era != cursor.event_id_era
            || view.continuation().commit != cursor.commit
        {
            return Err("formal Changes view continuation differs from committed journal".into());
        }
        Ok(Self {
            cursor,
            anchor: FormalReadAnchorRefV1 {
                position: commit.record().position,
                record_id: commit.head().record_id,
                active_authority_id: anchor
                    .authority()
                    .authority_id()
                    .map_err(|e| e.to_string())?,
            },
            read_authority_digest: read_authority_digest.into(),
            reader_public_key: reader_public_key.into(),
        })
    }

    pub(crate) fn anchor(&self) -> &FormalReadAnchorRefV1 {
        &self.anchor
    }
    pub fn from_verified_view<P: crate::formal_read::FormalReadProjection>(
        verified: &VerifiedFormalProjectionV1<P>,
        response: &ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        reader_public_key: &str,
        read_authority_digest: &str,
    ) -> Result<Self, String> {
        verified.matches(response, request)?;
        if response.formal_profile.as_ref() != Some(&FormalReadProfileV1::controlled())
            || response.execution_evidence_scope.is_some()
            || request.scope_id != response.version.visibility_scope
        {
            return Err("formal Changes cursor requires the exact verified read authority".into());
        }
        let record = verified.commit().record();
        let metadata = metadata_from_projection::<P>(&response.view, record)?;
        let verified_commit = committed_reference::<P>(record, &metadata, &response.view)?;
        if response.version.commit != verified_commit {
            return Err("formal Changes cursor CommitRef aliases differ from proof".into());
        }
        let cursor = FormalBusinessChangesCursorV1 {
            stream_id: response.continuation.stream_id.clone(),
            scope_id: request.scope_id.clone(),
            event_id_era: metadata.event_id_era,
            sequence: metadata.journal_high_watermark,
            commit: response.version.commit.clone(),
        };
        let anchor = FormalReadAnchorRefV1 {
            position: verified.commit().record().position,
            record_id: verified.commit().head().record_id,
            active_authority_id: verified
                .read_anchor()
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        };
        if anchor.position != cursor.commit.position
            || anchor.record_id != cursor.commit.execution_block_hash
            || read_authority_digest.trim().is_empty()
        {
            return Err("formal Changes cursor head/authority mismatch".into());
        }
        Ok(Self {
            cursor,
            anchor,
            read_authority_digest: read_authority_digest.into(),
            reader_public_key: reader_public_key.into(),
        })
    }
    pub fn cursor(&self) -> &FormalBusinessChangesCursorV1 {
        &self.cursor
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedFormalChangesPageV1 {
    changes: Vec<FormalBusinessChangeV1<Value>>,
    next: VerifiedFormalChangesCursorV1,
}
impl VerifiedFormalChangesPageV1 {
    pub fn changes(&self) -> &[FormalBusinessChangeV1<Value>] {
        &self.changes
    }
    pub fn next_cursor(&self) -> &VerifiedFormalChangesCursorV1 {
        &self.next
    }
}

/// The source cursor must be from the same exact read-authority identity,
/// caller key, scope, and current verified head. The current full projection
/// supplies range completeness; changes are independently compared against it.
pub fn verify_formal_changes_page<P: crate::formal_read::FormalReadProjection>(
    verifier: &mut FormalReadVerifier<P>,
    source: &VerifiedFormalChangesCursorV1,
    read_authority_digest: &str,
    page: &FormalBusinessChangesPageV1<P>,
    reader_public_key: &str,
) -> Result<VerifiedFormalChangesPageV1, String> {
    page.validate().map_err(|error| error.to_string())?;
    if source.read_authority_digest != read_authority_digest
        || source.reader_public_key != reader_public_key
        || page.from_anchor != source.anchor
        || page.from_exclusive != source.cursor
        || page.target_request.scope_id != source.cursor.scope_id
        || page.target_request.world != source.cursor.commit.world
        || verifier.anchor_ref()? != source.anchor
    {
        return Err(
            "HistoryUnavailable: formal Changes source cursor authority/audience/head mismatch"
                .into(),
        );
    }
    let mut next_verifier = verifier.clone();
    let verified =
        next_verifier.verify_view(&page.target_view, &page.target_request, reader_public_key)?;
    let metadata = projection_metadata::<P>(&page.target_view.view)?;
    let expected_stream_id = authority::request_digest(
        "formal-events-v1",
        &(&page.target_request.world, &page.target_request.scope_id),
    )?;
    if page.from_exclusive.event_id_era != metadata.event_id_era
        || page.from_exclusive.stream_id != expected_stream_id
        || page.target_view.continuation.era != metadata.event_id_era
        || page.target_view.continuation.sequence != metadata.journal_high_watermark
        || page.from_exclusive.sequence > metadata.journal_high_watermark
        || page.from_exclusive.sequence.saturating_add(1) < metadata.journal_retention_floor
        || page.to_inclusive.sequence > metadata.journal_high_watermark
        || page.to_inclusive.sequence < page.from_exclusive.sequence
        || page.to_inclusive.commit != page.target_view.version.commit
        || page.target_view.continuation.stream_id != page.from_exclusive.stream_id
        || page.target_view.continuation.scope_id != page.from_exclusive.scope_id
    {
        return Err(
            "HistoryUnavailable: formal Changes range is expired or outside the committed journal"
                .into(),
        );
    }
    let projection = page.target_view.view.read_metadata()?;
    let expected = projection
        .events
        .iter()
        .filter(|event| {
            event.get("id").and_then(Value::as_u64).is_some_and(|id| {
                id > page.from_exclusive.sequence && id <= page.to_inclusive.sequence
            })
        })
        .map(|event| {
            Ok((
                event
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or("formal event id")?,
                serde_json::to_value(event).map_err(|error| error.to_string())?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if expected.len() != page.changes.len()
        || expected
            .iter()
            .zip(&page.changes)
            .any(|((sequence, value), change)| {
                *sequence != change.cursor.sequence || value != &change.change
            })
    {
        return Err("formal Changes page omits, reorders, or alters an authorized event".into());
    }
    if page.to_inclusive.sequence != metadata.journal_high_watermark
        && page.changes.last().map(|change| change.cursor.sequence)
            != Some(page.to_inclusive.sequence)
    {
        return Err("formal Changes page stops between visible events".into());
    }
    let next = VerifiedFormalChangesCursorV1::from_verified_view::<P>(
        &verified,
        &page.target_view,
        &page.target_request,
        reader_public_key,
        read_authority_digest,
    )?;
    let mut next = next;
    next.cursor = page.next_cursor.clone();
    next.anchor = FormalReadAnchorRefV1 {
        position: verified.commit().record().position,
        record_id: verified.commit().head().record_id,
        active_authority_id: verified
            .read_anchor()
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?,
    };
    *verifier = next_verifier;
    Ok(VerifiedFormalChangesPageV1 {
        changes: page.changes.clone(),
        next,
    })
}
