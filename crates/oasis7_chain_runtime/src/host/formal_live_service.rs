//! Loopback-only signed HTTP surface for the formal local Node runtime.
//!
//! Read responses are produced only by `FormalCommittedReadService` from an
//! immutable World cut and opaque verified anchor. Submission validates the
//! existing signed action, then queues it in the actual `NodeRuntime`; this
//! adapter never records results or edits the World itself.
use ed25519_dalek::SigningKey;
pub(crate) use oasis7::world_service::formal_read_error::{FormalReadError, FormalReadResult};
use oasis7::{
    runtime::World,
    world_service::{
        agent_authority, agent_chat, authority, correlation,
        formal_read::{FormalReadHistoryV1, FormalReadVerifier, MAX_FORMAL_HISTORY_STEPS},
        formal_read_service::{FormalCommittedReadService, FormalReadPublication},
        game_task, gameplay,
        wire::{
            AuthenticatedLookup, CHANGES_PATH, LOOKUP_PATH, SUBMIT_PATH, SignedReadRequest,
            VIEW_PATH, WorldServicePayloadV1,
        },
    },
};
use oasis7_client_api::world_service::{
    AdmissionDurability, CommitRef, FormalReadAnchorRefV1, FormalReadProfileV1, IntentOutcome,
    IntentResponse, ReadWorldChangesRequest, ReadWorldViewRequest, ReadWorldViewResponse,
    SubmitIntentRequest, WORLD_SERVICE_CONTRACT_VERSION, WorldIdentity,
};
use oasis7_distfs::controlled_authority::{
    formal::{VerifiedFormalReadAnchorV1, business_index::BusinessIndexNodeReader},
    replicated_protocol::ClosedRecord,
};
use oasis7_node::NodeRuntime;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
#[path = "formal_live_test_control.rs"]
pub(crate) mod test_control;
#[cfg(test)]
#[path = "formal_live_service_tests.rs"]
mod tests;

#[path = "formal_live_business_service.rs"]
mod business_service;
#[path = "formal_live_changes_service.rs"]
mod changes_service;
#[path = "formal_live_full_sync_read_gate.rs"]
pub(crate) mod full_sync_read_gate;
#[path = "formal_live_full_sync.rs"]
mod full_sync_service;
const MAX_REQUEST_BODY: usize = 256 * 1024;
const MAX_HTTP_HEADER: usize = 8 * 1024;
const MAX_HTTP_REPLY: usize = 8 * 1024 * 1024;
const MAX_NODE_ERROR_CHARS: usize = 384;
const HTTP_CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);
const FORMAL_HTTP_THREAD_STACK_BYTES: usize = 8 * 1024 * 1024;
const SERVICE_READINESS_PATH: &str = "/healthz";
const CHAIN_STATUS_PATH: &str = "/v1/chain/status";

#[derive(Clone)]
pub(crate) struct PublishedReadCut {
    pub(crate) world: World,
    pub(crate) anchor: VerifiedFormalReadAnchorV1,
}

/// Bounded immutable cut material for an in-flight or recently completed
/// state-sync session. The byte budget counts canonical serialized closure
/// bytes; the number of opaque anchors and their metadata is independently
/// capped by `MAX_FULL_SYNC_CACHE_ENTRIES`.
#[derive(Default)]
pub(crate) struct FullSyncMaterialCache {
    pub(crate) entries: VecDeque<FullSyncMaterialCacheEntry>,
    pub(crate) serialized_bytes: usize,
}

pub(crate) struct FullSyncMaterialCacheEntry {
    pub(crate) cut: oasis7_client_api::world_service_full_sync::FormalFullSyncCutV1,
    pub(crate) anchor: VerifiedFormalReadAnchorV1,
    /// Last live cut whose ancestry from `anchor` was checked by the typed
    /// history resolver. This lets subsequent chunk requests reuse the
    /// qualification while same-epoch writes advance the published head.
    pub(crate) qualified_head: VerifiedFormalReadAnchorV1,
    pub(crate) serialized_material: Arc<[u8]>,
    pub(crate) last_used: Instant,
}

/// Atomic publication sink shared by the formal hook and read service.
#[derive(Default)]
pub(crate) struct PublishedReadState {
    current: Mutex<Option<PublishedReadCut>>,
    #[cfg(test)]
    test_control: Mutex<Option<Arc<test_control::BoundaryState>>>,
}
impl PublishedReadState {
    pub(crate) fn current(&self) -> Result<PublishedReadCut, String> {
        self.observe(|cut| Ok(cut.clone()))
    }

    pub(crate) fn current_for_read(&self) -> FormalReadResult<PublishedReadCut> {
        let current = self.current.lock().map_err(|_| {
            FormalReadError::Internal("formal read publication lock poisoned".into())
        })?;
        current.as_ref().cloned().ok_or_else(|| {
            FormalReadError::NotReady("formal read publication is not ready".into())
        })
    }

