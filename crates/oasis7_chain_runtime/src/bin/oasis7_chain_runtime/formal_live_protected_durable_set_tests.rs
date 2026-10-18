//! Actual healthy durable-set replacement through a running formal Node and
//! protected P/R endpoint children. The monotonic oracles are local test
//! simulations; these tests do not claim physical fault independence.
use super::*;
use crate::execution_bridge::formal_durable_set_arm::DurableSetArmPhaseV1;
use oasis7::world_service::AgentSignerDelegationChangeV1;
use oasis7_distfs::controlled_authority::formal::{
    DurableSetSealedEvidenceV1, HealthyEndpointLineagePageV1, HealthyEndpointLineageStepV1,
    NormalDurableSetTransitionBodyV1, SignedNormalDurableSetTransitionV1,
    VerifiedSealedDurableSetTransitionV1, normal_durable_set_transition_signing_bytes,
    transport::{
        ArchiveReadGrantBodyV1, RemoteDurableSetCandidateV1, RemoteFormalEndpoint,
        SignedArchiveReadGrantV1, archive_read_grant_signing_bytes,
    },
    verify_durable_set_activation, verify_formal_qualified, verify_normal_durable_set_transition,
    verify_sealed_durable_set_transition,
};
use std::{net::TcpStream, path::PathBuf, sync::Arc, time::Duration};

#[test]
#[ignore = "actual Node and protected endpoint processes; requires OASIS7_FORMAL_ENDPOINT_TEST_BIN"]
fn formal_protected_healthy_durable_set_rebind_and_restart() {
    run_fixture(false);
}

#[test]
#[ignore = "long actual healthy-set history; requires OASIS7_FORMAL_ENDPOINT_TEST_BIN"]
fn formal_protected_healthy_durable_set_cold_original_lookup_after_restart() {
    run_fixture(true);
}

fn run_fixture(long_history: bool) {
    let mode = if long_history {
        "healthy_set_long"
    } else {
        "healthy_set"
    };
    let started_at = Instant::now();
    // Bound the whole scenario. Authenticated successor staging shares the
    // remaining absolute budget across P/R; individual RPC and ordinary-ready
    // deadlines stay unchanged. The long case includes 40 commits and two reopens.
    let budget = if long_history {
        Duration::from_secs(20 * 60)
    } else {
        Duration::from_secs(5 * 60)
    };
    let scenario_deadline = ScenarioDeadline {
        started_at,
        expires_at: started_at + budget,
    };
    eprintln!(
        "C03 {mode} fixture started total_budget_ms={}",
        budget.as_millis()
    );
    thread::Builder::new()
        .name(format!("formal-{mode}"))
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            run_protected_runtime_case_with_deadline(
                false,
                Some(mode),
                false,
                Some(scenario_deadline),
            )
        })
        .expect("spawn healthy durable-set fixture")
        .join()
        .expect("healthy durable-set fixture thread panicked")
        .unwrap();
}

pub(super) struct HealthySetCase {
    pub(super) long_history: bool,
    pub(super) runtime: Option<FormalLiveRuntime>,
    pub(super) runtime_config_path: PathBuf,
    pub(super) writer_directory: PathBuf,
    pub(super) endpoint_binary: std::ffi::OsString,
    pub(super) root: PathBuf,
    pub(super) primary_directory: PathBuf,
    pub(super) primary_address: std::net::SocketAddr,
    pub(super) replica_address: std::net::SocketAddr,
    pub(super) primary_child: oracle_support::EndpointChild,
    pub(super) replica_child: oracle_support::EndpointChild,
    pub(super) primary_oracle: oracle_support::OracleHandle,
    pub(super) replica_oracle: oracle_support::OracleHandle,
    pub(super) primary_provision: oracle_support::durable_set_oracle::HealthyProvisionHandle,
    pub(super) oracle_authentication_file: PathBuf,
    pub(super) authority: VerifiedAuthorityV1,
    pub(super) initial_trust: InitialActivationTrustV1,
    pub(super) certificate: SignedInitialActivationV1,
    pub(super) initial_proof: FormalQualifiedCommitProofV1,
    pub(super) initial_anchor: VerifiedFormalReadAnchorV1,
    pub(super) writer_key: SigningKey,
    pub(super) owner_key: SigningKey,
    pub(super) owner_request: SubmitIntentRequest<WorldServicePayloadV1>,
    pub(super) world: WorldIdentity,
    pub(super) scenario_deadline: ScenarioDeadline,
    pub(super) public_client_config: WorldServiceClientConfig,
    pub(super) owner_client_config: WorldServiceClientConfig,
}

