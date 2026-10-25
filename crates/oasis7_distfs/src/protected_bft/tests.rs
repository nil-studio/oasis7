use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use oasis7_consensus::tendermint_v1::{
    BftConsensusActionV1, BftConsensusMachineV1, BftHeightProposerScheduleV1,
    BftSigningJournalEventV1, BftSigningJournalV1, BftTimeoutsV1, BftValidatorV1,
    verify_commit_certificate_token_v1,
};
use oasis7_proto::distributed_bft::{BftBlockV1, BftScopeV1, BftVotePhaseV1, BftVoteValueV1};

use super::backend::{BftSignerBackendSnapshotV1, BftSignerBackendV1};
use super::binding::{BftSignerBindingV1, BftSignerError, BftSignerTrustV1};
use super::client::{BftSignerLeaseV1, BftSignerProviderV1, RemoteBftSignerProviderV1};
use super::service::{BftSignerServiceV1, SoftwareBftSigningKeyV1};
use super::validation::{BftProtectedCheckpointV1, BftSignerCandidateV1, BftSignerStateV1};
use super::wire::{
    BFT_REQUEST_DOMAIN_V1, BftSignerCommandV1, BftSignerReplyKindV1, BftSignerRequestBodyV1,
    SignedBftSignerReplyV1, SignedBftSignerRequestV1, decode_bounded, encode_bounded, framed_cbor,
};

#[path = "acquire_race_tests.rs"]
mod acquire_race_tests;
#[path = "cas_poison_tests.rs"]
mod cas_poison_tests;
#[path = "protected_time_cache_tests.rs"]
mod protected_time_cache_tests;

struct MemoryLedger {
    state: BftSignerStateV1,
    binding_digest: String,
    lease_generation: u64,
    lease_token_digest: String,
    protected_time_floor_ms: i64,
    now_ms: i64,
    read_for_lease_calls: usize,
    fail_next_release: bool,
    corrupt_next_acquire_snapshot: bool,
    corrupt_next_cas_snapshot: bool,
    mutate_next_read_snapshot_without_digest: bool,
    next_cas_snapshot_floor_override_ms: Option<i64>,
}

struct AcquirePause {
    entered: Sender<()>,
    resume: Receiver<()>,
}

#[derive(Clone)]
struct MemoryBackend(Arc<Mutex<MemoryLedger>>, Arc<Mutex<Option<AcquirePause>>>);

impl MemoryBackend {
    fn provision(binding: &BftSignerBindingV1) -> Result<Self, BftSignerError> {
        Ok(Self(
            Arc::new(Mutex::new(MemoryLedger {
                state: BftSignerStateV1::genesis(binding)?,
                binding_digest: binding.digest()?,
                lease_generation: 0,
                lease_token_digest: String::new(),
                protected_time_floor_ms: 0,
                now_ms: 100,
                read_for_lease_calls: 0,
                fail_next_release: false,
                corrupt_next_acquire_snapshot: false,
                corrupt_next_cas_snapshot: false,
                mutate_next_read_snapshot_without_digest: false,
                next_cas_snapshot_floor_override_ms: None,
            })),
            Arc::new(Mutex::new(None)),
        ))
    }

    fn pause_next_acquire(&self) -> (Receiver<()>, Sender<()>) {
        let (entered, entered_rx) = mpsc::channel();
        let (resume_tx, resume) = mpsc::channel();
        *self.1.lock().expect("acquire pause lock") = Some(AcquirePause { entered, resume });
        (entered_rx, resume_tx)
    }

    fn snapshot(ledger: &MemoryLedger) -> BftSignerBackendSnapshotV1 {
        BftSignerBackendSnapshotV1 {
            state: ledger.state.clone(),
            lease_generation: ledger.lease_generation,
            lease_token_digest: ledger.lease_token_digest.clone(),
            protected_time_floor_ms: ledger.protected_time_floor_ms,
        }
    }

    fn require_lease(
        ledger: &MemoryLedger,
        generation: u64,
        token_digest: &str,
    ) -> Result<(), BftSignerError> {
        if ledger.lease_generation != generation || ledger.lease_token_digest != token_digest {
            return Err(BftSignerError("stale test lease".into()));
        }
        Ok(())
    }
}

impl BftSignerBackendV1 for MemoryBackend {
    fn load_provisioned(
        &mut self,
        binding_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        if ledger.binding_digest != binding_digest {
            return Err(BftSignerError("wrong binding".into()));
        }
        Ok(Self::snapshot(&ledger))
    }