    /// Borrow one atomic World/anchor cut for a small local observation. The
    /// callback must not perform network or retained-history reads: publication
    /// holds this same lock. Callers that need an owned cut still use `current`.
    pub(crate) fn observe<T>(
        &self,
        read: impl FnOnce(&PublishedReadCut) -> Result<T, String>,
    ) -> Result<T, String> {
        let current = self
            .current
            .lock()
            .map_err(|_| "formal read publication lock poisoned")?;
        let cut = current
            .as_ref()
            .ok_or("formal read publication is not ready")?;
        read(cut)
    }

    #[cfg(test)]
    pub(crate) fn position(&self) -> Result<u64, String> {
        self.observe(|cut| Ok(cut.anchor.head().record().position))
    }
    #[cfg(test)]
    pub(crate) fn control_boundaries(&self) -> Result<test_control::BoundaryControl, String> {
        let control = test_control::BoundaryControl::new();
        let mut slot = self
            .test_control
            .lock()
            .map_err(|_| "formal test control lock poisoned")?;
        if slot.is_some() {
            return Err("formal test runtime already has a boundary controller".into());
        }
        *slot = Some(control.shared());
        Ok(control)
    }
    #[cfg(test)]
    fn test_control_state(&self) -> Result<Option<Arc<test_control::BoundaryState>>, String> {
        self.test_control
            .lock()
            .map(|control| control.clone())
            .map_err(|_| "formal test control lock poisoned".into())
    }
    pub(crate) fn publish(
        &self,
        world: World,
        anchor: VerifiedFormalReadAnchorV1,
    ) -> Result<(), String> {
        // Validate the full immutable cut before making it visible. The lock
        // replaces World and anchor together, so a reader can never mix heads.
        FormalReadPublication::new(anchor.clone(), world.clone()).map_err(|error| error.to_string())?;
        let mut current = self
            .current
            .lock()
            .map_err(|_| "formal read publication lock poisoned")?;
        #[cfg(test)]
        let position = anchor.head().record().position;
        *current = Some(PublishedReadCut { world, anchor });
        drop(current);
        #[cfg(test)]
        if let Some(control) = self.test_control_state()? {
            control.published(position)?;
        }
        Ok(())
    }
}
impl super::execution_bridge::formal_node_hook::FormalPublicationSink for PublishedReadState {
    fn publish(&self, world: World, anchor: VerifiedFormalReadAnchorV1) -> Result<(), String> {
        PublishedReadState::publish(self, world, anchor)
    }
    #[cfg(test)]
    fn test_boundary_control(&self) -> Result<Option<Arc<test_control::BoundaryState>>, String> {
        self.test_control_state()
    }
}

