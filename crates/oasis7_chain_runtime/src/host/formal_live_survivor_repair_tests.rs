//! The lost P directory is rebuilt by its original protected provider. R is
//! only a source of immutable objects; the sole original drains in E before
//! either writer or endpoint sealing permits successor activation.
use super::*;
use crate::formal_live_planned_recovery_support::{
    CompletedRecovery, PlanInputs, PreparedRecovery, recovery_copy_budget,
};
use oasis7_distfs::controlled_authority::{
    fencing::{
        MonotonicEndpointProvider, ProtectedRecoveryRepairRequestV1,
        RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1,
    },
    formal::{
        endpoint::SignedFormalArchiveTransferV1,
        writer_signing::{FormalWriterSignerV1, WriterSigningCapabilityV1},
    },
    replicated_protocol::ClosedRecord,
};

pub(super) struct Inputs<'a> {
    pub root: &'a Path,
    pub config: &'a Value,
    pub resources: &'a mut SurvivorResources,
    pub replica: &'a mut RemoteFormalEndpoint,
    pub original_primary_checkpoint: &'a [u8],
    pub predecessor: &'a VerifiedFormalReadAnchorV1,
    pub successor: AuthorityDefinitionV1,
    pub fixed: &'a RecoveryTailWitnessV1,
    pub fixed_material: &'a ClosedRecord,
    pub frozen: &'a FormalFrozenCertificateV1,
    pub candidate: Option<&'a SignedCanonicalCandidateV1>,
    pub old_writer: &'a Path,
    pub block: &'a str,
    pub target: u64,
    pub initial_emits: usize,
    pub owner: WorldServiceClientConfig,
    pub request: &'a SubmitIntentRequest<WorldServicePayloadV1>,
}

pub(super) struct RecoveredOriginal {
    pub completed: CompletedRecovery,
    pub drained: draining_tests::DrainedOriginal,
    pub archive: SignedFormalArchiveTransferV1,
    pub origin_pages: PathBuf,
}

