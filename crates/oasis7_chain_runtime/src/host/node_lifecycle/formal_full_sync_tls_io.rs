//! Controlled TLS plaintext counting boundary; DNS and operation admission
//! remain caller responsibilities. This alone does not activate production V2.
//! Plaintext accepted by rustls is charged before ciphertext flushing. An
//! unsuccessful flush must never turn that accepted prefix into zero bytes.
use super::metered_io::remaining;
use super::operation_ledger::{AttemptPermit, Direction, ExchangePermit, Ledger};
use rustls::{ClientConfig, ClientConnection, pki_types::ServerName};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

struct DeadlineSocket {
    socket: TcpStream,
    deadline: Instant,
    request_timeout: Duration,
}
impl DeadlineSocket {
    fn timeout(&self) -> io::Result<Duration> {
        remaining(self.deadline, self.request_timeout)
            .map_err(|e| io::Error::new(io::ErrorKind::TimedOut, e))
    }
}
impl Read for DeadlineSocket {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.timeout()?))?;
        self.socket.read(output)
    }
}
impl Write for DeadlineSocket {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.timeout()?))?;
        self.socket.write(input)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.timeout()?;
        self.socket.flush()
    }
}

pub(super) struct TlsBodyIo {
    connection: ClientConnection,
    socket: DeadlineSocket,
    poisoned: bool,
    #[cfg(test)]
    before_settle: Option<Box<dyn FnOnce()>>,
}
impl TlsBodyIo {
    /// Caller must already hold durable exchange/duplex credit. The explicit
    /// address is not DNS resolution and must be authorized independently.
    pub fn connect(
        address: SocketAddr,
        name: ServerName<'static>,
        config: Arc<ClientConfig>,
        deadline: Instant,
        request_timeout: Duration,
    ) -> Result<Self, String> {
        if request_timeout.is_zero() || request_timeout > Duration::from_secs(60) {
            return Err("invalid TLS request timeout".into());
        }
        let socket = TcpStream::connect_timeout(&address, remaining(deadline, request_timeout)?)
            .map_err(|e| e.to_string())?;
        let mut connection = ClientConnection::new(config, name).map_err(|e| e.to_string())?;
        connection.set_buffer_limit(Some(64 * 1024));
        let mut socket = DeadlineSocket {
            socket,
            deadline,
            request_timeout,
        };
        connection
            .complete_io(&mut socket)
            .map_err(|e| e.to_string())?;
        remaining(deadline, request_timeout)?;
        if connection
            .alpn_protocol()
            .is_some_and(|protocol| protocol != b"http/1.1")
        {
            return Err("TLS negotiated unsupported HTTP protocol".into());
        }
        Ok(Self {
            connection,
            socket,
            poisoned: false,
            #[cfg(test)]
            before_settle: None,
        })
    }

    /// HTTP framing is bounded separately and excluded from body accounting.
    /// This reader consumes exactly one plaintext header byte, never body bytes.
    pub fn read_header_byte(&mut self) -> Result<u8, String> {
        let result = (|| {
            loop {
                self.check_live()?;
                let mut byte = [0];
                match self.connection.reader().read(&mut byte) {
                    Ok(1) => return Ok(byte[0]),
                    Ok(_) => return Err("TLS response ended within HTTP headers".into()),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        self.connection
                            .complete_io(&mut self.socket)
                            .map_err(|e| e.to_string())?;
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
        })();
        self.finish(result)
    }

    pub fn write_headers(&mut self, headers: &[u8]) -> Result<(), String> {
        let result = (|| {
            if headers.len() > 8 * 1024 {
                return Err("HTTP request headers exceed bound".into());
            }
            let mut rest = headers;
            while !rest.is_empty() {
                self.check_live()?;
                let n = self
                    .connection
                    .writer()
                    .write(rest)
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("TLS header writer made no progress".into());
                }
                rest = &rest[n..];
                self.flush_body()?;
            }
            Ok(())
        })();
        self.finish(result)
    }

    pub fn write_body(
        &mut self,
        ledger: &mut Ledger,
        attempt: &AttemptPermit,
        request: &ExchangePermit,
        body: &[u8],
    ) -> Result<usize, String> {
        let accepted = self.accept_body(ledger, attempt, request, body)?;
        self.flush_body()?;
        Ok(accepted)
    }

    fn finish<T>(&mut self, result: Result<T, String>) -> Result<T, String> {
        if result.is_err() {
            self.poisoned = true;
            let _ = self.socket.socket.shutdown(std::net::Shutdown::Both);
        }
        result
    }

    fn check_live(&self) -> Result<(), String> {
        if self.poisoned {
            return Err("TLS transport is terminal".into());
        }
        remaining(self.socket.deadline, self.socket.request_timeout)?;
        Ok(())
    }

    fn flush_body(&mut self) -> Result<(), String> {
        let result = (|| {
            self.check_live()?;
            while self.connection.wants_write() {
                self.check_live()?;
                let n = self
                    .connection
                    .write_tls(&mut self.socket)
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    return Err("TLS flush made no progress".into());
                }
            }
            self.check_live()
        })();
        self.finish(result)
    }