    fn acquire_lease(
        &mut self,
        binding_digest: &str,
        expected_generation: u64,
        new_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        if let Some(pause) = self
            .1
            .lock()
            .map_err(|_| BftSignerError("pause lock".into()))?
            .take()
        {
            pause
                .entered
                .send(())
                .map_err(|_| BftSignerError("test acquire pause receiver closed".into()))?;
            pause
                .resume
                .recv()
                .map_err(|_| BftSignerError("test acquire pause was not resumed".into()))?;
        }
        let mut ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        if ledger.binding_digest != binding_digest || ledger.lease_generation != expected_generation
        {
            return Err(BftSignerError("wrong binding".into()));
        }
        ledger.lease_generation = ledger
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerError("lease generation overflow".into()))?;
        ledger.lease_token_digest = new_lease_token_digest.to_string();
        let mut snapshot = Self::snapshot(&ledger);
        if ledger.corrupt_next_acquire_snapshot {
            ledger.corrupt_next_acquire_snapshot = false;
            snapshot.state.state_digest = "00".repeat(32);
        }
        Ok(snapshot)
    }

    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let mut ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        if ledger.fail_next_release {
            ledger.fail_next_release = false;
            return Err(BftSignerError("simulated uncertain release".into()));
        }
        if ledger.lease_generation > expected_generation
            && ledger.lease_token_digest != expected_lease_token_digest
        {
            return Ok(Self::snapshot(&ledger));
        }
        if ledger.lease_generation != expected_generation
            || ledger.lease_token_digest != expected_lease_token_digest
        {
            return Err(BftSignerError("release lease mismatch".into()));
        }
        ledger.lease_generation = ledger
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerError("lease generation overflow".into()))?;
        ledger.lease_token_digest.clear();
        Ok(Self::snapshot(&ledger))
    }

    fn read_for_lease(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let mut ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        Self::require_lease(&ledger, lease_generation, lease_token_digest)?;
        ledger.read_for_lease_calls = ledger
            .read_for_lease_calls
            .checked_add(1)
            .ok_or_else(|| BftSignerError("test read counter overflow".into()))?;
        let mut snapshot = Self::snapshot(&ledger);
        if ledger.mutate_next_read_snapshot_without_digest {
            ledger.mutate_next_read_snapshot_without_digest = false;
            snapshot.state.journal.height_started_at_ms = Some(
                snapshot
                    .state
                    .journal
                    .height_started_at_ms
                    .unwrap_or(0)
                    .saturating_add(1),
            );
        }
        Ok(snapshot)
    }

    fn protected_time_ms(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
    ) -> Result<i64, BftSignerError> {
        let mut ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        Self::require_lease(&ledger, lease_generation, lease_token_digest)?;
        ledger.now_ms = ledger
            .now_ms
            .checked_add(1)
            .ok_or_else(|| BftSignerError("test clock overflow".into()))?;
        if ledger.now_ms < ledger.protected_time_floor_ms {
            return Err(BftSignerError("test clock rollback".into()));
        }
        ledger.protected_time_floor_ms = ledger.now_ms;
        Ok(ledger.now_ms)
    }

    fn compare_and_swap(
        &mut self,
        lease_generation: u64,
        lease_token_digest: &str,
        expected_revision: u64,
        expected_state_digest: &str,
        minimum_event_time_ms: Option<i64>,
        next_state: BftSignerStateV1,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let mut ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        Self::require_lease(&ledger, lease_generation, lease_token_digest)?;
        if ledger.state.revision != expected_revision
            || ledger.state.state_digest != expected_state_digest
            || next_state.revision != expected_revision + 1
        {
            return Err(BftSignerError("test CAS mismatch".into()));
        }
        ledger.now_ms = ledger
            .now_ms
            .checked_add(1)
            .ok_or_else(|| BftSignerError("test clock overflow".into()))?;
        if ledger.now_ms < ledger.protected_time_floor_ms
            || minimum_event_time_ms.is_some_and(|time| time > ledger.now_ms)
        {
            return Err(BftSignerError("test clock fence".into()));
        }
        ledger.protected_time_floor_ms = ledger.now_ms.max(minimum_event_time_ms.unwrap_or(0));
        ledger.state = next_state;
        let mut snapshot = Self::snapshot(&ledger);
        if ledger.corrupt_next_cas_snapshot {
            ledger.corrupt_next_cas_snapshot = false;
            snapshot.state.state_digest = "00".repeat(32);
        }
        if let Some(floor) = ledger.next_cas_snapshot_floor_override_ms.take() {
            snapshot.protected_time_floor_ms = floor;
        }
        Ok(snapshot)
    }
}

struct TcpServiceServer {
    address: SocketAddr,
    drop_next_reply: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    history_certificate_replays: Arc<AtomicUsize>,
}

