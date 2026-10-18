//! C06: a failed protected replica pauses new qualification without hiding the
//! last exact committed read. The endpoint and Node are real child/runtime
//! processes; the monotonic oracle remains a test-only simulation.
use super::*;
use oasis7::world_service::{
    authority::{request_digest, verify_service_response},
    formal_read_service::FormalReadPublication,
    wire::SignedServiceResponse,
};
use oasis7_client_api::world_service::FormalReadAnchorRefV1;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
};

const CHAIN_STATUS_PATH: &str = "/v1/chain/status";

#[test]
fn availability_recovery_queues_original_before_releasing_published_cut() {
    use crate::formal_live_service::test_control::{BoundaryControl, Point};
    use oasis7_node::{NodeConsensusAction, NodeExecutionCommitContext};
    use std::sync::mpsc;
    let control = BoundaryControl::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    control
        .arm(Point::Published, Some(4), None, deadline)
        .unwrap();
    let (queued, incoming) = mpsc::channel::<Vec<u8>>();
    let (done, completion) = mpsc::channel();
    let shared = control.shared();
    let worker = thread::spawn(move || {
        shared.published(4).unwrap();
        let payload = incoming.recv_timeout(Duration::from_secs(5)).unwrap();
        let context = NodeExecutionCommitContext {
            world_id: "recovery-world".into(),
            node_id: "owner".into(),
            proposer_id: "owner".into(),
            height: 6,
            slot: 6,
            epoch: 0,
            node_block_hash: "b".repeat(64),
            action_root: "c".repeat(64),
            committed_actions: vec![
                NodeConsensusAction::from_payload(1, "owner", payload).unwrap(),
            ],
            committed_at_unix_ms: 0,
        };
        let _scope = shared.resume_scope(&context).unwrap();
        shared.published(5).unwrap();
        done.send(()).unwrap();
    });
    assert_eq!(control.wait(deadline).unwrap().position, 4);
    let original = b"unchanged signed original".to_vec();
    queued.send(original.clone()).unwrap();
    assert!(matches!(
        completion.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    control
        .arm(Point::Published, Some(5), Some(original), deadline)
        .unwrap();
    let observed = control.wait(deadline).unwrap();
    assert_eq!(observed.position, 5);
    assert_eq!(observed.action_count, 1);
    control.release().unwrap();
    completion.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
}

#[test]
fn availability_remote_verification_outlives_only_a_released_publication_pause() {
    use crate::formal_live_service::test_control::{BoundaryControl, Point};
    use std::sync::mpsc;
    let control = BoundaryControl::new();
    let prior_deadline = Instant::now() + Duration::from_secs(2);
    control
        .arm(Point::Published, Some(4), None, prior_deadline)
        .unwrap();
    let shared = control.shared();
    let (send, completed) = mpsc::channel();
    let worker = thread::spawn(move || send.send(shared.published(4)).unwrap());
    assert_eq!(control.wait(prior_deadline).unwrap().position, 4);
    control.release().unwrap();
    completed
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    worker.join().unwrap();
    // Model proof verification exceeding the old pause's entire lifetime.
    // The worker already returned Ok, so that expired lease cannot poison it.
    thread::sleep(
        prior_deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(10),
    );
    let fresh_deadline = Instant::now() + Duration::from_secs(2);
    control
        .arm(Point::Published, None, None, fresh_deadline)
        .unwrap();
    let shared = control.shared();
    let worker = thread::spawn(move || shared.published(5));
    assert_eq!(control.wait(fresh_deadline).unwrap().position, 5);
    control.release().unwrap();
    worker.join().unwrap().unwrap();
}

#[derive(Clone, Copy, Debug)]
pub(super) enum AdmissionCase {
    QueuedBehindEmptyFrozen,
    OriginalCandidateSaved,
}

/// Constructed by the shared process fixture after the normal signed initial
/// publication is readable and before the first owner request is submitted.
/// The replica listener must be a fixed loopback address so the same runtime
/// configuration can reconnect after the endpoint child is restarted.
pub(super) struct AvailabilityCase {
    pub(super) admission: AdmissionCase,
    pub(super) runtime: FormalLiveRuntime,
    pub(super) runtime_config: PathBuf,
    pub(super) endpoint_binary: std::ffi::OsString,
    pub(super) replica_config: PathBuf,
    pub(super) replica_address: SocketAddr,
    pub(super) replica_child: super::oracle_support::EndpointChild,
    pub(super) public_client_config: WorldServiceClientConfig,
    pub(super) owner_client_config: WorldServiceClientConfig,
    pub(super) world: WorldIdentity,
    pub(super) owner_request: SubmitIntentRequest<WorldServicePayloadV1>,
    pub(super) initial_view: oasis7::world_service::verified_view::VerifiedWorldView,
}

/// Entry point called by the shared fixture's C06 hook.
pub(super) fn exercise(mut case: AvailabilityCase) -> Result<(), String> {
    if case.initial_view.formal_commit().is_none() {
        return Err("C06 initial view lacks an externally verified formal commit".into());
    }

    let public_client = RemoteWorldServiceClient::new(case.public_client_config.clone())
        .map_err(|error| error.to_string())?;
    let owner_client = RemoteWorldServiceClient::new(case.owner_client_config.clone())
        .map_err(|error| error.to_string())?;

    // Fix both sides of the admission race. A prepared empty Tick leaves the
    // subsequently admitted owner envelope only in the volatile queue. A saved
    // original candidate has already executed, so recovery must reuse its exact
    // immutable record instead of executing the envelope again.
    use crate::formal_live_service::test_control::Point;
    let control = case.runtime.published.control_boundaries()?;
    let boundary_deadline = Instant::now() + Duration::from_secs(20);
    // Arrival/admission retains its original 20s observation budget. One new
    // pre-failure proof stage may use the existing configured HTTP deadline
    // (30s in this fixture), so the held candidate has an explicit finite
    // 20s+HTTP hold budget. Neither RPC nor boundary-arrival timeout increases.
    let boundary_hold_deadline = boundary_deadline
        .checked_add(case.public_client_config.timeout)
        .ok_or("C06 pre-failure proof hold deadline overflow")?;
    let (point, exact_action) = match case.admission {
        AdmissionCase::QueuedBehindEmptyFrozen => (Point::BeforeResume, None),
        AdmissionCase::OriginalCandidateSaved => (
            Point::CandidateSaved,
            Some(correlation::encode_consensus_intent(&case.owner_request)?),
        ),
    };
    control.arm(point, None, exact_action, boundary_hold_deadline)?;
    if matches!(case.admission, AdmissionCase::OriginalCandidateSaved) {
        assert_volatile_admission(
            owner_client
                .submit(case.owner_request.clone())
                .map_err(|error| format!("C06 submit original before candidate freeze: {error}"))?,
            &case.owner_request.correlation,
        )?;
    }
    let stopped_at = control.wait(boundary_deadline).map_err(|error| {
        format!(
            "C06 stage=initial_controlled_admission boundary={point:?} admission={:?}: {error}",
            case.admission
        )
    })?;
    let saved_original = match case.admission {
        AdmissionCase::QueuedBehindEmptyFrozen if stopped_at.action_count == 0 => None,
        AdmissionCase::OriginalCandidateSaved if stopped_at.action_count == 1 => Some((
            stopped_at.position,
            stopped_at
                .record_id
                .ok_or("C06 saved candidate record ID missing")?,
            stopped_at
                .node_block_hash
                .ok_or("C06 original Node block hash missing")?,
        )),
        _ => return Err("C06 controlled admission boundary had different input".into()),
    };
    // The fixed-read client must actually verify this exact cut while both
    // endpoints are available. A public CommitRef from the opaque runtime is
    // only a selector and cannot populate its verified-head cache. CandidateSaved
    // may have a newer empty predecessor than the fixture's initial HTTP view.
    let baseline = case.runtime.published.current()?;
    let (old_commit, expected_projection) = commit_ref_for_opaque_cut(&baseline, &case.world)?;
    let verified_baseline = public_client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: case.world.clone(),
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .map_err(|error| format!("C06 verify exact baseline before replica loss: {error}"))?;
    if verified_baseline.version().commit != old_commit
        || verified_baseline
            .formal_commit()
            .map(|commit| commit.head())
            != Some(baseline.anchor.head().head())
        || serde_cbor::to_vec(verified_baseline.projection()).map_err(|e| e.to_string())?
            != expected_projection
    {
        return Err("C06 pre-failure proof differs from exact controlled baseline".into());
    }
    case.replica_child.kill_sigkill_and_wait()?;
    let after_kill = case.runtime.published.current()?;
    if after_kill.anchor.head().head() != baseline.anchor.head().head() {
        return Err("C06 baseline changed across replica loss".into());
    }

    let admission = owner_client
        .submit(case.owner_request.clone())
        .map_err(|error| {
            format!("C06 ordinary owner submit before failure observation: {error}")
        })?;
    assert_volatile_admission(admission, &case.owner_request.correlation)?;
    let queued = case
        .runtime
        .runtime
        .lock()
        .map_err(|_| "C06 Node observation lock poisoned")?
        .snapshot()
        .consensus
        .pending_consensus_actions
        .submit_buffer_action_count;
    if queued != 1 {
        return Err(format!(
            "C06 expected one volatile original envelope, queued={queued}"
        ));
    }
    control.release()?;

    let status = wait_for_degraded_status(&case.public_client_config)?;
    if status.ready
        || status.can_propose
        || status.state != "degraded"
        || status.last_error.is_none()
        || status.committed_position != Some(old_commit.position)
        || status.committed_record_id.as_deref() != Some(old_commit.execution_block_hash.as_str())
    {
        return Err(format!(
            "C06 lost-replica status did not report degraded pending at the old cut: {status:?}"
        ));
    }
    match owner_client.submit(case.owner_request.clone()) {
        Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
            status: 503,
            message,
        }) if message == "formal runtime is not ready" => {}
        result => {
            return Err(format!(
                "C06 degraded runtime did not refuse new admission: {result:?}"
            ));
        }
    }

    // The owner request was volatile or refused after degradation; a lone endpoint must not
    // create a newer CommitRef or change the last published World.
    let live = case.runtime.published.current()?;
    if live.anchor.head().record().position != old_commit.position
        || live.anchor.head().head().record_id != old_commit.execution_block_hash
    {
        return Err("C06 published head moved after replica loss and volatile admission".into());
    }

    let old_read = public_client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: case.world.clone(),
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: Some(old_commit.clone()),
            deadline_unix_ms: None,
        })
        .map_err(|error| format!("C06 exact old committed read while degraded: {error}"))?;
    let actual_projection = serde_cbor::to_vec(old_read.projection())
        .map_err(|error| format!("C06 serialize verified old projection: {error}"))?;
    if old_read.version().commit != old_commit
        || old_read.formal_commit().map(|commit| commit.head()) != Some(live.anchor.head().head())
        || actual_projection != expected_projection
    {
        return Err(
            "C06 degraded read differed from the exact previously verified CommitRef".into(),
        );
    }
    if public_client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: case.world.clone(),
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .is_ok()
    {
        return Err("C06 degraded mode advertised an unfixed fresh read".into());
    }

    // Restart the same replica configuration and persistent store. The
    // returned address must match the address already held by the runtime.
    let (_restarted_replica_child, address) =
        oracle_support::start_endpoint(&case.endpoint_binary, &case.replica_config)?;
    if address != case.replica_address {
        return Err(format!(
            "C06 restarted replica changed its configured address: {} != {}",
            address, case.replica_address
        ));
    }
    // Reopen from the same writer directory only after the endpoint is back;
    // Recovery keeps the exact original operation, whether pending work was
    // durable or only volatile; it must not synthesize a replacement identity.
    stop_owned_protected_runtime(
        &mut case.runtime,
        Duration::from_secs(120),
        "C06_before_reopen",
    )?;
    drop(case.runtime);
    if let Some((_, _, block_hash)) = &saved_original {
        crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary::reset_execution_attempts(block_hash);
    }
    let mut reopened = FormalLiveRuntime::open_with_native_history(&case.runtime_config)?;
    // Observe the first natural recovery publication through a finite barrier.
    // Release it before remote proofs; later admissions acquire fresh barriers.
    let recovery_control = reopened.published.control_boundaries()?;
    let recovery_position = reopened
        .published
        .position()?
        .checked_add(1)
        .ok_or("C06 recovery boundary position overflow")?;
    let recovery_deadline = Instant::now() + Duration::from_secs(20);
    recovery_control.arm(
        Point::Published,
        Some(recovery_position),
        None,
        recovery_deadline,
    )?;
    reopened.start()?;
    let target_position = old_commit
        .position
        .checked_add(1)
        .ok_or("C06 CommitRef position overflow")?;
    let recovery_boundary = recovery_control
        .wait(recovery_deadline)
        .map_err(|error| format!("C06 exact recovery publication boundary: {error}"))?;
    if recovery_boundary.position != recovery_position || recovery_position < target_position {
        return Err("C06 recovery did not publish the exact successor boundary".into());
    }
    recovery_control.release()?;

    // Volatile admission is not durable pending state and may disappear on
    // reopen. Query the original first; a successor tick alone proves neither
    // the request's inclusion nor its successful canonical result.
    let recovered_commit = match owner_client.lookup_formal_original(case.owner_request.clone()) {
        Ok(response) => {
            if response.correlation != case.owner_request.correlation
                || response.formal_profile.as_ref()
                    != Some(&oasis7_client_api::world_service::FormalReadProfileV1::controlled())
                || response.execution_evidence_scope.is_some()
            {
                return Err("C06 recovery lookup changed the original request profile".into());
            }
            match response.outcome {
                oasis7_client_api::world_service::IntentOutcome::Committed { commit, .. } => {
                    eprintln!("C06 recovery lookup=committed replay=false");
                    Some(*commit)
                }
                outcome => return Err(format!("C06 unexpected recovery lookup: {outcome:?}")),
            }
        }
        Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
            status: 503,
            message,
        }) if message == "HistoryUnavailable: no result at the exact qualified lookup cut" => None,
        Err(error) => return Err(format!("C06 original lookup after recovery: {error}")),
    };
    if saved_original.is_none() {
        // The controlled predecessor was an empty Frozen continuation. Its
        // successor publication does not make the queued Volatile envelope
        // durable across owner teardown. Only the exact no-result response
        // permits re-admitting that same signed original once.
        let key = correlation::key_digest(&case.owner_request.correlation.key)?;
        if recovered_commit.is_some()
            || reopened
                .published
                .observe(|cut| fresh_request_result(&cut.world, &key, &case.owner_request))?
                .is_some()
        {
            return Err("C06 unfrozen volatile envelope unexpectedly survived as a result".into());
        }
        eprintln!("C06 recovery lookup=exact_cut_without_result replay=same_signed_original");
        // Signed lookup can involve remote history. It runs without holding a
        // finite test pause. Quiesce the next naturally produced publication
        // only after that proof has completed, including an in-flight Tick.
        // This adds a bounded 20s synchronization stage before the original
        // 20s action stage; the overall phase can therefore take longer.
        let barrier_deadline = Instant::now() + Duration::from_secs(20);
        recovery_control.arm(Point::Published, None, None, barrier_deadline)?;
        let barrier = recovery_control
            .wait(barrier_deadline)
            .map_err(|error| format!("C06 original replay admission barrier: {error}"))?;
        let replay_position = barrier
            .position
            .checked_add(1)
            .ok_or("C06 replay boundary position overflow")?;
        let replay_deadline = Instant::now() + Duration::from_secs(20);
        let admission = owner_client
            .submit(case.owner_request.clone())
            .map_err(|error| format!("C06 same original submit after recovery: {error}"))?;
        assert_volatile_admission(admission, &case.owner_request.correlation)?;
        // Queue the unchanged signed original while the prior publication is
        // held. Rearming releases that hold atomically, so its own next Tick
        // cannot be preceded by another empty Frozen continuation.
        recovery_control.arm(
            Point::Published,
            Some(replay_position),
            Some(correlation::encode_consensus_intent(&case.owner_request)?),
            replay_deadline,
        )?;
        let replayed = recovery_control
            .wait(replay_deadline)
            .map_err(|error| format!("C06 exact volatile original recovery boundary: {error}"))?;
        if replayed.position != replay_position || replayed.action_count != 1 {
            return Err("C06 replay did not use the immediate exact original continuation".into());
        }
        if reopened
            .published
            .observe(|cut| fresh_request_result(&cut.world, &key, &case.owner_request))?
            .is_none()
        {
            return Err("C06 exact replay publication lacks its original successful result".into());
        }
        recovery_control.release()?;
    } else if recovered_commit.is_none() {
        return Err("C06 durable original did not recover automatically".into());
    }
    let committed = lookup_original_commit(&owner_client, &case.owner_request)?;
    if recovered_commit.is_some_and(|original| original != committed) {
        return Err("C06 recovery changed the already committed original result".into());
    }
    if let Some((position, record_id, _)) = &saved_original {
        let attempts = crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary::execution_attempts();
        if committed.position != *position
            || committed.execution_block_hash != *record_id
            || attempts != 0
        {
            return Err(format!(
                "C06 durable original changed identity or re-executed: attempts={attempts}"
            ));
        }
    }
    if committed.position <= old_commit.position {
        return Err("C06 recovered request lacks a successor CommitRef".into());
    }
    let cut = reopened.published.current()?;
    let key = correlation::key_digest(&case.owner_request.correlation.key)
        .map_err(|error| error.to_string())?;
    let result = retained_result_from_hot_or_cold(&cut.world, &key)?;
    if result.request != case.owner_request || result.rejected.is_some() {
        return Err("C06 recovery did not preserve the exact original successful request".into());
    }
    let created_operation_id = match &case.owner_request.signed_payload {
        WorldServicePayloadV1::GameTaskCreate(signed) => {
            signed.request.client_operation_id.as_str()
        }
        _ => return Err("C06 fixture request must be a Game task creation".into()),
    };
    let task = game_task::current_control(&cut.world, "agent")
        .map_err(|error| error.to_string())?
        .ok_or("C06 reconciled Game task effect missing")?;
    if task.created_operation_id != created_operation_id {
        return Err("C06 recovery applied a different Game task effect".into());
    }

    // Repeating the same signed envelope must resolve to the same immutable
    // result and task, even after endpoint/runtime reopen.
    let duplicate_control = recovery_control;
    // All remote proof and typed readback above ran without a paused worker.
    // Hold a fresh natural publication only for queueing and atomic rearming.
    // As for replay, this is a separate bounded 20s synchronization stage
    // before the original 20s action stage.
    let barrier_deadline = Instant::now() + Duration::from_secs(20);
    duplicate_control.arm(Point::Published, None, None, barrier_deadline)?;
    let barrier = duplicate_control
        .wait(barrier_deadline)
        .map_err(|error| format!("C06 duplicate admission barrier: {error}"))?;
    let duplicate_deadline = Instant::now() + Duration::from_secs(20);
    let next_position = barrier
        .position
        .checked_add(1)
        .ok_or("C06 duplicate position overflow")?;
    let duplicate = owner_client
        .submit(case.owner_request.clone())
        .map_err(|error| format!("C06 duplicate original request: {error}"))?;
    assert_volatile_admission(duplicate, &case.owner_request.correlation)?;
    duplicate_control.arm(
        Point::Published,
        Some(next_position),
        Some(correlation::encode_consensus_intent(&case.owner_request)?),
        duplicate_deadline,
    )?;
    let observed = duplicate_control.wait(duplicate_deadline).map_err(|error|format!(
        "C06 stage=duplicate_after_volatile_admission_before_exact_publication admission={:?}: {error}",case.admission))?;
    if observed.position != next_position || observed.action_count != 1 {
        return Err("C06 duplicate envelope was not qualified in a successor continuation".into());
    }
    let after_retry = reopened.published.current()?;
    duplicate_control.release()?;
    if lookup_original_commit(&owner_client, &case.owner_request)? != committed {
        return Err("C06 retry changed the original CommitRef".into());
    }
    if game_task::current_control(&after_retry.world, "agent")
        .map_err(|error| error.to_string())?
        .as_ref()
        != Some(&task)
    {
        return Err("C06 retry duplicated or changed the canonical Game task effect".into());
    }

    stop_owned_protected_runtime(&mut reopened, Duration::from_secs(120), "C06_reopened")?;
    Ok(())
}