pub(super) fn exercise(mut case: HealthySetCase) -> Result<(), String> {
    case.scenario_deadline
        .report_stage("healthy-set exercise started")?;
    let mut runtime = case
        .runtime
        .take()
        .ok_or("healthy fixture runtime was already consumed")?;
    if runtime
        .active_authority()
        .context_digest()
        .map_err(|e| e.to_string())?
        != case.authority.context_digest().map_err(|e| e.to_string())?
    {
        return Err("healthy set must begin from the original configured C0 authority".into());
    }
    let new_replica_directory = case.root.join("healthy-successor-replica");
    oracle_support::private_directory(&new_replica_directory)?;
    case.scenario_deadline
        .report_stage("before exact transition arm")?;
    let (start_cut, transition, armed) = {
        // The public exact-arm wrapper locks only after its caller constructed
        // the candidate. A running Node may already have an original continuation
        // pending then. Wait for that original under the existing scenario budget,
        // and keep one gate across the first cut, signature and guarded core arm.
        let _admission = crate::formal_live_runtime::quiesce::acquire_after_pending_clear(
            &runtime.admission_gate,
            || runtime.durable_set_arm.pending_open(),
            case.scenario_deadline.expires_at,
        )?;
        if !runtime.started
            || !runtime
                .state
                .ready
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err("formal runtime is not ready to admit a durable-set arm".into());
        }
        let start_cut = runtime.published.current()?;
        let transition = signed_transition(
            &case.authority,
            &start_cut.anchor,
            &case.primary_directory,
            &new_replica_directory,
        )?;
        if start_cut.anchor.head().record().position + 1 != transition.body.boundary_position {
            return Err("healthy arm predecessor differs from the actual published cut".into());
        }
        let armed = crate::formal_live_runtime::quiesce::arm_before_deadline(
            case.scenario_deadline.expires_at,
            || {
                runtime.durable_set_arm.arm(
                    &transition.body.operation_id,
                    transition.clone(),
                    &start_cut.anchor,
                )
            },
        )?;
        (start_cut, transition, armed)
    }; // Release admission before any public request or boundary wait.
    let boundary_position = transition.body.boundary_position;
    let operation_id = transition.body.operation_id.clone();
    let transition_digest = transition.body.digest().map_err(|e| e.to_string())?;
    if armed.phase != DurableSetArmPhaseV1::Armed
        || armed.operation_id != operation_id
        || armed.boundary_position != boundary_position
        || armed.transition_digest != transition_digest
        || operation_id != fixtures::h(124)
        || boundary_position == 0
    {
        return Err("formal runtime did not persist the exact healthy transition arm".into());
    }

    let owner = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|error| error.to_string())?;
    case.scenario_deadline
        .report_stage("before boundary submit and qualification")?;
    if case.long_history {
        // Exercise the already-qualified-at-first-query path deterministically
        // in the long-history/cold-lookup scenario. The short case retains the
        // immediate Armed/pre-boundary admission path.
        wait_for_boundary(&runtime, boundary_position)?;
    }
    let initial_arm_status = runtime
        .query_durable_set_transition(&operation_id, boundary_position)?
        .ok_or("healthy transition arm was not persisted")?;
    if initial_arm_status.operation_id != operation_id
        || initial_arm_status.boundary_position != boundary_position
        || initial_arm_status.transition_digest != transition_digest
    {
        return Err("healthy transition query differs from the prepared operation".into());
    }
    if case.long_history && initial_arm_status.phase != DurableSetArmPhaseV1::BoundaryQualified {
        return Err(
            "long healthy-set scenario did not observe its exact qualified boundary".into(),
        );
    }
    if initial_arm_status.phase == DurableSetArmPhaseV1::Armed {
        // Preserve the pre-boundary admission when the exact current anchor
        // still admits it. The node may qualify the boundary while this HTTP
        // request is in flight, which is why the boundary checks below use
        // fixed verified anchors rather than another racy current-head read.
        assert_volatile_admission(
            owner
                .submit(case.owner_request.clone())
                .map_err(|error| format!("submit at healthy boundary: {error}"))?,
            &case.owner_request.correlation,
        )?;
        case.scenario_deadline
            .report_stage("original request admitted before healthy boundary")?;
    } else if initial_arm_status.phase != DurableSetArmPhaseV1::BoundaryQualified {
        return Err(format!(
            "healthy transition has an unexpected pre-boundary phase: {:?}",
            initial_arm_status.phase
        ));
    } else {
        case.scenario_deadline
            .report_stage("pre-boundary admission deferred for exact boundary proof")?;
    }
    wait_for_boundary(&runtime, boundary_position)?;
    let boundary_cut = runtime.published.current()?;
    if boundary_cut.anchor.head().record().position != boundary_position
        || boundary_cut
            .anchor
            .authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != case.authority.context_digest().map_err(|e| e.to_string())?
    {
        return Err("healthy boundary is not the exact old-authority Qualified cut".into());
    }
    let parent_anchor = start_cut.anchor.clone();
    let parent = parent_anchor.head().clone();
    if transition.body.predecessor != parent.head() {
        return Err("healthy transition predecessor differs from its prepared anchor".into());
    }
    let (boundary_candidate, boundary_proof) =
        crate::execution_bridge::formal_writer::read_published_candidate_at(
            &case.writer_directory,
            boundary_position,
        )?
        .ok_or("healthy boundary candidate/proof is missing")?;
    if boundary_candidate.candidate.record != boundary_proof.decision.record {
        return Err("healthy boundary candidate differs from its qualified proof".into());
    }
    verify_record_material(
        &boundary_proof.decision.record,
        &boundary_candidate.material,
    )
    .map_err(|error| format!("verify healthy boundary material: {error}"))?;
    let boundary =
        verify_formal_qualified(&boundary_proof, &case.authority, Some(&parent), None)
            .map_err(|error| format!("verify exact healthy boundary qualification: {error}"))?;
    let boundary_anchor = parent_anchor
        .clone()
        .after_verified_commit(case.authority.clone(), boundary.clone())
        .map_err(|error| format!("advance exact healthy boundary anchor: {error}"))?;
    let arm_status = runtime
        .durable_set_arm
        .query(&operation_id, boundary_position, &boundary_anchor)?
        .ok_or("healthy arm disappeared at the exact boundary")?;
    if arm_status.phase != DurableSetArmPhaseV1::BoundaryQualified
        || arm_status.operation_id != operation_id
        || arm_status.boundary_position != boundary_position
        || arm_status.transition_digest != transition_digest
    {
        return Err(format!(
            "healthy boundary arm state mismatch: {arm_status:?}"
        ));
    }
    let live_after_boundary = runtime.published.current()?;
    if live_after_boundary.anchor.head().record().position != boundary_position
        || live_after_boundary.anchor.head().head() != boundary_anchor.head().head()
        || live_after_boundary
            .anchor
            .authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != case.authority.context_digest().map_err(|e| e.to_string())?
    {
        return Err("healthy durable-set arm did not hold the exact qualified boundary".into());
    }
    case.scenario_deadline
        .report_stage("exact old-authority boundary qualified")?;

    if boundary.head() != boundary_anchor.head().head()
        || boundary_proof
            .decision
            .record
            .durable_set_transition_digest
            .as_deref()
            != Some(transition_digest.as_str())
    {
        return Err("healthy candidate differs from the armed transition boundary".into());
    }
    let _verified_transition =
        verify_normal_durable_set_transition(&transition, &case.authority, &parent, &boundary)
            .map_err(|error| format!("verify exact signed healthy transition: {error}"))?;

    let lineage_history = runtime.history.history_page(
        &case.initial_anchor,
        &boundary_anchor,
        oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS,
    )?;
    let lineage_steps = lineage_history
        .steps
        .into_iter()
        .map(|step| match step {
            oasis7::world_service::formal_read::FormalReadStepV1::Commit(proof) => {
                Ok(HealthyEndpointLineageStepV1::Commit(proof))
            }
            oasis7::world_service::formal_read::FormalReadStepV1::Recovery(_) => {
                Err("healthy endpoint lineage unexpectedly contains recovery".to_owned())
            }
            oasis7::world_service::formal_read::FormalReadStepV1::DurableSetActivation(_) => {
                Err("first healthy endpoint replacement unexpectedly follows another set".into())
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    if lineage_steps
        .last()
        .is_none_or(|step| !matches!(step, HealthyEndpointLineageStepV1::Commit(proof) if proof.decision.record == *boundary.record()))
    {
        return Err("healthy signed lineage does not terminate at the exact boundary".into());
    }
    let lineage_directory = case.root.join("healthy-lineage");
    oracle_support::private_directory(&lineage_directory)?;
    let lineage_page = HealthyEndpointLineagePageV1 {
        schema_version: 1,
        from: case.initial_anchor.head().head(),
        steps: lineage_steps,
        to: boundary.head(),
    };
    oracle_support::write_private(
        &lineage_directory.join("00000000000000000000.cbor"),
        &serde_cbor::to_vec(&lineage_page).map_err(|error| error.to_string())?,
    )?;

    case.scenario_deadline
        .report_stage("signed C0-to-boundary lineage persisted")?;
    case.scenario_deadline
        .report_stage("stopping old runtime at sealed boundary")?;
    stop_healthy_runtime(&mut runtime, case.scenario_deadline)?;
    let stopped_snapshot = runtime.published.current()?.world.snapshot();
    drop(runtime);
    case.scenario_deadline.report_stage("old runtime stopped")?;

    let mut old_primary = RemoteFormalEndpoint::new(
        case.primary_address,
        Duration::from_secs(5),
        case.authority.clone(),
        EndpointRole::Primary,
        case.writer_key.clone(),
        case.authority
            .definition()
            .world
            .genesis_head()
            .map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut old_replica = RemoteFormalEndpoint::new(
        case.replica_address,
        Duration::from_secs(5),
        case.authority.clone(),
        EndpointRole::Replica,
        case.writer_key.clone(),
        case.authority
            .definition()
            .world
            .genesis_head()
            .map_err(|e| e.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let primary_checkpoint = old_primary
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let replica_checkpoint = old_replica
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let primary_seal = old_primary
        .seal_durable_set(transition.clone(), primary_checkpoint)
        .map_err(|error| format!("seal original P at H: {error}"))?;
    let replica_seal = old_replica
        .seal_durable_set(transition.clone(), replica_checkpoint)
        .map_err(|error| format!("seal original R at H: {error}"))?;
    if old_primary
        .query_durable_set(transition.clone())
        .map_err(|error| error.to_string())?
        .is_none_or(|receipt| receipt.body != primary_seal.body)
        || old_replica
            .query_durable_set(transition.clone())
            .map_err(|error| error.to_string())?
            .is_none_or(|receipt| receipt.body != replica_seal.body)
    {
        return Err("old endpoints did not read back the exact immutable H seals".into());
    }
    let sealed_evidence = DurableSetSealedEvidenceV1 {
        transition: transition.clone(),
        primary_seal: primary_seal.clone(),
        replica_seal: replica_seal.clone(),
    };
    let sealed = Arc::new(
        verify_sealed_durable_set_transition(&sealed_evidence, &case.authority, &parent, &boundary)
            .map_err(|error| format!("verify dual protected seals at H: {error}"))?,
    );
    case.primary_provision.provision(sealed.clone())?;
    case.scenario_deadline
        .report_stage("both original P/R seals verified")?;

    let resource = case
        .authority
        .policy()
        .authorized_fencing_resource_bindings
        .first()
        .cloned()
        .ok_or("healthy test authority has no authorized fencing resource")?;
    let successor_replica_oracle =
        oracle_support::durable_set_oracle::start_healthy_replica_oracle(
            sealed.clone(),
            new_replica_directory.clone(),
            resource,
        )?;
    let archive_read_grant = archive_grant(&case, &sealed)?;

    // P must release its exclusive store before the same local instance is
    // reopened under the sealed healthy candidate process. Old R remains live
    // only while the new R independently copies its authenticated closure.
    case.primary_child.kill_sigkill_and_wait()?;
    case.scenario_deadline
        .report_stage("original P stopped; starting authenticated successor copy")?;
    let primary_management = free_loopback_address()?;
    let replica_management = free_loopback_address()?;
    let primary_bootstrap = serde_json::json!({
        "schema_version": 1,
        "lineage_directory": lineage_directory,
        "sealed": sealed.evidence(),
        "management_listen": primary_management,
        "source_address": null,
        "source_role": null,
        "archive_read_grant": null,
        "operation_byte_budget": 64 * 1024 * 1024,
    });
    let replica_bootstrap = serde_json::json!({
        "schema_version": 1,
        "lineage_directory": lineage_directory,
        "sealed": sealed.evidence(),
        "management_listen": replica_management,
        "source_address": case.replica_address,
        "source_role": EndpointRole::Replica,
        "archive_read_grant": archive_read_grant,
        "operation_byte_budget": 64 * 1024 * 1024,
    });
    let primary_healthy_config = case.root.join("healthy-primary-endpoint.json");
    let replica_healthy_config = case.root.join("healthy-replica-endpoint.json");
    write_healthy_endpoint_config(
        &primary_healthy_config,
        &case,
        &case.primary_directory,
        case.primary_oracle.address(),
        EndpointRole::Primary,
        case.primary_address,
        &primary_bootstrap,
    )?;
    write_healthy_endpoint_config(
        &replica_healthy_config,
        &case,
        &new_replica_directory,
        successor_replica_oracle.address(),
        EndpointRole::Replica,
        case.replica_address,
        &replica_bootstrap,
    )?;
    let mut new_primary =
        ManagedEndpointChild::start(&case.endpoint_binary, &primary_healthy_config)?;
    let mut new_replica =
        ManagedEndpointChild::start(&case.endpoint_binary, &replica_healthy_config)?;
    case.scenario_deadline
        .report_stage("successor P/R management children started")?;
    new_primary
        .wait_management(primary_management, case.scenario_deadline)
        .map_err(|error| {
            format!(
                "healthy primary management readiness pid={} config={}: {error}",
                new_primary.id(),
                primary_healthy_config.display()
            )
        })?;
    new_replica
        .wait_management(replica_management, case.scenario_deadline)
        .map_err(|error| {
            format!(
                "healthy replica management readiness pid={} config={}: {error}",
                new_replica.id(),
                replica_healthy_config.display()
            )
        })?;
    case.scenario_deadline
        .report_stage("successor P/R report Awaiting management")?;

    let mut primary_candidate = RemoteDurableSetCandidateV1::new(
        primary_management,
        Duration::from_secs(5),
        sealed.clone(),
        EndpointRole::Primary,
        case.writer_key.clone(),
    )
    .map_err(|error| error.to_string())?;
    let mut replica_candidate = RemoteDurableSetCandidateV1::new(
        replica_management,
        Duration::from_secs(5),
        sealed.clone(),
        EndpointRole::Replica,
        case.writer_key.clone(),
    )
    .map_err(|error| error.to_string())?;
    case.scenario_deadline
        .report_stage("before dual successor install")?;
    let primary_install = primary_candidate
        .install()
        .map_err(|error| format!("install healthy P candidate: {error}"))?;
    if primary_candidate
        .query_install()
        .map_err(|error| format!("query healthy P install: {error}"))?
        != primary_install
    {
        return Err("healthy P install query changed immutable receipt".into());
    }
    let replica_install = replica_candidate
        .install()
        .map_err(|error| format!("install copied healthy R candidate: {error}"))?;
    if replica_candidate
        .query_install()
        .map_err(|error| format!("query healthy R install: {error}"))?
        != replica_install
    {
        return Err("healthy R install query changed immutable receipt".into());
    }
    if primary_install.body.independent_inventory_digest
        != replica_install.body.independent_inventory_digest
    {
        return Err("healthy P/R independent copied inventories differ".into());
    }
    case.scenario_deadline
        .report_stage("both successor installs read back")?;
    let activation = verify_durable_set_activation(
        sealed.transition(),
        [&primary_seal, &replica_seal],
        [&primary_install, &replica_install],
    )
    .map_err(|error| format!("verify dual healthy install receipts: {error}"))?;
    case.scenario_deadline
        .report_stage("dual successor installs verified")?;

    // The source P and R are fenced and stopped before ordinary stable ports
    // become usable. The new R has already completed its authenticated copy.
    case.replica_child.kill_sigkill_and_wait()?;
    if TcpStream::connect_timeout(&case.primary_address, Duration::from_millis(100)).is_ok()
        || TcpStream::connect_timeout(&case.replica_address, Duration::from_millis(100)).is_ok()
    {
        return Err("ordinary P/R port became ready before dual activation".into());
    }
    if primary_candidate
        .activate(primary_install.clone(), replica_install.clone())
        .map_err(|error| format!("activate successor P from dual receipts: {error}"))?
        != primary_install
        || replica_candidate
            .activate(primary_install.clone(), replica_install.clone())
            .map_err(|error| format!("activate successor R from dual receipts: {error}"))?
            != replica_install
    {
        return Err("healthy activation changed an immutable install receipt".into());
    }
    case.scenario_deadline
        .report_stage("dual activation accepted; waiting for stable ports")?;
    new_primary.wait_ordinary(case.primary_address, Duration::from_secs(20))?;
    new_replica.wait_ordinary(case.replica_address, Duration::from_secs(20))?;
    case.scenario_deadline
        .report_stage("successor P/R activated on stable ports")?;

    let mut primary_readback = RemoteFormalEndpoint::new(
        case.primary_address,
        Duration::from_secs(20),
        activation.successor().clone(),
        EndpointRole::Primary,
        case.writer_key.clone(),
        boundary.head(),
    )
    .map_err(|error| error.to_string())?;
    let mut replica_readback = RemoteFormalEndpoint::new(
        case.replica_address,
        Duration::from_secs(20),
        activation.successor().clone(),
        EndpointRole::Replica,
        case.writer_key.clone(),
        boundary.head(),
    )
    .map_err(|error| error.to_string())?;
    case.scenario_deadline
        .report_stage("before dual protected native H restore")?;
    let primary_restored = primary_readback
        .restore_native_current_with_durable_set(&case.authority, &activation, None)
        .map_err(|error| format!("successor primary exact H restore: {error}"))?;
    let replica_restored = replica_readback
        .restore_native_current_with_durable_set(&case.authority, &activation, None)
        .map_err(|error| format!("successor replica exact H restore: {error}"))?;
    if primary_restored.commit().record() != boundary.record()
        || replica_restored.commit().record() != boundary.record()
        || primary_restored.proof() != replica_restored.proof()
        || primary_restored
            .active_authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != activation
                .successor()
                .context_digest()
                .map_err(|e| e.to_string())?
        || replica_restored
            .active_authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != activation
                .successor()
                .context_digest()
                .map_err(|e| e.to_string())?
    {
        return Err("dual protected native readback differs from the exact activated H cut".into());
    }
    case.scenario_deadline
        .report_stage("dual native H readback verified")?;

    let activation_step = oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1 {
        transition: transition.clone(),
        old_seals: [primary_seal, replica_seal],
        new_installs: [primary_install, replica_install],
    };
    crate::formal_live_history::persist_durable_set_activation_marker(
        &case.writer_directory,
        &activation_step,
        &activation,
    )?;
    drop((old_primary, old_replica));
    case.scenario_deadline
        .report_stage("healthy activation marker durably persisted")?;

    let mut runtime = FormalLiveRuntime::open_with_native_history(&case.runtime_config_path)?;
    if runtime
        .active_authority()
        .context_digest()
        .map_err(|e| e.to_string())?
        != activation
            .successor()
            .context_digest()
            .map_err(|e| e.to_string())?
        || runtime.published.current()?.anchor.head().record() != boundary.record()
        || runtime.published.current()?.world.snapshot() != stopped_snapshot
    {
        return Err(
            "C0 startup replay did not restore the exact healthy H snapshot/authority".into(),
        );
    }
    runtime.start()?;
    case.scenario_deadline
        .report_stage("runtime reopened at exact activated H")?;
    let owner = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|error| error.to_string())?;
    assert_volatile_admission(
        owner.submit(case.owner_request.clone()).map_err(|error| {
            format!("submit/reconcile original request after healthy set: {error}")
        })?,
        &case.owner_request.correlation,
    )?;
    let original_result = wait_for_request_result(&runtime, &case.owner_request)?;
    let original_commit = lookup_original_commit(&owner, &case.owner_request)?;
    if original_commit.position < boundary_position {
        return Err(
            "post-activation original lookup does not match its first canonical result".into(),
        );
    }
    wait_for_published_position(&runtime, boundary_position + 1)?;
    let first_successor = runtime.published.current()?;
    if first_successor.anchor.head().record().position <= boundary_position
        || first_successor
            .anchor
            .authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != activation
                .successor()
                .context_digest()
                .map_err(|e| e.to_string())?
    {
        return Err("successor did not qualify a Normal commit after sealed H".into());
    }
    let original_key = correlation::key_digest(&case.owner_request.correlation.key)
        .map_err(|error| error.to_string())?;
    let original_game_task = game_task::current_control(&first_successor.world, "agent")
        .map_err(|error| error.to_string())?
        .ok_or("original Game task was not present after healthy activation")?;
    if retained_result_from_hot_or_cold(&first_successor.world, &original_key)? != original_result
        || lookup_original_commit(&owner, &case.owner_request)? != original_commit
    {
        return Err("original result or CommitRef changed across first successor execution".into());
    }
    case.scenario_deadline
        .report_stage("first successor Normal and original dedup verified")?;

    if case.long_history {
        runtime = exercise_successor_long_history(
            runtime,
            &case,
            &original_result,
            &original_commit,
            &original_key,
            &original_game_task,
        )?;
        case.scenario_deadline
            .report_stage("long successor history and cold lookup verified")?;
    }

    if runtime.started {
        stop_healthy_runtime(&mut runtime, case.scenario_deadline)?;
    }
    let stopped = runtime.published.current()?;
    let stopped_snapshot = stopped.world.snapshot();
    drop(runtime);
    let mut reopened = FormalLiveRuntime::open_with_native_history(&case.runtime_config_path)?;
    let before_start = reopened.published.current()?;
    if before_start.world.snapshot() != stopped_snapshot
        || before_start.anchor.head().record() != stopped.anchor.head().record()
        || before_start
            .anchor
            .authority()
            .context_digest()
            .map_err(|e| e.to_string())?
            != activation
                .successor()
                .context_digest()
                .map_err(|e| e.to_string())?
    {
        return Err("second C0-based restart changed the exact healthy qualified cut".into());
    }
    reopened.start()?;
    let reopened_owner = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|error| error.to_string())?;
    if lookup_original_commit(&reopened_owner, &case.owner_request)? != original_commit {
        return Err("old request lookup changed after healthy-set process restart".into());
    }
    let reopened_cut = reopened.published.current()?;
    let public = RemoteWorldServiceClient::new(case.public_client_config.clone())
        .map_err(|error| error.to_string())?;
    let signed_view = read_current(&public, &case.world)
        .map_err(|error| format!("post-activation public signed view: {error}"))?;
    if signed_view
        .formal_read_anchor()
        .ok_or("successor public view lacks an opaque formal anchor")?
        .authority()
        .context_digest()
        .map_err(|error| error.to_string())?
        != activation
            .successor()
            .context_digest()
            .map_err(|error| error.to_string())?
    {
        return Err(
            "public signed read did not derive the active healthy successor authority".into(),
        );
    }
    if retained_result_from_hot_or_cold(&reopened_cut.world, &original_key)? != original_result {
        return Err("old canonical request body changed after healthy-set process restart".into());
    }
    case.scenario_deadline
        .report_stage("second restart and historical lookup verified")?;
    if case.long_history
        && reopened_cut
            .world
            .capability_revocation_state()
            .world_service_results
            .contains_key(&original_key)
    {
        return Err("long healthy-set original unexpectedly returned to the hot result map".into());
    }
    assert_volatile_admission(
        reopened_owner
            .submit(case.owner_request.clone())
            .map_err(|error| format!("resubmit old request after cold restore: {error}"))?,
        &case.owner_request.correlation,
    )?;
    case.scenario_deadline
        .report_stage("before cold original retry after restart")?;
    let retry_position = reopened_cut.anchor.head().record().position + 1;
    if case.long_history {
        let now = Instant::now();
        let remaining = case
            .scenario_deadline
            .expires_at
            .checked_duration_since(now)
            .filter(|remaining| !remaining.is_zero())
            .ok_or("cold original retry exceeded the original scenario deadline")?;
        wait_for_published_position_until(
            &reopened,
            retry_position,
            now + remaining.min(LONG_HISTORY_REQUEST_WAIT),
        )?;
    } else {
        wait_for_published_position(&reopened, retry_position)?;
    }
    let after_duplicate = reopened.published.current()?;
    let duplicate_commit = lookup_original_commit(&reopened_owner, &case.owner_request)?;
    // Complete the necessary signed HTTP read before closing ingress, then
    // inspect the immutable captured cut with the Node already stopping/stopped.
    stop_healthy_runtime(&mut reopened, case.scenario_deadline)?;
    if retained_result_from_hot_or_cold(&after_duplicate.world, &original_key)? != original_result
        || game_task::current_control(&after_duplicate.world, "agent")
            .map_err(|error| error.to_string())?
            != Some(original_game_task)
        || duplicate_commit != original_commit
    {
        return Err("cold original retry created a second Game task effect or result".into());
    }
    case.scenario_deadline
        .report_stage("cold retry preserved original CommitRef and effects")?;
    drop((
        case.primary_oracle,
        case.replica_oracle,
        successor_replica_oracle,
        primary_readback,
        replica_readback,
    ));
    Ok(())
}

pub(super) fn signed_transition(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalReadAnchorV1,
    primary_directory: &Path,
    new_replica_directory: &Path,
) -> Result<SignedNormalDurableSetTransitionV1, String> {
    let mut successor = authority.definition().clone();
    let epoch = successor
        .epoch
        .checked_add(1)
        .ok_or("authority epoch overflow")?;
    let boundary_position = parent
        .head()
        .record()
        .position
        .checked_add(1)
        .ok_or("durable-set boundary overflow")?;
    successor.epoch = epoch;
    successor.valid_from_position = boundary_position
        .checked_add(1)
        .ok_or("successor valid position overflow")?;
    successor.predecessor_authority_id = Some(
        authority
            .authority_id()
            .map_err(|error| error.to_string())?,
    );
    successor.primary.instance.authority_epoch = epoch;
    successor.primary.instance.storage_fingerprint =
        directory_fingerprint(primary_directory).map_err(|error| error.to_string())?;
    successor.replica.instance.authority_epoch = epoch;
    successor.replica.instance.endpoint_public_key = oracle_support::public(12);
    successor.replica.instance.instance_id = fixtures::h(122);
    successor.replica.instance.storage_id = fixtures::h(123);
    successor.replica.instance.storage_fingerprint =
        directory_fingerprint(new_replica_directory).map_err(|error| error.to_string())?;
    successor.replica.instance.endpoint_id = "healthy-successor-replica".into();
    successor.replica.fault_domain_id = "healthy-successor-replica-domain".into();
    let mut body = NormalDurableSetTransitionBodyV1 {
        schema_version: 1,
        world: successor.world.clone(),
        predecessor_authority_id: authority
            .authority_id()
            .map_err(|error| error.to_string())?,
        predecessor: parent.head().head(),
        boundary_position,
        operation_id: fixtures::h(124),
        successor,
        independent_closure_root: fixtures::h(125),
    };
    body.independent_closure_root = body
        .copy_contract_digest()
        .map_err(|error| error.to_string())?;
    Ok(SignedNormalDurableSetTransitionV1 {
        writer_signature_hex: fixtures::sign(
            normal_durable_set_transition_signing_bytes(&body)
                .map_err(|error| error.to_string())?,
            4,
        ),
        body,
    })
}

pub(super) fn archive_grant(
    case: &HealthySetCase,
    sealed: &VerifiedSealedDurableSetTransitionV1,
) -> Result<SignedArchiveReadGrantV1, String> {
    let body = ArchiveReadGrantBodyV1 {
        schema_version: 1,
        world: case.authority.definition().world.clone(),
        old_authority_id: case
            .authority
            .authority_id()
            .map_err(|error| error.to_string())?,
        source_instance_digest: case
            .authority
            .definition()
            .replica
            .instance
            .digest()
            .map_err(|error| error.to_string())?,
        transition_digest: sealed.transition().digest().into(),
        sealed_cut: sealed.transition().boundary().head(),
        next_epoch_floor: sealed.transition().body().successor.epoch,
        target_instance_digest: sealed
            .transition()
            .body()
            .successor
            .replica
            .instance
            .digest()
            .map_err(|error| error.to_string())?,
        reader_public_key: oracle_support::public(17),
    };
    Ok(SignedArchiveReadGrantV1 {
        writer_signature_hex: fixtures::sign(
            archive_read_grant_signing_bytes(&body).map_err(|error| error.to_string())?,
            4,
        ),
        body,
    })
}

pub(super) fn write_healthy_endpoint_config(
    path: &Path,
    case: &HealthySetCase,
    data_directory: &Path,
    oracle_address: std::net::SocketAddr,
    role: EndpointRole,
    listen: std::net::SocketAddr,
    bootstrap: &Value,
) -> Result<(), String> {
    let config = serde_json::json!({
        "schema_version": 2,
        "initial_trust": case.initial_trust,
        "initial_certificate": case.certificate,
        "initial_qualified": case.initial_proof,
        "recovery_bootstrap": null,
        "healthy_bootstrap": bootstrap,
        "role": role,
        "data_dir": data_directory,
        "oracle_authentication_key_file": case.oracle_authentication_file,
        "oracle_address": oracle_address,
        "listen": listen,
        "timeout_ms": 5000,
    });
    oracle_support::write_private(
        path,
        &serde_json::to_vec(&config).map_err(|error| error.to_string())?,
    )
}

fn exercise_successor_long_history(
    mut runtime: FormalLiveRuntime,
    case: &HealthySetCase,
    original_result: &oasis7::world_service::CanonicalIntentResultV1,
    original_commit: &oasis7_client_api::world_service::CommitRef,
    original_key: &str,
    original_game_task: &oasis7_agent_api::game::GameTaskControlV1,
) -> Result<FormalLiveRuntime, String> {
    let expected_authority = runtime
        .active_authority()
        .context_digest()
        .map_err(|e| e.to_string())?;
    let owner = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|error| error.to_string())?;
    for ordinal in 0..40_u64 {
        let request_started = Instant::now();
        case.scenario_deadline.report_stage(&format!(
            "successor history submitting request {}/40",
            ordinal + 1
        ))?;
        let cut = runtime.published.current()?;
        let binding = cut
            .world
            .current_cognition_runtime_binding()
            .map_err(|error| format!("successor long-history runtime binding: {error:?}"))?;
        let identity = cut
            .world
            .capability_revocation_state()
            .agent_identities
            .get("agent")
            .ok_or("healthy successor lost the canonical agent identity")?;
        let previous = cut
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
            generation: previous.map_or(Ok(1), |value| {
                value
                    .generation
                    .checked_add(1)
                    .ok_or("delegation generation overflow")
            })?,
            delegate_public_key: oracle_support::public(72),
            revoked: ordinal % 2 == 1,
            nonce: previous.map_or(Ok(1), |value| {
                value
                    .nonce
                    .checked_add(1)
                    .ok_or("delegation nonce overflow")
            })?,
        };
        let payload = WorldServicePayloadV1::Delegation(
            sign_read_request(
                "delegation",
                change.clone(),
                &hex::encode(case.owner_key.to_bytes()),
            )
            .map_err(|error| error.to_string())?,
        );
        let request = SubmitIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation: correlation::derive_correlation(case.world.clone(), &payload)
                .map_err(|error| error.to_string())?,
            deadline_unix_ms: None,
            signed_payload: payload,
        };
        assert_volatile_admission(
            owner
                .submit(request.clone())
                .map_err(|error| format!("healthy successor delegation {ordinal}: {error}"))?,
            &request.correlation,
        )?;
        // Admission may race a Tick already in formal qualification. Observe
        // that Tick and this request's Tick using the existing long-history
        // limit, without extending the scenario's original absolute expiry.
        let remaining = case
            .scenario_deadline
            .expires_at
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("healthy successor request exceeded the original scenario deadline")?;
        let result = wait_for_new_request_result(
            &runtime,
            &request,
            remaining.min(LONG_HISTORY_REQUEST_WAIT),
            "healthy successor",
        )
        .map_err(|error| format!("healthy successor change {ordinal}: {error}"))?;
        let after = runtime.published.current()?;
        if after
            .world
            .capability_revocation_state()
            .agent_signer_delegations
            .get("agent")
            != Some(&change)
            || result.request != request
        {
            return Err(format!(
                "healthy successor delegation {ordinal} did not produce its exact canonical state"
            ));
        }
        eprintln!(
            "formal_healthy_history stage=request ordinal={} stage_ms={} position={}",
            ordinal + 1,
            request_started.elapsed().as_millis(),
            runtime.published.position()?
        );
        if ordinal == 15 || ordinal == 31 {
            stop_healthy_runtime(&mut runtime, case.scenario_deadline)?;
            let before = runtime.published.current()?;
            let snapshot = before.world.snapshot();
            drop(runtime);
            let reopen_started = Instant::now();
            runtime = FormalLiveRuntime::open_with_native_history(&case.runtime_config_path)
                .map_err(|error| format!("healthy long-history reopen {ordinal}: {error}"))?;
            if runtime.published.current()?.world.snapshot() != snapshot
                || runtime
                    .active_authority()
                    .context_digest()
                    .map_err(|e| e.to_string())?
                    != expected_authority
            {
                return Err("healthy long-history reopen changed qualified successor state".into());
            }
            runtime.start()?;
            eprintln!(
                "formal_healthy_history stage=reopen completed={} stage_ms={}",
                ordinal + 1,
                reopen_started.elapsed().as_millis()
            );
        }
        case.scenario_deadline.report_stage(&format!(
            "successor history qualified {}/40 distinct requests",
            ordinal + 1
        ))?;
    }
    let cut = runtime.published.current()?;
    if cut.anchor.head().record().position <= 32 {
        return Err(
            "healthy successor did not advance beyond the retained hot result window".into(),
        );
    }
    if cut
        .world
        .capability_revocation_state()
        .world_service_results
        .contains_key(original_key)
    {
        return Err("long healthy set did not evict original result from the hot map".into());
    }
    let owner = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|error| error.to_string())?;
    if lookup_original_commit(&owner, &case.owner_request)? != *original_commit {
        return Err("signed lookup changed old CommitRef after healthy successor history".into());
    }
    stop_healthy_runtime(&mut runtime, case.scenario_deadline)?;
    let cold_started = Instant::now();
    if retained_result_from_hot_or_cold(&cut.world, original_key)? != *original_result
        || game_task::current_control(&cut.world, "agent").map_err(|e| e.to_string())?
            != Some(original_game_task.clone())
    {
        return Err("cold healthy-set lookup changed original result or Game task".into());
    }
    eprintln!(
        "formal_healthy_history stage=stopped_cold_history stage_ms={}",
        cold_started.elapsed().as_millis()
    );
    Ok(runtime)
}

#[path = "formal_live_protected_durable_set_tests/runtime_wait.rs"]
mod runtime_wait;
pub(super) use runtime_wait::stop_healthy_runtime;
use runtime_wait::{wait_for_boundary, wait_for_request_result};

#[path = "formal_live_protected_durable_set_tests/managed_endpoint.rs"]
mod managed_endpoint;
pub(super) use managed_endpoint::ManagedEndpointChild;