impl TcpServiceServer {
    fn start(service: Arc<BftSignerServiceV1>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test provider");
        listener
            .set_nonblocking(true)
            .expect("nonblocking test listener");
        let address = listener.local_addr().expect("test provider address");
        let drop_next_reply = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        let history_certificate_replays = service.history_certificate_replay_counter();
        let thread_drop_reply = drop_next_reply.clone();
        let thread_shutdown = shutdown.clone();
        let thread = thread::spawn(move || {
            let mut clients = Vec::new();
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("set accepted test stream to blocking mode");
                        let service = service.clone();
                        let drop_reply = thread_drop_reply.clone();
                        clients.push(thread::spawn(move || {
                            if let Err(error) = serve_test_client(stream, service, drop_reply) {
                                eprintln!("test BFT signer TCP service failed: {error}");
                            }
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
            for client in clients {
                let _ = client.join();
            }
        });
        Self {
            address,
            drop_next_reply,
            shutdown,
            thread: Some(thread),
            history_certificate_replays,
        }
    }

    fn drop_next_reply(&self) {
        self.drop_next_reply.store(true, Ordering::SeqCst);
    }
}

impl Drop for TcpServiceServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_test_client(
    stream: TcpStream,
    service: Arc<BftSignerServiceV1>,
    drop_next_reply: Arc<AtomicBool>,
) -> Result<(), BftSignerError> {
    let mut transport = DropReplyTransport {
        stream,
        drop_next_reply,
    };
    service.serve_connection(&mut transport)
}

fn raw_signed_request(
    binding: &BftSignerBindingV1,
    key: &SigningKey,
    challenge: &str,
    command: BftSignerCommandV1,
) -> Vec<u8> {
    let body = BftSignerRequestBodyV1 {
        schema_version: 1,
        binding_digest: binding.digest().expect("binding digest"),
        challenge: challenge.to_string(),
        lease: None,
        command,
    };
    let preimage = framed_cbor(BFT_REQUEST_DOMAIN_V1, &body, usize::MAX)
        .expect("encode signed request preimage");
    let request = SignedBftSignerRequestV1 {
        body,
        authentication_public_key: hex::encode(key.verifying_key().to_bytes()),
        signature_hex: hex::encode(key.sign(&preimage).to_bytes()),
    };
    encode_bounded(&request).expect("encode signed request")
}

fn exchange_raw(
    stream: &mut TcpStream,
    frame: &[u8],
) -> Result<SignedBftSignerReplyV1, BftSignerError> {
    let frame_len = u32::try_from(frame.len())
        .map_err(|_| BftSignerError("test request frame length".into()))?;
    stream
        .write_all(&frame_len.to_be_bytes())
        .and_then(|_| stream.write_all(frame))
        .map_err(|error| BftSignerError(format!("test request write: {error}")))?;
    let mut length = [0; 4];
    stream
        .read_exact(&mut length)
        .map_err(|error| BftSignerError(format!("test response header: {error}")))?;
    let response_len = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| BftSignerError("test response length".into()))?;
    if response_len == 0 || response_len > super::validation::BFT_SIGNER_MAX_FRAME_BYTES_V1 {
        return Err(BftSignerError("test response frame bound".into()));
    }
    let mut response = vec![0; response_len];
    stream
        .read_exact(&mut response)
        .map_err(|error| BftSignerError(format!("test response body: {error}")))?;
    decode_bounded(&response)
}

struct DropReplyTransport {
    stream: TcpStream,
    drop_next_reply: Arc<AtomicBool>,
}

impl Read for DropReplyTransport {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.stream.read(bytes)
    }
}

impl Write for DropReplyTransport {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.drop_next_reply.swap(false, Ordering::SeqCst) {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "test drops provider reply",
            ));
        }
        self.stream.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

struct Fixture {
    binding: BftSignerBindingV1,
    requester_key: SigningKey,
    provider_key: SigningKey,
    validator_key: SigningKey,
    backend: MemoryBackend,
    server: TcpServiceServer,
}

impl Fixture {
    fn new() -> Self {
        let validator_key = SigningKey::from_bytes(&[7; 32]);
        let requester_key = SigningKey::from_bytes(&[8; 32]);
        let provider_key = SigningKey::from_bytes(&[9; 32]);
        let scope = BftScopeV1 {
            world_id: "protected-world".into(),
            chain_id: "protected-chain".into(),
            genesis_hash: "11".repeat(32),
        };
        let binding = BftSignerBindingV1 {
            schema_version: 1,
            scope,
            authority_epoch: 1,
            validators: vec![BftValidatorV1 {
                validator_id: "validator-a".into(),
                public_key_hex: hex::encode(validator_key.verifying_key().to_bytes()),
                voting_power: 1,
            }],
            local_validator_id: "validator-a".into(),
            local_signing_public_key: hex::encode(validator_key.verifying_key().to_bytes()),
            timeouts: BftTimeoutsV1 {
                propose_ms: 100,
                prevote_ms: 100,
                precommit_ms: 100,
                round_delta_ms: 10,
            },
            operator_instance_id: "test-provider-1".into(),
            requester_auth_public_key: hex::encode(requester_key.verifying_key().to_bytes()),
        };
        let backend = MemoryBackend::provision(&binding).expect("provision test fixture");
        let service = Arc::new(
            BftSignerServiceV1::open(
                binding.clone(),
                Box::new(SoftwareBftSigningKeyV1::new(provider_key.clone())),
                Box::new(SoftwareBftSigningKeyV1::new(validator_key.clone())),
                Box::new(backend.clone()),
            )
            .expect("open provisioned service"),
        );
        let server = TcpServiceServer::start(service);
        Self {
            binding,
            requester_key,
            provider_key,
            validator_key,
            backend,
            server,
        }
    }

