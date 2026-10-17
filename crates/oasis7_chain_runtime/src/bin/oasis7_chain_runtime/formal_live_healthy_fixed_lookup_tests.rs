//! Exact H selectors come from verified fixture publications, never a service hint.
use super::*;
use oasis7_distfs::controlled_authority::{
    formal::verify_record_material, replicated_protocol::ArtifactRole,
};

pub(super) fn boundary_selector(
    case: &HealthySetCase,
    position: u64,
    client: &RemoteWorldServiceClient,
) -> Result<oasis7_client_api::world_service::CommitRef, String> {
    let mut anchor = case.initial_anchor.clone();
    if position <= anchor.head().record().position
        || position.saturating_sub(anchor.head().record().position)
            > oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS as u64
    {
        return Err("exact H fixture selector requires bounded forward qualified history".into());
    }
    let mut selected = None;
    let mut steps = Vec::new();
    for next in (anchor.head().record().position + 1)..=position {
        let (candidate, proof) =
            crate::execution_bridge::formal_writer::read_published_candidate_at(
                &case.writer_directory,
                next,
            )?
            .ok_or("exact H fixture publication missing")?;
        let commit = verify_formal_qualified(&proof, &case.authority, Some(anchor.head()), None)
            .map_err(|error| error.to_string())?;
        if candidate.candidate.record != proof.decision.record {
            return Err("exact H fixture candidate differs from qualified record".into());
        }
        verify_record_material(&proof.decision.record, &candidate.material)
            .map_err(|error| error.to_string())?;
        anchor = anchor
            .after_verified_commit(case.authority.clone(), commit)
            .map_err(|error| error.to_string())?;
        steps.push(oasis7::world_service::formal_read::FormalReadStepV1::Commit(Box::new(proof)));
        selected = Some(candidate);
    }
    // Replay actual signed proof pages through the client's original C0 pin.
    // Page hashes and end cursors are locators, never replacement trust.
    let start = client
        .formal_replay_cursor()
        .map_err(|error| error.to_string())?;
    let mut end = start.clone();
    end.anchor = oasis7_client_api::world_service::FormalReadAnchorRefV1 {
        position: anchor.head().record().position,
        record_id: anchor.head().head().record_id,
        active_authority_id: anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?,
    };
    end.checkpoint_digest = anchor
        .checkpoint()
        .digest()
        .map_err(|error| error.to_string())?;
    if start != end {
        let configured = client
            .config()
            .formal_read_trust
            .as_ref()
            .ok_or("exact H lookup needs original external C0 trust")?;
        let page = oasis7::world_service::formal_read::FormalReadHistoryPageV1 {
            schema_version: 1,
            start,
            history: oasis7::world_service::formal_read::FormalReadHistoryV1 {
                schema_version: 1,
                steps,
            },
            end: end.clone(),
        };
        let bytes = page.encode_bytes()?;
        let digest = blake3::hash(&bytes).to_hex().to_string();
        client
            .resume_formal_history_pages(&configured.checkpoint_digest, [Ok((digest, bytes))], &end)
            .map_err(|error| format!("verify exact C0-to-H lookup history: {error}"))?;
    }
    let candidate = selected.ok_or("exact H fixture candidate unavailable")?;
    let decode = |role: ArtifactRole| -> Result<&[u8], String> {
        let digest = candidate
            .material
            .roots
            .get(&role)
            .ok_or("exact H artifact role missing")?;
        let object = candidate
            .material
            .objects
            .iter()
            .find(|object| &object.content_hash == digest)
            .ok_or("exact H artifact bytes missing")?;
        Ok(&object.bytes)
    };
    let expanded_snapshot = candidate
        .material
        .snapshot_bytes()
        .map_err(|error| error.to_string())?;
    let snapshot: oasis7::runtime::Snapshot =
        serde_cbor::from_slice(&expanded_snapshot).map_err(|error| error.to_string())?;
    let journal: oasis7::runtime::Journal = serde_cbor::from_slice(decode(ArtifactRole::Journal)?)
        .map_err(|error| error.to_string())?;
    let world = oasis7::runtime::World::from_snapshot(snapshot, journal)
        .map_err(|error| format!("restore exact H selector World: {error:?}"))?;
    // This checks canonical Snapshot, identity, governing and projection roots
    // against the opaque qualified anchor before deriving a read selector.
    let publication =
        oasis7::world_service::formal_read_service::FormalReadPublication::new(anchor, world)?;
    let projection = fixtures::projection(publication.world());
    let fixed = fixtures::commit_ref(publication.anchor(), &projection);
    fixed.validate().map_err(|error| error.to_string())?;
    Ok(fixed)
}

pub(super) struct QualifiedLineage {
    pub predecessor: VerifiedFormalCommitV1,
    pub boundary: VerifiedFormalCommitV1,
    pub anchor: VerifiedFormalReadAnchorV1,
    pub steps: Vec<HealthyEndpointLineageStepV1>,
}

pub(super) fn qualified_lineage(
    case: &HealthySetCase,
    transition: &SignedNormalDurableSetTransitionV1,
    client: &RemoteWorldServiceClient,
    expected_proof: &FormalQualifiedCommitProofV1,
) -> Result<QualifiedLineage, String> {
    let position = transition.body.boundary_position;
    let mut anchor = case.initial_anchor.clone();
    if position <= anchor.head().record().position
        || position.saturating_sub(anchor.head().record().position)
            > oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS as u64
    {
        return Err("provision H lineage exceeds bounded original C0 history".into());
    }
    let mut predecessor = None;
    let mut steps = Vec::new();
    for next in (anchor.head().record().position + 1)..=position {
        let (candidate, proof) =
            crate::execution_bridge::formal_writer::read_published_candidate_at(
                &case.writer_directory,
                next,
            )?
            .ok_or("provision C0-to-H qualified publication missing")?;
        let commit = verify_formal_qualified(&proof, &case.authority, Some(anchor.head()), None)
            .map_err(|error| format!("verify provision lineage position {next}: {error}"))?;
        if candidate.candidate.record != proof.decision.record
            || (next == position && &proof != expected_proof)
        {
            return Err("provision lineage changed exact qualified H proof/material".into());
        }
        verify_record_material(&proof.decision.record, &candidate.material)
            .map_err(|error| format!("verify provision material position {next}: {error}"))?;
        predecessor = Some(anchor.head().clone());
        anchor = anchor
            .after_verified_commit(case.authority.clone(), commit)
            .map_err(|error| format!("advance provision lineage position {next}: {error}"))?;
        steps.push(HealthyEndpointLineageStepV1::Commit(Box::new(proof)));
    }
    let cached = client
        .formal_current_anchor()
        .map_err(|error| error.to_string())?;
    let predecessor = predecessor.ok_or("provision H predecessor missing")?;
    if cached.checkpoint() != anchor.checkpoint()
        || cached.head().record() != &expected_proof.decision.record
        || cached
            .authority()
            .context_digest()
            .map_err(|error| error.to_string())?
            != case
                .authority
                .context_digest()
                .map_err(|error| error.to_string())?
        || predecessor.head() != transition.body.predecessor
        || cached
            .head()
            .record()
            .durable_set_transition_digest
            .as_deref()
            != Some(
                transition
                    .body
                    .digest()
                    .map_err(|error| error.to_string())?
                    .as_str(),
            )
    {
        return Err(
            "provision H differs from original C0 client lineage or signed transition".into(),
        );
    }
    Ok(QualifiedLineage {
        predecessor,
        boundary: cached.head().clone(),
        anchor: cached,
        steps,
    })
}
