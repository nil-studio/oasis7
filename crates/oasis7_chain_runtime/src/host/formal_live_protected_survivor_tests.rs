//! Actual Runtime/Wasmtime R-only recovery. Oracle/issuer custody is simulated.
use super::super::*;
use super::{CheckpointParent, HotBody, Store};
use crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary as boundary;
use oasis7_distfs::controlled_authority::formal::{
    coordinator::FormalDurabilityEndpoint,
    endpoint::{
        FileFormalArchiveSourceV1, FormalQueryOutcomeV1, FormalRequestStateV1,
        verify_formal_archive_transfer,
    },
    issuer::IssuerAnchorV1,
    writer_signing::WriterSigningCapabilityV1,
};
use std::os::unix::process::ExitStatusExt;
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
};

#[path = "formal_live_survivor_draining_tests.rs"]
mod draining_tests;
#[path = "formal_live_survivor_participant_tests.rs"]
pub(crate) mod governed_participant;
#[path = "formal_live_survivor_next_request_tests.rs"]
mod next_request_tests;
#[path = "formal_live_survivor_repair_tests.rs"]
mod repair_tests;
#[path = "formal_live_protected_revalidation_tests.rs"]
pub(crate) mod revalidation_tests;

pub(in super::super) struct SurvivorResources {
    pub governed_participant: bool,
    pub revalidation: Option<revalidation_tests::RevalidationCase>,
    pub writer_custody: super::writer_custody_fixture::WriterCustodyFixture,
    pub primary_child: Option<super::EndpointChild>,
    pub replica_child: Option<super::EndpointChild>,
    pub pdir: PathBuf,
    pub rdir: PathBuf,
    pub primary_oracle_address: std::net::SocketAddr,
    pub primary_config: PathBuf,
    pub replica_config: PathBuf,
    pub replica_address: std::net::SocketAddr,
    pub authority: VerifiedAuthorityV1,
    pub trust: InitialActivationTrustV1,
    pub certificate: SignedInitialActivationV1,
    pub anchor: VerifiedFormalReadAnchorV1,
    pub endpoint_binary: std::ffi::OsString,
    pub oracle_auth: PathBuf,
}
struct RuntimeChild(Child);
impl Drop for RuntimeChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait_file(child: &mut RuntimeChild, file: &Path, expected: &[u8]) -> Result<(), String> {
    let until = Instant::now() + Duration::from_secs(90);
    while Instant::now() < until {
        if std::fs::read(file).ok().as_deref() == Some(expected) {
            return Ok(());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("survivor child exited {status}"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(format!("survivor marker timeout {}", file.display()))
}
#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; custody/fault domains simulated"]
fn protected_endpoint_children_replica_only_frozen_and_prepared_resume_without_original_private_key()
 {
    let mut failures = Vec::new();
    for phase in ["survivor_frozen", "survivor_prepared"] {
        // Exercise legacy denial, registration after recovery, and registration
        // as the frozen/prepared original independently, without short-circuiting.
        for scenario in [
            "legacy",
            "registration_after_recovery",
            "registration_original",
        ] {
            let outcome = thread::Builder::new()
                .stack_size(32 * 1024 * 1024)
                .spawn(move || {
                    if scenario != "legacy" {
                        governed_participant::run_recovery_scenario(
                            phase,
                            scenario == "registration_original",
                        )
                    } else {
                        run_protected_runtime_round_trip(true, Some(phase))
                    }
                })
                .expect("spawn survivor scenario")
                .join();
            eprintln!("survivor phase={phase}, scenario={scenario}: {outcome:?}");
            if !matches!(&outcome, Ok(Ok(()))) {
                failures.push(format!("phase={phase}, scenario={scenario}: {outcome:?}"));
            }
        }
    }
    if let Err(error) = revalidation_tests::run_paired_revalidation() {
        failures.push(format!("paired current/frozen revalidation: {error}"));
    }
    assert!(
        failures.is_empty(),
        "survivor scenarios failed: {failures:?}"
    );
}
#[allow(clippy::too_many_arguments)]
pub(in super::super) fn run(
    mode: &str,
    config: &Path,
    public: WorldServiceClientConfig,
    owner: WorldServiceClientConfig,
    world: &WorldIdentity,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    initial_emits: usize,
    mut resources: SurvivorResources,
) -> Result<(), String> {
    let root = config.parent().ok_or("survivor config parent")?;
    let phase = if mode == "survivor_frozen" {
        "input_frozen"
    } else {
        "replica_prepared"
    };
    let ready = root.join("survivor-child-ready");
    let marker = root.join("survivor-child-phase");
    let target_request = root.join("survivor-original-request.cbor");
    super::write_private(
        &target_request,
        &serde_cbor::to_vec(request).map_err(|e| e.to_string())?,
    )?;
    let mut child = RuntimeChild(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "protected_runtime_crash_child",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("OASIS7_FORMAL_TEST_CHILD_CONFIG", config)
            .env("OASIS7_FORMAL_TEST_CHILD_READY", &ready)
            .env("OASIS7_FORMAL_TEST_CRASH_PHASE", phase)
            .env("OASIS7_FORMAL_TEST_CRASH_REQUEST", &target_request)
            .env("OASIS7_FORMAL_TEST_CRASH_MARKER", &marker)
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    wait_file(&mut child, &ready, b"ready")?;
    let client = RemoteWorldServiceClient::new(public).map_err(|e| e.to_string())?;
    // Require a recovery cut beyond C0 instead of racing the first empty tick.
    let advanced_position = resources
        .anchor
        .head()
        .head()
        .position
        .checked_add(1)
        .ok_or("survivor initial position overflow")?;
    read_until_position(&client, world, advanced_position)?;
    let mut original_request = request.clone();
    if let Some(case) = &resources.revalidation {
        revalidation_tests::before_submit(case, &mut original_request)?;
        let replacement = target_request.with_extension("original.tmp");
        super::write_private(
            &replacement,
            &serde_cbor::to_vec(&original_request).map_err(|e| e.to_string())?,
        )?;
        std::fs::rename(&replacement, &target_request).map_err(|e| e.to_string())?;
        std::fs::File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| e.to_string())?;
    }
    let request = &original_request;
    let admitted = client.submit(request.clone()).map_err(|e| e.to_string())?;
    assert_volatile_admission(admitted, &request.correlation)?;
    wait_file(&mut child, &marker, phase.as_bytes())?;
    child.0.kill().map_err(|e| e.to_string())?;
    assert_eq!(child.0.wait().map_err(|e| e.to_string())?.signal(), Some(9));
    drop(child);
    if let Some(case) = &resources.revalidation {
        revalidation_tests::after_frozen(case, request)?;
    }
    let mut cfg: Value = serde_json::from_slice(&std::fs::read(config).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let old_writer = PathBuf::from(
        cfg["writer_directory"]
            .as_str()
            .ok_or("old writer directory")?,
    );
    let original_block = crate::execution_bridge_real_tests::real_execution_bridge::formal_writer::pending_test_block_hash(&old_writer.join("pending.cbor"))?;
    let target_position = crate::execution_bridge_real_tests::real_execution_bridge::formal_writer::pending_test_position(&old_writer.join("pending.cbor"))?;
    // Kept only as an acceptance oracle: the production repair must obtain
    // these exact signed bytes and their floor from the original provider.
    let original_primary_checkpoint =
        std::fs::read(resources.pdir.join("formal-checkpoint-v1.cbor"))
            .map_err(|error| error.to_string())?;
    Store::verify_public(
        &original_primary_checkpoint,
        &resources
            .authority
            .definition()
            .primary
            .instance
            .endpoint_public_key,
    )
    .map_err(|_| "original P checkpoint signature before directory loss")?;
    // Delete the necessary endpoint's counterpart, not merely its network route.
    drop(resources.primary_child.take());
    std::fs::remove_dir_all(&resources.pdir).map_err(|e| e.to_string())?;
    let mut replica = RemoteFormalEndpoint::new(
        resources.replica_address,
        Duration::from_secs(30),
        resources.authority.clone(),
        EndpointRole::Replica,
        WriterSigningCapabilityV1::new(resources.writer_custody.recovery.clone()),
        resources.anchor.head().head(),
    )
    .map_err(|e| e.to_string())?;
    let restored = replica
        .restore_native_current(&resources.authority, None, None)
        .map_err(|e| e.to_string())?;
    let origin_pages = root.join("survivor-origin-history");
    super::private_directory(&origin_pages)?;
    let predecessor = write_origin_pages(
        &mut replica,
        &resources.trust,
        &resources.anchor,
        restored.commit().record().position,
        &origin_pages,
    )?;
    if predecessor.head().head() != restored.commit().head() {
        return Err("survivor origin pages did not derive exact sealed cut".into());
    }
    let (tail, original_candidate, frozen_certificate, fixed_material) =
        match restored.pending().ok_or("survivor missing original slot")? {
            FormalRequestStateV1::Frozen {
                certificate: Some(cert),
                material,
                ..
            } if mode == "survivor_frozen" => (
                RecoveryTailWitnessV1::Frozen(cert.clone()),
                None,
                cert.clone(),
                material.clone(),
            ),
            FormalRequestStateV1::Prepared { candidate, .. } if mode == "survivor_prepared" => {
                let cert = candidate.frozen.clone().ok_or("prepared missing frozen")?;
                (
                    RecoveryTailWitnessV1::Prepared {
                        frozen: cert.clone(),
                        candidate: candidate.candidate.clone(),
                    },
                    Some(candidate.candidate.clone()),
                    cert,
                    candidate
                        .frozen_material
                        .clone()
                        .ok_or("prepared original frozen material")?,
                )
            }
            _ => return Err("survivor interrupted at wrong durable stage".into()),
        };
    if let Some(case) = &resources.revalidation {
        revalidation_tests::assert_original_manifest(
            case,
            &frozen_certificate
                .reservation
                .body
                .governing_manifest_digest,
        )?;
    }
    let p2 = root.join("survivor-primary-2");
    let r2 = root.join("survivor-replica-2");
    super::private_directory(&p2)?;
    super::private_directory(&r2)?;
    let mut definition = resources.authority.definition().clone();
    definition.epoch = 2;
    definition.writer_public_key = fixtures::public(10);
    definition.predecessor_authority_id = Some(
        resources
            .authority
            .authority_id()
            .map_err(|e| e.to_string())?,
    );
    definition.valid_from_position = target_position;
    for (binding, dir, n) in [
        (&mut definition.primary, &p2, 11u8),
        (&mut definition.replica, &r2, 12u8),
    ] {
        binding.instance.authority_epoch = 2;
        binding.instance.endpoint_id = format!("survivor-{n}");
        binding.instance.endpoint_public_key = fixtures::public(n);
        binding.instance.instance_id = fixtures::h(n + 20);
        binding.instance.storage_id = fixtures::h(n + 30);
        binding.instance.storage_fingerprint =
            directory_fingerprint(dir).map_err(|e| e.to_string())?;
        binding.fault_domain_id = format!("simulated-survivor-{n}");
    }
    let recovered = repair_tests::recover(repair_tests::Inputs {
        root,
        config: &cfg,
        resources: &mut resources,
        replica: &mut replica,
        original_primary_checkpoint: &original_primary_checkpoint,
        predecessor: &predecessor,
        successor: definition,
        fixed: &tail,
        fixed_material: &fixed_material,
        frozen: &frozen_certificate,
        candidate: original_candidate.as_ref(),
        old_writer: &old_writer,
        block: &original_block,
        target: target_position,
        initial_emits,
        owner: owner.clone(),
        request,
    })?;
    let permission = recovered.completed.permission;
    let verified = recovered.completed.verified;
    let archive = recovered.archive;
    let drained = recovered.drained;
    let origin_pages = recovered.origin_pages;
    drop(replica);
    let transfer = verify_formal_archive_transfer(
        &archive,
        &resources.authority,
        Arc::new(
            FileFormalArchiveSourceV1::new(resources.rdir.clone()).map_err(|e| e.to_string())?,
        ),
    )
    .map_err(|e| e.to_string())?;
    let old = Store::verify_public(
        &archive.checkpoint_bytes,
        &resources
            .authority
            .definition()
            .replica
            .instance
            .endpoint_public_key,
    )
    .map_err(|_| "survivor archived checkpoint")?;
    let hot = |role: EndpointRole| -> Result<HotBody, String> {
        let active = verified.recovery().successor();
        let instance = if role == EndpointRole::Primary {
            &active.definition().primary.instance
        } else {
            &active.definition().replica.instance
        };
        Ok(HotBody {
            version: 1,
            instance_digest: instance.digest().map_err(|e| e.to_string())?,
            authority_id: active.authority_id().map_err(|e| e.to_string())?,
            activation_ref: active.activation_ref().into(),
            epoch: 2,
            revision: 0,
            head: verified.cut().head(),
            head_node: None,
            imported_head_node: old.head_node.clone().or(old.imported_head_node.clone()),
            import_digest: Some(transfer.digest().map_err(|e| e.to_string())?),
            base_parent: Some(CheckpointParent {
                record: verified.cut().record().clone(),
                finalizing_authority_id: verified.cut().finalizing_authority_id().into(),
            }),
            index: old.index.clone(),
            pending: None,
            decision_head: None,
            seal: None,
            durable_set: None,
            data_closure: None,
            recovery_plan: None,
            recovery_handoff: old.recovery_handoff.clone(),
            writer_plan_ack: None,
            recovery_progress: None,
            recovery_drained: None,
            healthy_seal_authorization: None,
        })
    };
    let p_oracle = super::oracle::Oracle::start_registered(
        verified.recovery().successor().clone(),
        EndpointRole::Primary,
        fixtures::key(11),
        resources
            .authority
            .policy()
            .authorized_fencing_resource_bindings[0]
            .clone(),
        Some(hot(EndpointRole::Primary)?),
    );
    let r_oracle = super::oracle::Oracle::start_registered(
        verified.recovery().successor().clone(),
        EndpointRole::Replica,
        fixtures::key(12),
        resources
            .authority
            .policy()
            .authorized_fencing_resource_bindings[0]
            .clone(),
        Some(hot(EndpointRole::Replica)?),
    );
    let endpoint_config = |dir: &Path, role, address| -> Result<PathBuf, String> {
        let path = dir.join("endpoint.json");
        let value = serde_json::json!({"schema_version":2,"initial_trust":resources.trust,"initial_certificate":resources.certificate,"initial_qualified":null,
            "role":role,"data_dir":dir,"oracle_authentication_key_file":resources.oracle_auth,"oracle_address":address,"listen":"127.0.0.1:0","timeout_ms":30000,
            "recovery_bootstrap":{"predecessor_checkpoint":permission.predecessor_checkpoint,"predecessor_checkpoint_digest":permission.predecessor_checkpoint_digest,
            "certificate":permission.certificate,"fencing":permission.fencing,"tail":permission.tail,"issuer_witness":permission.issuer_witness,
            "minimum_issuer_anchor":permission.minimum_issuer_anchor,"archive_manifest":archive,"archive_directory":resources.rdir,
            "recovery_copy_max_bytes":crate::formal_live_planned_recovery_support::recovery_copy_budget(&resources.rdir)?}});
        super::write_private(
            &path,
            &serde_json::to_vec(&value).map_err(|e| e.to_string())?,
        )?;
        Ok(path)
    };
    let (_pchild, pa) = super::start_endpoint(
        &resources.endpoint_binary,
        &endpoint_config(&p2, EndpointRole::Primary, p_oracle.address)?,
    )?;
    let (_rchild, ra) = super::start_endpoint(
        &resources.endpoint_binary,
        &endpoint_config(&r2, EndpointRole::Replica, r_oracle.address)?,
    )?;
    // The Runtime receives only a fresh service capability. The old writer
    // cache was already removed before the real old-epoch drain.
    if old_writer.exists() {
        return Err("original writer cache reappeared during R-only recovery".into());
    }
    let successor_custody = super::writer_custody_fixture::WriterCustodyFixture::initialize(
        &root.join("survivor-successor-writer-custody"),
        &resources.endpoint_binary,
        super::writer_custody_fixture::Authorization {
            authority: verified.recovery().successor(),
            trust: &resources.trust,
            certificate: &resources.certificate,
            recovery: Some((&permission, &permission.minimum_issuer_anchor)),
        },
        &fixtures::key(10),
        [26, 27],
    )?;
    let writer = root.join("survivor-writer");
    super::private_directory(&writer)?;
    let pf = root.join("survivor-permission.json");
    super::write_private(
        &pf,
        &serde_json::to_vec(&permission).map_err(|e| e.to_string())?,
    )?;
    let ff = root.join("survivor-floor.json");
    let floor_file = serde_json::json!({"schema_version":1,"world":resources.authority.definition().world,
        "full_policy_digest":resources.authority.policy().digest().map_err(|e| e.to_string())?,
        "recovery_issuer_public_key":resources.authority.policy().recovery_issuer_public_key,
        "issuer_anchor_public_key":resources.authority.policy().issuer_anchor_public_key,"anchor":permission.minimum_issuer_anchor});
    super::write_private(
        &ff,
        &serde_json::to_vec(&floor_file).map_err(|e| e.to_string())?,
    )?;
    cfg["primary"] = serde_json::to_value(pa).unwrap();
    cfg["replica"] = serde_json::to_value(ra).unwrap();
    cfg["writer_directory"] = serde_json::to_value(writer).unwrap();
    cfg["schema_version"] = 2.into();
    cfg.as_object_mut()
        .ok_or("successor runtime config object")?
        .remove("writer_key_file");
    cfg["writer_custody"] = successor_custody.runtime_config();
    cfg["approved_initial_file"] = Value::Null;
    cfg["recovery_permission_file"] = serde_json::to_value(pf).unwrap();
    cfg["recovery_issuer_minimum_file"] = serde_json::to_value(ff).unwrap();
    cfg["recovery_origin_history_directory"] = serde_json::to_value(origin_pages).unwrap();
    let replacement = config.with_extension("successor-config.tmp");
    // Reuse the create-new, fsynced mode-0600 fixture writer for staging, then
    // atomically replace only the runtime configuration. C0 trust is untouched.
    super::write_private(
        &replacement,
        &serde_json::to_vec(&cfg).map_err(|e| e.to_string())?,
    )?;
    std::fs::rename(&replacement, config).map_err(|e| e.to_string())?;
    std::fs::File::open(root)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| e.to_string())?;
    let original_ref = Some(drained.reference.clone());
    let exact_effect = Some(drained.effect.clone());
    for _reopen in 0..2 {
        boundary::reset_execution_attempts(&original_block);
        let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
        runtime.start()?;
        wait_for_published_position(
            &runtime,
            target_position
                .checked_add(1)
                .ok_or("survivor control position overflow")?,
        )?;
        let client = RemoteWorldServiceClient::new(owner.clone()).map_err(|e| e.to_string())?;
        let reference = lookup_original_commit(&client, request)?;
        if reference.position != target_position {
            return Err("survivor changed original first position".into());
        }
        let publication = runtime
            .history
            .fixed_publication(&reference)
            .map_err(|error| format!("survivor initial original fixed publication: {error}"))?;
        if wasm_emit_count(publication.world()) != initial_emits + 1 {
            return Err("survivor Wasmtime effect not exactly once".into());
        }
        if boundary::execution_attempts() != 0 {
            return Err("successor reexecuted the already drained old-epoch original".into());
        }
        let effect = original_effect(publication.world(), request)?;
        if let Some(original) = &exact_effect {
            if original != &effect {
                return Err("survivor original agent/owner/resources/result changed".into());
            }
        } else {
            return Err("survivor lost actual old-epoch effect oracle".into());
        }
        if original_ref.as_ref() != Some(&reference) {
            return Err("survivor reopen changed immutable result".into());
        }
        let mut endpoint = RemoteFormalEndpoint::new(
            pa,
            Duration::from_secs(30),
            verified.recovery().successor().clone(),
            EndpointRole::Primary,
            WriterSigningCapabilityV1::new(successor_custody.normal.clone()),
            reference_to_head(&reference),
        )
        .map_err(|e| e.to_string())?;
        let queried = endpoint
            .query(&frozen_certificate.reservation.body.request_digest)
            .map_err(|e| e.to_string())?;
        let FormalQueryOutcomeV1::Recorded(entry) = queried else {
            return Err("survivor original missing".into());
        };
        let FormalRequestStateV1::Qualified { candidate, proof } = *entry else {
            return Err("survivor original not Qualified".into());
        };
        if candidate.frozen.as_ref() != Some(&frozen_certificate) {
            return Err("survivor changed original Frozen certificate".into());
        }
        if proof.decision.record != *drained.anchor.head().record()
            || candidate.candidate.record != proof.decision.record
        {
            return Err("survivor changed the exact old-epoch canonical result".into());
        }
        if let Some(original) = &original_candidate {
            if &candidate.candidate != original
                || proof.decision.writer_signature_hex != original.writer_signature_hex
                || proof.decision.branch != FormalCompletionBranchV1::Normal
            {
                return Err("survivor resigned prepared original".into());
            }
        } else if proof.decision.branch != FormalCompletionBranchV1::Normal {
            return Err("survivor lost original old-epoch Normal completion".into());
        }
        let before = runtime.published.current()?;
        let before_emits = wasm_emit_count(&before.world);
        let next = before
            .anchor
            .head()
            .record()
            .position
            .checked_add(1)
            .ok_or("survivor position overflow")?;
        let admission = client.submit(request.clone());
        if let Some(case) = &resources.revalidation {
            revalidation_tests::assert_retry_observation(case, admission, request)?;
        } else {
            assert_volatile_admission(admission.map_err(|e| e.to_string())?, &request.correlation)?;
        }
        wait_for_published_position(&runtime, next)?;
        let after = runtime.published.current()?;
        if wasm_emit_count(&after.world) != before_emits
            || after.world.state().agents.len() != 1
            || lookup_original_commit(&client, request)? != reference
        {
            return Err("survivor duplicate request produced a second original effect".into());
        }
        let original = runtime
            .history
            .fixed_publication(&reference)
            .map_err(|error| format!("survivor later original fixed publication: {error}"))?;
        if Some(original_effect(original.world(), request)?) != exact_effect {
            return Err("survivor duplicate changed original agent/owner/resources/result".into());
        }
        stop_owned_protected_runtime(
            &mut runtime,
            Duration::from_secs(120),
            "survivor_before_reopen",
        )?;
    }
    if resources.governed_participant {
        return governed_participant::submit_after_recovery(
            config,
            owner,
            world,
            request,
            original_ref
                .as_ref()
                .ok_or("survivor original result missing")?,
        );
    }
    next_request_tests::submit_after_recovery(
        config,
        owner,
        world,
        request,
        original_ref
            .as_ref()
            .ok_or("survivor original result missing")?,
    )?;
    Ok(())
}
fn reference_to_head(reference: &oasis7_client_api::world_service::CommitRef) -> FormalHeadRefV1 {
    FormalHeadRefV1 {
        position: reference.position,
        record_id: reference.execution_block_hash.clone(),
    }
}

pub(crate) fn write_origin_pages(
    endpoint: &mut RemoteFormalEndpoint,
    trust: &InitialActivationTrustV1,
    origin: &VerifiedFormalReadAnchorV1,
    cut_position: u64,
    directory: &Path,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    use oasis7::world_service::formal_read::{
        FormalReadHistoryPageV1, FormalReadHistoryV1, FormalReadReplayCursorV1, FormalReadStepV1,
    };
    let cursor = |anchor: &VerifiedFormalReadAnchorV1| -> Result<FormalReadReplayCursorV1, String> {
        Ok(FormalReadReplayCursorV1 {
            schema_version: 1,
            anchor: oasis7_client_api::world_service::FormalReadAnchorRefV1 {
                position: anchor.head().record().position,
                record_id: anchor.head().head().record_id,
                active_authority_id: anchor
                    .authority()
                    .authority_id()
                    .map_err(|e| e.to_string())?,
            },
            checkpoint_digest: anchor.checkpoint().digest().map_err(|e| e.to_string())?,
            last_recovery_digest: None,
            pending_recovery_digest: None,
            requires_v2_checkpoint: false,
        })
    };
    let mut current = origin.clone();
    let mut index = 0u64;
    while current.head().record().position < cut_position {
        let next = current
            .head()
            .record()
            .position
            .checked_add(1)
            .ok_or("survivor history position overflow")?;
        let historical = endpoint
            .locate_native_position(next, origin.authority(), origin.authority(), None)
            .map_err(|e| e.to_string())?;
        let proof = historical.proof().clone();
        // The native token is a fetch aid. The actual origin authorization is
        // independently rebuilt from each signed qualified canonical successor.
        let commit =
            verify_formal_qualified(&proof, current.authority(), Some(current.head()), None)
                .map_err(|e| e.to_string())?;
        let advanced = current
            .after_verified_commit(current.authority().clone(), commit)
            .map_err(|e| e.to_string())?;
        let page = FormalReadHistoryPageV1 {
            schema_version: 1,
            start: cursor(&current)?,
            history: FormalReadHistoryV1 {
                schema_version: 1,
                steps: vec![FormalReadStepV1::Commit(Box::new(proof))],
            },
            end: cursor(&advanced)?,
        };
        // Re-run the same production page verifier before writing the fixture.
        let checked =
            oasis7::world_service::formal_recovery_lineage::verify_recovery_predecessor_lineage(
                trust,
                &current,
                &advanced.checkpoint(),
                [Ok(page.clone())],
            )?;
        super::write_private(
            &directory.join(format!("{index:020}.cbor")),
            &page.encode_bytes()?,
        )?;
        current = checked;
        index = index.checked_add(1).ok_or("survivor page index overflow")?;
    }
    Ok(current)
}

fn original_effect(
    world: &oasis7_runtime::World,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<Vec<u8>, String> {
    if matches!(
        &request.signed_payload,
        WorldServicePayloadV1::GameParticipantRegister { .. }
    ) {
        governed_participant::registered_effect(world, request)
    } else {
        first_agent_test_effect::first_agent_effect(world, request)
    }
}