    fn provider(&self) -> RemoteBftSignerProviderV1 {
        let trust = BftSignerTrustV1 {
            schema_version: 1,
            provider_public_key: hex::encode(self.provider_key.verifying_key().to_bytes()),
            binding_digest: self.binding.digest().expect("binding digest"),
        };
        RemoteBftSignerProviderV1::new(
            self.server.address,
            Duration::from_secs(2),
            self.binding.clone(),
            self.requester_key.clone(),
            trust,
        )
        .expect("construct remote signer client")
    }

    fn reopened_service(&self) -> Arc<BftSignerServiceV1> {
        Arc::new(
            BftSignerServiceV1::open(
                self.binding.clone(),
                Box::new(SoftwareBftSigningKeyV1::new(self.provider_key.clone())),
                Box::new(SoftwareBftSigningKeyV1::new(self.validator_key.clone())),
                Box::new(self.backend.clone()),
            )
            .expect("reopen provisioned service on same backend"),
        )
    }
}

fn acquire_started(
    fixture: &Fixture,
) -> (Box<dyn BftSignerLeaseV1>, BftProtectedCheckpointV1, i64) {
    let provider = fixture.provider();
    let mut lease = provider
        .acquire(&fixture.binding)
        .expect("acquire test lease");
    let genesis = lease.checkpoint().expect("read test genesis");
    let checkpoint = lease
        .start_height_timer(genesis.state.revision)
        .expect("start provider-controlled timer");
    let started_at = checkpoint
        .state
        .journal
        .height_started_at_ms
        .expect("height timer persisted");
    (lease, checkpoint, started_at)
}

fn last_event_time(checkpoint: &BftProtectedCheckpointV1) -> i64 {
    checkpoint
        .state
        .journal
        .events()
        .last()
        .map(BftSigningJournalEventV1::event_time_ms)
        .expect("journal has an event")
}

#[test]
fn captured_prepare_acquire_pair_cannot_replay_across_connection_or_service_restart() {
    let fixture = Fixture::new();
    let prepare_frame = raw_signed_request(
        &fixture.binding,
        &fixture.requester_key,
        &"11".repeat(32),
        BftSignerCommandV1::PrepareAcquire {
            binding: Box::new(fixture.binding.clone()),
        },
    );
    let mut first_stream = TcpStream::connect(fixture.server.address).expect("first connection");
    first_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let prepared = exchange_raw(&mut first_stream, &prepare_frame).expect("prepare response");
    let (expected_generation, old_prepare_nonce) = match &prepared.body.result {
        BftSignerReplyKindV1::Prepared {
            expected_generation,
            prepare_nonce,
        } => (*expected_generation, prepare_nonce.clone()),
        result => panic!("expected PrepareAcquire response, got {result:?}"),
    };
    assert_eq!(expected_generation, 0);
    assert!(prepared.body.state.is_none());

    let acquire_frame = raw_signed_request(
        &fixture.binding,
        &fixture.requester_key,
        &"22".repeat(32),
        BftSignerCommandV1::Acquire {
            binding: Box::new(fixture.binding.clone()),
            expected_generation,
            prepare_nonce: old_prepare_nonce.clone(),
        },
    );
    let acquired = exchange_raw(&mut first_stream, &acquire_frame).expect("acquire response");
    assert!(matches!(
        acquired.body.result,
        BftSignerReplyKindV1::Acquired
    ));
    assert_eq!(acquired.body.lease_generation, 1);
    assert!(acquired.body.lease_token.is_some());
    drop(first_stream);
    wait_for_generation(&fixture.backend, 2);

    // Reopening the service retains the same backend generation. Replaying
    // only the old Acquire has no session-local Prepare state.
    let reopened_service = fixture.reopened_service();
    let reopened_server = TcpServiceServer::start(reopened_service);
    let mut acquire_only = TcpStream::connect(reopened_server.address).expect("new connection");
    acquire_only
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let rejected = exchange_raw(&mut acquire_only, &acquire_frame).expect("replayed acquire");
    assert!(matches!(
        rejected.body.result,
        BftSignerReplyKindV1::Rejected { .. }
    ));
    drop(acquire_only);

    // Replaying the old signed pair also fails: the new connection's server
    // generated one-use nonce differs from the captured response.
    let mut replay_stream = TcpStream::connect(reopened_server.address).expect("replay connection");
    replay_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let replay_prepared =
        exchange_raw(&mut replay_stream, &prepare_frame).expect("replayed prepare");
    let BftSignerReplyKindV1::Prepared { prepare_nonce, .. } = replay_prepared.body.result else {
        panic!("expected new prepared session nonce");
    };
    assert_ne!(prepare_nonce, old_prepare_nonce);
    let rejected = exchange_raw(&mut replay_stream, &acquire_frame).expect("old acquire frame");
    assert!(matches!(
        rejected.body.result,
        BftSignerReplyKindV1::Rejected { .. }
    ));
    drop(replay_stream);

    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(ledger.lease_generation, 2);
    assert!(ledger.lease_token_digest.is_empty());
}

