//! Resume the sole original through the shipping Runtime before sealing E.
//! The old writer cache is removed; only authenticated P/R originals refill it.
use super::*;
use crate::formal_live_service::test_control::Point;

pub(super) struct Inputs<'a> {
    pub config: &'a Value,
    pub old_writer: &'a Path,
    pub directory: &'a Path,
    pub primary: std::net::SocketAddr,
    pub replica: std::net::SocketAddr,
    pub custody: &'a super::super::writer_custody_fixture::WriterCustodyFixture,
    pub writer_revision: u64,
    pub owner: WorldServiceClientConfig,
    pub request: &'a SubmitIntentRequest<WorldServicePayloadV1>,
    pub block: &'a str,
    pub target: u64,
    pub initial_emits: usize,
    pub frozen: &'a FormalFrozenCertificateV1,
    pub candidate: Option<&'a SignedCanonicalCandidateV1>,
}

pub(super) struct DrainedOriginal {
    pub anchor: VerifiedFormalReadAnchorV1,
    pub reference: oasis7_client_api::world_service::CommitRef,
    pub effect: Vec<u8>,
}

pub(super) fn run(input: Inputs<'_>) -> Result<DrainedOriginal, String> {
    std::fs::remove_dir_all(input.old_writer).map_err(|error| error.to_string())?;
    if input.old_writer.exists() {
        return Err("R-only original writer cache survived the loss injection".into());
    }
    super::super::private_directory(input.directory)?;
    let new_writer = input.directory.join("writer");
    super::super::private_directory(&new_writer)?;
    let mut config = input.config.clone();
    config["schema_version"] = 2.into();
    config
        .as_object_mut()
        .ok_or("drain config object")?
        .remove("writer_key_file");
    config["writer_directory"] = serde_json::to_value(&new_writer).unwrap();
    config["writer_custody"] = input.custody.recovery_runtime_config(input.writer_revision);
    config["primary"] = serde_json::to_value(input.primary).unwrap();
    config["replica"] = serde_json::to_value(input.replica).unwrap();
    let config_path = input.directory.join("runtime.json");
    super::super::write_private(
        &config_path,
        &serde_json::to_vec(&config).map_err(|error| error.to_string())?,
    )?;
    boundary::reset_execution_attempts(input.block);
    let started = Instant::now();
    let scenario = ScenarioDeadline {
        started_at: started,
        expires_at: started + Duration::from_secs(120),
    };
    let mut runtime = FormalLiveRuntime::open_with_native_history(&config_path)?;
    let control = runtime.published.control_boundaries()?;
    control.arm(
        Point::Published,
        Some(input.target),
        None,
        scenario.expires_at,
    )?;
    runtime.start()?;
    wait_for_published_position(&runtime, input.target)?;
    let observed = control.wait(scenario.expires_at)?;
    if observed.point != Point::Published || observed.position != input.target {
        return Err("R-only drain did not publish the exact original old-epoch position".into());
    }
    let expected = usize::from(input.candidate.is_none());
    if boundary::execution_attempts() != expected {
        return Err(format!(
            "R-only old-epoch drain executed {} times, expected {expected}",
            boundary::execution_attempts(),
        ));
    }
    let owner = RemoteWorldServiceClient::new(input.owner).map_err(|error| error.to_string())?;
    let reference = lookup_original_commit(&owner, input.request)?;
    if reference.position != input.target {
        return Err("R-only drain moved the original result position".into());
    }
    let publication = runtime
        .history
        .fixed_publication(&reference)
        .map_err(|error| error.to_string())?;
    let (candidate, proof) = crate::execution_bridge_real_tests::real_execution_bridge::formal_writer::read_published_candidate_at(
        &new_writer, input.target,
    )?.ok_or("R-only drain actual published original missing")?;
    if proof.decision.branch != FormalCompletionBranchV1::Normal
        || candidate.branch != FormalCompletionBranchV1::Normal
        || candidate.frozen.as_ref() != Some(input.frozen)
        || proof.decision.record != *publication.anchor().head().record()
        || proof.decision.record.original_authority_id != input.frozen.reservation.body.authority_id
        || proof.decision.record.original_epoch != input.frozen.reservation.body.epoch
        || input.candidate.is_some_and(|original| {
            original != &candidate.candidate
                || proof.decision.writer_signature_hex != original.writer_signature_hex
        })
    {
        return Err("R-only drain changed original epoch, Frozen, candidate or signature".into());
    }
    if wasm_emit_count(publication.world()) != input.initial_emits + 1 {
        return Err(
            "R-only old-epoch drain did not preserve exactly one original Wasmtime effect".into(),
        );
    }
    let drained = DrainedOriginal {
        anchor: publication.anchor().clone(),
        reference,
        effect: original_effect(publication.world(), input.request)?,
    };
    super::super::super::crash_tests::stop_after_controlled_publication(
        &mut runtime,
        &control,
        scenario,
    )?;
    eprintln!(
        "CA-R1/R2 old_epoch_drain position={} original_writer_cache=absent actual_execution={} original_signature=preserved",
        input.target, expected,
    );
    Ok(drained)
}
