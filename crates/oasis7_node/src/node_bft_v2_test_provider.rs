//! Real loopback-TCP V2 signer fixture for Node tests.
//!
//! The backend is an in-memory test double, not durable or protected storage.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::protected_bft_v2::{
    BftSignerAcquireOutcomeV2, BftSignerBackendSnapshotV2, BftSignerBackendV2, BftSignerBindingV2,
    BftSignerCheckpointV2, BftSignerErrorV2, BftSignerProviderV2, BftSignerServiceV2,
    BftSignerStateV2, BftSignerTrustV2, BftSigningKeyHandleV2, RemoteBftSignerProviderV2,
};
use oasis7_proto::distributed_bft::v2::BftCommitCertificateV2;

const TEST_PROVIDER_TIMEOUT: Duration = Duration::from_secs(3);
const TEST_CLOCK_INCREMENT_MS: i64 = 10;
const MAX_TEST_PROVIDER_CONNECTIONS: usize = 64;

pub(crate) struct SoftwareKey(pub(crate) SigningKey);

impl BftSigningKeyHandleV2 for SoftwareKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }

    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV2> {
        Ok(self.0.sign(preimage).to_bytes())
    }
}

struct MemoryLedger {
    binding_digest: String,
    snapshot: BftSignerBackendSnapshotV2,
    now_ms: i64,
    fail_next_install: bool,
    drop_next_install_reply: bool,
    install_reply_lost: bool,
}

/// In-memory backend implementing the same atomic generation/state contract
/// as the V2 service expects. It is test infrastructure only.
#[derive(Clone)]
pub(crate) struct MemoryBackend(Arc<Mutex<MemoryLedger>>);

impl MemoryBackend {
    pub(crate) fn provision(binding: &BftSignerBindingV2) -> Result<Self, BftSignerErrorV2> {
        Ok(Self(Arc::new(Mutex::new(MemoryLedger {
            binding_digest: binding.digest()?,
            snapshot: BftSignerBackendSnapshotV2 {
                state: BftSignerStateV2::genesis(binding)?,
                lease_generation: 1,
                lease_token_digest: String::new(),
                protected_time_floor_ms: 100,
            },
            now_ms: 100,
            fail_next_install: false,
            drop_next_install_reply: false,
            install_reply_lost: false,
        }))))
    }

    pub(crate) fn snapshot(&self) -> BftSignerBackendSnapshotV2 {
        self.0
            .lock()
            .expect("Node V2 test backend mutex")
            .snapshot
            .clone()
    }

    pub(crate) fn fail_next_complete_transition_install(&self) {
        self.0
            .lock()
            .expect("Node V2 test backend mutex")
            .fail_next_install = true;
    }

    pub(crate) fn drop_next_complete_transition_install_reply(&self) {
        self.0
            .lock()
            .expect("Node V2 test backend mutex")
            .drop_next_install_reply = true;
    }