#[test]
fn uncertain_connection_release_poisons_service_before_another_acquire() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let lease = provider.acquire(&fixture.binding).expect("first lease");
    fixture
        .backend
        .0
        .lock()
        .expect("test backend lock")
        .fail_next_release = true;
    drop(lease);

    for _ in 0..200 {
        if !fixture
            .backend
            .0
            .lock()
            .expect("test backend lock")
            .fail_next_release
        {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        provider.acquire(&fixture.binding).is_err(),
        "a release with unknown durability must poison the service"
    );
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(ledger.lease_generation, 1);
    assert!(!ledger.lease_token_digest.is_empty());
}

fn wait_for_generation(backend: &MemoryBackend, expected: u64) {
    for _ in 0..200 {
        if backend
            .0
            .lock()
            .expect("test backend lock")
            .lease_generation
            == expected
        {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("provider generation did not reach {expected}");
}

#[test]
fn remote_tcp_clock_is_fresh_without_journal_revision_and_new_lease_fences_old_client() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let mut old_lease = provider.acquire(&fixture.binding).expect("first lease");
    let initial = old_lease.checkpoint().expect("initial checkpoint");
    assert_eq!(initial.state.revision, 0);
    let time = old_lease
        .protected_time_ms()
        .expect("protected clock sample");
    assert!(time > 0);
    let after_clock = old_lease.checkpoint().expect("checkpoint after clock");
    assert_eq!(after_clock.state.revision, initial.state.revision);
    assert_eq!(after_clock.state.state_digest, initial.state.state_digest);

    let mut new_lease = provider.acquire(&fixture.binding).expect("second lease");
    assert!(
        old_lease.checkpoint().is_err(),
        "older lease must be fenced"
    );
    drop(old_lease);
    let current = new_lease.checkpoint().expect("new lease checkpoint");
    assert_eq!(current.state, initial.state);
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(ledger.lease_generation, 2);
    assert!(!ledger.lease_token_digest.is_empty());
}

#[test]
fn lost_proposal_reply_reopens_exact_committed_signature_and_rejects_conflict() {
    let fixture = Fixture::new();
    let provider = fixture.provider();
    let mut lease = provider.acquire(&fixture.binding).expect("lease");
    let genesis = lease.checkpoint().expect("genesis checkpoint");
    for origin in [0, 50_000] {
        let mut caller_started_height = genesis.state.candidate();
        caller_started_height.journal.height_started_at_ms = Some(origin);
        assert!(
            lease
                .advance(
                    genesis.state.revision,
                    &genesis.state.state_digest,
                    caller_started_height,
                )
                .is_err(),
            "generic advance must not let a caller choose timer origin {origin}"
        );
    }
    assert_eq!(
        lease
            .checkpoint()
            .expect("checkpoint after rejected timer")
            .state,
        genesis.state,
        "rejected timer candidate must leave the journal unchanged"
    );
    let checkpoint = lease
        .start_height_timer(genesis.state.revision)
        .expect("provider starts timer from a fresh protected clock");
    let height_started_at = checkpoint
        .state
        .journal
        .height_started_at_ms
        .expect("durably persisted height start");
    assert!(height_started_at > 0);
    let repeated_start = lease
        .start_height_timer(genesis.state.revision)
        .expect("lost timer response retry is idempotent");
    assert_eq!(repeated_start.state, checkpoint.state);

    let set = fixture.binding.validate().expect("fixed set");
    let schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.binding.authority_epoch)
        .expect("genesis schedule");
    let machine = BftConsensusMachineV1::new(
        set,
        fixture.binding.local_validator_id.clone(),
        1,
        Some(schedule.starting_priority().clone()),
        height_started_at,
        fixture.binding.timeouts,
    )
    .expect("BFT machine");
    let block =
        protected_time_cache_tests::block(&fixture.binding.scope, schedule.parent_commit_ref());
    let intent = machine
        .proposal_intent(block.clone(), &fixture.binding.local_validator_id)
        .expect("deterministic proposal intent");
    let mut reserve = checkpoint.state.candidate();
    reserve.journal.pending_proposal_intent = Some(intent.clone());
    let reserved = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            reserve,
        )
        .expect("reserve exact proposal before signing");

    fixture.server.drop_next_reply();
    let lost_reply = lease.sign_reserved_proposal(reserved.state.revision, &intent);
    assert!(
        lost_reply.is_err(),
        "test transport drops the signed response"
    );
    drop(lease);

    let mut reopened_lease = provider
        .acquire(&fixture.binding)
        .expect("reacquire after loss");
    let after_reopen = reopened_lease
        .checkpoint()
        .expect("read persisted completed signing decision");
    assert_eq!(after_reopen.state.journal.events().len(), 1);
    assert!(
        after_reopen
            .state
            .journal
            .pending_proposal_intent()
            .is_none()
    );
    let event_signature = match &after_reopen.state.journal.events()[0] {
        oasis7_consensus::tendermint_v1::BftSigningJournalEventV1::Proposal {
            proposal,
            local: true,
            ..
        } => {
            proposal
                .header
                .verify_signature()
                .expect("stored proposal signature");
            proposal.header.signature_hex.clone()
        }
        event => panic!("expected local signed proposal, found {event:?}"),
    };

    let (replayed, replay_checkpoint) = reopened_lease
        .sign_reserved_proposal(reserved.state.revision, &intent)
        .expect("exact retry returns original signature");
    assert_eq!(replayed.header.signature_hex, event_signature);
    assert_eq!(replay_checkpoint.state, after_reopen.state);

    let mut conflicting = intent;
    conflicting.header.block_id = "ff".repeat(32);
    conflicting.block.action_root = "ee".repeat(32);
    assert!(
        reopened_lease
            .sign_reserved_proposal(after_reopen.state.revision, &conflicting)
            .is_err(),
        "a second value cannot reuse the completed proposal slot"
    );
    let ledger = fixture.backend.0.lock().expect("test backend lock");
    assert_eq!(ledger.state.journal.events().len(), 1);
    assert_eq!(ledger.state.revision, after_reopen.state.revision);
    let _ = fixture.validator_key;
}

