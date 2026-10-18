//! Same actual formal writer/authority/history before and after maintenance.
//! This is not a synthetic raw CAS comparison or independent fault-domain proof.
use super::*;
use oasis7::world_service::AgentSignerDelegationChangeV1;
use std::{
    ffi::OsString,
    fs::{File, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::PathBuf,
};
#[path = "formal_live_protected_full_node_storage_io.rs"]
mod io;

#[test]
#[ignore = "manual full-node maintenance capacity; real protected endpoints and native history"]
fn formal_full_node_storage_maintenance_preserves_history() {
    thread::Builder::new()
        .name("formal-full-node-storage".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            run_protected_runtime_fixture(
                false,
                None,
                false,
                None,
                false,
                None,
                FixtureCase::FullNodeStorage,
            )
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}

pub(super) struct Case {
    pub runtime: FormalLiveRuntime,
    pub runtime_config: PathBuf,
    pub root: PathBuf,
    pub endpoint_binary: OsString,
    pub primary_config: PathBuf,
    pub replica_config: PathBuf,
    pub primary_child: oracle_support::EndpointChild,
    pub replica_child: oracle_support::EndpointChild,
    pub primary_address: SocketAddr,
    pub replica_address: SocketAddr,
    pub owner_config: WorldServiceClientConfig,
    pub identity: WorldIdentity,
    pub owner_key: SigningKey,
    pub original: SubmitIntentRequest<WorldServicePayloadV1>,
    pub original_commit: oasis7_client_api::world_service::CommitRef,
}
struct Budget {
    started: Instant,
    maximum: Duration,
    progress: Option<File>,
    complete: bool,
    requested: u64,
    completed_commits: u64,
}
impl Budget {
    fn new(requested: u64) -> Result<Self, String> {
        let seconds = std::env::var("OASIS7_STORAGE_MAX_SECONDS")
            .unwrap_or_else(|_| "18000".into())
            .parse::<u64>()
            .map_err(|e| e.to_string())?;
        if !(1..=21600).contains(&seconds) {
            return Err("full-node time budget must be 1..21600 seconds".into());
        }
        let progress = std::env::var_os("OASIS7_STORAGE_FULL_NODE_PROGRESS_PATH")
            .map(|path| OpenOptions::new().create_new(true).write(true).open(path))
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            started: Instant::now(),
            maximum: Duration::from_secs(seconds),
            progress,
            complete: false,
            requested,
            completed_commits: 0,
        })
    }
    fn emit(&mut self, value: Value) -> Result<(), String> {
        eprintln!("STORAGE_FULL_NODE_PROGRESS={value}");
        if let Some(file) = &mut self.progress {
            writeln!(file, "{value}")
                .and_then(|_| file.sync_data())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn check(&mut self, stage: &str, completed: u64) -> Result<(), String> {
        if self.started.elapsed() >= self.maximum {
            return Err(format!(
                "full-node budget exhausted at {stage}; partial {completed}/{}",
                self.requested
            ));
        }
        if completed % 100 == 0 {
            self.emit(serde_json::json!({"status":"running","stage":stage,"completed":completed,"requested_commits":self.requested,"elapsed_ms":self.started.elapsed().as_millis()}))?;
        }
        Ok(())
    }
}
impl Drop for Budget {
    fn drop(&mut self) {
        if !self.complete {
            let _ = self.emit(serde_json::json!({"status":"partial","requested_commits":self.requested,"executed_commits":self.completed_commits,"elapsed_ms":self.started.elapsed().as_millis()}));
        }
    }
}
fn submit_change(case: &Case, runtime: &FormalLiveRuntime, ordinal: u64) -> Result<(), String> {
    let owner =
        RemoteWorldServiceClient::new(case.owner_config.clone()).map_err(|e| e.to_string())?;
    let current = runtime.published.current()?;
    let binding = current
        .world
        .current_cognition_runtime_binding()
        .map_err(|e| format!("full-node binding: {e:?}"))?;
    let identity = current
        .world
        .capability_revocation_state()
        .agent_identities
        .get("agent")
        .ok_or("full-node agent identity missing")?;
    let previous = current
        .world
        .capability_revocation_state()
        .agent_signer_delegations
        .get("agent");
    let change = AgentSignerDelegationChangeV1 {
        world_id: binding.world_id,
        branch_id: binding.branch_id,
        agent_id: "agent".into(),
        owner_binding: identity.owner_binding.clone(),
        agent_identity_generation: identity.generation,
        generation: previous.map_or(Ok(1), |v| {
            v.generation
                .checked_add(1)
                .ok_or("delegation generation overflow")
        })?,
        delegate_public_key: hex::encode(fixtures::key(72).verifying_key().to_bytes()),
        revoked: ordinal % 2 == 1,
        nonce: previous.map_or(Ok(1), |v| {
            v.nonce.checked_add(1).ok_or("delegation nonce overflow")
        })?,
    };
    let payload = WorldServicePayloadV1::Delegation(
        sign_read_request(
            "delegation",
            change.clone(),
            &hex::encode(case.owner_key.to_bytes()),
        )
        .map_err(|e| e.to_string())?,
    );
    let request = SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: correlation::derive_correlation(case.identity.clone(), &payload)
            .map_err(|e| e.to_string())?,
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    assert_volatile_admission(
        owner.submit(request.clone()).map_err(|e| e.to_string())?,
        &request.correlation,
    )?;
    wait_for_new_request_result(
        runtime,
        &request,
        Duration::from_secs(120),
        "full-node exact workload",
    )?;
    if runtime
        .published
        .current()?
        .world
        .capability_revocation_state()
        .agent_signer_delegations
        .get("agent")
        != Some(&change)
    {
        return Err("full-node exact delegation did not execute".into());
    }
    Ok(())
}
fn restart(case: &mut Case) -> Result<(), String> {
    let (primary, address) =
        oracle_support::start_endpoint(&case.endpoint_binary, &case.primary_config)?;
    if address != case.primary_address {
        return Err("primary restart changed address".into());
    }
    case.primary_child = primary;
    let (replica, address) =
        oracle_support::start_endpoint(&case.endpoint_binary, &case.replica_config)?;
    if address != case.replica_address {
        return Err("replica restart changed address".into());
    }
    case.replica_child = replica;
    Ok(())
}
fn stop_endpoints(case: &mut Case) -> Result<(), String> {
    case.primary_child.kill_sigkill_and_wait()?;
    case.replica_child.kill_sigkill_and_wait()
}
fn receipt(case: &Case, budget: &mut Budget) -> Result<Value, String> {
    let primary = io::logical_inventory(&mut io::open_endpoint(&case.primary_config)?, budget)?;
    let replica = io::logical_inventory(&mut io::open_endpoint(&case.replica_config)?, budget)?;
    Ok(serde_json::json!({"primary":primary,"replica":replica}))
}
fn compact(config: &Path, budget: &mut Budget) -> Result<u64, String> {
    let mut endpoint = io::open_endpoint(config)?;
    for steps in 1.. {
        budget.check("endpoint_maintenance", steps)?;
        let progress = endpoint
            .advance_physical_generation()
            .map_err(|e| e.to_string())?;
        if !progress.building {
            return Ok(steps);
        }
    }
    unreachable!()
}
pub(super) fn exercise(mut case: Case) -> Result<(), String> {
    let requested = std::env::var("OASIS7_STORAGE_FULL_NODE_COMMITS")
        .unwrap_or_else(|_| "10".into())
        .parse::<u64>()
        .map_err(|e| e.to_string())?;
    if ![10, 1000, 10000].contains(&requested) {
        return Err("full-node commits must be 10,1000,10000".into());
    }
    let mut budget = Budget::new(requested)?;
    io::fix_listen(&case.primary_config, case.primary_address)?;
    io::fix_listen(&case.replica_config, case.replica_address)?;
    stop_owned_protected_runtime(
        &mut case.runtime,
        Duration::from_secs(120),
        "full_node_initial_quiesce",
    )?;
    stop_endpoints(&mut case)?;
    io::settle_endpoint(&case.primary_config)?;
    io::settle_endpoint(&case.replica_config)?;
    let initial = io::inventory(&case.root)?;
    restart(&mut case)?;
    drop(case.runtime);
    case.runtime = FormalLiveRuntime::open_with_native_history(&case.runtime_config)?;
    case.runtime.start()?;
    let workload_started = Instant::now();
    for ordinal in 0..requested {
        budget.check("actual_formal_workload", ordinal)?;
        submit_change(&case, &case.runtime, ordinal)?;
        budget.completed_commits = ordinal + 1;
    }
    let workload_ms = workload_started.elapsed().as_millis();
    stop_owned_protected_runtime(
        &mut case.runtime,
        Duration::from_secs(120),
        "full_node_before_maintenance",
    )?;
    let current = case.runtime.published.current()?;
    let fixed_head_ref =
        serde_json::to_value(current.anchor.head().head()).map_err(|e| e.to_string())?;
    let fixed_head_position = current.anchor.head().record().position;
    let fixed_head = io::digest(&current.anchor.head().head())?;
    let authority = io::digest(current.anchor.authority().definition())?;
    let baseline_read_started = Instant::now();
    let typed_before = io::typed_history(&current.world, &mut budget)?;
    let baseline_typed_readback_ms = baseline_read_started.elapsed().as_millis();
    let original_key =
        correlation::key_digest(&case.original.correlation.key).map_err(|e| e.to_string())?;
    let original_result = retained_result_from_hot_or_cold(&current.world, &original_key)?;
    let original_effect =
        game_task::current_control(&current.world, "agent").map_err(|e| e.to_string())?;
    let original_emits = wasm_emit_count(&current.world);
    let cold_evicted = current.world.tick_consensus_record_count() > 32;
    if requested >= 1000 && !cold_evicted {
        return Err("full-node long workload did not enter cold history".into());
    }
    drop(current);
    stop_endpoints(&mut case)?;
    let logical_before = receipt(&case, &mut budget)?;
    let published_before = io::published_receipt(
        &case.root,
        case.original_commit.position,
        fixed_head_position,
        &mut budget,
    )?;
    let before = io::inventory(&case.root)?;
    restart(&mut case)?;
    let maintenance_started = Instant::now();
    let mut writer_steps = 0_u64;
    loop {
        budget.check("writer_maintenance", writer_steps)?;
        writer_steps += 1;
        if case.runtime.advance_storage_maintenance_for_test()? {
            break;
        }
    }
    // Release every runtime-owned remote job/cache before endpoint retirement.
    let runtime_config = case.runtime_config.clone();
    drop(case.runtime);
    // Remaining case fields are used individually after the runtime move.
    case.primary_child.kill_sigkill_and_wait()?;
    case.replica_child.kill_sigkill_and_wait()?;
    let primary_steps = compact(&case.primary_config, &mut budget)?;
    let replica_steps = compact(&case.replica_config, &mut budget)?;
    let logical_after = serde_json::json!({"primary":io::logical_inventory(&mut io::open_endpoint(&case.primary_config)?,&mut budget)?,"replica":io::logical_inventory(&mut io::open_endpoint(&case.replica_config)?,&mut budget)?});
    if logical_before != logical_after {
        return Err("maintenance changed original logical history bytes".into());
    }
    let published_after_maintenance = io::published_receipt(
        &case.root,
        case.original_commit.position,
        fixed_head_position,
        &mut budget,
    )?;
    if published_before != published_after_maintenance {
        return Err("maintenance changed writer published original bytes/name inventory".into());
    }
    let after_maintenance = io::inventory(&case.root)?;
    let maintenance_ms = maintenance_started.elapsed().as_millis();
    let (primary, address) =
        oracle_support::start_endpoint(&case.endpoint_binary, &case.primary_config)?;
    if address != case.primary_address {
        return Err("primary cold reopen changed address".into());
    }
    case.primary_child = primary;
    let (replica, address) =
        oracle_support::start_endpoint(&case.endpoint_binary, &case.replica_config)?;
    if address != case.replica_address {
        return Err("replica cold reopen changed address".into());
    }
    case.replica_child = replica;
    let cold_started = Instant::now();
    let mut reopened = FormalLiveRuntime::open_with_native_history(&runtime_config)?;
    let current = reopened.published.current()?;
    if io::digest(&current.anchor.head().head())? != fixed_head
        || io::digest(current.anchor.authority().definition())? != authority
    {
        return Err("maintenance/cold restore changed fixed head or authority".into());
    }
    let typed_after = io::typed_history(&current.world, &mut budget)?;
    if typed_before != typed_after {
        return Err("maintenance/cold restore changed typed history/snapshot/journal".into());
    }
    drop(current);
    // No live endpoint maintenance may race complete-directory accounting.
    case.primary_child.kill_sigkill_and_wait()?;
    case.replica_child.kill_sigkill_and_wait()?;
    io::settle_endpoint(&case.primary_config)?;
    io::settle_endpoint(&case.replica_config)?;
    let published_after_cold_restore = io::published_receipt(
        &case.root,
        case.original_commit.position,
        fixed_head_position,
        &mut budget,
    )?;
    if published_before != published_after_cold_restore {
        return Err("cold restore changed writer published original bytes/name inventory".into());
    }
    let after_cold_restore = io::inventory(&case.root)?;
    let cold_restore_ms = cold_started.elapsed().as_millis();
    let (primary, address) =
        oracle_support::start_endpoint(&case.endpoint_binary, &case.primary_config)?;
    if address != case.primary_address {
        return Err("post-measure primary restart changed address".into());
    }
    case.primary_child = primary;
    let (replica, address) =
        oracle_support::start_endpoint(&case.endpoint_binary, &case.replica_config)?;
    if address != case.replica_address {
        return Err("post-measure replica restart changed address".into());
    }
    case.replica_child = replica;
    reopened.start()?;
    let owner =
        RemoteWorldServiceClient::new(case.owner_config.clone()).map_err(|e| e.to_string())?;
    if lookup_original_commit(&owner, &case.original)? != case.original_commit {
        return Err("cold original CommitRef changed".into());
    }
    let next = reopened.published.position()? + 1;
    assert_volatile_admission(
        owner
            .submit(case.original.clone())
            .map_err(|e| e.to_string())?,
        &case.original.correlation,
    )?;
    wait_for_published_position(&reopened, next)?;
    let after = reopened.published.current()?;
    if lookup_original_commit(&owner, &case.original)? != case.original_commit
        || retained_result_from_hot_or_cold(&after.world, &original_key)? != original_result
        || game_task::current_control(&after.world, "agent").map_err(|e| e.to_string())?
            != original_effect
        || wasm_emit_count(&after.world) != original_emits
    {
        return Err("cold original retry changed result or effect".into());
    }
    drop(after);
    // Restore the runtime field solely to reuse the identical owner-signed request helper.
    case.runtime = reopened;
    submit_change(&case, &case.runtime, requested)?;
    stop_owned_protected_runtime(
        &mut case.runtime,
        Duration::from_secs(120),
        "full_node_final_stop",
    )?;
    let final_cut = case.runtime.published.current()?;
    if retained_result_from_hot_or_cold(&final_cut.world, &original_key)? != original_result
        || game_task::current_control(&final_cut.world, "agent").map_err(|e| e.to_string())?
            != original_effect
        || wasm_emit_count(&final_cut.world) != original_emits
    {
        return Err("continuation changed original unique effect/result".into());
    }
    let growth_before_maintenance = io::allocated_change(&initial, &before)?;
    let maintenance_net_change = io::allocated_change(&before, &after_maintenance)?;
    let cold_restore_net_change = io::allocated_change(&after_maintenance, &after_cold_restore)?;
    budget.check("completed", requested)?;
    let sections = [
        serde_json::json!({
            "schema_version": 1, "status": "passed", "executed_commits": requested,
            "fixed_head": fixed_head, "fixed_head_position": fixed_head_position,
            "original_commit_position": case.original_commit.position,
            "fixed_head_ref": fixed_head_ref, "authority_digest": authority,
            "scope": "same actual formal writer directories before/after maintenance; process-cold restart without OS-cache flush; external installed binaries/build caches excluded; no raw-mode comparison or S6 evidence",
        }),
        serde_json::json!({
            "growth_before_maintenance_allocated_bytes": growth_before_maintenance,
            "maintenance_net_allocated_change_bytes": maintenance_net_change,
            "cold_restore_net_allocated_change_bytes": cold_restore_net_change,
            "allocated_change_convention": "after minus before; negative means reclaimed, no positivity assumption",
            "initial": initial, "before": before,
            "after_maintenance": after_maintenance, "after_cold_restore": after_cold_restore,
            "accounting_quiescent": true,
            "accounting_quiescence": "Node stopped or not started; both owned endpoint children SIGKILLed and waited, synchronously reopened/guarded/closed to settle recovery before inventory; no OS-cache flush; restart/quiesce/recovery costs included in phase timing",
        }),
        serde_json::json!({
            "published_before": published_before,
            "published_after_maintenance": published_after_maintenance,
            "published_after_cold_restore": published_after_cold_restore,
            "published_history_unchanged": true,
            "logical_before": logical_before, "logical_after": logical_after,
            "typed_history_before": typed_before, "typed_history_after": typed_after,
            "cold_history_evicted": cold_evicted,
        }),
        serde_json::json!({
            "workload_execution_ms": workload_ms,
            "baseline_full_typed_readback_ms": baseline_typed_readback_ms,
            "maintenance_ms": maintenance_ms,
            "maintenance_timing_scope": "writer maintenance, endpoint restarts, generation convergence and complete fresh logical readback",
            "writer_maintenance_steps": writer_steps,
            "primary_generation_steps": primary_steps, "replica_generation_steps": replica_steps,
            "cold_restore_and_full_typed_readback_ms": cold_restore_ms,
            "elapsed_ms": budget.started.elapsed().as_millis(),
        }),
        serde_json::json!({
            "fixed_head_unchanged": true, "logical_history_unchanged": true,
            "typed_history_unchanged": true, "original_result_unchanged": true,
            "original_effect_unchanged": true, "continuation_completed": true,
        }),
    ];
    let mut report = serde_json::Map::new();
    for section in sections {
        let Value::Object(fields) = section else {
            unreachable!("report sections are objects")
        };
        report.extend(fields);
    }
    eprintln!("STORAGE_FULL_NODE_REPORT={}", Value::Object(report));
    budget.complete = true;
    Ok(())
}

#[test]
fn full_node_published_receipt_detects_original_mutation_and_missing_head() {
    let root = oracle_support::PrivateRoot::new().unwrap();
    let writer = root.path().join("formal-writer");
    oracle_support::private_directory(&writer).unwrap();
    let publications = writer.join("published");
    oracle_support::private_directory(&publications).unwrap();
    let original = publications.join(format!("{:020}.cbor", 1));
    let head = publications.join(format!("{:020}.cbor", 2));
    oracle_support::write_private(&original, b"original immutable writer bytes").unwrap();
    oracle_support::write_private(&head, b"fixed head writer bytes").unwrap();
    let mut budget = Budget {
        started: Instant::now(),
        maximum: Duration::from_secs(10),
        progress: None,
        complete: true,
        requested: 10,
        completed_commits: 10,
    };
    let before = io::published_receipt(root.path(), 1, 2, &mut budget).unwrap();
    std::fs::write(&original, b"changed original writer bytes").unwrap();
    let changed = io::published_receipt(root.path(), 1, 2, &mut budget).unwrap();
    assert_ne!(before["digest"], changed["digest"]);
    assert_ne!(before["original_digest"], changed["original_digest"]);
    std::fs::remove_file(head).unwrap();
    assert!(io::published_receipt(root.path(), 1, 2, &mut budget).is_err());
}
