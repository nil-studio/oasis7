//! Independently configured issuer egress. No private input bytes cross HTTP.
use super::*;
use crate::formal_live_service::full_sync_read_gate::FullSyncV3ReadGate;
use crate::formal_live_service::{
    http::{read_http_request_with_body_limit, write_http_reply},
    parse_strict,
};
use oasis7_distfs::protected_bft_v3::BftSigningKeyHandleV3;
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) struct AdmissionGrantIssuerV1 {
    gate: Arc<FullSyncV3ReadGate>,
    issuer: Arc<dyn BftSigningKeyHandleV3>,
}
impl AdmissionGrantIssuerV1 {
    pub(crate) fn new(
        gate: Arc<FullSyncV3ReadGate>,
        issuer: Arc<dyn BftSigningKeyHandleV3>,
    ) -> Self {
        Self { gate, issuer }
    }
    pub(crate) fn serve_loopback(
        self: Arc<Self>,
        bind: SocketAddr,
    ) -> Result<AdmissionGrantServerV1, String> {
        if !bind.ip().is_loopback() || bind.port() == 0 {
            return Err("admission issuer bind requires nonzero loopback address".into());
        }
        let listener = TcpListener::bind(bind).map_err(|_| "admission issuer bind failed")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "admission issuer listener mode failed")?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let join = thread::Builder::new()
            .name("bft-admission-grant-issuer".into())
            .spawn(move || {
                while !stopped.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                            let _ = self.serve_connection(&mut stream);
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => return Err("admission issuer listener failed".into()),
                    }
                }
                Ok(())
            })
            .map_err(|_| "admission issuer worker spawn failed")?;
        Ok(AdmissionGrantServerV1 {
            stop,
            join: Some(join),
        })
    }
    fn serve_connection(&self, stream: &mut TcpStream) -> Result<(), String> {
        let deadline = Instant::now() + CONNECTION_TIMEOUT;
        // Accepted BSD/macOS sockets may inherit the listener's O_NONBLOCK.
        stream
            .set_nonblocking(false)
            .map_err(|_| "admission issuer socket mode failed")?;
        stream
            .set_write_timeout(Some(CONNECTION_TIMEOUT))
            .map_err(|_| "admission issuer reply timeout failed")?;
        let (method, path, body) =
            read_http_request_with_body_limit(stream, deadline, wire::MAX_FRAME)?;
        let result = if method == "POST" && path == wire::PATH {
            self.issue_reply(&body, deadline)
        } else {
            Err("admission issuer route rejected".into())
        };
        let (status, bytes) = match result {
            Ok(bytes) => (200, bytes),
            Err(_) => (
                403,
                br#"{"schema_version":1,"code":"admission_issuance_rejected"}"#.to_vec(),
            ),
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("admission issuer original deadline exhausted".into());
        }
        stream
            .set_write_timeout(Some(remaining))
            .map_err(|_| "admission issuer reply deadline failed")?;
        write_http_reply(stream, status, &bytes)
    }
    pub(crate) fn issue_reply(&self, body: &[u8], deadline: Instant) -> Result<Vec<u8>, String> {
        if body.len() > wire::MAX_FRAME || Instant::now() >= deadline {
            return Err("admission issuer request bound".into());
        }
        let request: wire::GrantRequest = parse_strict(body)?;
        let grant = self
            .gate
            .issue_original_admission_grant(&request, self.issuer.as_ref(), deadline)
            .map_err(|_| "admission original-input issuer authorization rejected")?;
        let bytes =
            serde_json::to_vec(&grant).map_err(|_| "admission issuer reply encoding failed")?;
        if bytes.len() > wire::MAX_FRAME || Instant::now() >= deadline {
            return Err("admission issuer original reply bound".into());
        }
        Ok(bytes)
    }
}
pub(crate) struct AdmissionGrantServerV1 {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<(), String>>>,
}
impl AdmissionGrantServerV1 {
    pub(crate) fn is_finished(&self) -> bool {
        self.join.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        self.join.take().map_or(Ok(()), |join| {
            join.join()
                .map_err(|_| "admission issuer worker panicked".to_owned())?
        })
    }
}
impl Drop for AdmissionGrantServerV1 {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