#[test]
fn provider_rejects_untrusted_timer_origins_and_timeout_events_atomically() {
    let fixture = Fixture::new();
    let (mut lease, started, started_at) = acquire_started(&fixture);
    let before = started.state.clone();

    for event_time in [started_at + 1, started_at + 200] {
        let mut candidate = started.state.candidate();
        candidate
            .journal
            .events
            .push(BftSigningJournalEventV1::Timeout { now_ms: event_time });
        {
            let mut ledger = fixture.backend.0.lock().expect("test backend lock");
            ledger.now_ms = if event_time == started_at + 1 {
                started_at
            } else {
                started_at + 1
            };
        }
        assert!(
            lease
                .advance(before.revision, &before.state_digest, candidate,)
                .is_err(),
            "timeout at {event_time} must be rejected before deadline or beyond protected time"
        );
        assert_eq!(
            lease
                .checkpoint()
                .expect("state after rejected timeout")
                .state,
            before,
            "rejected timeout must not change durable BFT state"
        );
    }
}

#[test]
fn provider_clock_rollback_blocks_reserved_signing() {
    let fixture = Fixture::new();
    let (mut lease, checkpoint, started_at) = acquire_started(&fixture);
    let set = fixture.binding.validate().expect("fixed validator set");
    let schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.binding.authority_epoch)
        .expect("genesis schedule");
    let machine = BftConsensusMachineV1::new(
        set,
        fixture.binding.local_validator_id.clone(),
        1,
        Some(schedule.starting_priority().clone()),
        started_at,
        fixture.binding.timeouts,
    )
    .expect("BFT machine");
    let intent = machine
        .proposal_intent(
            protected_time_cache_tests::block(&fixture.binding.scope, schedule.parent_commit_ref()),
            &fixture.binding.local_validator_id,
        )
        .expect("proposal intent");
    let mut reserve = checkpoint.state.candidate();
    reserve.journal.pending_proposal_intent = Some(intent.clone());
    let reserved = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            reserve,
        )
        .expect("reserve before rollback");
    {
        let mut ledger = fixture.backend.0.lock().expect("test backend lock");
        ledger.now_ms = ledger.protected_time_floor_ms - 2;
    }
    assert!(
        lease
            .sign_reserved_proposal(reserved.state.revision, &intent)
            .is_err(),
        "provider clock rollback must stop a new signature"
    );
    let after = lease.checkpoint().expect("state after rollback refusal");
    assert_eq!(after.state, reserved.state);
    assert!(after.state.journal.events().is_empty());
    assert!(after.state.journal.pending_proposal_intent().is_some());
}

