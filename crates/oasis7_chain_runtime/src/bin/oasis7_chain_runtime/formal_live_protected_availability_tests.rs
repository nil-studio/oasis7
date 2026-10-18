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

/// Constructed by the shared process fixture after the normal signed initial
/// publication is readable and before the first owner request is submitted.
/// The replica listener must be a fixed loopback address so the same runtime
/// configuration can reconnect after the endpoint child is restarted.
pub(super) struct AvailabilityCase {
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

    // Lose the real replica first. The runtime may not have observed the
    // failure yet, so an immediate ordinary owner submit can still be accepted
    // as volatile work; qualification must then stop when the writer notices
    // that one protected side is gone.
    case.replica_child.kill_sigkill_and_wait()?;

    // Select the baseline from the runtime's opaque, atomically published
    // anchor after the kill. The public CommitRef below is only a selector:
    // the external HTTP client still verifies its exact signed proof from the
    // configured C0 trust. This avoids assuming an earlier initial view is
    // still the current cut after background ticks.
    let baseline = case.runtime.published.current()?;
    let (old_commit, expected_projection) = commit_ref_for_opaque_cut(&baseline, &case.world)?;

    let admitted = match owner_client.submit(case.owner_request.clone()) {
        Ok(admission) => {
            assert_volatile_admission(admission, &case.owner_request.correlation)?;
            true
        }
        Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
            status: 503,
            message,
        }) if message == "formal runtime is not ready" => false,
        Err(error) => return Err(format!("C06 ordinary owner submit: {error}")),
    };

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
    // startup must reconcile the exact original request rather than synthesize
    // a replacement request or duplicate its effect.
    case.runtime.stop(Duration::from_secs(10))?;
    drop(case.runtime);
    let mut reopened = FormalLiveRuntime::open_with_native_history(&case.runtime_config)?;
    reopened.start()?;
    let target_position = old_commit
        .position
        .checked_add(1)
        .ok_or("C06 CommitRef position overflow")?;
    wait_for_published_position(&reopened, target_position)?;

    if !admitted {
        let admission = owner_client
            .submit(case.owner_request.clone())
            .map_err(|error| format!("C06 same original submit after recovery: {error}"))?;
        assert_volatile_admission(admission, &case.owner_request.correlation)?;
    }
    // A background empty tick can publish a successor before this request is
    // canonical. Wait for its exact result before asking the external client
    // to prove it; an arbitrary height is not evidence of request completion.
    let (cut, result) = wait_for_canonical_request_result(&reopened, &case.owner_request)?;
    let committed = lookup_original_commit(&owner_client, &case.owner_request)?;
    if committed.position <= old_commit.position {
        return Err("C06 recovered request lacks a successor CommitRef".into());
    }
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
    let duplicate = owner_client
        .submit(case.owner_request.clone())
        .map_err(|error| format!("C06 duplicate original request: {error}"))?;
    assert_volatile_admission(duplicate, &case.owner_request.correlation)?;
    let next_position = reopened
        .published
        .current()?
        .anchor
        .head()
        .record()
        .position
        .checked_add(1)
        .ok_or("C06 duplicate position overflow")?;
    wait_for_published_position(&reopened, next_position)?;
    if lookup_original_commit(&owner_client, &case.owner_request)? != committed {
        return Err("C06 retry changed the original CommitRef".into());
    }
    let after_retry = reopened.published.current()?;
    if game_task::current_control(&after_retry.world, "agent")
        .map_err(|error| error.to_string())?
        .as_ref()
        != Some(&task)
    {
        return Err("C06 retry duplicated or changed the canonical Game task effect".into());
    }

    reopened.stop(Duration::from_secs(10))?;
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
        .spawn(super::run_protected_runtime_availability_case)
        .expect("spawn protected availability fixture")
        .join()
        .expect("protected availability fixture thread panicked")
        .unwrap();
}