pub(super) fn recover(input: Inputs<'_>) -> Result<RecoveredOriginal, String> {
    let resources = input.resources;
    if resources.pdir.exists() || input.original_primary_checkpoint.is_empty() {
        return Err("R-only repair requires the actual complete original P directory loss".into());
    }
    let old_primary = Store::verify_public(
        input.original_primary_checkpoint,
        &resources
            .authority
            .definition()
            .primary
            .instance
            .endpoint_public_key,
    )
    .map_err(|_| "original protected P acceptance checkpoint signature")?;
    let pending = RecoveryPendingHandoffV1::empty();
    let source_checkpoint = std::fs::read(resources.rdir.join("formal-checkpoint-v1.cbor"))
        .map_err(|error| error.to_string())?;
    let source_checkpoint_file = input.root.join("survivor-original-r-checkpoint.cbor");
    super::super::write_private(&source_checkpoint_file, &source_checkpoint)?;
    let issuer_directory = input.root.join("survivor-original-plan");
    let prepared = PreparedRecovery::prepare(
        PlanInputs {
            directory: &issuer_directory,
            original: input.predecessor,
            trust: &resources.trust,
            operation_id: fixtures::h(240),
            successor: input.successor,
            fixed: input.fixed,
            fixed_material: Some(input.fixed_material),
            pending: &pending,
            issuer_key: &fixtures::key(1),
            witness_key: &fixtures::key(2),
            minimum: IssuerAnchorV1::initial(resources.authority.policy())
                .map_err(|error| error.to_string())?,
            authorization: RecoveryPlanAuthorityV1 {
                recovery_credential_public_key: resources
                    .writer_custody
                    .recovery_credential_public_key(),
                isolation_evidence_digest: fixtures::h(241),
            },
        },
        &resources.writer_custody.recovery,
    )?;
    if resources
        .writer_custody
        .normal
        .sign_frozen(&input.frozen.reservation.body)
        .is_ok()
    {
        return Err("old normal writer credential still signed after the actual plan CAS".into());
    }
    prepared.install_on(input.replica)?;
    let mut old_replica = resources
        .replica_child
        .take()
        .ok_or("original R child missing")?;
    old_replica.kill_sigkill_and_wait()?;
    drop(old_replica);
    let recovery_auth = input.root.join("survivor-recovery-oracle-auth");
    super::super::write_private(
        &recovery_auth,
        hex::encode(fixtures::key(25).to_bytes()).as_bytes(),
    )?;
    if resources.writer_custody.recovery_credential_public_key() != fixtures::public(25) {
        return Err("survivor fixture recovery credentials drifted".into());
    }
    let mut replica_config: Value = serde_json::from_slice(
        &std::fs::read(&resources.replica_config).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    replica_config["oracle_authentication_key_file"] =
        serde_json::to_value(&recovery_auth).unwrap();
    let replica_config_file = input.root.join("survivor-replica-draining.json");
    super::super::write_private(
        &replica_config_file,
        &serde_json::to_vec(&replica_config).map_err(|error| error.to_string())?,
    )?;
    let (replica_child, mut replica_address) =
        super::super::start_endpoint(&resources.endpoint_binary, &replica_config_file)?;
    resources.replica_child = Some(replica_child);
    resources.replica_address = replica_address;
    *input.replica = RemoteFormalEndpoint::new(
        replica_address,
        Duration::from_secs(30),
        resources.authority.clone(),
        EndpointRole::Replica,
        WriterSigningCapabilityV1::new(resources.writer_custody.recovery.clone()),
        input.predecessor.head().head(),
    )
    .map_err(|error| error.to_string())?;

    let replacement = input.root.join("survivor-original-p-repaired");
    super::super::private_directory(&replacement)?;
    let pending_file = input.root.join("survivor-original-pending.cbor");
    super::super::write_private(
        &pending_file,
        &serde_cbor::to_vec(&pending).map_err(|error| error.to_string())?,
    )?;
    let pending_size = serde_cbor::to_vec(
        input
            .replica
            .restore_native_current(&resources.authority, None, None)
            .map_err(|error| error.to_string())?
            .pending()
            .ok_or("original R lost fixed pending")?,
    )
    .map_err(|error| error.to_string())?
    .len() as u64;
    // The source inventory plus one same-size P entry and the bounded index
    // path covers role-specific receipt reconstruction without an implicit quota.
    let copy_max_bytes = recovery_copy_budget(&resources.rdir)?
        .checked_add(pending_size)
        .ok_or("original repair copy budget overflow")?;
    let repair_config_file = input.root.join("survivor-original-p-repair.json");
    let repair_config = serde_json::json!({
        "schema_version": 2,
        "initial_trust": resources.trust,
        "initial_certificate": resources.certificate,
        "initial_qualified": null,
        "role": EndpointRole::Primary,
        "data_dir": replacement,
        "oracle_address": resources.primary_oracle_address,
        "oracle_authentication_key_file": recovery_auth,
        "listen": "127.0.0.1:0", "timeout_ms": 30000,
        "recovery_repair_bootstrap": {
            "plan": prepared.plan, "fixed": input.fixed, "writer_ack": prepared.writer_ack,
            "pending_handoff_file": pending_file,
            "survivor_checkpoint_file": source_checkpoint_file,
            "survivor_archive_directory": resources.rdir,
            "minimum_revision": old_primary.revision,
            "copy_max_bytes": copy_max_bytes,
        },
    });
    super::super::write_private(
        &repair_config_file,
        &serde_json::to_vec(&repair_config).map_err(|error| error.to_string())?,
    )?;
    let (mut primary_child, mut primary_address) =
        super::super::start_endpoint(&resources.endpoint_binary, &repair_config_file)?;
    let provider = RemoteMonotonicEndpointProvider::new(
        resources.primary_oracle_address,
        Duration::from_secs(30),
        fixtures::key(25),
        TrustedFencingPolicyV1 {
            provider_public_key: resources
                .authority
                .policy()
                .fencing_provider_public_key
                .clone(),
            authorized_resource_bindings: resources
                .authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        },
    )
    .map_err(|error| error.to_string())?;
    let grant_request = ProtectedRecoveryRepairRequestV1 {
        schema_version: 1,
        plan: prepared.plan.clone(),
        fixed: input.fixed.clone(),
        directory_fingerprint: directory_fingerprint(&replacement)
            .map_err(|error| error.to_string())?,
        minimum_revision: old_primary.revision,
    };
    {
        // This actual authenticated provider reply, after the shipping repair,
        // proves the backup was an oracle only and never initialized new P state.
        let guard = provider
            .repair_protected(
                &resources.authority.definition().primary.instance,
                grant_request,
            )
            .map_err(|error| error.to_string())?;
        let grant = guard
            .recovery_repair_grant()
            .ok_or("repaired provider omitted original grant")?;
        if grant.original_native_checkpoint != input.original_primary_checkpoint
            || grant.original_checkpoint
                != Store::binding(&old_primary, input.original_primary_checkpoint)
            || guard.checkpoint().revision < old_primary.revision
        {
            return Err("repair replaced original P protected bytes or lowered its floor".into());
        }
    }
    let mut primary = RemoteFormalEndpoint::new(
        primary_address,
        Duration::from_secs(30),
        resources.authority.clone(),
        EndpointRole::Primary,
        WriterSigningCapabilityV1::new(resources.writer_custody.recovery.clone()),
        input.predecessor.head().head(),
    )
    .map_err(|error| error.to_string())?;
    prepared.install_on(&mut primary)?;
    let primary_original = primary
        .restore_native_current(&resources.authority, None, None)
        .map_err(|error| error.to_string())?;
    if primary_original.commit().record() != input.predecessor.head().record()
        || primary_original
            .active_authority()
            .context_digest()
            .map_err(|error| error.to_string())?
            != resources
                .authority
                .context_digest()
                .map_err(|error| error.to_string())?
        || resources.pdir.exists()
    {
        return Err("repair changed original P authority/parent or recreated the deleted original directory".into());
    }
    // A completed repair must cold-open using its registered local closure.
    // Keep the exact old input files, but make the surviving archive genuinely
    // unavailable while both original endpoint processes are stopped.
    let repaired_native = std::fs::read(replacement.join("formal-checkpoint-v1.cbor"))
        .map_err(|error| error.to_string())?;
    let original_replica_fingerprint =
        directory_fingerprint(&resources.rdir).map_err(|error| error.to_string())?;
    primary_child.kill_sigkill_and_wait()?;
    let mut warm_replica = resources
        .replica_child
        .take()
        .ok_or("repair R child missing")?;
    warm_replica.kill_sigkill_and_wait()?;
    drop(warm_replica);
    let offline_archive = input.root.join("survivor-original-r-offline");
    std::fs::rename(&resources.rdir, &offline_archive).map_err(|error| error.to_string())?;
    if resources.rdir.exists() {
        return Err("survivor archive was not actually offline for repaired P cold open".into());
    }
    let (cold_primary, cold_address) =
        super::super::start_endpoint(&resources.endpoint_binary, &repair_config_file)?;
    primary_child = cold_primary;
    primary_address = cold_address;
    primary = RemoteFormalEndpoint::new(
        primary_address,
        Duration::from_secs(30),
        resources.authority.clone(),
        EndpointRole::Primary,
        WriterSigningCapabilityV1::new(resources.writer_custody.recovery.clone()),
        input.predecessor.head().head(),
    )
    .map_err(|error| error.to_string())?;
    let cold_original = primary
        .restore_native_current(&resources.authority, None, None)
        .map_err(|error| {
            format!("repaired P cold native restore with R archive offline: {error}")
        })?;
    if std::fs::read(replacement.join("formal-checkpoint-v1.cbor"))
        .map_err(|error| error.to_string())?
        != repaired_native
        || cold_original.commit().record() != primary_original.commit().record()
        || cold_original.pending() != primary_original.pending()
        || cold_original.checkpoint() != primary_original.checkpoint()
    {
        return Err(
            "repaired P cold open changed original native/fixed state without its source".into(),
        );
    }
    std::fs::rename(&offline_archive, &resources.rdir).map_err(|error| error.to_string())?;
    if directory_fingerprint(&resources.rdir).map_err(|error| error.to_string())?
        != original_replica_fingerprint
    {
        return Err("returning the survivor archive changed its original storage instance".into());
    }
    let (cold_replica, cold_address) =
        super::super::start_endpoint(&resources.endpoint_binary, &replica_config_file)?;
    resources.replica_child = Some(cold_replica);
    replica_address = cold_address;
    resources.replica_address = replica_address;
    *input.replica = RemoteFormalEndpoint::new(
        replica_address,
        Duration::from_secs(30),
        resources.authority.clone(),
        EndpointRole::Replica,
        WriterSigningCapabilityV1::new(resources.writer_custody.recovery.clone()),
        input.predecessor.head().head(),
    )
    .map_err(|error| error.to_string())?;
    eprintln!(
        "CA-R2 repaired_original_cold source_archive=offline native_bytes=unchanged phase=passed"
    );
    let drained = draining_tests::run(draining_tests::Inputs {
        config: input.config,
        old_writer: input.old_writer,
        directory: &input.root.join("survivor-original-drain"),
        primary: primary_address,
        replica: replica_address,
        custody: &resources.writer_custody,
        writer_revision: prepared.writer_ack.body.revision,
        owner: input.owner,
        request: input.request,
        block: input.block,
        target: input.target,
        initial_emits: input.initial_emits,
        frozen: input.frozen,
        candidate: input.candidate,
    })?;
    let origin_pages = input.root.join("survivor-drained-origin-history");
    super::super::private_directory(&origin_pages)?;
    let original = write_origin_pages(
        input.replica,
        &resources.trust,
        &resources.anchor,
        input.target,
        &origin_pages,
    )?;
    if original.head().record() != drained.anchor.head().record() {
        return Err(
            "independent original lineage differs from actual old-epoch drained result".into(),
        );
    }
    let completed = prepared.finish(
        &drained.anchor,
        &resources.writer_custody.recovery,
        &mut primary,
        input.replica,
    )?;
    let archive = input
        .replica
        .archive_manifest()
        .map_err(|error| error.to_string())?;
    if completed.drained.qualified.decision.record != *drained.anchor.head().record()
        || completed.verified.recovery().body().tail != RecoveryTailV1::Empty
        || completed.verified.recovery().body().first_normal_position != input.target.checked_add(2)
    {
        return Err(
            "drained schema3 permission did not preserve sole original and control positions"
                .into(),
        );
    }
    primary_child.kill_sigkill_and_wait()?;
    let mut replica_child = resources
        .replica_child
        .take()
        .ok_or("draining R child missing")?;
    replica_child.kill_sigkill_and_wait()?;
    drop((primary_child, replica_child));
    eprintln!(
        "CA-R1/R2 original_provider_repair original_directory=absent original_floor={} old_epoch_drained={} phase=passed",
        old_primary.revision, input.target
    );
    Ok(RecoveredOriginal {
        completed,
        drained,
        archive,
        origin_pages,
    })
}
