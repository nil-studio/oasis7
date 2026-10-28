use super::*;

pub(super) struct OriginalOwnerBoundaryInputs<'a> {
    pub(super) case: &'a HealthySetCase,
    pub(super) signed: &'a SignedNormalDurableSetTransitionV1,
    pub(super) owner_client: &'a RemoteWorldServiceClient,
    pub(super) before_arm_commit: &'a oasis7_client_api::world_service::CommitRef,
    pub(super) fault_mode: Option<HealthyOwnerFaultMode>,
    pub(super) old_owner_sigkill_injected: &'a mut bool,
    pub(super) owned: &'a mut OwnedShippingChild,
    pub(super) owner: &'a formal_live_runtime::FormalRuntimeOwnerSnapshotV1,
    pub(super) original_operation: &'a str,
    pub(super) original_request_digest: &'a str,
    pub(super) config_digest: &'a str,
    pub(super) runtime_binary: &'a std::ffi::OsString,
    pub(super) store: &'a Path,
    pub(super) record: &'a mut Value,
}

pub(super) fn capture_original_result_and_inject_old_owner_fault(
    inputs: OriginalOwnerBoundaryInputs<'_>,
) -> Result<(oasis7_client_api::world_service::CommitRef, Value), String> {
    let OriginalOwnerBoundaryInputs {
        case,
        signed,
        owner_client,
        before_arm_commit,
        fault_mode,
        old_owner_sigkill_injected,
        owned,
        owner,
        original_operation,
        original_request_digest,
        config_digest,
        runtime_binary,
        store,
        record,
    } = inputs;
    let fixed = fixed_lookup::boundary_selector(case, signed.body.boundary_position, owner_client)?;
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
    if **commit != *before_arm_commit {
        return Err("exact H lookup changed the pre-arm original CommitRef".into());
    }
    let original_commit = (**commit).clone();
    let original_outcome = serde_json::to_value(response.outcome).map_err(|e| e.to_string())?;
    if fault_mode == Some(HealthyOwnerFaultMode::OldOwnerSigkillAfterH)
        && !*old_owner_sigkill_injected
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
        if persisted_owner["snapshot"]["instance"]["incarnation"] != owner.instance.incarnation
            || persisted_owner["process_start_identity"]
                .as_str()
                .is_none_or(str::is_empty)
        {
            return Err("SIGKILL fixture lacks the exact persisted owner birth identity".into());
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
        super::healthy_fault_tests::assert_no_published_h_plus_one(&case.writer_directory, h)?;
        let original_transition = record["healthy_durable_set_progress"]["transition"].clone();
        *record = cli_revision(
            runtime_binary,
            &case.root,
            "resume",
            store,
            "healthy-cli-operation",
            record,
        )?;
        if record["state"] == "succeeded"
            || record["healthy_durable_set_progress"]["stage"] != "sealing_old_pair"
            || record["healthy_durable_set_progress"]["transition"] != original_transition
        {
            return Err(format!(
                "same operation did not resume the exact H transition after owner SIGKILL: {record}"
            ));
        }
        super::healthy_fault_tests::assert_no_published_h_plus_one(&case.writer_directory, h)?;
        *old_owner_sigkill_injected = true;
    }
    Ok((original_commit, original_outcome))
}

pub(super) struct WriterLockFaultInputs<'a> {
    pub(super) case: &'a mut HealthySetCase,
    pub(super) core_id: &'a str,
    pub(super) runtime_binary: &'a std::ffi::OsString,
    pub(super) store: &'a Path,
    pub(super) planned: &'a Value,
    pub(super) record: &'a mut Value,
    pub(super) stable: &'a Value,
    pub(super) signed: &'a SignedNormalDurableSetTransitionV1,
    pub(super) fault_injected: &'a mut bool,
}

pub(super) fn inject_successor_writer_lock_fault(
    inputs: WriterLockFaultInputs<'_>,
) -> Result<(), String> {
    let WriterLockFaultInputs {
        case,
        core_id,
        runtime_binary,
        store,
        planned,
        record,
        stable,
        signed,
        fault_injected,
    } = inputs;
    let successor_operation = format!("healthy:{core_id}");
    let request_digest = planned["request_digest"]
        .as_str()
        .ok_or("healthy operation request digest absent")?;
    let successor_config_digest = planned["healthy_durable_set_plan"]["successor_config_digest"]
        .as_str()
        .ok_or("successor config digest absent")?;
    let fixed_trust_digest = planned["healthy_durable_set_plan"]["external_read_trust_digest"]
        .as_str()
        .ok_or("fixed C0 trust digest absent")?;
    let writer_lease = super::healthy_fault_tests::hold_formal_writer_lock(&case.writer_directory)?;
    *record = cli_revision(
        runtime_binary,
        &case.root,
        "resume",
        store,
        "healthy-cli-operation",
        record,
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
    *record = cli_revision(
        runtime_binary,
        &case.root,
        "resume",
        store,
        "healthy-cli-operation",
        record,
    )?;
    for _ in 0..24 {
        raw_seed::reject_exited_retry(record)?;
        if record["healthy_durable_set_progress"]["stage"] == "waiting_first_normal_qualified" {
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
        *record = cli_revision(
            runtime_binary,
            &case.root,
            "resume",
            store,
            "healthy-cli-operation",
            record,
        )?;
    }
    if record["state"] == "succeeded"
        || record["healthy_durable_set_progress"]["stage"] != "waiting_first_normal_qualified"
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
        return Err(
            "successful successor retry changed or dropped the original failed attempt receipt"
                .into(),
        );
    }
    *fault_injected = true;

    Ok(())
}