/// Build a read selector and comparison projection only from the exact opaque
/// publication already held by the runtime. The returned CommitRef never
/// creates trust: `RemoteWorldServiceClient` must prove it from its external
/// origin before the test accepts the old read.
fn commit_ref_for_opaque_cut(
    cut: &crate::formal_live_service::PublishedReadCut,
    world: &WorldIdentity,
) -> Result<(oasis7_client_api::world_service::CommitRef, Vec<u8>), String> {
    let record = cut.anchor.head().record();
    let anchor_ref = FormalReadAnchorRefV1 {
        position: record.position,
        record_id: cut.anchor.head().head().record_id.clone(),
        active_authority_id: cut
            .anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?,
    };
    let publication = FormalReadPublication::new(cut.anchor.clone(), cut.world.clone())?;
    let request = ReadWorldViewRequest {
        observation_nonce: Some("07".repeat(32)),
        formal_anchor: Some(anchor_ref),
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        world: world.clone(),
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let response = publication.view_at_own_anchor(&request, &super::oracle_support::public(92))?;
    let projection = serde_cbor::to_vec(&response.view)
        .map_err(|error| format!("C06 serialize opaque current projection: {error}"))?;
    Ok((response.version.commit, projection))
}

fn wait_for_degraded_status(
    config: &WorldServiceClientConfig,
) -> Result<crate::formal_live_service::FormalLiveStatusV1, String> {
    let address: SocketAddr = config
        .endpoint
        .strip_prefix("http://")
        .ok_or("C06 status endpoint must be HTTP loopback")?
        .parse()
        .map_err(|error| format!("C06 status address: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = signed_status(address, &config.trusted_service_public_key)?;
        if status.state == "degraded" && status.last_error.is_some() {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "C06 runtime did not report degraded after replica loss: {status:?}"
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn signed_status(
    address: SocketAddr,
    trusted_service_public_key: &str,
) -> Result<crate::formal_live_service::FormalLiveStatusV1, String> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|error| format!("C06 connect status: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .write_all(b"GET /v1/chain/status HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| format!("C06 read signed status: {error}"))?;
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("C06 status response lacks HTTP header boundary")?
        + 4;
    let head = std::str::from_utf8(&bytes[..split])
        .map_err(|error| format!("C06 status HTTP header: {error}"))?;
    if !head
        .lines()
        .next()
        .is_some_and(|line| line.contains(" 200 "))
    {
        return Err(format!("C06 status endpoint did not return 200: {head:?}"));
    }
    let response: SignedServiceResponse<crate::formal_live_service::FormalLiveStatusV1> =
        serde_json::from_slice(&bytes[split..])
            .map_err(|error| format!("C06 decode signed status: {error}"))?;
    let expected_digest = request_digest(CHAIN_STATUS_PATH, &())?;
    verify_service_response(
        CHAIN_STATUS_PATH,
        &expected_digest,
        &response,
        trusted_service_public_key,
    )?;
    Ok(response.payload)
}

#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; endpoint and Node processes run locally"]
fn protected_endpoint_children_replica_unavailable_preserves_exact_read_and_reconciles_original() {
    std::thread::Builder::new()
        .name("formal-protected-availability".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            for admission in [
                AdmissionCase::QueuedBehindEmptyFrozen,
                AdmissionCase::OriginalCandidateSaved,
            ] {
                eprintln!("C06 controlled admission={admission:?}");
                super::run_protected_runtime_availability_case(admission)?;
            }
            Ok::<_, String>(())
        })
        .expect("spawn protected availability fixture")
        .join()
        .expect("protected availability fixture thread panicked")
        .unwrap();
}
