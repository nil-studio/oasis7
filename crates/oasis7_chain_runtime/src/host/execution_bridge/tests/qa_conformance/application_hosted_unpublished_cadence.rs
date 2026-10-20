//! Real signed uncommitted responses retain the original request and retry cadence.
use super::*;
static SIGNED_RECEIVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static SIGNED_UNKNOWN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static ORIGINAL_PAYLOAD: Mutex<Option<[u8; 32]>> = Mutex::new(None);
static ORIGINAL_KEY: Mutex<Option<String>> = Mutex::new(None);
static ORIGINAL_STARTED: Mutex<Option<Instant>> = Mutex::new(None);

pub(super) fn active(gate: &http_fixture::WorldGate) -> bool {
    gate.root.lock().unwrap().as_ref().is_some_and(|root| {
        root.join("pending-cadence-gated").exists()
            && !root.join("pending-cadence-sampled").exists()
    })
}

pub(super) fn observe_submission(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    response: &IntentResponse<serde_json::Value>,
    gate: &http_fixture::WorldGate,
) {
    if !active(gate) {
        return;
    }
    assert!(matches!(response.outcome, IntentOutcome::Received { .. }));
    ORIGINAL_STARTED
        .lock()
        .unwrap()
        .get_or_insert_with(Instant::now);
    SIGNED_RECEIVED.fetch_add(1, Ordering::SeqCst);
    let digest = *blake3::hash(&serde_json::to_vec(&request.signed_payload).unwrap()).as_bytes();
    let mut original = ORIGINAL_PAYLOAD.lock().unwrap();
    if let Some(expected) = *original {
        assert!(
            expected == digest,
            "replay preserves original signed payload"
        );
    } else {
        *original = Some(digest);
    }
    let key = correlation::key_digest(&request.correlation.key).unwrap();
    let mut original_key = ORIGINAL_KEY.lock().unwrap();
    if let Some(expected) = original_key.as_ref() {
        assert!(expected == &key, "replay preserves original key");
    } else {
        *original_key = Some(key);
    }
}

pub(super) fn observe_signed_unknown(
    original: &WorldServicePayloadV1,
    response: &IntentResponse<serde_json::Value>,
    gate: &http_fixture::WorldGate,
) {
    if active(gate)
        && matches!(original, WorldServicePayloadV1::Cognition(_))
        && matches!(response.outcome, IntentOutcome::Unknown)
    {
        assert!(
            ORIGINAL_KEY.lock().unwrap().as_ref()
                == Some(&correlation::key_digest(&response.correlation.key).unwrap()),
            "Lookup retains original Cognition key"
        );
        assert!(
            ORIGINAL_PAYLOAD.lock().unwrap().as_ref()
                == Some(blake3::hash(&serde_json::to_vec(original).unwrap()).as_bytes()),
            "Lookup retains original signed Cognition payload"
        );
        SIGNED_UNKNOWN.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn real_tcp_hosted_unpublished_lookup_retains_periodic_retry_and_original_intent() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "pending-cadence",
    );
}