    /// Simulates an external test operator proving the old service is stopped
    /// and fencing its lease. It deliberately preserves signer state and time.
    fn operator_fence_stopped_service_for_test(
        &self,
        expected_generation: u64,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let mut ledger = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("Node V2 test backend mutex poisoned".into()))?;
        if ledger.snapshot.lease_generation != expected_generation
            || ledger.snapshot.lease_token_digest.is_empty()
        {
            return Err(BftSignerErrorV2(
                "Node V2 test operator fence did not match an active stale lease".into(),
            ));
        }
        ledger.snapshot.lease_generation = expected_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV2("Node V2 test generation overflow".into()))?;
        ledger.snapshot.lease_token_digest.clear();
        Ok(Self::snapshot_of(&ledger))
    }

    fn take_install_reply_loss(&self) -> bool {
        let Ok(mut ledger) = self.0.lock() else {
            return false;
        };
        std::mem::take(&mut ledger.install_reply_lost)
    }

    fn with_active_lease<T>(
        &self,
        generation: u64,
        token_digest: &str,
        operation: impl FnOnce(&mut MemoryLedger) -> Result<T, BftSignerErrorV2>,
    ) -> Result<T, BftSignerErrorV2> {
        let mut ledger = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("Node V2 test backend mutex poisoned".into()))?;
        Self::require_lease(&ledger, generation, token_digest)?;
        operation(&mut ledger)
    }

    fn require_lease(
        ledger: &MemoryLedger,
        generation: u64,
        token_digest: &str,
    ) -> Result<(), BftSignerErrorV2> {
        if generation != ledger.snapshot.lease_generation
            || token_digest.is_empty()
            || token_digest != ledger.snapshot.lease_token_digest
        {
            return Err(BftSignerErrorV2("Node V2 test lease is fenced".into()));
        }
        Ok(())
    }

    fn snapshot_of(ledger: &MemoryLedger) -> BftSignerBackendSnapshotV2 {
        ledger.snapshot.clone()
    }

    fn advance_clock(ledger: &mut MemoryLedger) -> Result<i64, BftSignerErrorV2> {
        let now_ms = ledger
            .now_ms
            .checked_add(TEST_CLOCK_INCREMENT_MS)
            .ok_or_else(|| BftSignerErrorV2("Node V2 test clock overflow".into()))?;
        if now_ms < ledger.snapshot.protected_time_floor_ms {
            return Err(BftSignerErrorV2("Node V2 test clock rollback".into()));
        }
        ledger.now_ms = now_ms;
        ledger.snapshot.protected_time_floor_ms = now_ms;
        Ok(now_ms)
    }
}

impl BftSignerBackendV2 for MemoryBackend {
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let ledger = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("Node V2 test backend mutex poisoned".into()))?;
        if binding_digest != ledger.binding_digest {
            return Err(BftSignerErrorV2("Node V2 test binding mismatch".into()));
        }
        Ok(Self::snapshot_of(&ledger))
    }

    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerAcquireOutcomeV2, BftSignerErrorV2> {
        let mut ledger = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("Node V2 test backend mutex poisoned".into()))?;
        if binding_digest != ledger.binding_digest {
            return Err(BftSignerErrorV2("Node V2 test binding mismatch".into()));
        }
        if expected_generation != ledger.snapshot.lease_generation
            || !ledger.snapshot.lease_token_digest.is_empty()
        {
            return Ok(BftSignerAcquireOutcomeV2::Conflict(Self::snapshot_of(
                &ledger,
            )));
        }
        ledger.snapshot.lease_generation = ledger
            .snapshot
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerErrorV2("Node V2 test generation overflow".into()))?;
        ledger.snapshot.lease_token_digest = new_lease_token_digest.to_string();
        Ok(BftSignerAcquireOutcomeV2::Acquired(Self::snapshot_of(
            &ledger,
        )))
    }

    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        let mut ledger = self
            .0
            .lock()
            .map_err(|_| BftSignerErrorV2("Node V2 test backend mutex poisoned".into()))?;
        if ledger.snapshot.lease_generation == expected_generation
            && ledger.snapshot.lease_token_digest == expected_token_digest
        {
            ledger.snapshot.lease_generation = ledger
                .snapshot
                .lease_generation
                .checked_add(1)
                .ok_or_else(|| BftSignerErrorV2("Node V2 test generation overflow".into()))?;
            ledger.snapshot.lease_token_digest.clear();
        }
        Ok(Self::snapshot_of(&ledger))
    }

    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, |ledger| {
            Ok(Self::snapshot_of(ledger))
        })
    }

    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, Self::advance_clock)
    }

    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, |ledger| {
            if ledger.snapshot.state.revision != expected_revision
                || ledger.snapshot.state.state_digest != expected_state_digest
                || expected_revision.checked_add(1) != Some(next_state.revision)
            {
                return Err(BftSignerErrorV2("Node V2 test CAS mismatch".into()));
            }
            if minimum_event_time_ms
                .is_some_and(|time| time > ledger.snapshot.protected_time_floor_ms)
            {
                return Err(BftSignerErrorV2(
                    "Node V2 test event time is in the future".into(),
                ));
            }
            ledger.snapshot.state = next_state;
            Ok(Self::snapshot_of(ledger))
        })
    }

    fn complete_transition_install(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: i64,
        next_lease_token_digest: &str,
        next_state: BftSignerStateV2,
    ) -> Result<BftSignerBackendSnapshotV2, BftSignerErrorV2> {
        self.with_active_lease(lease_generation, lease_token_digest, |ledger| {
            if ledger.fail_next_install {
                ledger.fail_next_install = false;
                return Err(BftSignerErrorV2(
                    "injected Node V2 test install failure".into(),
                ));
            }
            if ledger.snapshot.state.revision != expected_revision
                || ledger.snapshot.state.state_digest != expected_state_digest
                || expected_revision.checked_add(1) != Some(next_state.revision)
                || minimum_event_time_ms > ledger.snapshot.protected_time_floor_ms
            {
                return Err(BftSignerErrorV2("Node V2 test install CAS mismatch".into()));
            }
            ledger.snapshot.state = next_state;
            ledger.snapshot.lease_generation = lease_generation
                .checked_add(1)
                .ok_or_else(|| BftSignerErrorV2("Node V2 test generation overflow".into()))?;
            ledger.snapshot.lease_token_digest = next_lease_token_digest.to_string();
            if ledger.drop_next_install_reply {
                ledger.drop_next_install_reply = false;
                ledger.install_reply_lost = true;
            }
            Ok(Self::snapshot_of(ledger))
        })
    }
}