#[test]
fn provider_rejects_unprovisioned_binding_bad_keys_and_wrong_provider_trust() {
    let fixture = Fixture::new();

    let mut unknown_binding = fixture.binding.clone();
    unknown_binding.scope.world_id = "not-provisioned".into();
    assert!(fixture.provider().acquire(&unknown_binding).is_err());
    let wrong_backend = MemoryBackend::provision(&unknown_binding).expect("other binding");
    assert!(
        BftSignerServiceV1::open(
            fixture.binding.clone(),
            Box::new(SoftwareBftSigningKeyV1::new(fixture.provider_key.clone())),
            Box::new(SoftwareBftSigningKeyV1::new(fixture.validator_key.clone())),
            Box::new(wrong_backend),
        )
        .is_err()
    );

    assert!(
        BftSignerServiceV1::open(
            fixture.binding.clone(),
            Box::new(SoftwareBftSigningKeyV1::new(fixture.provider_key.clone())),
            Box::new(SoftwareBftSigningKeyV1::new(fixture.requester_key.clone())),
            Box::new(fixture.backend.clone()),
        )
        .is_err()
    );

    let wrong_trust = BftSignerTrustV1 {
        schema_version: 1,
        provider_public_key: hex::encode(fixture.requester_key.verifying_key().to_bytes()),
        binding_digest: fixture.binding.digest().expect("binding digest"),
    };
    assert!(
        RemoteBftSignerProviderV1::new(
            fixture.server.address,
            Duration::from_secs(2),
            fixture.binding.clone(),
            fixture.requester_key.clone(),
            wrong_trust,
        )
        .is_err(),
        "provider trust must not reuse validator or requester identity"
    );
}

#[test]
fn provider_vote_slot_retry_is_exact_and_conflicts_are_rejected() {
    let fixture = Fixture::new();
    let (mut lease, checkpoint, started_at) = acquire_started(&fixture);
    let set = fixture.binding.validate().expect("fixed validator set");
    let schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.binding.authority_epoch)
        .expect("genesis schedule");
    let mut machine = BftConsensusMachineV1::new(
        set,
        fixture.binding.local_validator_id.clone(),
        1,
        Some(schedule.starting_priority().clone()),
        started_at,
        fixture.binding.timeouts,
    )
    .expect("BFT machine");
    let timeout_at = started_at
        + i64::try_from(fixture.binding.timeouts.propose_ms)
            .expect("fixture timeout fits protected clock");
    let vote_intent = machine
        .on_timeout(timeout_at, &fixture.binding.local_validator_id)
        .expect("propose timeout")
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) => Some(intent),
            _ => None,
        })
        .expect("timeout requests local prevote");
    let mut timed_out = checkpoint.state.candidate();
    timed_out
        .journal
        .events
        .push(BftSigningJournalEventV1::Timeout { now_ms: timeout_at });
    {
        let mut ledger = fixture.backend.0.lock().expect("test backend lock");
        ledger.now_ms = timeout_at;
    }
    let checkpoint = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            timed_out,
        )
        .expect("persist due timeout");
    let mut reserved_vote = checkpoint.state.candidate();
    reserved_vote.journal.pending_vote_intent = Some(vote_intent.clone());
    let reserved = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            reserved_vote,
        )
        .expect("reserve exact vote");
    let (signed_vote, completed) = lease
        .sign_reserved_vote(reserved.state.revision, &vote_intent)
        .expect("sign and persist vote");
    let event_signature = match completed.state.journal.events().last().expect("vote event") {
        BftSigningJournalEventV1::Vote {
            vote, local: true, ..
        } => {
            vote.verify_signature().expect("stored vote signature");
            vote.signature_hex.clone()
        }
        event => panic!("expected local vote event, found {event:?}"),
    };
    let (retried_vote, retry_checkpoint) = lease
        .sign_reserved_vote(reserved.state.revision, &vote_intent)
        .expect("same slot retry returns original decision");
    assert_eq!(signed_vote.signature_hex, event_signature);
    assert_eq!(retried_vote.signature_hex, event_signature);
    assert_eq!(retry_checkpoint.state, completed.state);

    let mut conflict = vote_intent;
    conflict.value = BftVoteValueV1::Block {
        block_id: "aa".repeat(32),
    };
    assert!(
        lease
            .sign_reserved_vote(completed.state.revision, &conflict)
            .is_err(),
        "another value cannot reuse the completed vote slot"
    );
    assert_eq!(
        lease.checkpoint().expect("final vote state").state,
        completed.state
    );
}