/// Storage-backed resolvers use exact cut lookups and never discover trust by
/// searching storage or accepting a service pin. Profile-v2 governance checks
/// replay from the configured external origin under one operation deadline;
/// `history_page` remains capped at 128 steps per page.
pub(crate) trait FormalLiveHistoryResolver: Send + Sync {
    /// Startup must establish semantic history before a profile-v2 writer serves.
    fn verify_governance_history(&self, anchor: &VerifiedFormalReadAnchorV1) -> Result<(), String> {
        if anchor.authority().definition().profile_version == 2 {
            Err("HistoryUnavailable: authenticated governance history unavailable".into())
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    fn business_result(
        &self,
        _cut: &CommitRef,
        _correlation: &oasis7_client_api::world_service::RequestCorrelation,
        _scope: &str,
        _reader: &str,
    ) -> Result<
        Option<(
            oasis7_client_api::world_service::FormalBusinessResultProofV1,
            Option<String>,
        )>,
        FormalReadError,
    > {
        Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: authenticated business history is unavailable".into()))
    }
    /// Resolve a lookup from the exact publication selected and verified by
    /// `read_view`. Production lookup uses this typed path; the DTO-based
    /// method above remains available for failure-only native diagnostics.
    fn business_result_for_publication(
        &self,
        _cut: FormalReadPublication,
        _correlation: &oasis7_client_api::world_service::RequestCorrelation,
        _scope: &str,
        _reader: &str,
    ) -> Result<
        Option<(
            oasis7_client_api::world_service::FormalBusinessResultProofV1,
            Option<String>,
        )>,
        FormalReadError,
    > {
        Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: authenticated business history is unavailable".into()))
    }
    fn origin_publication(
        &self,
        anchor: &FormalReadAnchorRefV1,
    ) -> Result<FormalReadPublication, FormalReadError>;

    fn fixed_publication(&self, commit: &CommitRef) -> Result<FormalReadPublication, FormalReadError>;

    /// Select the exact native publication no more than `max_steps` after the
    /// caller's current anchor, capped at the live head. Implementations must
    /// locate this cut through authenticated native history, not a directory
    /// scan or an unauthenticated position lookup.
    fn forward_publication(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        live: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadPublication, FormalReadError>;

    fn history_page(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        target: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadHistoryV1, FormalReadError>;

    /// Return exact committed closure from the independently checked native
    /// primary/replica history at this already published anchor.
    fn full_sync_material(
        &self,
        _anchor: &VerifiedFormalReadAnchorV1,
    ) -> Result<ClosedRecord, FormalReadError> {
        Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: full-sync committed material is unavailable".into()))
    }

    /// Read one content-addressed retained-history object. The caller must
    /// prove its path from the exact requested snapshot root before returning
    /// bytes.
    fn full_sync_business_object(
        &self,
        _anchor: &VerifiedFormalReadAnchorV1,
        _digest: &str,
    ) -> Result<Option<Vec<u8>>, FormalReadError> {
        Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: full-sync retained CAS is unavailable".into()))
    }

    /// Warm the shared content-addressed cache from private local publications
    /// only after replaying their proofs from the configured external origin.
    /// Implementations must treat this as a byte-cache optimization: it cannot
    /// establish or advance a read anchor. Resolver test doubles may leave the
    /// default no-op implementation.
    fn prewarm_qualified_business_history(
        &self,
        _target: &VerifiedFormalReadAnchorV1,
        _reader: &dyn BusinessIndexNodeReader,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Warm only as a byte-cache optimization before the first native restore.
    /// The caller must supply a position and record ID obtained from identical
    /// fresh protected Head reads on both replicas; its authority ID must come
    /// from validated configuration, not either Head. The hint is never an
    /// anchor: implementations must replay the configured external origin and
    /// compare the resulting opaque authority/world context before seeding.
    /// The caller must still perform the complete dual native restore.
    fn prewarm_qualified_business_history_hint(
        &self,
        _hint: &FormalReadAnchorRefV1,
        _reader: &dyn BusinessIndexNodeReader,
    ) -> Result<Option<super::formal_live_history::VerifiedBusinessPrewarmReceipt>, String> {
        Ok(None)
    }

    /// Consume at most one successful hint warmup from this same open. An
    /// implementation may reuse its verified bytes only when the independently
    /// restored writer anchor and the complete opaque context match exactly.
    /// `true` reports reuse; it never authorizes skipping the dual native restore.
    fn prewarm_qualified_business_history_after_hint(
        &self,
        target: &VerifiedFormalReadAnchorV1,
        reader: &dyn BusinessIndexNodeReader,
        _receipt: Option<super::formal_live_history::VerifiedBusinessPrewarmReceipt>,
    ) -> Result<bool, String> {
        self.prewarm_qualified_business_history(target, reader)?;
        Ok(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalLiveStatusV1 {
    pub api_version: String,
    pub runtime_profile: String,
    pub state: String,
    pub ready: bool,
    pub can_propose: bool,
    pub node_id: String,
    pub world: WorldIdentity,
    pub node_running: bool,
    pub tick_count: u64,
    pub committed_position: Option<u64>,
    pub committed_record_id: Option<String>,
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_delivery: Option<super::formal_live_effect_delivery::FormalEffectDeliveryStatusV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormalServiceErrorV1 {
    api_version: String,
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FormalHealthV1 {
    api_version: String,
    ready: bool,
    profile: String,
}

#[derive(Clone)]
pub(crate) struct FormalLiveState {
    pub(crate) ready: Arc<AtomicBool>,
    pub(crate) stopping: Arc<AtomicBool>,
    pub(crate) node_running: Arc<AtomicBool>,
    pub(crate) tick_count: Arc<std::sync::atomic::AtomicU64>,
    pub(crate) last_error: Arc<Mutex<Option<String>>>,
    pub(crate) effect_delivery:
        Arc<Mutex<Option<super::formal_live_effect_delivery::FormalEffectDeliveryStatusV1>>>,
}
impl Default for FormalLiveState {
    fn default() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(false)),
            stopping: Arc::new(AtomicBool::new(false)),
            node_running: Arc::new(AtomicBool::new(false)),
            tick_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_error: Arc::new(Mutex::new(None)),
            effect_delivery: Arc::new(Mutex::new(None)),
        }
    }
}

pub(crate) struct FormalLiveService {
    node_id: String,
    world: WorldIdentity,
    signer: SigningKey,
    runtime: Arc<Mutex<NodeRuntime>>,
    published: Arc<PublishedReadState>,
    trust: oasis7::world_service::client::FormalReadTrustConfigV1,
    history: Arc<dyn FormalLiveHistoryResolver>,
    full_sync_material_cache: Mutex<FullSyncMaterialCache>,
    state: FormalLiveState,
    admission_gate: Arc<Mutex<()>>,
    full_sync_read_gate: Option<Arc<full_sync_read_gate::FullSyncV3ReadGate>>,
}
impl FormalLiveService {
    #[expect(
        clippy::too_many_arguments,
        reason = "The formal service receives its explicit trust and runtime dependencies."
    )]
    pub(crate) fn new(
        node_id: String,
        world: WorldIdentity,
        signer: SigningKey,
        runtime: Arc<Mutex<NodeRuntime>>,
        published: Arc<PublishedReadState>,
        trust: oasis7::world_service::client::FormalReadTrustConfigV1,
        history: Arc<dyn FormalLiveHistoryResolver>,
        state: FormalLiveState,
        admission_gate: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            node_id,
            world,
            signer,
            runtime,
            published,
            trust,
            history,
            full_sync_material_cache: Mutex::new(FullSyncMaterialCache::default()),
            state,
            admission_gate,
            full_sync_read_gate: None,
        }
    }

    pub(crate) fn with_full_sync_read_gate(
        mut self,
        gate: Arc<full_sync_read_gate::FullSyncV3ReadGate>,
    ) -> Result<Self, String> {
        let provider_key = hex::encode(self.signer.verifying_key().to_bytes());
        gate.validate_service_binding(&self.trust.initial_trust, &self.world, &provider_key)?;
        self.full_sync_read_gate = Some(gate);
        Ok(self)
    }

    pub(crate) fn serve_loopback(
        self: Arc<Self>,
        bind: SocketAddr,
    ) -> Result<FormalHttpServer, String> {
        if !bind.ip().is_loopback() || bind.port() == 0 {
            return Err("formal service bind must be a nonzero loopback address".into());
        }
        let listener =
            TcpListener::bind(bind).map_err(|error| format!("formal status bind: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let service = Arc::clone(&self);
        let join = thread::Builder::new()
            .name("oasis7-formal-http".into())
            // Formal proof/history validation and signed page construction
            // run inline on this single bounded HTTP worker. Use a fixed
            // stack budget rather than the platform default for spawned threads.
            .stack_size(FORMAL_HTTP_THREAD_STACK_BYTES)
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, peer)) => {
                            if !peer.ip().is_loopback() {
                                continue;
                            }
                            if let Err(error) = serve_connection(&service, &mut stream) {
                                let _ = write_http_reply(&mut stream, 400, &plain_error(&error));
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => {
                            if let Ok(mut last_error) = service.state.last_error.lock() {
                                *last_error = Some(format!("formal HTTP listener: {error}"));
                            }
                            service.state.ready.store(false, Ordering::Release);
                            break;
                        }
                    }
                }
            })
            .map_err(|error| format!("formal HTTP thread: {error}"))?;
        Ok(FormalHttpServer {
            stop,
            join: Some(join),
        })
    }

    fn handle(&self, method: &str, path: &str, body: &[u8]) -> HttpReply {
        match (method, path) {
            ("GET", SERVICE_READINESS_PATH) => self.handle_health(),
            ("GET", CHAIN_STATUS_PATH) => self.handle_status(),
            ("POST", VIEW_PATH) => self.handle_view(body),
            ("POST", SUBMIT_PATH) => self.handle_submit(body),
            ("POST", LOOKUP_PATH) => self.handle_lookup(body),
            ("POST", CHANGES_PATH) => self.handle_changes(body),
            (
                "POST",
                oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_MANIFEST_PATH,
            ) => full_sync_service::handle_manifest(self, body),
            ("POST", oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH) => {
                full_sync_service::handle_object(self, body)
            }
            _ => signed_error_with_digest(
                404,
                path,
                authority::request_digest(path, &body),
                "not_found",
                "formal route unavailable",
                &self.signer,
            ),
        }
    }

    fn handle_health(&self) -> HttpReply {
        let status = self.runtime_status();
        signed_reply(
            200,
            SERVICE_READINESS_PATH,
            &(),
            &FormalHealthV1 {
                api_version: "oasis7.formal-live-health/v1".into(),
                ready: status.ready,
                profile: "formal_live_v1".into(),
            },
            &self.signer,
        )
    }

    fn handle_status(&self) -> HttpReply {
        signed_reply(
            200,
            CHAIN_STATUS_PATH,
            &(),
            &self.runtime_status(),
            &self.signer,
        )
    }

    fn runtime_status(&self) -> FormalLiveStatusV1 {
        let stopping = self.state.stopping.load(Ordering::Acquire);
        let (snapshot, runtime_error) = if stopping {
            (None, None)
        } else {
            match self.runtime.lock() {
                Ok(runtime) => (Some(runtime.snapshot()), None),
                Err(_) => (None, Some("formal Node runtime lock poisoned".to_owned())),
            }
        };
        if let Some(snapshot) = &snapshot {
            self.state
                .node_running
                .store(snapshot.running, Ordering::Release);
            self.state
                .tick_count
                .store(snapshot.tick_count, Ordering::Release);
        }
        let cut = self
            .published
            .observe(|cut| {
                Ok((
                    cut.anchor.head().head().record_id.clone(),
                    cut.anchor.head().record().position,
                ))
            })
            .ok();
        let state_error = self
            .state
            .last_error
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let node_running = snapshot.as_ref().map_or_else(
            || self.state.node_running.load(Ordering::Acquire),
            |snapshot| snapshot.running,
        );
        let node_error = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.last_error.clone())
            .map(|error| error.chars().take(MAX_NODE_ERROR_CHARS).collect());
        let last_error = state_error.or(node_error).or(runtime_error);
        let ready = self.state.ready.load(Ordering::Acquire)
            && !stopping
            && node_running
            && cut.is_some()
            && last_error.is_none();
        let (record, position) = cut.unzip();
        FormalLiveStatusV1 {
            api_version: "oasis7.formal-live-status/v1".into(),
            runtime_profile: "formal_live_v1".into(),
            state: if stopping {
                "stopping"
            } else if last_error.is_some() {
                "degraded"
            } else if ready {
                "ready"
            } else if node_running {
                "starting"
            } else {
                "stopped"
            }
            .into(),
            ready,
            can_propose: ready,
            node_id: self.node_id.clone(),
            world: self.world.clone(),
            node_running,
            tick_count: snapshot.as_ref().map_or_else(
                || self.state.tick_count.load(Ordering::Acquire),
                |snapshot| snapshot.tick_count,
            ),
            committed_position: position,
            committed_record_id: record,
            last_error,
            effect_delivery: self
                .state
                .effect_delivery
                .lock()
                .ok()
                .and_then(|state| state.clone()),
        }
    }

    fn handle_view(&self, body: &[u8]) -> HttpReply {
        let request: SignedReadRequest<ReadWorldViewRequest> = match parse_strict(body) {
            Ok(request) => request,
            Err(error) => {
                return raw_error(VIEW_PATH, body, "invalid_request", &error, &self.signer);
            }
        };
        let digest = authority::request_digest(VIEW_PATH, &request);
        let result = self.read_view(&request);
        match result {
            Ok(response) => {
                signed_reply_with_digest(200, VIEW_PATH, digest, &response, &self.signer)
            }
            Err(error) => {
                let status = match error {
                    FormalReadError::HistoryUnavailable(_) => 410,
                    FormalReadError::NotReady(_) => 503,
                    _ => 400,
                };
                signed_error_with_digest(
                    status,
                    VIEW_PATH,
                    digest,
                    "formal_read_rejected",
                    &error.to_string(),
                    &self.signer,
                )
            }
        }
    }

    fn read_view(
        &self,
        signed: &SignedReadRequest<ReadWorldViewRequest>,
    ) -> Result<ReadWorldViewResponse<oasis7_world_projection::WorldServiceProjection>, FormalReadError>
    {
        self.read_view_with_publication(signed)
            .map(|(response, _publication)| response)
    }

    /// Read a signed view while retaining the exact private publication which
    /// produced it. Business lookup consumes this typed publication directly;
    /// it never reconstructs trust from the response's public CommitRef.
    fn read_view_with_publication(
        &self,
        signed: &SignedReadRequest<ReadWorldViewRequest>,
    ) -> Result<
        (
            ReadWorldViewResponse<oasis7_world_projection::WorldServiceProjection>,
            FormalReadPublication,
        ),
        FormalReadError,
    > {
        #[cfg(test)]
        let mut lookup_stage = {
            let enabled = std::env::var_os("OASIS7_FORMAL_LOOKUP_TIMING").is_some();
            let started = std::time::Instant::now();
            let mut previous = started;
            move |stage: &str| {
                if enabled {
                    eprintln!(
                        "formal_read_view_timing stage={stage} stage_ms={} elapsed_ms={}",
                        previous.elapsed().as_millis(),
                        started.elapsed().as_millis()
                    );
                }
                previous = std::time::Instant::now();
            }
        };
        let status = self.runtime_status();
        let degraded_read_only =
            !status.ready && self.state.ready.load(Ordering::Acquire) && status.state == "degraded";
        if !status.ready && !degraded_read_only {
            return Err(FormalReadError::NotReady("formal runtime is not ready".into()));
        }
        let request = &signed.request;
        request
            .validate_formal_shape()
            .map_err(|error| FormalReadError::RequestRejected(error.to_string()))?;
        if request.world != self.world {
            return Err(FormalReadError::RequestRejected("formal read requires the configured world".into()));
        }
        authority::verify_read_request(VIEW_PATH, signed).map_err(FormalReadError::RequestRejected)?;
        let current = self.published.current_for_read()?;
        if degraded_read_only {
            // A failed write path may keep serving the last independently
            // qualified publication. In degraded mode, do not present that
            // stale cut as a fresh read: the caller must pin the exact current
            // CommitRef, and its normal proof verification below remains in
            // force. The fixed CommitRef is only a selector; it cannot create
            // or advance the opaque published anchor.
            let fixed = request
                .fixed_commit
                .as_ref()
                .ok_or_else(|| FormalReadError::RequestRejected("formal runtime is degraded; an exact committed read is required".into()))?;
            if fixed.world != self.world
                || fixed.position != current.anchor.head().record().position
                || fixed.execution_block_hash != current.anchor.head().head().record_id
            {
                return Err(FormalReadError::RequestRejected("formal runtime is degraded; only its last qualified CommitRef is readable".into()));
            }
        }
        // Always authorize against the currently published World before
        // resolving any older or intermediate target. A historical target's
        // permission state must never revive a revoked Agent reader.
        FormalReadPublication::new(current.anchor.clone(), current.world.clone())?
            .authorize_audience(&request.scope_id, &signed.subject_public_key)?;
        let origin_ref = request
            .formal_anchor
            .as_ref()
            .ok_or_else(|| FormalReadError::RequestRejected("formal read caller anchor missing".into()))?;
        #[cfg(test)]
        lookup_stage("authorization_and_current_publication");
        let origin = if *origin_ref == anchor_ref(&current.anchor)? {
            FormalReadPublication::new(current.anchor.clone(), current.world.clone())?
        } else {
            self.history.origin_publication(origin_ref)?
        };
        #[cfg(test)]
        lookup_stage("origin_material");
        validate_anchor_ref(origin_ref, origin.anchor())?;
        let origin_anchor = origin.anchor().clone();
        let target = if let Some(fixed) = &request.fixed_commit {
            let target = if fixed.position == current.anchor.head().record().position
                && fixed.execution_block_hash == current.anchor.head().head().record_id
                && fixed.world == self.world
            {
                FormalReadPublication::new(current.anchor.clone(), current.world.clone())?
            } else {
                self.history.fixed_publication(fixed)?
            };
            let target_anchor = target.anchor().clone();
            if !same_anchor(&target_anchor, &current.anchor)? {
                if target_anchor.head().record().position > current.anchor.head().record().position
                {
                    return Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: fixed formal read is ahead of the live publisher".into()));
                }
                let live_publication =
                    FormalReadPublication::new(current.anchor.clone(), current.world.clone())?;
                let target_to_live = self.history.history_page(
                    &target_anchor,
                    &current.anchor,
                    MAX_FORMAL_HISTORY_STEPS,
                )?;
                if target_to_live.schema_version != 1 {
                    return Err(FormalReadError::Integrity("formal fixed-to-live history schema".into()));
                }
                if target_to_live.steps.len() > MAX_FORMAL_HISTORY_STEPS {
                    return Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: fixed-to-live history exceeds the 128-step bound".into()));
                }
                let live_probe = ReadWorldViewRequest {
                    observation_nonce: Some("07".repeat(32)),
                    formal_anchor: Some(anchor_ref(&target_anchor)?),
                    contract_version:
                        oasis7_client_api::world_service::WORLD_SERVICE_CONTRACT_VERSION,
                    world: self.world.clone(),
                    scope_id: "public".into(),
                    min_commit: None,
                    fixed_commit: None,
                    deadline_unix_ms: None,
                };
                let live_probe = authority::sign_read_request(
                    VIEW_PATH,
                    live_probe,
                    &hex::encode(self.signer.to_bytes()),
                ).map_err(FormalReadError::Internal)?;
                let live_service = FormalCommittedReadService::new(
                    self.trust.initial_trust.clone(),
                    live_publication,
                )?;
                let live_response = live_service.read(&live_probe, &target, &target_to_live)?;
                let mut verifier =
                    FormalReadVerifier::new(&self.trust.initial_trust, target_anchor.clone()).map_err(FormalReadError::Integrity)?;
                oasis7_world_projection::formal_read::verify_view(
                    &mut verifier,
                    &live_response,
                    &live_probe.request,
                    &live_probe.subject_public_key,
                ).map_err(FormalReadError::Integrity)?;
            }
            target
        } else {
            // Use the already qualified current World for nearby readers. A
            // distant reader advances through an exact native intermediate cut
            // so the per-page bound never becomes a total-history limit.
            let distance = current
                .anchor
                .head()
                .record()
                .position
                .checked_sub(origin_anchor.head().record().position)
                .ok_or_else(|| FormalReadError::HistoryUnavailable("HistoryUnavailable: caller anchor is ahead of live publication".into()))?;
            if distance > MAX_FORMAL_HISTORY_STEPS as u64 {
                self.history.forward_publication(
                    &origin_anchor,
                    &current.anchor,
                    MAX_FORMAL_HISTORY_STEPS,
                )?
            } else {
                FormalReadPublication::new(current.anchor.clone(), current.world.clone())?
            }
        };
        let target_anchor = target.anchor().clone();
        if target_anchor.head().record().position > current.anchor.head().record().position {
            return Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: selected formal cut is ahead of live publisher".into()));
        }
        let history = if same_anchor(&origin_anchor, &target_anchor)? {
            FormalReadHistoryV1 {
                schema_version: 1,
                steps: Vec::new(),
            }
        } else {
            self.history
                .history_page(&origin_anchor, &target_anchor, MAX_FORMAL_HISTORY_STEPS)?
        };
        if history.schema_version != 1 {
            return Err(FormalReadError::Integrity("formal read history schema".into()));
        }
        if history.steps.len() > MAX_FORMAL_HISTORY_STEPS {
            return Err(FormalReadError::HistoryUnavailable("HistoryUnavailable: formal read history page exceeds the 128-step bound".into()));
        }
        #[cfg(test)]
        lookup_stage("target_selection_and_history_page");
        let service = FormalCommittedReadService::new(self.trust.initial_trust.clone(), target)?;
        let response = service.read(signed, &origin, &history)?;
        #[cfg(test)]
        lookup_stage("committed_read_service");
        response
            .validate(request)
            .map_err(|error| FormalReadError::Internal(error.to_string()))?;
        Ok((response, service.into_publication()))
    }

    fn handle_submit(&self, body: &[u8]) -> HttpReply {
        let request: SubmitIntentRequest<WorldServicePayloadV1> = match parse_strict(body) {
            Ok(request) => request,
            Err(error) => {
                return raw_error(SUBMIT_PATH, body, "invalid_request", &error, &self.signer);
            }
        };
        let digest = authority::request_digest(SUBMIT_PATH, &request);
        match self.submit(request) {
            Ok(response) => {
                signed_reply_with_digest(200, SUBMIT_PATH, digest, &response, &self.signer)
            }
            Err((status, code, message)) => {
                signed_error_with_digest(status, SUBMIT_PATH, digest, code, &message, &self.signer)
            }
        }
    }

    fn submit(
        &self,
        request: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<IntentResponse<serde_json::Value>, (u16, &'static str, String)> {
        let _admission = self.admission_gate.lock().map_err(|_| {
            (
                503,
                "runtime_unavailable",
                "Node admission gate poisoned".into(),
            )
        })?;
        request
            .validate()
            .map_err(|error| (400, "invalid_request", error.to_string()))?;
        if !self.runtime_status().ready {
            return Err((503, "not_ready", "formal runtime is not ready".into()));
        }
        if request
            .deadline_unix_ms
            .is_some_and(|deadline| deadline < now_unix_ms())
        {
            return Err((408, "expired", "submit deadline has expired".into()));
        }
        let cut = self
            .published
            .current()
            .map_err(|error| (503, "not_ready", error))?;
        if request.correlation.key.world != self.world {
            return Err((
                400,
                "world_mismatch",
                "submit world differs from formal runtime".into(),
            ));
        }
        let actual_correlation =
            correlation::derive_correlation(self.world.clone(), &request.signed_payload)
                .map_err(|error| (400, "invalid_payload", error))?;
        if actual_correlation != request.correlation {
            return Err((
                400,
                "correlation_mismatch",
                "signed payload correlation mismatch".into(),
            ));
        }
        validate_payload(&cut.world, &request.signed_payload)
            .map_err(|error| (400, "invalid_payload", error))?;
        if let WorldServicePayloadV1::GameParticipantRegister { owner, admission } =
            &request.signed_payload
        {
            let binding = cut
                .world
                .current_cognition_runtime_binding()
                .map_err(|e| (400, "invalid_payload", format!("{e:?}")))?;
            oasis7_runtime::service_admission::participant::validate_registration(
                &cut.world,
                owner,
                admission,
                &binding.branch_id,
                binding.reorg_epoch,
            )
            .map_err(|e| (400, "invalid_payload", e))?;
        }
        let key_digest = correlation::key_digest(&request.correlation.key)
            .map_err(|error| (400, "invalid_payload", error))?;
        let digest = blake3::hash(key_digest.as_bytes());
        let action_id =
            u64::from_be_bytes(digest.as_bytes()[..8].try_into().expect("hash width")).max(1);
        let action = correlation::encode_consensus_intent(&request)
            .map_err(|error| (400, "invalid_payload", error))?;
        let runtime = self.runtime.lock().map_err(|_| {
            (
                503,
                "runtime_unavailable",
                "Node runtime lock poisoned".into(),
            )
        })?;
        runtime
            .submit_consensus_action_payload(action_id, action)
            .map_err(|error| {
                (
                    503,
                    "action_queue_unavailable",
                    format!("Node action queue rejected submit: {error:?}"),
                )
            })?;
        Ok(IntentResponse {
            formal_profile: Some(FormalReadProfileV1::controlled()),
            execution_evidence_scope: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation: actual_correlation,
            outcome: IntentOutcome::Received {
                durability: AdmissionDurability::Volatile,
            },
        })
    }

    fn handle_lookup(&self, body: &[u8]) -> HttpReply {
        if let Ok(request) =
            parse_strict::<oasis7::world_service::wire::FormalAuthenticatedLookupV1>(body)
        {
            return self.handle_formal_lookup(request);
        }
        let request: AuthenticatedLookup = match parse_strict(body) {
            Ok(request) => request,
            Err(error) => {
                return raw_error(LOOKUP_PATH, body, "invalid_request", &error, &self.signer);
            }
        };
        let digest = authority::request_digest(LOOKUP_PATH, &request);
        let validation = (|| {
            request
                .request
                .validate()
                .map_err(|error| error.to_string())?;
            let actual = correlation::derive_correlation(self.world.clone(), &request.original)?;
            if actual.key != request.request.key {
                return Err("lookup key does not match the signed payload".into());
            }
            validate_payload(&self.published.current()?.world, &request.original)?;
            Ok::<_, String>(())
        })();
        if let Err(error) = validation {
            return signed_error_with_digest(
                400,
                LOOKUP_PATH,
                digest,
                "invalid_lookup",
                &error,
                &self.signer,
            );
        }
        signed_error_with_digest(
            503,
            LOOKUP_PATH,
            digest,
            "formal_business_proof_unavailable",
            "lookup requires a qualified formal business-result proof",
            &self.signer,
        )
    }

    fn handle_changes(&self, body: &[u8]) -> HttpReply {
        if let Ok(request) =
            parse_strict::<oasis7::world_service::wire::FormalAuthenticatedChangesV1>(body)
        {
            return self.handle_formal_changes(request);
        }
        let request: SignedReadRequest<ReadWorldChangesRequest> = match parse_strict(body) {
            Ok(request) => request,
            Err(error) => {
                return raw_error(CHANGES_PATH, body, "invalid_request", &error, &self.signer);
            }
        };
        let digest = authority::request_digest(CHANGES_PATH, &request);
        let validation = (|| {
            authority::verify_read_request(CHANGES_PATH, &request)?;
            request
                .request
                .validate()
                .map_err(|error| error.to_string())?;
            if request.request.cursor.commit.world != self.world
                || request.request.cursor.scope_id != "public"
            {
                return Err("formal changes require the configured world and public scope".into());
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = validation {
            return signed_error_with_digest(
                400,
                CHANGES_PATH,
                digest,
                "invalid_changes_request",
                &error,
                &self.signer,
            );
        }
        signed_error_with_digest(
            503,
            CHANGES_PATH,
            digest,
            "formal_business_proof_unavailable",
            "changes require qualified formal business-result proofs",
            &self.signer,
        )
    }
}

fn validate_payload(world: &World, payload: &WorldServicePayloadV1) -> Result<(), String> {
    match payload {
        WorldServicePayloadV1::GameStarterClaim(signed) => {
            let binding = world
                .current_cognition_runtime_binding()
                .map_err(|e| format!("{e:?}"))?;
            oasis7_runtime::service_admission::starter::validate_owner(
                world,
                signed,
                &binding.branch_id,
                binding.reorg_epoch,
            )
        }
        WorldServicePayloadV1::GameParticipantRegister { owner, admission } => {
            let binding = world
                .current_cognition_runtime_binding()
                .map_err(|e| format!("{e:?}"))?;
            oasis7_runtime::service_admission::participant::validate_admission(
                world,
                owner,
                admission,
                &binding.branch_id,
                binding.reorg_epoch,
            )
        }
        WorldServicePayloadV1::GameTaskCreate(signed) => game_task::validate_owner(world, signed),
        WorldServicePayloadV1::GameGoalPublish(signed) => {
            game_task::validate_publish_owner(world, signed)
        }
        WorldServicePayloadV1::GameExecutorControl(signed) => {
            game_task::validate_executor_owner(world, signed)
        }
        WorldServicePayloadV1::GameplayJson(bytes) => {
            gameplay::authenticated_action(world, bytes).map(|_| ())
        }
        WorldServicePayloadV1::SafeAlternative(signed) => {
            oasis7_runtime::service_admission::safe_alternative::validate_owner(world, signed)
        }
        WorldServicePayloadV1::AgentChat(request) => agent_chat::validate_owner(world, request),
        WorldServicePayloadV1::Cognition(signed) => {
            agent_authority::validate_cognition(world, signed)
        }
        WorldServicePayloadV1::FeedbackAck(signed) => {
            agent_authority::validate_feedback_ack(world, signed)
        }
        WorldServicePayloadV1::Delegation(signed) => {
            agent_authority::validate_delegation(world, signed)
        }
        WorldServicePayloadV1::Scheduler(signed) => {
            authority::verify_read_request("scheduler", signed)?;
            agent_authority::validate_agent_signer(
                world,
                &signed.request.agent_id,
                &signed.subject_public_key,
                signed.request.delegation_generation,
            )
        }
    }
}

fn anchor_ref(anchor: &VerifiedFormalReadAnchorV1) -> Result<FormalReadAnchorRefV1, FormalReadError> {
    Ok(FormalReadAnchorRefV1 {
        position: anchor.head().record().position,
        record_id: anchor.head().head().record_id.clone(),
        active_authority_id: anchor
            .authority()
            .authority_id()
            .map_err(FormalReadError::from)?,
    })
}

fn validate_anchor_ref(
    expected: &FormalReadAnchorRefV1,
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<(), FormalReadError> {
    if &anchor_ref(anchor)? != expected {
        return Err(FormalReadError::Integrity("formal read origin differs from exact caller anchor".into()));
    }
    Ok(())
}

fn same_anchor(
    first: &VerifiedFormalReadAnchorV1,
    second: &VerifiedFormalReadAnchorV1,
) -> Result<bool, FormalReadError> {
    Ok(anchor_ref(first)? == anchor_ref(second)?)
}

pub(super) fn parse_strict<T: DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    if body.is_empty() || body.len() > MAX_REQUEST_BODY {
        return Err("request body is empty or exceeds the 256 KiB bound".into());
    }
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let mut ignored = false;
    let request = serde_ignored::deserialize(&mut decoder, |_| ignored = true)
        .map_err(|_| "invalid JSON request".to_owned())?;
    decoder
        .end()
        .map_err(|_| "invalid trailing JSON request data".to_owned())?;
    if ignored {
        return Err("unsupported request field".into());
    }
    Ok(request)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[path = "formal_live_service_http.rs"]
pub(super) mod http;
pub(crate) use http::FormalHttpServer;
use http::*;