/// Live authenticated signer service and its real remote TCP client.
pub(crate) struct ProtectedBftV2TestProvider {
    pub(crate) binding: BftSignerBindingV2,
    pub(crate) provider: Arc<RemoteBftSignerProviderV2>,
    pub(crate) backend: MemoryBackend,
    pub(crate) server: TcpServiceServer,
}

impl ProtectedBftV2TestProvider {
    pub(crate) fn start(
        binding: BftSignerBindingV2,
        requester_key: SigningKey,
        provider_key: SigningKey,
        validator_keys: Vec<SigningKey>,
    ) -> Result<Self, BftSignerErrorV2> {
        let backend = MemoryBackend::provision(&binding)?;
        Self::start_with_backend(
            binding,
            backend,
            requester_key,
            provider_key,
            validator_keys,
        )
    }

    /// Reopens the same provisioned test backend behind a new real TCP
    /// service. This models service restart while preserving its durable
    /// checkpoint and lease-generation state.
    pub(crate) fn start_with_backend(
        binding: BftSignerBindingV2,
        backend: MemoryBackend,
        requester_key: SigningKey,
        provider_key: SigningKey,
        validator_keys: Vec<SigningKey>,
    ) -> Result<Self, BftSignerErrorV2> {
        let provider_public_key = hex::encode(provider_key.verifying_key().to_bytes());
        let trust = BftSignerTrustV2 {
            schema_version: 2,
            binding_digest: binding.digest()?,
            provider_public_key,
        };
        let signing_keys = validator_keys
            .into_iter()
            .map(|key| Box::new(SoftwareKey(key)) as Box<dyn BftSigningKeyHandleV2>)
            .collect();
        let service = Arc::new(BftSignerServiceV2::open(
            binding.clone(),
            Box::new(SoftwareKey(provider_key)),
            signing_keys,
            Box::new(backend.clone()),
        )?);
        let server = TcpServiceServer::start(service.clone(), backend.clone())?;
        let provider = Arc::new(RemoteBftSignerProviderV2::new(
            server.address(),
            TEST_PROVIDER_TIMEOUT,
            binding.clone(),
            requester_key,
            trust,
        )?);
        Ok(Self {
            binding,
            provider,
            backend,
            server,
        })
    }

