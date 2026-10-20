//! Signed service cut plus exact dual-endpoint proof for a running successor.
use super::super::super::{
    formal_live_config::ValidatedFormalLiveConfigV1, node_lifecycle::local_io as io,
};
use super::{FormalHandoffReadbackV1, recovery_permission_digest};
use oasis7::world_service::client::{
    RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort,
};
use oasis7_client_api::world_service::{
    ReadWorldViewRequest, WORLD_SERVICE_CONTRACT_VERSION, WorldIdentity,
};
use oasis7_distfs::controlled_authority::{
    formal::{
        FormalCompletionBranchV1, FormalRecordKindV1, VerifiedFormalCommitV1,
        VerifiedFormalReadAnchorV1, transport::RemoteFormalEndpoint, verify_formal_candidate,
        verify_formal_qualified, verify_record_material,
    },
    replicated_protocol::EndpointRole,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

const MAX_FORMAL_READ_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const SUCCESSOR_CUT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Read the successor's atomically published WorldService cut through the
/// ordinary externally pinned reader, then prove that exact cut independently
/// against both protected native endpoints. The live writer may advance while
/// either full native restore runs, so these restorations target the immutable
/// reader cut and separately verify that each signed current endpoint head is
/// a qualified descendant of it.
pub(super) fn verify_successor_published_cut(
    validated: &ValidatedFormalLiveConfigV1,
    expected_recovery_permission_digest: &str,
    predecessor_writer_directory: &Path,
) -> Result<FormalHandoffReadbackV1, String> {
    let recovery = validated
        .recovery
        .as_ref()
        .ok_or("FENCING_UNPROVEN: successor recovery permission disappeared")?;
    let authority = validated.active_authority().clone();
    let authority_id = authority
        .authority_id()
        .map_err(|error| error.to_string())?;
    let expected_world = WorldIdentity {
        world_id: authority.definition().world.world_id.clone(),
        genesis_digest: authority.definition().world.genesis_digest.clone(),
    };
    // Runtime readiness only means the owner loop has started. Recovery may
    // initially publish the already-qualified recovery cut, so keep polling
    // that verified baseline until an execution strictly beyond it is
    // published. The same configured read timeout bounds the whole poll, and
    // each request receives only the remaining budget.
    let (read_anchor, published) = read_successor_execution_after_cut(
        validated,
        &authority,
        &authority_id,
        &expected_world,
        recovery.cut().record().position,
    )?;
    let target = published.head();
    let record = published.record();

    // Reuse the same bounded, hash-verifying reader as runtime startup. Only
    // bytes from the already approved initial closure or an exact predecessor
    // publication bound by the externally pinned InitialActivation anchor may
    // seed its old root. The typed ordinary-reader anchor below still selects
    // the exact recovery/Execution cut; local material cannot mint or advance
    // it.
    let business_reader = super::super::super::formal_live_runtime::business_reader_for(validated)?;
    if let Some(initial) = validated.approved_initial.as_ref() {
        business_reader.seed_verified_writes(
            super::super::super::execution_bridge::formal_writer::initial_business_nodes(
                &initial.candidate,
            )?,
        )?;
    } else if validated.anchor.head().record().kind == FormalRecordKindV1::InitialActivation {
        let initial_position = validated.anchor.head().record().position;
        if let Some((candidate, proof)) =
            super::super::super::execution_bridge::formal_writer::read_published_candidate_at(
                predecessor_writer_directory,
                initial_position,
            )?
        {
            let writes =
                initial_business_nodes_from_verified_anchor(&validated.anchor, &candidate, &proof)?;
            business_reader.seed_verified_writes(writes)?;
        }
    }
    let history = super::super::super::formal_live_history::from_validated(
        validated,
        business_reader.clone(),
    )?;
    history
        .prewarm_qualified_business_history(&read_anchor, business_reader.as_ref())
        .map_err(|error| format!("successor verified business-history prewarm: {error}"))?;

    let minimum = validated.anchor.head().head().clone();
    let endpoint = |address, role| {
        RemoteFormalEndpoint::new(
            address,
            validated.timeout,
            authority.clone(),
            role,
            validated.writer_key.clone(),
            minimum.clone(),
        )
        .map_err(|error| format!("successor formal {role:?} endpoint: {error}"))
    };
    let primary = endpoint(validated.config.primary, EndpointRole::Primary)?
        .restore_native_at_cached(
            &target,
            &authority,
            recovery.original_authority(),
            Some(recovery.recovery()),
            business_reader.as_ref(),
        )
        .map_err(|error| format!("successor primary exact-cut restore: {error}"))?;
    let replica = endpoint(validated.config.replica, EndpointRole::Replica)?
        .restore_native_at_cached(
            &target,
            &authority,
            recovery.original_authority(),
            Some(recovery.recovery()),
            business_reader.as_ref(),
        )
        .map_err(|error| format!("successor replica exact-cut restore: {error}"))?;

    let primary_current = primary.current_head();
    let replica_current = replica.current_head();
    let primary_checkpoint = primary.checkpoint();
    let replica_checkpoint = replica.checkpoint();
    if primary
        .authority()
        .authority_id()
        .map_err(|error| error.to_string())?
        != authority_id
        || replica
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?
            != authority_id
        || primary.minimum() != &minimum
        || replica.minimum() != &minimum
        || primary.reference() != target
        || replica.reference() != target
        || primary.commit().record() != record
        || replica.commit().record() != record
        || primary.commit().finalizing_authority_id() != published.finalizing_authority_id()
        || replica.commit().finalizing_authority_id() != published.finalizing_authority_id()
        || primary.candidate() != replica.candidate()
        || primary.proof() != replica.proof()
        || primary_current.position < target.position
        || replica_current.position < target.position
        || !primary_checkpoint.head.qualified
        || !replica_checkpoint.head.qualified
        || primary_checkpoint.head.position != primary_current.position
        || replica_checkpoint.head.position != replica_current.position
        || primary_checkpoint.head.decision_hash != primary_current.record_id
        || replica_checkpoint.head.decision_hash != replica_current.record_id
        || primary_current.position < recovery.cut().record().position
        || replica_current.position < recovery.cut().record().position
    {
        return Err("FENCING_UNPROVEN: signed reader cut is not independently present beneath qualified successor endpoint heads".into());
    }

    let primary_checkpoint_digest = io::digest(primary_checkpoint)?;
    let replica_checkpoint_digest = io::digest(replica_checkpoint)?;
    let qualified_evidence_digest = io::digest(&(
        record,
        primary.candidate(),
        primary.proof(),
        published.finalizing_authority_id(),
        &target,
        primary_current,
        replica_current,
        &primary_checkpoint_digest,
        &replica_checkpoint_digest,
    ))?;
    let recovery_permission_digest = recovery_permission_digest(validated)?
        .ok_or("FENCING_UNPROVEN: successor recovery permission is absent")?;
    if recovery_permission_digest != expected_recovery_permission_digest {
        return Err(
            "FENCING_UNPROVEN: successor recovery permission differs from admission".into(),
        );
    }
    Ok(FormalHandoffReadbackV1 {
        active_authority_id: authority_id,
        active_authority_epoch: authority.definition().epoch,
        profile_version: authority.definition().profile_version,
        qualified_position: target.position,
        qualified_record_id: target.record_id,
        after_state_root: record.after_state_root.clone(),
        primary_checkpoint_digest,
        replica_checkpoint_digest,
        qualified_evidence_digest,
        recovery_permission_digest,
    })
}

fn read_successor_execution_after_cut(
    validated: &ValidatedFormalLiveConfigV1,
    authority: &oasis7_distfs::controlled_authority::formal::VerifiedAuthorityV1,
    authority_id: &str,
    expected_world: &WorldIdentity,
    recovery_cut_position: u64,
) -> Result<(VerifiedFormalReadAnchorV1, VerifiedFormalCommitV1), String> {
    let mut selected_anchor = None;
    let published = poll_until_successor_execution(validated.timeout, |remaining| {
        let reader = RemoteWorldServiceClient::new(WorldServiceClientConfig {
            formal_read_trust: Some(validated.trust.clone()),
            endpoint: format!("http://{}", validated.config.status_bind),
            trusted_service_public_key: validated.node_keypair.public_key_hex.clone(),
            expected_world: expected_world.clone(),
            scope_id: "public".into(),
            read_private_key_hex: validated.node_keypair.private_key_hex.clone(),
            timeout: remaining.min(validated.timeout),
            max_response_bytes: MAX_FORMAL_READ_RESPONSE_BYTES,
        })
        .map_err(|error| format!("successor signed formal reader: {error}"))?;
        let view = reader
            .read_view(ReadWorldViewRequest {
                observation_nonce: None,
                formal_anchor: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: expected_world.clone(),
                scope_id: "public".into(),
                // The recovery cut is checked against the returned private
                // typed commit below. It cannot be represented as a complete
                // service CommitRef without a projection runtime binding.
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            })
            .map_err(|error| format!("successor signed formal read: {error}"))?;
        let published = view
            .formal_commit()
            .ok_or("FENCING_UNPROVEN: successor reader returned no verified formal commit")?
            .clone();
        let target = published.head();
        let record = published.record();
        let read_anchor = view
            .formal_read_anchor()
            .ok_or("FENCING_UNPROVEN: successor reader returned no verified read anchor")?;
        let record_id = record.record_id().map_err(|error| error.to_string())?;

        // Only a structurally valid, signed cut under the active successor
        // authority can be treated as a lagging publication. Any malformed
        // fields or wrong authority remain immediate fencing failures.
        if read_anchor.head().record() != record
            || read_anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?
                != authority_id
            || record.world != authority.definition().world
            || record.after_state_root.is_empty()
            || target.position != record.position
            || target.record_id != record_id
            || view.version().commit.position != record.position
            || view.version().commit.execution_block_hash != record_id
            || view.version().commit.state_root_ref != record.after_state_root
        {
            return Err(
                "FENCING_UNPROVEN: ordinary signed reader cut is malformed or uses the wrong active authority".into(),
            );
        }

        if record.position <= recovery_cut_position {
            return Ok(None);
        }
        validate_successor_published_commit(&published, authority, recovery_cut_position)?;
        selected_anchor = Some(read_anchor.clone());
        Ok(Some(published))
    })?;
    let read_anchor = selected_anchor
        .ok_or("FENCING_UNPROVEN: selected successor execution lost its verified read anchor")?;
    Ok((read_anchor, published))
}

/// Validate local initial-closure bytes against the exact externally pinned
/// initial anchor before adding their content-addressed nodes to the shared
/// cache. Local files provide bytes only; the verified anchor and qualified
/// proof provide the authorization boundary.
pub(super) fn initial_business_nodes_from_verified_anchor(
    anchor: &VerifiedFormalReadAnchorV1,
    candidate: &oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1,
    proof: &oasis7_distfs::controlled_authority::formal::FormalQualifiedCommitProofV1,
) -> Result<
    Vec<oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1>,
    String,
> {
    let record = anchor.head().record();
    if record.kind != FormalRecordKindV1::InitialActivation
        || record.position != 1
        || candidate.candidate.record != *record
        || candidate.branch != FormalCompletionBranchV1::Normal
        || candidate.frozen.is_some()
        || candidate.frozen_material.is_some()
        || proof.decision.record != candidate.candidate.record
        || proof.decision.writer_signature_hex != candidate.candidate.writer_signature_hex
        || proof.decision.branch != FormalCompletionBranchV1::Normal
        || proof.decision.frozen.is_some()
    {
        return Err(
            "FENCING_UNPROVEN: predecessor initial publication differs from the exact external anchor".into(),
        );
    }
    verify_formal_candidate(
        &candidate.candidate,
        None,
        anchor.authority(),
        None,
        None,
        FormalCompletionBranchV1::Normal,
    )
    .map_err(|error| format!("FENCING_UNPROVEN: predecessor initial candidate: {error}"))?;
    let verified = verify_formal_qualified(proof, anchor.authority(), None, None)
        .map_err(|error| format!("FENCING_UNPROVEN: predecessor initial proof: {error}"))?;
    if verified.record() != record || verified.head() != anchor.head().head() {
        return Err(
            "FENCING_UNPROVEN: predecessor initial proof differs from the exact external anchor"
                .into(),
        );
    }
    verify_record_material(&candidate.candidate.record, &candidate.material)
        .map_err(|error| format!("FENCING_UNPROVEN: predecessor initial closure: {error}"))?;
    super::super::super::execution_bridge::formal_writer::initial_business_nodes(candidate)
}

pub(super) fn poll_until_successor_execution(
    timeout: Duration,
    mut read: impl FnMut(Duration) -> Result<Option<VerifiedFormalCommitV1>, String>,
) -> Result<VerifiedFormalCommitV1, String> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or("successor readback deadline is out of range")?;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: no post-recovery successor execution was published before the signed readback deadline; query the same handoff operation".into());
        }
        if let Some(published) = read(remaining)? {
            return Ok(published);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: no post-recovery successor execution was published before the signed readback deadline; query the same handoff operation".into());
        }
        std::thread::sleep(SUCCESSOR_CUT_POLL_INTERVAL.min(remaining));
    }
}

pub(super) fn validate_successor_published_commit(
    published: &VerifiedFormalCommitV1,
    authority: &oasis7_distfs::controlled_authority::formal::VerifiedAuthorityV1,
    recovery_cut_position: u64,
) -> Result<(), String> {
    let record = published.record();
    let authority_id = authority
        .authority_id()
        .map_err(|error| error.to_string())?;
    if record.kind != FormalRecordKindV1::Execution
        || record.world != authority.definition().world
        || record.original_authority_id != authority_id
        || record.original_epoch != authority.definition().epoch
        || published.finalizing_authority_id() != authority_id
        || record.position <= recovery_cut_position
        || record.after_state_root.is_empty()
        || published.head().record_id != record.record_id().map_err(|error| error.to_string())?
    {
        return Err(
            "FENCING_UNPROVEN: ordinary signed reader cut is not a qualified successor execution after the recovery cut".into(),
        );
    }
    Ok(())
}