pub(super) fn child_ready() {
    if std::env::var("PRE2_APP_ADMISSION").as_deref() != Ok("pending-cadence") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    fs::write(root.join("pending-cadence-ready"), b"ready").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("pending-cadence-gated").exists() {
        assert!(Instant::now() < deadline, "parent commit gate handshake");
        thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn observe_child(summary: &serde_json::Value, started: &mut Option<Instant>) {
    if std::env::var("PRE2_APP_ADMISSION").as_deref() != Ok("pending-cadence") {
        return;
    }
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    if root.join("pending-cadence-sampled").exists() {
        return;
    }
    if summary["hosted_service_phase"] != "act"
        || summary["io_lifecycle"]["applied_count"]
            .as_u64()
            .unwrap_or(0)
            < 3
    {
        return;
    }
    let beginning = started.get_or_insert_with(Instant::now);
    assert_ne!(
        summary["terminal_states"]["agent-a"]["status"], "committed",
        "Unpublished original cannot publish committed memory"
    );
    assert_eq!(summary["io_lifecycle"]["apply_error_count"], 0);
    if beginning.elapsed() >= Duration::from_secs(1) {
        fs::write(root.join("pending-cadence-sampled"), b"sampled").unwrap();
    }
}

pub(super) fn observe_parent(fixture: &Fixture, root: &std::path::Path) -> thread::JoinHandle<()> {
    let root = root.to_path_buf();
    let requests = fixture.lookup_digests.clone();
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !root.join("pending-cadence-ready").exists() {
            assert!(
                Instant::now() < deadline,
                "child genuine original Act setup"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let baseline = requests.lock().unwrap().len();
        SIGNED_RECEIVED.store(0, Ordering::SeqCst);
        SIGNED_UNKNOWN.store(0, Ordering::SeqCst);
        *ORIGINAL_PAYLOAD.lock().unwrap() = None;
        *ORIGINAL_KEY.lock().unwrap() = None;
        *ORIGINAL_STARTED.lock().unwrap() = None;
        fs::write(root.join("pending-cadence-gated"), b"gated").unwrap();
        while !root.join("pending-cadence-sampled").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let elapsed = ORIGINAL_STARTED
            .lock()
            .unwrap()
            .expect("real gated original Submit observed")
            .elapsed();
        let observed = requests.lock().unwrap()[baseline..].to_vec();
        fs::write(root.join("pending-cadence-released"), b"released").unwrap();
        assert!(
            root.join("pending-cadence-sampled").exists(),
            "actual unpublished sample completed"
        );
        let lookups = observed
            .iter()
            .filter(|entry| ORIGINAL_KEY.lock().unwrap().as_ref() == Some(entry))
            .collect::<Vec<_>>();
        assert!(
            SIGNED_UNKNOWN.load(Ordering::SeqCst) >= 3,
            "verified signed HTTP200 Unknown Lookup responses required"
        );
        assert!(
            SIGNED_RECEIVED.load(Ordering::SeqCst) > 0,
            "real signed Submit Received observed"
        );
        assert!(
            lookups.len() >= 3,
            "actual repeated signed Unknown Lookup required"
        );
        assert!(
            observed.len() as u128 <= elapsed.as_millis() / 50 + 4,
            "no-progress I/O must retain 50ms retry cadence: count={} elapsed_ms={}",
            observed.len(),
            elapsed.as_millis()
        );
        assert!(
            lookups.iter().all(|entry| *entry == lookups[0]),
            "exact original key retained"
        );
        assert!(
            observed
                .iter()
                .filter_map(|entry| entry.strip_prefix("submit:"))
                .all(|key| key == lookups[0]),
            "authorized replay retains exact original key"
        );
        println!(
            "unpublished_cadence_lookup_count={} submit_received_count={} total_intent_io_count={} elapsed_ms={} original_retained=true",
            lookups.len(),
            SIGNED_RECEIVED.load(Ordering::SeqCst),
            observed.len(),
            elapsed.as_millis()
        );
    })
}

pub(super) fn hold_original_commit(gate: &http_fixture::WorldGate) {
    let Some(root) = gate.root.lock().unwrap().clone() else {
        return;
    };
    if !root.join("pending-cadence-gated").exists() {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("pending-cadence-released").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        root.join("pending-cadence-released").exists(),
        "original admitted commit released after unpublished sample"
    );
}

pub(super) fn verify_captured_response(
    path: &str,
    body: &[u8],
    payload: &[u8],
    public_key: &str,
    gate: &http_fixture::WorldGate,
) {
    match path {
        SUBMIT_PATH => {
            let request: SubmitIntentRequest<WorldServicePayloadV1> =
                serde_json::from_slice(body).unwrap();
            let signed: SignedServiceResponse<IntentResponse<serde_json::Value>> =
                serde_json::from_slice(payload).unwrap();
            oasis7::world_service::authority::verify_service_response(
                path,
                &oasis7::world_service::authority::request_digest(path, &request).unwrap(),
                &signed,
                public_key,
            )
            .unwrap();
            signed.payload.validate(&request.correlation).unwrap();
            application_hosted_unpublished_cadence::observe_submission(
                &request,
                &signed.payload,
                gate,
            );
        }
        VIEW_PATH => {
            let request: SignedReadRequest<ReadWorldViewRequest> =
                serde_json::from_slice(body).unwrap();
            let signed: SignedServiceResponse<
                ReadWorldViewResponse<oasis7::world_service::projection::WorldServiceProjection>,
            > = serde_json::from_slice(payload).unwrap();
            oasis7::world_service::authority::verify_service_response(
                path,
                &oasis7::world_service::authority::request_digest(path, &request).unwrap(),
                &signed,
                public_key,
            )
            .unwrap();
            signed.payload.validate(&request.request).unwrap();
        }
        CHANGES_PATH => {
            let request: SignedReadRequest<ReadWorldChangesRequest> =
                serde_json::from_slice(body).unwrap();
            let signed: SignedServiceResponse<ReadWorldChangesResponse<serde_json::Value>> =
                serde_json::from_slice(payload).unwrap();
            oasis7::world_service::authority::verify_service_response(
                path,
                &oasis7::world_service::authority::request_digest(path, &request).unwrap(),
                &signed,
                public_key,
            )
            .unwrap();
            signed.payload.validate(&request.request).unwrap();
            assert!(
                serde_json::to_vec(&signed.payload).unwrap().len() as u64
                    <= request.request.max_bytes
            );
        }
        LOOKUP_PATH => {
            // Lookup authenticates the original signed payload rather
            // than wrapping the lookup in a SignedReadRequest.
            let request: wire::AuthenticatedLookup = serde_json::from_slice(body).unwrap();
            let signed: SignedServiceResponse<IntentResponse<serde_json::Value>> =
                serde_json::from_slice(payload).unwrap();
            oasis7::world_service::authority::verify_service_response(
                path,
                &oasis7::world_service::authority::request_digest(path, &request).unwrap(),
                &signed,
                public_key,
            )
            .unwrap();
            request.request.validate().unwrap();
            let expected =
                derive_correlation(request.request.key.world.clone(), &request.original).unwrap();
            assert_eq!(expected.key, request.request.key);
            signed.payload.validate(&expected).unwrap();
            application_hosted_unpublished_cadence::observe_signed_unknown(
                &request.original,
                &signed.payload,
                gate,
            );
        }
        _ => unreachable!("only authenticated service routes are captured"),
    }
}