    /// Models an explicit external operator fencing an uncertain, stopped
    /// service in tests. This is not automatic service recovery: the old TCP
    /// server and every session are shut down and joined first, then the test
    /// backend advances only its lease generation and clears the lease token.
    /// The signer state, history, pending intents, receipts, and protected-time
    /// floor must remain unchanged before the same backend is reopened.
    pub(crate) fn operator_fence_and_restart_for_test(
        &mut self,
        expected_generation: u64,
        requester_key: SigningKey,
        provider_key: SigningKey,
        validator_keys: Vec<SigningKey>,
    ) -> Result<(), BftSignerErrorV2> {
        self.server.shutdown_and_join()?;

        let before = self.backend.snapshot();
        if before.lease_generation != expected_generation || before.lease_token_digest.is_empty() {
            return Err(BftSignerErrorV2(
                "Node V2 test operator fence expected an active stale lease".into(),
            ));
        }
        let stale_token_digest = before.lease_token_digest.clone();
        let fenced = self
            .backend
            .operator_fence_stopped_service_for_test(expected_generation)?;
        if fenced.state != before.state
            || fenced.protected_time_floor_ms != before.protected_time_floor_ms
            || fenced.lease_generation != expected_generation.saturating_add(1)
            || !fenced.lease_token_digest.is_empty()
        {
            return Err(BftSignerErrorV2(
                "Node V2 test operator fence changed signer state or time".into(),
            ));
        }
        if self
            .backend
            .read_for_lease(expected_generation, &stale_token_digest)
            .is_ok()
        {
            return Err(BftSignerErrorV2(
                "Node V2 test operator fence left stale credentials usable".into(),
            ));
        }

        let replacement = Self::start_with_backend(
            self.binding.clone(),
            self.backend.clone(),
            requester_key,
            provider_key,
            validator_keys,
        )?;
        *self = replacement;
        Ok(())
    }

    /// Advance the actual remote signer through verified certificates. This
    /// is test setup for late-join and recovery scenarios, not a backend
    /// mutation shortcut.
    pub(crate) fn append_history(
        &self,
        certificates: &[BftCommitCertificateV2],
    ) -> Result<BftSignerCheckpointV2, BftSignerErrorV2> {
        let mut lease = self.provider.acquire(&self.binding)?;
        let mut checkpoint = lease.checkpoint()?;
        for certificate in certificates {
            if let Some(journal) = checkpoint.state.journal.as_ref() {
                if journal.height_started_at_ms().is_none() {
                    checkpoint = lease.start_height_timer(checkpoint.state.revision)?;
                }
                let journal =
                    checkpoint.state.journal.as_ref().ok_or_else(|| {
                        BftSignerErrorV2("test signer journal disappeared".into())
                    })?;
                match journal.pending_commit().cloned() {
                    Some(pending) if pending == certificate.clone() => {}
                    Some(_) => {
                        return Err(BftSignerErrorV2(
                            "test history certificate conflicts with the reserved commit".into(),
                        ));
                    }
                    None => {
                        let mut candidate = checkpoint.state.candidate();
                        candidate
                            .journal
                            .as_mut()
                            .ok_or_else(|| {
                                BftSignerErrorV2("test signer journal disappeared".into())
                            })?
                            .pending_commit = Some(certificate.clone());
                        checkpoint = lease.advance(
                            checkpoint.state.revision,
                            &checkpoint.state.state_digest,
                            candidate,
                        )?;
                    }
                }
            }
            checkpoint = lease.append_committed_certificate(
                checkpoint.state.revision,
                &checkpoint.state.state_digest,
                certificate.clone(),
            )?;
        }
        Ok(checkpoint)
    }
}

pub(crate) struct TcpServiceServer {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    errors: Arc<Mutex<Vec<String>>>,
    connections: Arc<Mutex<BTreeMap<usize, TcpStream>>>,
    backend: MemoryBackend,
    thread: Option<JoinHandle<()>>,
}

