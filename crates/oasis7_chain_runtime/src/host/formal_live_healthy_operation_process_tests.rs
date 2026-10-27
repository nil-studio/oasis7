//! Shipping-binary owner IPC and OperationStore CLI composition. Test-only
//! monotonic oracles simulate custody; no remote deployment is performed.
#[path = "formal_live_healthy_operation_cli_support.rs"]
mod cli_support;
use cli_support::{
    ExactOwnerCleanup, OwnedShippingChild, cli, cli_error, cli_revision, utf8, wait_owner,
};
#[path = "formal_live_healthy_fixed_lookup_tests.rs"]
mod fixed_lookup;
#[path = "formal_live_healthy_operation_process_tests/raw_seed.rs"]
mod raw_seed;
#[path = "formal_live_healthy_operation_stop.rs"]
mod stopping;
use super::durable_set_tests::{HealthySetCase, ManagedEndpointChild, stop_healthy_runtime};
use super::*;
use crate::{formal_live_runtime, node_lifecycle::local_io as io};
use oasis7_distfs::controlled_authority::formal::{
    DurableSetSealedEvidenceV1, HealthyEndpointLineagePageV1, HealthyEndpointLineageStepV1,
    SignedNormalDurableSetTransitionV1, verify_formal_qualified,
    verify_sealed_durable_set_transition,
};
use std::{
    fs::File,
    process::{Command, Stdio},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HealthyOwnerFaultMode {
    SuccessorWriterLock,
    OldOwnerSigkillAfterH,
}

#[test]
#[ignore = "shipping runtime and endpoint processes; requires OASIS7_CHAIN_RUNTIME_BIN/OASIS7_FORMAL_ENDPOINT_TEST_BIN"]
fn formal_protected_healthy_durable_set_operation_cli_owner_and_original_result() {
    let started_at = Instant::now();
    let deadline = ScenarioDeadline {
        started_at,
        expires_at: started_at + Duration::from_secs(8 * 60),
    };
    thread::Builder::new()
        .name("healthy-operation-process".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            run_protected_runtime_case_with_deadline(
                false,
                Some("healthy_operation"),
                false,
                Some(deadline),
            )
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}

pub(super) fn exercise(case: HealthySetCase) -> Result<(), String> {
    exercise_with_fault(case, None)
}

pub(super) fn exercise_with_fault(
    mut case: HealthySetCase,
    fault_mode: Option<HealthyOwnerFaultMode>,
) -> Result<(), String> {
    let runtime_binary = std::env::var_os("OASIS7_CHAIN_RUNTIME_BIN")
        .ok_or("OASIS7_CHAIN_RUNTIME_BIN must name shipping runtime")?;
    if !Path::new(&runtime_binary).is_absolute() {
        return Err("shipping runtime path must be absolute".into());
    }
    case.scenario_deadline
        .report_stage("actual op CLI fixture begins")?;
    if let Some(mut initial_runtime) = case.runtime.take() {
        stop_healthy_runtime(&mut initial_runtime, case.scenario_deadline)?;
        drop(initial_runtime);
    } // shipping owner alone holds the real writer lock
    let original_operation = "fixture-original-healthy-owner";
    let original_request_digest =
        io::digest(&("shipping owner fixture", &case.runtime_config_path))?;
    let config_digest = io::sha256(&case.runtime_config_path)?;
    let stdout =
        File::create(case.root.join("shipping-owner.stdout")).map_err(|e| e.to_string())?;
    let stderr =
        File::create(case.root.join("shipping-owner.stderr")).map_err(|e| e.to_string())?;
    let child = Command::new(&runtime_binary)
        .args(["op", "formal-owner-worker", "--config"])
        .arg(&case.runtime_config_path)
        .args([
            "--operation-id",
            original_operation,
            "--request-digest",
            &original_request_digest,
            "--config-digest",
            &config_digest,
        ])
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut owned = OwnedShippingChild(child);
    let owner = wait_owner(
        &mut owned.0,
        &case.runtime_config_path,
        original_operation,
        &original_request_digest,
        &config_digest,
    )
    .map_err(|error| {
        let path = case.root.join("shipping-owner.stderr");
        let stderr = std::fs::read(&path).unwrap_or_default();
        let bounded = &stderr[..stderr.len().min(16 * 1024)];
        format!(
            "{error}; stderr_file={}: {}",
            path.display(),
            String::from_utf8_lossy(bounded)
        )
    })?;
    let initial_client = RemoteWorldServiceClient::new(case.public_client_config.clone())
        .map_err(|error| error.to_string())?;
    let initial_view = read_current(&initial_client, &case.world)?;
    // The coordinator already verified exact C0 before spawning the owner.
    // Background ticks may advance the current cut; the client still verifies
    // its signed lineage from that unchanged external C0 trust pin.
    if initial_view.version().commit.position < 1 || initial_view.formal_commit().is_none() {
        return Err(
            "shipping owner did not serve a qualified read from the external C0 pin".into(),
        );
    }
    let new_replica_directory = case.root.join("operation-successor-replica");
    oracle_support::private_directory(&new_replica_directory)?;
    let replica = super::durable_set_tests::signed_transition(
        &case.authority,
        &case.initial_anchor,
        &case.primary_directory,
        &new_replica_directory,
    )?
    .body
    .successor
    .replica;
    let primary_management = free_loopback_address()?;
    let replica_management = free_loopback_address()?;
    let store = case.root.join("actual-operation-store");
    let request_path = case.root.join("healthy-operation-request.json");
    let mut request = serde_json::json!({
        "api_version": "oasis7.local-operation/v1", "operation_id": "healthy-cli-cancel-before-arm",
        "world_id": owner.instance.world_id, "chain_id": owner.instance.chain_id,
        "kind": "remove_critical_replica", "config": case.runtime_config_path,
        "expected_generation": owner.instance.authority_epoch, "expected_config_digest": owner.instance.config_digest,
        "expected_instance": null, "authorization": { "kind": "healthy_durable_set", "authorization": {
            "expected_owner": owner.instance, "successor_runtime_config_file": case.runtime_config_path,
            "successor_replica": replica, "primary_management": primary_management, "replica_management": replica_management
        }}
    });
    io::write(&request_path, &request)?;
    let _ = cli(
        &runtime_binary,
        &case.root,
        &["plan", "--request", utf8(&request_path)?],
    )?;
    let cancelled = cli(
        &runtime_binary,
        &case.root,
        &[
            "submit",
            "--request",
            utf8(&request_path)?,
            "--store",
            utf8(&store)?,
        ],
    )?;
    let cancelled = cli_revision(
        &runtime_binary,
        &case.root,
        "cancel",
        &store,
        "healthy-cli-cancel-before-arm",
        &cancelled,
    )?;
    if cancelled["state"] != "cancelled" || cancelled["dispatched"] != false {
        return Err("before-arm CLI cancellation dispatched a native effect".into());
    }
    let owner_client = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|e| e.to_string())?;
    assert_volatile_admission(
        owner_client
            .submit(case.owner_request.clone())
            .map_err(|e| e.to_string())?,
        &case.owner_request.correlation,
    )?;
    // Commit the original request while the old owner can still publish Normal
    // records. Arming H freezes that cut; later admission alone is volatile.
    let original_deadline = Instant::now() + Duration::from_secs(20);
    let before_arm_commit = loop {
        match lookup_original_commit(&owner_client, &case.owner_request) {
            Ok(commit) => break commit,
            Err(error) if Instant::now() >= original_deadline => {
                return Err(format!(
                    "original request was not committed before arm: {error}; {}",
                    cli_support::original_commit_failure_status(&case.owner_client_config)
                ));
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    };
    case.scenario_deadline
        .report_stage("original request committed before arm")?;
    // The history-page resume API intentionally starts from a fresh C0 cache.
    // Keep the proven original receipt above and replay H using unchanged C0 trust.
    let owner_client = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|e| e.to_string())?;
    request["operation_id"] = serde_json::json!("healthy-cli-operation");
    io::write(&request_path, &request)?;
    let planned = cli(
        &runtime_binary,
        &case.root,
        &["plan", "--request", utf8(&request_path)?],
    )?;
    let core_id = planned["healthy_durable_set_plan"]["core_operation_id"]
        .as_str()
        .ok_or("core operation id absent")?
        .to_owned();
    let initial_pin_digest = case
        .initial_anchor
        .checkpoint()
        .digest()
        .map_err(|error| error.to_string())?;
    if planned["healthy_durable_set_plan"]["external_initial_pin_digest"] != initial_pin_digest {
        return Err("healthy operation plan changed the original external C0 pin".into());
    }
    let mut record = cli(
        &runtime_binary,
        &case.root,
        &[
            "submit",
            "--request",
            utf8(&request_path)?,
            "--store",
            utf8(&store)?,
        ],
    )?;
    // IPC wrong incarnation must fail before signing or native arm.
    let mut wrong = owner.instance.clone();
    wrong.incarnation = fixtures::h(238);
    if formal_live_runtime::control_prepare_durable_set_transition(
        &case.runtime_config_path,
        &wrong,
        &core_id,
        replica.clone(),
        Duration::from_secs(5),
    )
    .is_ok()
    {
        return Err("shipping owner accepted wrong incarnation prepare".into());
    }
    record = cli_revision(
        &runtime_binary,
        &case.root,
        "resume",
        &store,
        "healthy-cli-operation",
        &record,
    )?;
    let signed: SignedNormalDurableSetTransitionV1 =
        serde_json::from_value(record["healthy_durable_set_progress"]["transition"].clone())
            .map_err(|e| format!("CLI prepare failed: {e}; {record}"))?;
    let retry = formal_live_runtime::control_prepare_durable_set_transition(
        &case.runtime_config_path,
        &owner.instance,
        &core_id,
        replica.clone(),
        Duration::from_secs(5),
    )?;
    if retry != signed {
        return Err("shipping prepare retry changed original signed transition".into());
    }
    let query = formal_live_runtime::control_durable_set_transition(
        &case.runtime_config_path,
        &owner.instance,
        formal_live_runtime::DurableSetControlRequestV1::Query {
            operation_id: core_id.clone(),
            boundary_position: signed.body.boundary_position,
        },
        Duration::from_secs(5),
    )?
    .ok_or("shipping arm query missing")?;
    if query.transition_digest != signed.body.digest().map_err(|e| e.to_string())? {
        return Err("shipping IPC query differs from original transition".into());
    }
    let mut conflicting = replica.clone();
    conflicting.instance.instance_id = fixtures::h(239);
    if formal_live_runtime::control_prepare_durable_set_transition(
        &case.runtime_config_path,
        &owner.instance,
        &core_id,
        conflicting,
        Duration::from_secs(5),
    )
    .is_ok()
    {
        return Err(
            "shipping owner accepted original operation with changed replica target".into(),
        );
    }
    let revision = record["revision"].as_u64().ok_or("revision")?.to_string();
    cli_error(
        &runtime_binary,
        &case.root,
        &[
            "cancel",
            "--store",
            utf8(&store)?,
            "--id",
            "healthy-cli-operation",
            "--revision",
            &revision,
        ],
        "CANCELLATION_AFTER_ARM",
    )?;
    let mut original_commit = None;
    let mut original_outcome = None;
    let _successor_cleanup = ExactOwnerCleanup {
        config: case.runtime_config_path.clone(),
        operation: format!("healthy:{core_id}"),
        request_digest: planned["request_digest"]
            .as_str()
            .ok_or("request digest absent")?
            .into(),
        config_digest: config_digest.clone(),
    };
    // Separate shipping CLI processes resume the same CAS operation, then
    // stop at the durable external provisioning boundary.
    let mut old_owner_sigkill_injected = false;
    for _ in 0..12 {
        case.scenario_deadline
            .report_stage("CLI resume before external candidate provisioning")?;
        record = cli_revision(
            &runtime_binary,
            &case.root,
            "resume",
            &store,
            "healthy-cli-operation",
            &record,
        )?;
        if record["healthy_durable_set_progress"]["stage"] == "stopping_old_owner" {
            let fixed = fixed_lookup::boundary_selector(
                &case,
                signed.body.boundary_position,
                &owner_client,
            )?;
            let response = owner_client
                .lookup_formal_original_at(case.owner_request.clone(), fixed)
                .map_err(|error| format!("exact H signed original lookup: {error}"))?;
            if response.correlation != case.owner_request.correlation
                || response.formal_profile.as_ref()
                    != Some(&oasis7_client_api::world_service::FormalReadProfileV1::controlled())
                || response.execution_evidence_scope.is_some()
            {
                return Err("exact H original lookup profile/correlation mismatch".into());
            }
            let oasis7_client_api::world_service::IntentOutcome::Committed { commit, .. } =
                &response.outcome
            else {
                return Err("exact H lookup did not prove original committed outcome".into());
            };
            if **commit != before_arm_commit {
                return Err("exact H lookup changed the pre-arm original CommitRef".into());
            }
            original_commit = Some((**commit).clone());
            original_outcome =
                Some(serde_json::to_value(response.outcome).map_err(|e| e.to_string())?);
            if fault_mode == Some(HealthyOwnerFaultMode::OldOwnerSigkillAfterH)
                && !old_owner_sigkill_injected
            {
                let h = signed.body.boundary_position;
                let (_, boundary_proof) =
                    crate::execution_bridge::formal_writer::read_published_candidate_at(
                        &case.writer_directory,
                        h,
                    )?
                    .ok_or("SIGKILL fixture requires the actual qualified H proof")?;
                let boundary =
                    fixed_lookup::qualified_lineage(&case, &signed, &owner_client, &boundary_proof)
                        .map_err(|error| format!("select actual SIGKILL boundary H: {error}"))?
                        .boundary;
                if boundary.head().position != h
                    || boundary_proof
                        .decision
                        .record
                        .durable_set_transition_digest
                        .as_deref()
                        != Some(signed.body.digest().map_err(|e| e.to_string())?.as_str())
                {
                    return Err("SIGKILL fixture observed a different qualified H".into());
                }
                owned.kill_sigkill_and_wait()?;
                let owner_directory = super::healthy_fault_tests::owner_operation_directory(
                    &case.writer_directory,
                    original_operation,
                )?;
                let persisted_owner: Value = serde_json::from_slice(
                    &std::fs::read(owner_directory.join("owner.json"))
                        .map_err(|error| format!("read killed owner receipt: {error}"))?,
                )
                .map_err(|error| format!("decode killed owner receipt: {error}"))?;
                if persisted_owner["snapshot"]["instance"]["incarnation"]
                    != owner.instance.incarnation
                    || persisted_owner["process_start_identity"]
                        .as_str()
                        .is_none_or(str::is_empty)
                {
                    return Err(
                        "SIGKILL fixture lacks the exact persisted owner birth identity".into(),
                    );
                }
                let interrupted = formal_live_runtime::query_operation_owner(
                    &case.runtime_config_path,
                    original_operation,
                    &original_request_digest,
                    &config_digest,
                    Some(&owner.instance.incarnation),
                )?
                .ok_or("SIGKILL owner receipt disappeared")?;
                if interrupted.state != "interrupted" || interrupted.instance != owner.instance {
                    return Err(format!(
                        "SIGKILL did not preserve exact interrupted owner at H: {interrupted:?}"
                    ));
                }
                super::healthy_fault_tests::assert_no_published_h_plus_one(
                    &case.writer_directory,
                    h,
                )?;
                let original_transition =
                    record["healthy_durable_set_progress"]["transition"].clone();
                record = cli_revision(
                    &runtime_binary,
                    &case.root,
                    "resume",
                    &store,
                    "healthy-cli-operation",
                    &record,
                )?;
                if record["state"] == "succeeded"
                    || record["healthy_durable_set_progress"]["stage"] != "sealing_old_pair"
                    || record["healthy_durable_set_progress"]["transition"] != original_transition
                {
                    return Err(format!(
                        "same operation did not resume the exact H transition after owner SIGKILL: {record}"
                    ));
                }
                super::healthy_fault_tests::assert_no_published_h_plus_one(
                    &case.writer_directory,
                    h,
                )?;
                old_owner_sigkill_injected = true;
            }
        }
        if record["healthy_durable_set_progress"]["stage"] == "waiting_external_candidates" {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if record["healthy_durable_set_progress"]["stage"] != "waiting_external_candidates" {
        return Err(format!(
            "operation failed to reach exact sealed external boundary: {record}"
        ));
    }
    if fault_mode == Some(HealthyOwnerFaultMode::OldOwnerSigkillAfterH)
        && !old_owner_sigkill_injected
    {
        return Err("old-owner SIGKILL fault was not injected at the qualified H boundary".into());
    }
    let stable = record.clone();
    record = cli_revision(
        &runtime_binary,
        &case.root,
        "resume",
        &store,
        "healthy-cli-operation",
        &record,
    )?;
    if record["state"] == "succeeded"
        || record["healthy_durable_set_progress"]["transition"]
            != stable["healthy_durable_set_progress"]["transition"]
    {
        return Err(
            "unprovisioned candidates fabricated success or changed original transition".into(),
        );
    }
    let status = cli(
        &runtime_binary,
        &case.root,
        &[
            "status",
            "--store",
            utf8(&store)?,
            "--id",
            "healthy-cli-operation",
        ],
    )?;
    if status != record {
        return Err("shipping op status changed original stored operation".into());
    }
    let original_commit = original_commit
        .ok_or("original committed result was not captured before exact owner stop")?;
    let (_, proof) = crate::execution_bridge::formal_writer::read_published_candidate_at(
        &case.writer_directory,
        signed.body.boundary_position,
    )?
    .ok_or("H proof missing")?;
    let lineage = fixed_lookup::qualified_lineage(&case, &signed, &owner_client, &proof)
        .map_err(|error| format!("select provision qualified H: {error}"))?;
    let boundary = lineage.boundary;
    let predecessor = lineage.predecessor;
    let higher_anchor = lineage.anchor;
    // A different lawful checkpoint under the SAME authority is still a
    // different external origin. A config.json SHA alone must not admit it.
    let validated = crate::formal_live_config::FormalLiveConfigV1::load(&case.runtime_config_path)?;
    let trust_file = validated.config.formal_read_trust_file.clone();
    let trust_bytes = std::fs::read(&trust_file).map_err(|e| e.to_string())?;

    let mut changed_trust = validated.trust.clone();
    changed_trust.checkpoint = higher_anchor.checkpoint();
    changed_trust.checkpoint_digest = changed_trust
        .checkpoint
        .digest()
        .map_err(|e| e.to_string())?;
    changed_trust.verified_anchor()?;
    raw_seed::replace_private_fixture_bytes(
        &case.root,
        &trust_file,
        &serde_json::to_vec(&changed_trust).map_err(|e| e.to_string())?,
    )?;
    let before_pin_change = record["healthy_durable_set_progress"].clone();
    let changed_pin_result = cli_revision(
        &runtime_binary,
        &case.root,
        "resume",
        &store,
        "healthy-cli-operation",
        &record,
    );
    // Restore the exact captured bytes even if the negative CLI call fails.
    raw_seed::replace_private_fixture_bytes(&case.root, &trust_file, &trust_bytes)?;
    record = changed_pin_result?;
    if !matches!(
        record["blocker"].as_str(),
        Some("approved initial publication differs from exact pinned initial activation")
            | Some("healthy immutable external C0 trust/checkpoint dependency changed")
    ) || record["healthy_durable_set_progress"]["stage"] != before_pin_change["stage"]
        || record["healthy_durable_set_progress"]["transition"] != before_pin_change["transition"]
    {
        return Err(format!(
            "changed external trust was not refused without progress mutation: {record}"
        ));
    }
    let seals = &record["healthy_durable_set_progress"]["old_seals"];
    let evidence = DurableSetSealedEvidenceV1 {
        transition: signed.clone(),
        primary_seal: serde_json::from_value(seals[0].clone()).map_err(|e| e.to_string())?,
        replica_seal: serde_json::from_value(seals[1].clone()).map_err(|e| e.to_string())?,
    };
    let sealed = Arc::new(
        verify_sealed_durable_set_transition(&evidence, &case.authority, &predecessor, &boundary)
            .map_err(|error| format!("verify original P/R seals at exact H: {error}"))?,
    );
    case.primary_provision
        .provision(sealed.clone())
        .map_err(|error| format!("provision successor primary protected oracle: {error}"))?;
    let resource = case
        .authority
        .policy()
        .authorized_fencing_resource_bindings
        .first()
        .cloned()
        .ok_or("fencing resource missing")?;
    let successor_oracle = oracle_support::durable_set_oracle::start_healthy_replica_oracle(
        sealed.clone(),
        new_replica_directory.clone(),
        resource,
    )
    .map_err(|error| format!("provision successor replica protected oracle: {error}"))?;
    let lineage_directory = case.root.join("operation-lineage");
    oracle_support::private_directory(&lineage_directory)?;
    let page = HealthyEndpointLineagePageV1 {
        schema_version: 1,
        from: case.initial_anchor.head().head(),
        steps: lineage.steps,
        to: boundary.head(),
    };
    oracle_support::write_private(
        &lineage_directory.join("00000000000000000000.cbor"),
        &serde_cbor::to_vec(&page).map_err(|e| e.to_string())?,
    )?;
    let archive_grant = super::durable_set_tests::archive_grant(&case, &sealed)?;
    // Exact fixture-owned old P releases its store. Old R remains solely as
    // the authenticated source for the independently copied new R closure.
    case.primary_child.kill_sigkill_and_wait()?;
    let primary_bootstrap = serde_json::json!({ "schema_version":1, "lineage_directory":lineage_directory, "sealed":sealed.evidence(), "management_listen":primary_management, "source_address":null, "source_role":null, "archive_read_grant":null, "operation_byte_budget":64*1024*1024 });
    let replica_bootstrap = serde_json::json!({ "schema_version":1, "lineage_directory":lineage_directory, "sealed":sealed.evidence(), "management_listen":replica_management, "source_address":case.replica_address, "source_role":EndpointRole::Replica, "archive_read_grant":archive_grant, "operation_byte_budget":64*1024*1024 });
    let pconfig = case.root.join("operation-primary-endpoint.json");
    let rconfig = case.root.join("operation-replica-endpoint.json");
    super::durable_set_tests::write_healthy_endpoint_config(
        &pconfig,
        &case,
        &case.primary_directory,
        case.primary_oracle.address(),
        EndpointRole::Primary,
        case.primary_address,
        &primary_bootstrap,
    )?;
    super::durable_set_tests::write_healthy_endpoint_config(
        &rconfig,
        &case,
        &new_replica_directory,
        successor_oracle.address(),
        EndpointRole::Replica,
        case.replica_address,
        &replica_bootstrap,
    )?;
    raw_seed::copy_objects(&case.primary_directory, &new_replica_directory)?;
    let mut new_primary = ManagedEndpointChild::start(&case.endpoint_binary, &pconfig)?;
    let mut new_replica = ManagedEndpointChild::start(&case.endpoint_binary, &rconfig)?;
    new_primary.wait_management(primary_management, case.scenario_deadline)?;
    new_replica.wait_management(replica_management, case.scenario_deadline)?;
    let mut old_replica_stopped = false;
    let mut submitted_after_reopen = false;
    let mut successor_writer_lock_fault_injected = false;
    // CLI calls advance control-plane stages, not the asynchronous successor
    // publication clock. Keep waiting for the same operation's actual Qualified
    // readback within the original scenario deadline, regardless of call speed.
    loop {
        case.scenario_deadline
            .report_stage("actual op CLI forward recovery")
            .map_err(|error| format!("{error}; last healthy operation: {record}"))?;
        let stage = record["healthy_durable_set_progress"]["stage"]
            .as_str()
            .unwrap_or("");
        if stage == "activating_candidates" && !old_replica_stopped {
            case.replica_child.kill_sigkill_and_wait()?;
            old_replica_stopped = true;
        }
        if stage == "waiting_first_normal_qualified" && !submitted_after_reopen {
            new_primary.wait_ordinary(case.primary_address, Duration::from_secs(20))?;
            new_replica.wait_ordinary(case.replica_address, Duration::from_secs(20))?;
            let change = oasis7::world_service::AgentSignerDelegationChangeV1 {
                world_id: case.world.world_id.clone(),
                branch_id: "main".into(),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                agent_identity_generation: 1,
                generation: 1,
                delegate_public_key: oracle_support::public(72),
                revoked: false,
                nonce: 1,
            };
            let payload = WorldServicePayloadV1::Delegation(
                sign_read_request(
                    "delegation",
                    change,
                    &hex::encode(case.owner_key.to_bytes()),
                )
                .map_err(|e| e.to_string())?,
            );
            let next = SubmitIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                correlation: correlation::derive_correlation(case.world.clone(), &payload)
                    .map_err(|e| e.to_string())?,
                deadline_unix_ms: None,
                signed_payload: payload,
            };
            let client = RemoteWorldServiceClient::new(case.owner_client_config.clone())
                .map_err(|e| e.to_string())?;
            assert_volatile_admission(
                client.submit(next.clone()).map_err(|e| e.to_string())?,
                &next.correlation,
            )?;
            submitted_after_reopen = true;
        }
        if stage == "reopening_original_c0"
            && fault_mode == Some(HealthyOwnerFaultMode::SuccessorWriterLock)
            && !successor_writer_lock_fault_injected
        {
            let successor_operation = format!("healthy:{core_id}");
            let request_digest = planned["request_digest"]
                .as_str()
                .ok_or("healthy operation request digest absent")?;
            let successor_config_digest =
                planned["healthy_durable_set_plan"]["successor_config_digest"]
                    .as_str()
                    .ok_or("successor config digest absent")?;
            let fixed_trust_digest =
                planned["healthy_durable_set_plan"]["external_read_trust_digest"]
                    .as_str()
                    .ok_or("fixed C0 trust digest absent")?;
            let writer_lease =
                super::healthy_fault_tests::hold_formal_writer_lock(&case.writer_directory)?;
            record = cli_revision(
                &runtime_binary,
                &case.root,
                "resume",
                &store,
                "healthy-cli-operation",
                &record,
            )?;
            if record["state"] == "succeeded"
                || record["state"] != "reconciling"
                || record["healthy_durable_set_progress"]["stage"] != "reopening_original_c0"
                || record["healthy_durable_set_progress"]["transition"]
                    != stable["healthy_durable_set_progress"]["transition"]
            {
                return Err(format!(
                    "successor writer-lock startup failure was reported as completion or changed the original operation: {record}"
                ));
            }
            let owner_directory = super::healthy_fault_tests::owner_operation_directory(
                &case.writer_directory,
                &successor_operation,
            )?;
            let failed_bytes = std::fs::read(owner_directory.join("owner.json"))
                .map_err(|error| format!("read failed successor owner receipt: {error}"))?;
            let failed_record: Value = serde_json::from_slice(&failed_bytes)
                .map_err(|error| format!("decode failed successor owner receipt: {error}"))?;
            let failed_instance: formal_live_runtime::FormalRuntimeInstanceV1 =
                serde_json::from_value(failed_record["snapshot"]["instance"].clone())
                    .map_err(|error| format!("decode failed successor identity: {error}"))?;
            if failed_record["snapshot"]["state"] != "failed"
                || failed_record["attempt"] != 1
                || failed_instance.operation_id != successor_operation
                || failed_instance.request_digest != request_digest
                || failed_instance.config_digest != successor_config_digest
                || failed_record["snapshot"]["last_error"]
                    .as_str()
                    .is_none_or(|error| !error.contains("formal writer directory is occupied"))
            {
                return Err(format!(
                    "real writer.lock contention did not persist the exact failed startup receipt: {failed_record}"
                ));
            }
            let fixed_trust_path = owner_directory.join("fixed-read-trust.json");
            let fixed_trust_before = std::fs::read(&fixed_trust_path)
                .map_err(|error| format!("read first fixed C0 trust receipt: {error}"))?;
            let trust_receipt: Value = serde_json::from_slice(&fixed_trust_before)
                .map_err(|error| format!("decode first fixed C0 trust receipt: {error}"))?;
            if trust_receipt["request_digest"] != request_digest
                || trust_receipt["config_digest"] != successor_config_digest
                || trust_receipt["read_trust_digest"] != fixed_trust_digest
            {
                return Err("failed successor startup changed its fixed C0 trust identity".into());
            }
            let failed_owner = formal_live_runtime::query_operation_owner_with_fixed_trust(
                &case.runtime_config_path,
                &successor_operation,
                request_digest,
                successor_config_digest,
                None,
                fixed_trust_digest,
            )?
            .ok_or("failed successor operation owner receipt disappeared")?;
            if failed_owner.state != "interrupted" || failed_owner.instance != failed_instance {
                return Err(format!(
                    "failed startup was not reconciled by exact PID/start identity: {failed_owner:?}"
                ));
            }
            super::healthy_fault_tests::assert_no_published_h_plus_one(
                &case.writer_directory,
                signed.body.boundary_position,
            )?;
            drop(writer_lease);
            let original_transition = record["healthy_durable_set_progress"]["transition"].clone();
            record = cli_revision(
                &runtime_binary,
                &case.root,
                "resume",
                &store,
                "healthy-cli-operation",
                &record,
            )?;
            for _ in 0..24 {
                raw_seed::reject_exited_retry(&record)?;
                if record["healthy_durable_set_progress"]["stage"]
                    == "waiting_first_normal_qualified"
                {
                    break;
                }
                let observed = formal_live_runtime::query_operation_owner_with_fixed_trust(
                    &case.runtime_config_path,
                    &successor_operation,
                    request_digest,
                    successor_config_digest,
                    None,
                    fixed_trust_digest,
                )?
                .ok_or("retry successor owner receipt missing")?;
                eprintln!(
                    "writer-lock retry owner state={} last_error={:?}",
                    observed.state, observed.last_error
                );
                if (observed.state == "failed"
                    && observed.instance.incarnation != failed_instance.incarnation)
                    || record["healthy_durable_set_progress"]["stage"] != "reopening_original_c0"
                    || record["healthy_durable_set_progress"]["transition"] != original_transition
                {
                    return Err(format!(
                        "retry changed original transition or failed: {observed:?}"
                    ));
                }
                record = cli_revision(
                    &runtime_binary,
                    &case.root,
                    "resume",
                    &store,
                    "healthy-cli-operation",
                    &record,
                )?;
            }
            if record["state"] == "succeeded"
                || record["healthy_durable_set_progress"]["stage"]
                    != "waiting_first_normal_qualified"
                || record["healthy_durable_set_progress"]["transition"] != original_transition
            {
                return Err(format!(
                    "same operation did not resume the original fixed-C0 transition after writer-lock release: {record}"
                ));
            }
            let ready_owner = formal_live_runtime::query_operation_owner_with_fixed_trust(
                &case.runtime_config_path,
                &successor_operation,
                request_digest,
                successor_config_digest,
                None,
                fixed_trust_digest,
            )?
            .ok_or("retried exact successor owner did not become queryable")?;
            if ready_owner.state != "ready"
                || ready_owner.instance.operation_id != failed_instance.operation_id
                || ready_owner.instance.request_digest != failed_instance.request_digest
                || ready_owner.instance.config_digest != failed_instance.config_digest
                || ready_owner.instance.node_id != failed_instance.node_id
                || ready_owner.instance.world_id != failed_instance.world_id
                || ready_owner.instance.chain_id != failed_instance.chain_id
                || ready_owner.instance.authority_id != failed_instance.authority_id
                || ready_owner.instance.authority_epoch != failed_instance.authority_epoch
                || ready_owner.instance.profile_version != failed_instance.profile_version
                || ready_owner.instance.incarnation == failed_instance.incarnation
                || std::fs::read(&fixed_trust_path).map_err(|error| error.to_string())?
                    != fixed_trust_before
            {
                return Err(format!(
                    "successor retry changed exact owner or fixed-C0 identity: {ready_owner:?}"
                ));
            }
            let history: Value = serde_json::from_slice(
                &std::fs::read(owner_directory.join("attempt-history.json"))
                    .map_err(|error| format!("read failed owner attempt receipt: {error}"))?,
            )
            .map_err(|error| format!("decode failed owner attempt receipt: {error}"))?;
            let current_owner: Value = serde_json::from_slice(
                &std::fs::read(owner_directory.join("owner.json"))
                    .map_err(|error| format!("read retried owner receipt: {error}"))?,
            )
            .map_err(|error| format!("decode retried owner receipt: {error}"))?;
            let exit_observation = history["attempts"][0]["exit_observation"].as_str();
            if current_owner["attempt"] != 2
                || current_owner["snapshot"]["state"] != "ready"
                || history["attempts"]
                    .as_array()
                    .is_none_or(|attempts| attempts.len() != 1)
                || history["attempts"][0]["attempt"] != 1
                || history["attempts"][0]["snapshot"]["state"] != "failed"
                || history["attempts"][0]["snapshot"]["instance"]["incarnation"]
                    != failed_instance.incarnation
                || history["attempts"][0]["snapshot"]["instance"]["config_digest"]
                    != successor_config_digest
                || !matches!(
                    exit_observation,
                    Some("pid_absent" | "pid_reused_with_different_start_identity")
                )
            {
                return Err("successful successor retry changed or dropped the original failed attempt receipt".into());
            }
            successor_writer_lock_fault_injected = true;
        }
        record = cli_revision(
            &runtime_binary,
            &case.root,
            "resume",
            &store,
            "healthy-cli-operation",
            &record,
        )?;
        // A bounded CLI call can straddle the deadline; do not accept even a
        // successful result returned after the original scenario budget.
        case.scenario_deadline
            .report_stage("actual op CLI forward recovery result")
            .map_err(|error| format!("{error}; last healthy operation: {record}"))?;
        if record["state"] == "succeeded" {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if record["state"] != "succeeded" || !submitted_after_reopen {
        return Err(format!(
            "actual CLI healthy operation did not qualify new Normal successor: {record}"
        ));
    }
    if fault_mode == Some(HealthyOwnerFaultMode::SuccessorWriterLock)
        && !successor_writer_lock_fault_injected
    {
        return Err(
            "successor writer-lock fault was not injected at the exact reopen stage".into(),
        );
    }
    let readback = &record["healthy_durable_set_progress"]["normal_readback"];
    if readback["position"]
        .as_u64()
        .is_none_or(|n| n <= signed.body.boundary_position)
    {
        return Err("CLI operation succeeded at H without first successor Normal".into());
    }
    let successor: formal_live_runtime::FormalRuntimeInstanceV1 =
        serde_json::from_value(record["healthy_durable_set_progress"]["successor_owner"].clone())
            .map_err(|e| e.to_string())?;
    if successor.incarnation == owner.instance.incarnation {
        return Err("CLI reused stopped predecessor incarnation".into());
    }
    if successor.authority_id
        != record["healthy_durable_set_progress"]["normal_readback"]["authority_id"]
    {
        return Err(
            "first successor Normal proof used a different authority from its exact owner".into(),
        );
    }
    super::healthy_fault_tests::verify_successor_h_plus_one(
        &record,
        &case.writer_directory,
        signed.body.boundary_position,
        &case.authority,
        &predecessor,
        &boundary,
    )?;
    let public_owner = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|e| e.to_string())?;
    if lookup_original_commit(&public_owner, &case.owner_request)? != original_commit {
        return Err("old original CommitRef changed after operation successor".into());
    }
    let after_outcome = serde_json::to_value(
        public_owner
            .lookup_formal_original(case.owner_request.clone())
            .map_err(|e| e.to_string())?
            .outcome,
    )
    .map_err(|e| e.to_string())?;
    if original_outcome.as_ref() != Some(&after_outcome) {
        return Err("original result changed across healthy operation qualification".into());
    }
    let recovered = cli(
        &runtime_binary,
        &case.root,
        &[
            "recover",
            "--request",
            utf8(&request_path)?,
            "--store",
            utf8(&store)?,
        ],
    )?;
    if recovered != record {
        return Err("recover original id did not return immutable operation outcome".into());
    }
    let missing_store = case.root.join("missing-original-ledger");
    cli_error(
        &runtime_binary,
        &case.root,
        &[
            "recover",
            "--request",
            utf8(&request_path)?,
            "--store",
            utf8(&missing_store)?,
        ],
        "ORIGINAL_OUTCOME_UNKNOWN",
    )?;
    // Admission for another replacement must derive the current authority
    // through the same fixed C0 marker lineage, not stop at the first epoch.
    let second_directory = case.root.join("second-operation-successor-replica");
    oracle_support::private_directory(&second_directory)?;
    let mut second_replica = signed.body.successor.replica.clone();
    second_replica.instance.endpoint_id = "healthy-second-successor-replica".into();
    second_replica.instance.endpoint_public_key = oracle_support::public(18);
    second_replica.instance.instance_id = fixtures::h(240);
    second_replica.instance.storage_id = fixtures::h(241);
    second_replica.instance.storage_fingerprint =
        directory_fingerprint(&second_directory).map_err(|e| e.to_string())?;
    second_replica.fault_domain_id = "healthy-second-successor-domain".into();
    let mut second_request = request.clone();
    second_request["operation_id"] = serde_json::json!("healthy-cli-second-admission");
    second_request["expected_generation"] = serde_json::json!(successor.authority_epoch);
    second_request["expected_config_digest"] = serde_json::json!(successor.config_digest);
    second_request["authorization"]["authorization"]["expected_owner"] =
        serde_json::to_value(&successor).map_err(|e| e.to_string())?;
    second_request["authorization"]["authorization"]["successor_replica"] =
        serde_json::to_value(second_replica).map_err(|e| e.to_string())?;
    let second_request_path = case.root.join("healthy-second-operation-request.json");
    io::write(&second_request_path, &second_request)?;
    let second_planned = cli(
        &runtime_binary,
        &case.root,
        &["plan", "--request", utf8(&second_request_path)?],
    )?;
    if second_planned["healthy_durable_set_plan"]["external_initial_pin_digest"]
        != planned["healthy_durable_set_plan"]["external_initial_pin_digest"]
    {
        return Err("second healthy admission repinned the external C0 origin".into());
    }
    let second_submitted = cli(
        &runtime_binary,
        &case.root,
        &[
            "submit",
            "--request",
            utf8(&second_request_path)?,
            "--store",
            utf8(&store)?,
        ],
    )?;
    let second_cancelled = cli_revision(
        &runtime_binary,
        &case.root,
        "cancel",
        &store,
        "healthy-cli-second-admission",
        &second_submitted,
    )?;
    if second_cancelled["state"] != "cancelled" {
        return Err("second-epoch admission cannot be cancelled before arm".into());
    }
    stopping::stop_exact_successor_owner(
        &case.runtime_config_path,
        &successor,
        case.scenario_deadline,
    )?;
    case.scenario_deadline
        .report_stage("shipping CLI operation and original result verified")?;
    drop((new_primary, new_replica, successor_oracle));
    Ok(())
}
