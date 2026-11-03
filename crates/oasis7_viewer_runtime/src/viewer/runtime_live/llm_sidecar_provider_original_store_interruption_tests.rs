use super::{FixtureProviderAuthority, capture_root, read_http_request, test_provider_context};
use crate::viewer::ProviderOriginalMaterialOwnerV1;
use oasis7::simulator::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ProviderDispositionEvidenceV1, ProviderMaterialAttemptV1, ProviderMaterialOwnerBindingV1,
    ProviderOriginalMaterialRefsV1, ProviderOriginalMaterialStore, ProviderPendingAttemptPhaseV1,
    ProviderPendingRootsCursorV1, ProviderPendingRootsPageV1, ProviderSpoolReservation,
};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

/// Test adapter that cancels immediately after the real durable dispatch
/// marker. This exercises the native owner and transport boundary without a
/// timing race or a fake terminal disposition.
struct CancelAfterDispatchStore<'a> {
    inner: &'a dyn ProviderOriginalMaterialStore,
    dispatched: AtomicBool,
}

impl ProviderOriginalMaterialStore for CancelAfterDispatchStore<'_> {
    fn owner_binding(&self) -> &ProviderMaterialOwnerBindingV1 {
        self.inner.owner_binding()
    }

    fn pending_protection_root(&self) -> Result<[u8; 32], String> {
        self.inner.pending_protection_root()
    }

    fn protected_roots_page(
        &self,
        cursor: Option<&ProviderPendingRootsCursorV1>,
        limit: usize,
    ) -> Result<ProviderPendingRootsPageV1, String> {
        self.inner.protected_roots_page(cursor, limit)
    }

    fn validate_request_owner(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<(), String> {
        self.inner.validate_request_owner(request)
    }

    fn memory_budget(&self) -> MemoryBudget {
        self.inner.memory_budget()
    }

    fn request_body_limit_bytes(&self) -> u64 {
        self.inner.request_body_limit_bytes()
    }

    fn response_body_limit_bytes(&self) -> u64 {
        self.inner.response_body_limit_bytes()
    }

    fn reserve_body_spools(
        &self,
        request_limit: u64,
        response_limit: u64,
    ) -> Result<Box<dyn ProviderSpoolReservation>, String> {
        self.inner
            .reserve_body_spools(request_limit, response_limit)
    }

    fn cancelled(&self) -> bool {
        self.dispatched.load(Ordering::Acquire) || self.inner.cancelled()
    }

    fn persist_request(
        &self,
        request: &ContinuousAgentRequestContextV1,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<ProviderMaterialAttemptV1, String> {
        self.inner
            .persist_request(request, expected_body_id, original_body, length)
    }

    fn mark_dispatched(&self, attempt: &ProviderMaterialAttemptV1) -> Result<(), String> {
        self.inner.mark_dispatched(attempt)?;
        self.dispatched.store(true, Ordering::Release);
        Ok(())
    }

    fn persist_response_body(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String> {
        self.inner
            .persist_response_body(attempt, status, expected_body_id, original_body, length)
    }

    fn finish_response(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String> {
        self.inner
            .finish_response(attempt, status, response_object_id, response)
    }

    fn verify_commit_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String> {
        self.inner
            .verify_commit_materials(materials, request, response)
    }

    fn mark_committed(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        self.inner.mark_committed(materials, receipt_id, action_id)
    }

    fn mark_rejected(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String> {
        self.inner.mark_rejected(attempt, evidence)
    }
}

pub(super) fn direct_capture_request(request_id: &str) -> ContinuousAgentRequestContextV1 {
    let mut request =
        test_provider_context("provider-owner-agent", "provider-owner-turn", request_id, 1)
            .request_context;
    let digest = |label: &str| {
        let digest = crate::simulator::h_v1("oasis7.test.provider-original.v1", &label);
        assert!(digest.is_canonical_blake3());
        digest
    };
    request.runtime_binding.base_world_hash = digest("fixture world hash");
    request.runtime_binding.runtime_manifest_hash = digest("fixture manifest hash");
    request.observation_digest = digest("fixture observation digest");
    request.capability_catalog_digest = digest("fixture capability catalog digest");
    request.capability_invocation_context_digest = digest("fixture capability invocation digest");
    request.memory_snapshot_digest = digest("fixture memory snapshot digest");
    request.goal_snapshot_digest = digest("fixture goal snapshot digest");
    request.continuation_digest = digest("fixture continuation digest");
    request.request_digest = request.request_digest();
    request.validate().expect("validate provider owner request");
    request
}

fn fixture_owner(
    root: &std::path::Path,
) -> (
    ProviderMaterialOwnerBindingV1,
    Arc<FixtureProviderAuthority>,
    Arc<ProviderOriginalMaterialOwnerV1>,
) {
    let binding = ProviderMaterialOwnerBindingV1 {
        world_id: "world".to_string(),
        genesis_root: [0x71; 32],
        owner_id: [0x72; 32],
    };
    let authority = Arc::new(FixtureProviderAuthority {
        world_id: binding.world_id.clone(),
        owner_id: binding.owner_id,
    });
    let owner = ProviderOriginalMaterialOwnerV1::create(
        root,
        binding.clone(),
        MemoryBudget::new(64 * 1024 * 1024, 8 * 1024 * 1024).expect("provider owner memory pool"),
        PageCompression::Raw,
        authority.clone(),
    )
    .expect("create provider owner");
    (binding, authority, owner)
}

pub(super) fn fixture_owner_with_memory(
    root: &std::path::Path,
    memory: MemoryBudget,
) -> (
    ProviderMaterialOwnerBindingV1,
    Arc<ProviderOriginalMaterialOwnerV1>,
) {
    let binding = ProviderMaterialOwnerBindingV1 {
        world_id: "world".to_string(),
        genesis_root: [0x71; 32],
        owner_id: [0x72; 32],
    };
    let authority = Arc::new(FixtureProviderAuthority {
        world_id: binding.world_id.clone(),
        owner_id: binding.owner_id,
    });
    let owner = ProviderOriginalMaterialOwnerV1::create(
        root,
        binding.clone(),
        memory,
        PageCompression::Raw,
        authority,
    )
    .expect("create provider owner with selected memory pool");
    (binding, owner)
}

fn read_original_object(
    root: &std::path::Path,
    id: [u8; 32],
    max_bytes: usize,
    memory: MemoryBudget,
) -> Arc<oasis7_proto::memory_budget::ReservedValue<Vec<u8>>> {
    let pages = IncrementalPageStore::open(
        &root.join("pages"),
        memory,
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .expect("open original object pages");
    let cancelled = AtomicBool::new(false);
    let mut physical = PageIoBudget {
        reads_remaining: 10_000,
        read_bytes_remaining: 64 * 1024 * 1024,
        writes_remaining: 0,
        write_bytes_remaining: 0,
    };
    let objects =
        IncrementalObjectStore::open(&pages, MemoryClass::Recovery, &mut physical, &cancelled)
            .expect("open original object capability");
    let mut logical = ValueIoBudget {
        nodes_remaining: 10_000,
        read_bytes_remaining: 64 * 1024 * 1024,
        // `read_object_owned` stages the complete verified range before it
        // returns. Admit at most the caller's explicit object bound; the
        // actual allocation is separately charged by the shared memory pool.
        write_bytes_remaining: u64::try_from(max_bytes).expect("object bound fits u64"),
    };
    objects
        .read_object_owned(
            &id,
            max_bytes,
            &mut ObjectIo {
                physical: &mut physical,
                logical: &mut logical,
                class: MemoryClass::Recovery,
                cancelled: &cancelled,
            },
        )
        .expect("read and verify original object bytes")
}

#[derive(Clone)]
struct HttpReply {
    status: u16,
    body: Vec<u8>,
    location: Option<String>,
}

struct LocalHttpEndpoint {
    stopped: Arc<AtomicBool>,
    requests: Arc<AtomicU64>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LocalHttpEndpoint {
    fn spawn(listener: TcpListener, reply: HttpReply) -> Self {
        listener
            .set_nonblocking(true)
            .expect("set endpoint listener nonblocking");
        let stopped = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicU64::new(0));
        let thread_stopped = Arc::clone(&stopped);
        let thread_requests = Arc::clone(&requests);
        let worker = thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        thread_requests.fetch_add(1, Ordering::Relaxed);
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .expect("set endpoint read timeout");
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .expect("set endpoint write timeout");
                        let (path, _) = read_http_request(&mut stream);
                        assert_eq!(path, "/v1/world-simulator/decision-context");
                        let reason = match reply.status {
                            200 => "OK",
                            307 => "Temporary Redirect",
                            308 => "Permanent Redirect",
                            503 => "Service Unavailable",
                            status => panic!("unsupported fixture status {status}"),
                        };
                        let location = reply
                            .location
                            .as_ref()
                            .map(|value| format!("Location: {value}\r\n"))
                            .unwrap_or_default();
                        write!(
                            stream,
                            "HTTP/1.1 {} {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
                            reply.status,
                            reason,
                            reply.body.len(),
                            location,
                        )
                        .expect("write fixture response headers");
                        stream
                            .write_all(&reply.body)
                            .expect("write fixture response body");
                        stream.flush().expect("flush fixture response");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if thread_stopped.load(Ordering::Acquire) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                    Err(error) => panic!("fixture endpoint accept failed: {error}"),
                }
            }
            // Drain any connection accepted immediately before shutdown so a
            // late duplicate request cannot escape the endpoint count.
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        thread_requests.fetch_add(1, Ordering::Relaxed);
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .expect("set drained endpoint read timeout");
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .expect("set drained endpoint write timeout");
                        let (path, _) = read_http_request(&mut stream);
                        assert_eq!(path, "/v1/world-simulator/decision-context");
                        write!(
                            stream,
                            "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            reply.status,
                            reply.body.len(),
                        )
                        .expect("write drained fixture response headers");
                        stream
                            .write_all(&reply.body)
                            .expect("write drained fixture response body");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => panic!("fixture endpoint drain failed: {error}"),
                }
            }
        });
        Self {
            stopped,
            requests,
            worker: Some(worker),
        }
    }

    fn request_count(&self) -> u64 {
        self.requests.load(Ordering::Acquire)
    }

    fn finish(mut self) -> u64 {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("join local HTTP endpoint");
        }
        self.request_count()
    }
}