impl TcpServiceServer {
    fn start(
        service: Arc<BftSignerServiceV2>,
        backend: MemoryBackend,
    ) -> Result<Self, BftSignerErrorV2> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| BftSignerErrorV2(format!("bind Node V2 test provider: {error}")))?;
        listener.set_nonblocking(true).map_err(|error| {
            BftSignerErrorV2(format!("configure Node V2 test listener: {error}"))
        })?;
        let address = listener.local_addr().map_err(|error| {
            BftSignerErrorV2(format!("read Node V2 test listener address: {error}"))
        })?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let errors = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(Mutex::new(BTreeMap::new()));
        let next_connection_id = Arc::new(AtomicUsize::new(0));
        let thread_shutdown = shutdown.clone();
        let thread_errors = errors.clone();
        let thread_connections = connections.clone();
        let thread_next_connection_id = next_connection_id.clone();
        let thread_backend = backend.clone();
        let thread = thread::spawn(move || {
            let mut clients = Vec::new();
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let stream_config = stream
                            .set_nonblocking(false)
                            .and_then(|()| stream.set_read_timeout(Some(TEST_PROVIDER_TIMEOUT)))
                            .and_then(|()| stream.set_write_timeout(Some(TEST_PROVIDER_TIMEOUT)));
                        if let Err(error) = stream_config {
                            if let Ok(mut recorded) = thread_errors.lock() {
                                recorded
                                    .push(format!("configure Node V2 test connection: {error}"));
                            }
                            continue;
                        }
                        let connection_id = match thread_next_connection_id.fetch_update(
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                            |next| next.checked_add(1),
                        ) {
                            Ok(id) => id,
                            Err(_) => {
                                if let Ok(mut recorded) = thread_errors.lock() {
                                    recorded.push("Node V2 test connection id overflow".into());
                                }
                                break;
                            }
                        };
                        let control_stream = match stream.try_clone() {
                            Ok(control_stream) => control_stream,
                            Err(error) => {
                                if let Ok(mut recorded) = thread_errors.lock() {
                                    recorded
                                        .push(format!("clone Node V2 test connection: {error}"));
                                }
                                continue;
                            }
                        };
                        let mut accepted = match thread_connections.lock() {
                            Ok(accepted) => accepted,
                            Err(_) => {
                                if let Ok(mut recorded) = thread_errors.lock() {
                                    recorded.push("Node V2 test connection list poisoned".into());
                                }
                                continue;
                            }
                        };
                        if thread_shutdown.load(Ordering::SeqCst) {
                            continue;
                        }
                        if accepted.len() >= MAX_TEST_PROVIDER_CONNECTIONS {
                            drop(accepted);
                            if let Ok(mut recorded) = thread_errors.lock() {
                                recorded.push("Node V2 test connection limit exceeded".into());
                            }
                            continue;
                        }
                        accepted.insert(connection_id, control_stream);
                        drop(accepted);
                        let service = service.clone();
                        let errors = thread_errors.clone();
                        let connections = thread_connections.clone();
                        let backend = thread_backend.clone();
                        clients.push(thread::spawn(move || {
                            let mut transport = DropReplyTransport { stream, backend };
                            if let Err(error) = service.serve_connection(&mut transport)
                                && let Ok(mut recorded) = errors.lock()
                            {
                                recorded.push(error.to_string());
                            }
                            if let Ok(mut accepted) = connections.lock() {
                                accepted.remove(&connection_id);
                            }
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => {
                        if let Ok(mut recorded) = thread_errors.lock() {
                            recorded.push(format!("accept Node V2 test connection: {error}"));
                        }
                        break;
                    }
                }
            }
            for client in clients {
                let _ = client.join();
            }
        });
        Ok(Self {
            address,
            shutdown,
            errors,
            connections,
            backend,
            thread: Some(thread),
        })
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(crate) fn fail_next_complete_transition_install(&self) {
        self.backend.fail_next_complete_transition_install();
    }

    pub(crate) fn drop_next_complete_transition_install_reply(&self) {
        self.backend.drop_next_complete_transition_install_reply();
    }

    pub(crate) fn errors(&self) -> Vec<String> {
        self.errors
            .lock()
            .expect("Node V2 test server errors")
            .clone()
    }

    pub(crate) fn shutdown(&mut self) {
        let _ = self.shutdown_and_join();
    }

    fn shutdown_and_join(&mut self) -> Result<(), BftSignerErrorV2> {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if let Ok(mut connections) = self.connections.lock() {
            for (_, stream) in std::mem::take(&mut *connections) {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| BftSignerErrorV2("Node V2 test server thread panicked".into()))?;
        }
        Ok(())
    }
}

impl Drop for TcpServiceServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct DropReplyTransport {
    stream: TcpStream,
    backend: MemoryBackend,
}

impl Read for DropReplyTransport {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(bytes)
    }
}

impl Write for DropReplyTransport {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let drop_install_reply = self.backend.take_install_reply_loss();
        if drop_install_reply {
            let _ = self.stream.shutdown(Shutdown::Both);
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "injected Node V2 test reply loss",
            ));
        }
        self.stream.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}
