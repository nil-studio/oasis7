//! Test-only provider connection lifecycle and completion observation.
use super::*;

pub(super) struct TcpServiceServer {
    pub(super) address: SocketAddr,
    drop_next_reply: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub(super) history_certificate_replays: Arc<AtomicUsize>,
    pub(super) completed_connections: Arc<AtomicUsize>,
}

impl TcpServiceServer {
    pub(super) fn start(service: Arc<BftSignerServiceV1>) -> Self {
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
        let completed_connections = Arc::new(AtomicUsize::new(0));
        let thread_completed = completed_connections.clone();
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
                        let completed = thread_completed.clone();
                        clients.push(thread::spawn(move || {
                            if let Err(error) = serve_test_client(stream, service, drop_reply) {
                                eprintln!("test BFT signer TCP service failed: {error}");
                            }
                            // serve_connection returns only after its lease release validation.
                            completed.fetch_add(1, Ordering::Release);
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
            completed_connections,
        }
    }

    pub(super) fn wait_for_completed_connection_after(&self, before: usize) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while self.completed_connections.load(Ordering::Acquire) <= before {
            assert!(
                std::time::Instant::now() < deadline,
                "provider connection lease release validation did not complete"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn drop_next_reply(&self) {
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