    fn accept_body(
        &mut self,
        ledger: &mut Ledger,
        attempt: &AttemptPermit,
        request: &ExchangePermit,
        body: &[u8],
    ) -> Result<usize, String> {
        let result = self
            .check_live()
            .and_then(|()| self.accept_body_inner(ledger, attempt, request, body));
        self.finish(result)
    }

    fn accept_body_inner(
        &mut self,
        ledger: &mut Ledger,
        attempt: &AttemptPermit,
        request: &ExchangePermit,
        body: &[u8],
    ) -> Result<usize, String> {
        self.socket.deadline = self.socket.deadline.min(attempt.deadline());
        remaining(self.socket.deadline, self.socket.request_timeout)?;
        let count = ledger.body_window_limit(Direction::Tx, body.len())?;
        let reservation = ledger.reserve(attempt, request, Direction::Tx, count as u64)?;
        if let Err(error) = remaining(self.socket.deadline, self.socket.request_timeout) {
            ledger.settle(&reservation, 0)?;
            return Err(error);
        }
        // Unlike StreamOwned::write this call cannot hide an accepted prefix
        // behind a later network flush error.
        let accepted = match self.connection.writer().write(&body[..count]) {
            Ok(n) => n,
            Err(error) => {
                ledger.settle(&reservation, 0)?;
                return Err(error.to_string());
            }
        };
        #[cfg(test)]
        if let Some(fault) = self.before_settle.take() {
            fault();
        }
        ledger.settle(&reservation, accepted as u64)?;
        if accepted == 0 {
            return Err("TLS body writer made no progress".into());
        }
        Ok(accepted)
    }

    pub fn read_body(
        &mut self,
        ledger: &mut Ledger,
        attempt: &AttemptPermit,
        request: &ExchangePermit,
        output: &mut [u8],
    ) -> Result<usize, String> {
        let result = self
            .check_live()
            .and_then(|()| self.read_body_inner(ledger, attempt, request, output));
        self.finish(result)
    }