impl Drop for LocalHttpEndpoint {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn durable_dispatch_cancellation_survives_reopen_and_blocks_same_identity_send() {
    let request = direct_capture_request("request-before-send-cancel");
    let request_bytes = serde_json::to_vec(&request).expect("encode exact request body");
    let request_id = *blake3::hash(&request_bytes).as_bytes();
    let root = capture_root();
    let (binding, authority, owner) = fixture_owner(&root);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind no-send endpoint");
    listener
        .set_nonblocking(true)
        .expect("make no-send endpoint nonblocking");
    let base_url = format!(
        "http://{}/v1",
        listener.local_addr().expect("endpoint address")
    );
    let client = oasis7::simulator::ProviderLoopbackHttpClient::new(base_url.as_str(), None, 5_000)
        .expect("create captured provider client");
    let cancel_after_dispatch = CancelAfterDispatchStore {
        inner: owner.as_ref(),
        dispatched: AtomicBool::new(false),
    };

    let error = client
        .request_decision_with_context_and_material_store(&request, &cancel_after_dispatch)
        .expect_err("cancellation after durable dispatch must stop before HTTP");
    assert!(error.to_string().contains("after dispatch marker"));
    assert!(cancel_after_dispatch.dispatched.load(Ordering::Acquire));
    assert_eq!(
        listener
            .accept()
            .expect_err("HTTP must not be attempted")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );

