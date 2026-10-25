// Test-only simulated storage and software keys; never production custody.
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::protected_bft::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub(crate) struct SoftwareKey(pub(crate) SigningKey);
impl BftSigningKeyHandleV1 for SoftwareKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerError> {
        Ok(self.0.sign(bytes).to_bytes())
    }
}
pub(crate) struct MemoryLedger {
    pub(crate) state: BftSignerStateV1,
    pub(crate) binding_digest: String,
    pub(crate) lease_generation: u64,
    pub(crate) lease_token_digest: String,
    pub(crate) protected_time_floor_ms: i64,
    pub(crate) now_ms: i64,
    pub(crate) read_for_lease_calls: usize,
}

#[derive(Clone)]
pub(crate) struct MemoryBackend(pub(crate) Arc<Mutex<MemoryLedger>>);

impl MemoryBackend {
    pub(crate) fn provision(binding: &BftSignerBindingV1) -> Result<Self, BftSignerError> {
        Ok(Self(Arc::new(Mutex::new(MemoryLedger {
            state: BftSignerStateV1::genesis(binding)?,
            binding_digest: binding.digest()?,
            lease_generation: 0,
            lease_token_digest: String::new(),
            protected_time_floor_ms: 0,
            now_ms: 100,
            read_for_lease_calls: 0,
        }))))
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
        let mut ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        if ledger.binding_digest != binding_digest {
            return Err(BftSignerError("wrong binding".into()));
        }
        if ledger.lease_generation != expected_generation {
            return Err(BftSignerError("stale acquire generation".into()));
        }
        ledger.lease_generation = ledger
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerError("lease generation overflow".into()))?;
        ledger.lease_token_digest = new_lease_token_digest.to_string();
        Ok(Self::snapshot(&ledger))
    }

    fn release_lease(
        &mut self,
        expected_generation: u64,
        expected_token_digest: &str,
    ) -> Result<BftSignerBackendSnapshotV1, BftSignerError> {
        let mut ledger = self.0.lock().map_err(|_| BftSignerError("lock".into()))?;
        if ledger.lease_generation > expected_generation
            && ledger.lease_token_digest != expected_token_digest
        {
            return Ok(Self::snapshot(&ledger));
        }
        Self::require_lease(&ledger, expected_generation, expected_token_digest)?;
        ledger.lease_generation = ledger
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| BftSignerError("release generation overflow".into()))?;
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
        ledger.read_for_lease_calls += 1;
        Ok(Self::snapshot(&ledger))
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
        Ok(Self::snapshot(&ledger))
    }
}

pub(crate) struct TcpServiceServer {
    pub(crate) address: SocketAddr,
    drop_next_reply: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TcpServiceServer {
    pub(crate) fn start(service: Arc<BftSignerServiceV1>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test provider");
        listener
            .set_nonblocking(true)
            .expect("nonblocking test listener");
        let address = listener.local_addr().expect("test provider address");
        let drop_next_reply = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_drop_reply = drop_next_reply.clone();
        let thread_shutdown = shutdown.clone();
        let thread = thread::spawn(move || {
            let mut clients = Vec::new();
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream
                            .set_nonblocking(false)
                            .expect("blocking accepted TCP stream");
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
        }
    }

    pub(crate) fn drop_next_reply(&self) {
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
                std::io::ErrorKind::ConnectionReset,
                "test dropped reply",
            ));
        }
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}