#[test]
fn provider_refuses_new_signature_for_a_committed_old_height() {
    let fixture = Fixture::new();
    let (mut lease, mut checkpoint, started_at) = acquire_started(&fixture);
    let set = fixture.binding.validate().expect("fixed validator set");
    let schedule = BftHeightProposerScheduleV1::genesis(&set, fixture.binding.authority_epoch)
        .expect("genesis schedule");
    let mut machine = BftConsensusMachineV1::new(
        set.clone(),
        fixture.binding.local_validator_id.clone(),
        1,
        Some(schedule.starting_priority().clone()),
        started_at,
        fixture.binding.timeouts,
    )
    .expect("BFT machine");

    let old_height_intent = machine
        .proposal_intent(
            protected_time_cache_tests::block(&fixture.binding.scope, schedule.parent_commit_ref()),
            &fixture.binding.local_validator_id,
        )
        .expect("height-one proposal intent");
    let mut candidate = checkpoint.state.candidate();
    candidate.journal.pending_proposal_intent = Some(old_height_intent.clone());
    checkpoint = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            candidate,
        )
        .expect("reserve height-one proposal");
    let (proposal, completed_proposal) = lease
        .sign_reserved_proposal(checkpoint.state.revision, &old_height_intent)
        .expect("sign height-one proposal");
    let actions = machine
        .receive_proposal(proposal, true, last_event_time(&completed_proposal))
        .expect("admit proposal");
    let prevote_intent = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) if intent.phase == BftVotePhaseV1::Prevote => {
                Some(intent)
            }
            _ => None,
        })
        .expect("proposal requests prevote");

    let mut candidate = completed_proposal.state.candidate();
    candidate.journal.pending_vote_intent = Some(prevote_intent.clone());
    checkpoint = lease
        .advance(
            completed_proposal.state.revision,
            &completed_proposal.state.state_digest,
            candidate,
        )
        .expect("reserve prevote");
    let (prevote, completed_prevote) = lease
        .sign_reserved_vote(checkpoint.state.revision, &prevote_intent)
        .expect("sign prevote");
    let actions = machine
        .record_local_vote(prevote, last_event_time(&completed_prevote))
        .expect("record prevote");
    let precommit_intent = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::SignVote(intent) if intent.phase == BftVotePhaseV1::Precommit => {
                Some(intent)
            }
            _ => None,
        })
        .expect("quorum prevote requests precommit");

    let mut candidate = completed_prevote.state.candidate();
    candidate.journal.pending_vote_intent = Some(precommit_intent.clone());
    checkpoint = lease
        .advance(
            completed_prevote.state.revision,
            &completed_prevote.state.state_digest,
            candidate,
        )
        .expect("reserve precommit");
    let (precommit, completed_precommit) = lease
        .sign_reserved_vote(checkpoint.state.revision, &precommit_intent)
        .expect("sign precommit");
    let actions = machine
        .record_local_vote(precommit, last_event_time(&completed_precommit))
        .expect("record precommit");
    let certificate = actions
        .into_iter()
        .find_map(|action| match action {
            BftConsensusActionV1::Commit(certificate) => Some(certificate),
            _ => None,
        })
        .expect("single-validator precommit forms a QC");

    let mut pending_commit = completed_precommit.state.candidate();
    pending_commit.journal.pending_commit = Some(certificate.clone());
    checkpoint = lease
        .advance(
            completed_precommit.state.revision,
            &completed_precommit.state.state_digest,
            pending_commit,
        )
        .expect("persist exact pending QC");
    let verified = verify_commit_certificate_token_v1(&set, &schedule, &certificate)
        .expect("verify height-one QC");
    let next_schedule = verified.next_height_schedule();
    let next_height = BftSignerCandidateV1 {
        history: vec![certificate],
        journal: BftSigningJournalV1::for_schedule(
            next_schedule,
            fixture.binding.local_validator_id.clone(),
            fixture.binding.local_signing_public_key.clone(),
        ),
    };
    checkpoint = lease
        .advance(
            checkpoint.state.revision,
            &checkpoint.state.state_digest,
            next_height,
        )
        .expect("commit QC and advance protected height");
    assert_eq!(checkpoint.state.journal.height, 2);

    assert!(
        lease
            .sign_reserved_proposal(checkpoint.state.revision, &old_height_intent)
            .is_err(),
        "an old-height intent cannot obtain a new signature after commit"
    );
    assert_eq!(
        lease.checkpoint().expect("height-two checkpoint").state,
        checkpoint.state,
        "rejected stale signing must leave protected state unchanged"
    );
}