    fn read_body_inner(
        &mut self,
        ledger: &mut Ledger,
        attempt: &AttemptPermit,
        request: &ExchangePermit,
        output: &mut [u8],
    ) -> Result<usize, String> {
        self.socket.deadline = self.socket.deadline.min(attempt.deadline());
        remaining(self.socket.deadline, self.socket.request_timeout)?;
        let count = ledger.body_window_limit(Direction::Rx, output.len())?;
        let reservation = ledger.reserve(attempt, request, Direction::Rx, count as u64)?;
        let result = (|| {
            loop {
                remaining(self.socket.deadline, self.socket.request_timeout)?;
                match self.connection.reader().read(&mut output[..count]) {
                    Ok(n) => return Ok(n),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        self.connection
                            .complete_io(&mut self.socket)
                            .map_err(|e| e.to_string())?;
                    }
                    Err(e) => return Err(e.to_string()),
                }
            }
        })();
        match result {
            Ok(n) => {
                ledger.settle(&reservation, n as u64)?;
                remaining(self.socket.deadline, self.socket.request_timeout)?;
                Ok(n)
            }
            Err(error) => {
                // No plaintext reader returned bytes in this invocation.
                ledger.settle(&reservation, 0)?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::local_io;
    use super::super::operation_ledger::{Identity, Policy};
    use super::*;
    use rustls::{
        RootCertStore, ServerConfig, ServerConnection, StreamOwned, pki_types::PrivatePkcs8KeyDer,
    };
    use std::net::TcpListener;

    #[test]
    fn real_tls_plaintext_accounting_and_failed_flush_keep_accepted_bytes() {
        exercise_real_tls_accounting(false);
    }

    #[test]
    fn settlement_failure_closes_tls_and_forbids_later_flush() {
        exercise_real_tls_accounting(true);
    }

    fn exercise_real_tls_accounting(fail_settle: bool) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = RootCertStore::empty();
        roots.add(cert.cert.der().clone()).unwrap();
        let client = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.cert.der().clone()],
                PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
            )
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(5);
            let socket = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < until => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("bounded TLS accept: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut stream =
                StreamOwned::new(ServerConnection::new(Arc::new(server)).unwrap(), socket);
            let mut body = [0; 7];
            stream.read_exact(&mut body).unwrap();
            assert_eq!(&body, b"request");
            stream.write_all(b"reply").unwrap();
            stream.flush().unwrap();
        });
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-tls-ledger-{}",
            local_io::random_id().unwrap()
        ));
        local_io::private_dir(&dir).unwrap();
        let policy = Policy {
            api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
            attempt_timeout_ms: 10_000,
            max_attempts: 2,
            max_encoded_io_bytes: 1024,
            max_exchanges: 2,
            max_proof_pages: 2,
            max_proof_bytes: 1024,
            max_inflight_requests: 1,
            reopen_timeout_ms: 1000,
        };
        let identity = Identity {
            operation_id: local_io::random_id().unwrap(),
            request_digest: "11".repeat(32),
            policy_digest: local_io::digest(&policy).unwrap(),
            binding_digest: "22".repeat(32),
        };
        let mut ledger = Ledger::create(&dir.join("ledger.json"), identity, policy).unwrap();
        let attempt = ledger.begin_attempt().unwrap();
        let exchange = ledger.begin_exchange(&attempt, 16, 16).unwrap();
        let mut tls = TlsBodyIo::connect(
            address,
            ServerName::try_from("localhost").unwrap(),
            Arc::new(client),
            attempt.deadline(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(
            tls.write_body(&mut ledger, &attempt, &exchange, b"request")
                .unwrap(),
            7
        );
        let mut reply = [0; 5];
        let mut offset = 0;
        while offset < reply.len() {
            let n = tls
                .read_body(&mut ledger, &attempt, &exchange, &mut reply[offset..])
                .unwrap();
            assert_ne!(n, 0);
            offset += n;
        }
        assert_eq!(&reply, b"reply");
        assert_eq!(ledger.snapshot().encoded_tx_actual, 7);
        assert_eq!(ledger.snapshot().encoded_rx_actual, 5);
        if fail_settle {
            let path = dir.join("ledger.json");
            tls.before_settle = Some(Box::new(move || std::fs::write(path, b"{").unwrap()));
            assert!(
                tls.accept_body(&mut ledger, &attempt, &exchange, b"late")
                    .is_err()
            );
            assert!(tls.poisoned);
            assert!(tls.flush_body().is_err());
            assert!(
                tls.read_body(&mut ledger, &attempt, &exchange, &mut reply)
                    .is_err()
            );
            // In-memory predecessor retains the unresolved reservation; no
            // successful durable settlement was reported by the failed write.
            assert!(ledger.snapshot().reservation.is_some());
            assert_eq!(ledger.snapshot().encoded_tx_actual, 7);
            drop(tls);
        } else {
            assert_eq!(
                tls.accept_body(&mut ledger, &attempt, &exchange, b"late")
                    .unwrap(),
                4
            );
            tls.socket.deadline = Instant::now();
            assert!(tls.flush_body().is_err());
            assert_eq!(ledger.snapshot().encoded_tx_actual, 11);
            assert!(ledger.snapshot().reservation.is_none());
            drop(tls);
            ledger.finish_exchange(&attempt, &exchange).unwrap();
            ledger.finish_attempt(&attempt).unwrap();
        }
        drop(ledger);
        worker.join().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn tls_configs(trust: bool) -> (Arc<ClientConfig>, Arc<ServerConfig>) {
        tls_configs_for_name(trust, "localhost")
    }

    fn tls_configs_for_name(trust: bool, name: &str) -> (Arc<ClientConfig>, Arc<ServerConfig>) {
        let cert = rcgen::generate_simple_self_signed(vec![name.into()]).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = RootCertStore::empty();
        if trust {
            roots.add(cert.cert.der().clone()).unwrap();
        }
        let client = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.cert.der().clone()],
                PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()).into(),
            )
            .unwrap();
        (Arc::new(client), Arc::new(server))
    }

    fn bounded_accept(listener: TcpListener) -> TcpStream {
        listener.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((socket, _)) => {
                    socket
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    socket
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    return socket;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("bounded TLS accept: {e}"),
            }
        }
    }

    #[test]
    fn tls_rejects_untrusted_ca_and_wrong_hostname_before_body() {
        for (trust, name) in [(false, "localhost"), (true, "wrong.invalid")] {
            let (client, server) = tls_configs(trust);
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let worker = std::thread::spawn(move || {
                let mut socket = bounded_accept(listener);
                let mut tls = ServerConnection::new(server).unwrap();
                let _ = tls.complete_io(&mut socket);
                let mut body = [0; 1];
                assert!(!matches!(tls.reader().read(&mut body), Ok(1)));
            });
            assert!(
                TlsBodyIo::connect(
                    address,
                    ServerName::try_from(name).unwrap(),
                    client,
                    Instant::now() + Duration::from_secs(2),
                    Duration::from_secs(1)
                )
                .is_err()
            );
            worker.join().unwrap();
        }
    }

    #[test]
    fn slow_tls_handshake_obeys_original_deadline() {
        let (client, _) = tls_configs(true);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _socket = bounded_accept(listener);
            let _ = wait.recv_timeout(Duration::from_secs(3));
        });
        let started = Instant::now();
        let result = TlsBodyIo::connect(
            address,
            ServerName::try_from("localhost").unwrap(),
            client,
            started + Duration::from_millis(100),
            Duration::from_secs(2),
        );
        let elapsed = started.elapsed();
        let _ = release.send(());
        worker.join().unwrap();
        assert!(result.is_err());
        // Large scheduler allowance, still separates the absolute 100 ms
        // deadline from the otherwise permitted two-second request timeout.
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    }
    #[test]
    fn https_error_body_counts_plaintext_not_headers_or_tls_frames() {
        let (client, server) = tls_configs(true);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let socket = bounded_accept(listener);
            let mut stream = StreamOwned::new(ServerConnection::new(server).unwrap(), socket);
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                assert!(headers.len() < 8192);
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                headers.push(byte[0]);
            }
            assert!(headers.starts_with(b"POST /v1/test HTTP/1.1\r\nHost: localhost\r\n"));
            let mut body = [0; 7];
            stream.read_exact(&mut body).unwrap();
            assert_eq!(&body, b"request");
            stream.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 5\r\nRetry-After: 1\r\nConnection: close\r\n\r\nreply").unwrap();
            stream.flush().unwrap();
        });
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-https-ledger-{}",
            local_io::random_id().unwrap()
        ));
        local_io::private_dir(&dir).unwrap();
        let policy = Policy {
            api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
            attempt_timeout_ms: 10_000,
            max_attempts: 2,
            max_encoded_io_bytes: 1024,
            max_exchanges: 2,
            max_proof_pages: 2,
            max_proof_bytes: 1024,
            max_inflight_requests: 1,
            reopen_timeout_ms: 1000,
        };
        let identity = Identity {
            operation_id: local_io::random_id().unwrap(),
            request_digest: "11".repeat(32),
            policy_digest: local_io::digest(&policy).unwrap(),
            binding_digest: "22".repeat(32),
        };
        let mut ledger = Ledger::create(&dir.join("operation.json"), identity, policy).unwrap();
        let attempt = ledger.begin_attempt().unwrap();
        let response = super::super::tls_http::post(
            super::super::tls_http::Request {
                address: address.into(),
                server_name: ServerName::try_from("localhost").unwrap(),
                tls_config: client,
                authority: "localhost",
                path: "/v1/test",
                body: b"request",
                response_bound: 128,
                request_timeout: Duration::from_secs(2),
            },
            &mut ledger,
            &attempt,
            attempt.deadline(),
        )
        .unwrap();
        assert_eq!(response.status, 429);
        assert_eq!(response.retry_after.as_deref(), Some("1"));
        assert_eq!(response.body, b"reply");
        assert_eq!(ledger.snapshot().encoded_tx_actual, 7);
        assert_eq!(ledger.snapshot().encoded_rx_actual, 5);
        assert_eq!(ledger.snapshot().exchanges_started, 1);
        assert!(ledger.snapshot().active_request.is_none());
        assert!(ledger.snapshot().reservation.is_none());
        worker.join().unwrap();
        drop(ledger);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn shared_https_adapter_binds_literal_ip_to_certificate_ip_san() {
        use super::super::shared_exchange::PreparedTransport;
        use oasis7_game_client::formal_sync_exchange::{EncodedFullSyncExchange, FullSyncExchange};
        for certificate_name in ["127.0.0.1", "localhost"] {
            let valid = certificate_name == "127.0.0.1";
            let (client, server) = tls_configs_for_name(true, certificate_name);
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = format!("https://{}", listener.local_addr().unwrap());
            let worker = std::thread::spawn(move || {
                let socket = bounded_accept(listener);
                let mut stream = StreamOwned::new(ServerConnection::new(server).unwrap(), socket);
                if !valid {
                    let mut byte = [0];
                    assert!(stream.read_exact(&mut byte).is_err());
                    return;
                }
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    assert!(headers.len() < 8192);
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                }
                let mut body = [0; 7];
                stream.read_exact(&mut body).unwrap();
                assert_eq!(&body, b"request");
                stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 5\r\nConnection: close\r\n\r\nerror").unwrap();
                stream.flush().unwrap();
            });
            let dir = std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("oasis7-ip-tls-{}", local_io::random_id().unwrap()));
            local_io::private_dir(&dir).unwrap();
            let policy = Policy {
                api_version: "oasis7.formal-full-sync-transfer-policy/v1".into(),
                attempt_timeout_ms: 10_000,
                max_attempts: 1,
                max_encoded_io_bytes: 1024,
                max_exchanges: 1,
                max_proof_pages: 1,
                max_proof_bytes: 1024,
                max_inflight_requests: 1,
                reopen_timeout_ms: 1000,
            };
            let prepared =
                PreparedTransport::prepare(std::slice::from_ref(&endpoint), Some(client)).unwrap();
            let original_request = serde_json::json!({"endpoint": &endpoint, "config":{"max_total_sync_bytes":0}, "transfer_policy": &policy, "transport_binding": prepared.binding_digest()});
            let identity = Identity {
                operation_id: local_io::random_id().unwrap(),
                request_digest: local_io::digest(&original_request).unwrap(),
                policy_digest: local_io::digest(&policy).unwrap(),
                binding_digest: "22".repeat(32),
            };
            let mut ledger = Ledger::create(&dir.join("operation.json"), identity, policy).unwrap();
            ledger.bind_original_request(&original_request).unwrap();
            let attempt = ledger.begin_attempt().unwrap();
            let shared = prepared.admit(ledger, attempt).unwrap();
            let result = shared.exchange(EncodedFullSyncExchange {
                endpoint: &endpoint,
                path: oasis7_client_api::world_service::VIEW_PATH,
                signed_body: b"request",
                response_bound: 128,
                request_timeout: Duration::from_secs(2),
            });
            if valid {
                assert_eq!(result.unwrap().body, b"error");
            } else {
                assert!(result.is_err());
            }
            let snapshot = shared.snapshot();
            assert_eq!(snapshot.exchanges_started, 1);
            assert_eq!(snapshot.encoded_tx_actual, if valid { 7 } else { 0 });
            assert_eq!(snapshot.encoded_rx_actual, if valid { 5 } else { 0 });
            assert!(snapshot.active_request.is_none());
            worker.join().unwrap();
            drop(shared);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn tls_rejects_negotiated_h2_before_http1_body() {
        let (client, server) = tls_configs(true);
        let mut client = Arc::try_unwrap(client).unwrap();
        let mut server = Arc::try_unwrap(server).unwrap();
        client.alpn_protocols = vec![b"h2".to_vec()];
        server.alpn_protocols = vec![b"h2".to_vec()];
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let mut socket = bounded_accept(listener);
            let mut connection = ServerConnection::new(Arc::new(server)).unwrap();
            let _ = connection.complete_io(&mut socket);
            let mut body = [0];
            assert!(!matches!(connection.reader().read(&mut body), Ok(1)));
        });
        let result = TlsBodyIo::connect(
            address,
            ServerName::try_from("localhost").unwrap(),
            Arc::new(client),
            Instant::now() + Duration::from_secs(2),
            Duration::from_secs(1),
        );
        assert!(matches!(result, Err(error) if error.contains("unsupported HTTP protocol")));
        worker.join().unwrap();
    }
}