    let before_reopen = owner
        .pending_protection_root()
        .expect("read pending root after pre-send interruption");
    let pending = owner
        .protected_roots_page(None, 4)
        .expect("inspect durable dispatch marker");
    assert_eq!(pending.attempts.len(), 1);
    let summary = &pending.attempts[0];
    assert_eq!(
        summary.phase,
        ProviderPendingAttemptPhaseV1::DispatchedOrUnknown
    );
    assert_eq!(summary.attempt.request_object_id, request_id);
    assert_eq!(summary.response_object_id, None);
    assert_eq!(summary.response_status, None);
    assert_eq!(summary.response_context_object_id, None);
    assert!(pending.object_ids.contains(&request_id));
    assert!(owner.close_ready().is_err());
    drop(owner);

    let recovery_memory =
        MemoryBudget::new(64 * 1024 * 1024, 8 * 1024 * 1024).expect("recovery pool");
    let recovered = ProviderOriginalMaterialOwnerV1::open(
        &root,
        &binding,
        recovery_memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
        authority.clone(),
    )
    .expect("reopen interrupted attempt from a new pool");
    assert_eq!(
        recovered
            .pending_protection_root()
            .expect("recovered pending root"),
        before_reopen
    );
    let recovered_page = recovered
        .protected_roots_page(None, 4)
        .expect("read recovered pending attempt");
    assert_eq!(recovered_page.attempts.len(), 1);
    assert_eq!(
        recovered_page.attempts[0].phase,
        ProviderPendingAttemptPhaseV1::DispatchedOrUnknown
    );
    assert_eq!(recovered_page.attempts[0].attempt, summary.attempt);
    let recovered_request =
        read_original_object(&root, request_id, request_bytes.len(), recovery_memory);
    assert_eq!(recovered_request.as_slice(), request_bytes.as_slice());
    assert!(recovered.close_ready().is_err());
    drop(recovered_request);
    drop(recovered);

    let reopened = ProviderOriginalMaterialOwnerV1::open(
        &root,
        &binding,
        MemoryBudget::new(64 * 1024 * 1024, 8 * 1024 * 1024).expect("ordinary retry pool"),
        PageCompression::Raw,
        MemoryClass::Ordinary,
        authority,
    )
    .expect("open pending owner for attempted same-identity replay");
    let retry_error = client
        .request_decision_with_context_and_material_store(&request, reopened.as_ref())
        .expect_err("the durable seen index must refuse a replay");
    assert!(
        retry_error
            .to_string()
            .contains("already consumed or dispatched")
    );
    assert_eq!(
        reopened
            .pending_protection_root()
            .expect("pending root after refused replay"),
        before_reopen
    );
    assert_eq!(
        listener
            .accept()
            .expect_err("replay must not reach HTTP")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(reopened.close_ready().is_err());
    drop(reopened);
    std::fs::remove_dir_all(root).expect("remove provider owner fixture");
}

#[test]
fn redirected_non_success_bodies_remain_pending_and_never_reach_second_endpoint() {
    for status in [307, 308] {
        let root = capture_root();
        let (binding, authority, owner) = fixture_owner(&root);
        let request = direct_capture_request(&format!("request-redirect-{status}"));
        let response_body = format!("redirect response body for HTTP {status}").into_bytes();
        let first_listener =
            TcpListener::bind("127.0.0.1:0").expect("bind redirect source endpoint");
        let second_listener =
            TcpListener::bind("127.0.0.1:0").expect("bind redirect target endpoint");
        let first_url = format!(
            "http://{}/v1",
            first_listener
                .local_addr()
                .expect("redirect source address")
        );
        let second_url = format!(
            "http://{}/v1/world-simulator/decision-context",
            second_listener
                .local_addr()
                .expect("redirect target address")
        );
        let first_endpoint = LocalHttpEndpoint::spawn(
            first_listener,
            HttpReply {
                status,
                body: response_body.clone(),
                location: Some(second_url),
            },
        );
        let second_endpoint = LocalHttpEndpoint::spawn(
            second_listener,
            HttpReply {
                status: 200,
                body: b"unexpected redirected request".to_vec(),
                location: None,
            },
        );
        let client =
            oasis7::simulator::ProviderLoopbackHttpClient::new(first_url.as_str(), None, 5_000)
                .expect("create captured redirect client");

        let error = client
            .request_decision_with_context_and_material_store(&request, owner.as_ref())
            .expect_err("redirect status must be reported without following Location");
        assert!(matches!(
            &error,
            oasis7::simulator::ProviderLoopbackHttpError::UnexpectedStatus {
                status_code,
                ..
            } if *status_code == status
        ));
        assert!(
            error.to_string().contains("redirect response body"),
            "the original non-success body should remain in the transport error"
        );
        assert!(!error.retryable(), "redirect status is not retryable");
        assert_eq!(first_endpoint.request_count(), 1);
        assert_eq!(second_endpoint.request_count(), 0);

        let pending_root = owner
            .pending_protection_root()
            .expect("pending root after redirect response");
        let pending = owner
            .protected_roots_page(None, 4)
            .expect("inspect body-only response record");
        assert_eq!(pending.attempts.len(), 1);
        let summary = &pending.attempts[0];
        assert_eq!(
            summary.phase,
            ProviderPendingAttemptPhaseV1::ResponseBodyDurable
        );
        assert_eq!(
            summary.response_object_id,
            Some(*blake3::hash(&response_body).as_bytes())
        );
        assert_eq!(summary.response_status, Some(status));
        assert_eq!(summary.response_context_object_id, None);
        assert!(
            pending
                .object_ids
                .contains(&summary.attempt.request_object_id)
        );
        assert!(
            pending
                .object_ids
                .contains(blake3::hash(&response_body).as_bytes())
        );
        assert!(owner.close_ready().is_err());
        drop(owner);

        let recovery_memory =
            MemoryBudget::new(64 * 1024 * 1024, 8 * 1024 * 1024).expect("recovery pool");
        let recovered = ProviderOriginalMaterialOwnerV1::open(
            &root,
            &binding,
            recovery_memory.clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
            authority.clone(),
        )
        .expect("reopen response-body-only attempt");
        assert_eq!(
            recovered
                .pending_protection_root()
                .expect("recovered pending root"),
            pending_root
        );
        let recovered_page = recovered
            .protected_roots_page(None, 4)
            .expect("read body-only response after restart");
        assert_eq!(recovered_page.attempts.len(), 1);
        let recovered_attempt = &recovered_page.attempts[0];
        assert_eq!(
            recovered_attempt.phase,
            ProviderPendingAttemptPhaseV1::ResponseBodyDurable
        );
        assert_eq!(recovered_attempt.response_status, Some(status));
        assert_eq!(recovered_attempt.response_context_object_id, None);
        let response_id = recovered_attempt
            .response_object_id
            .expect("body-only response object remains referenced");
        let stored_response =
            read_original_object(&root, response_id, response_body.len(), recovery_memory);
        assert_eq!(stored_response.as_slice(), response_body.as_slice());
        assert!(recovered.close_ready().is_err());
        drop(stored_response);
        drop(recovered);

        let reopened = ProviderOriginalMaterialOwnerV1::open(
            &root,
            &binding,
            MemoryBudget::new(64 * 1024 * 1024, 8 * 1024 * 1024).expect("ordinary retry pool"),
            PageCompression::Raw,
            MemoryClass::Ordinary,
            authority,
        )
        .expect("reopen owner before same-identity retry");
        let retry_root = reopened
            .pending_protection_root()
            .expect("root before refused retry");
        let retry_error = client
            .request_decision_with_context_and_material_store(&request, reopened.as_ref())
            .expect_err("same attempt cannot be sent again after a redirect");
        assert!(
            retry_error
                .to_string()
                .contains("already consumed or dispatched")
        );
        assert_eq!(
            reopened
                .pending_protection_root()
                .expect("root after refused retry"),
            retry_root
        );
        assert!(reopened.close_ready().is_err());
        drop(reopened);
        assert_eq!(second_endpoint.finish(), 0);
        assert_eq!(first_endpoint.finish(), 1);
        std::fs::remove_dir_all(root).expect("remove redirect owner fixture");
    }
}
