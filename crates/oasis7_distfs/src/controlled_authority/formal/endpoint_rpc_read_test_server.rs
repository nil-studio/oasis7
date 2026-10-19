//! Test-only RPC ownership. An assertion failure must not detach a server from
//! the storage/provider lifetime that it is still using.
use super::*;
use std::{collections::VecDeque, fmt::Debug, net::TcpListener, time::Instant};
pub(super) const RPC_TIMEOUT: Duration = Duration::from_secs(3);
const DIAGNOSTIC_LIMIT: usize = 32;
#[derive(Default)]
pub(super) struct Diagnostics {
    events: Mutex<VecDeque<String>>,
    pub(super) errors: Mutex<Vec<FormalError>>,
    pub(super) joined: AtomicBool,
}
impl Diagnostics {
    fn record(&self, event: String) {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if events.len() == DIAGNOSTIC_LIMIT {
            events.pop_front();
        }
        events.push_back(event.chars().take(2048).collect());
    }
    pub(super) fn report(&self) -> String {
        let events = self
            .events
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        format!("RPC test diagnostics: {events:?}")
    }
}
pub(super) struct OwnedRpcServer {
    pub(super) address: SocketAddr,
    pub(super) diagnostics: Arc<Diagnostics>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl OwnedRpcServer {
    pub(super) fn start<K: Send + 'static>(
        endpoint: FileFormalEndpoint,
        keepalive: K,
    ) -> Result<Self, FormalError> {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|error| invalid(error.to_string()))?;
        let address = listener
            .local_addr()
            .map_err(|error| invalid(error.to_string()))?;
        let server = super::super::super::super::super::transport::FormalEndpointServer::new(
            endpoint,
            RPC_TIMEOUT,
        )?;
        let stop = Arc::new(AtomicBool::new(false));
        let diagnostics = Arc::new(Diagnostics::default());
        let server_stop = stop.clone();
        let captured = diagnostics.clone();
        let worker = thread::Builder::new()
            .name("formal-read-rpc-test".into())
            .spawn(move || {
                // Declared before server so the endpoint closes before the owner
                // resources, even if a deadline forces the parent to retain them
                // in this still-running worker until its process exits.
                let _keepalive = keepalive;
                let mut server = server;
                while !server_stop.load(Ordering::Acquire) {
                    let started = Instant::now();
                    captured.record("server serve_one begin".into());
                    let result = server.serve_one(&listener);
                    captured.record(format!(
                        "server serve_one elapsed_us={} result={result:?}",
                        started.elapsed().as_micros()
                    ));
                    if server_stop.load(Ordering::Acquire) {
                        break;
                    }
                    if let Err(error) = result {
                        let mut errors = captured
                            .errors
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        if errors.len() == DIAGNOSTIC_LIMIT {
                            errors.remove(0);
                        }
                        errors.push(error);
                    }
                }
            })
            .map_err(|error| invalid(error.to_string()))?;
        Ok(Self {
            address,
            diagnostics,
            stop,
            worker: Some(worker),
        })
    }
    pub(super) fn call<T, E: Debug>(
        &self,
        phase: &'static str,
        call: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let started = Instant::now();
        self.diagnostics.record(format!("client {phase} begin"));
        let result = call();
        let outcome = match &result {
            Ok(_) => "Ok".into(),
            Err(error) => format!("Err({error:?})"),
        };
        self.diagnostics.record(format!(
            "client {phase} elapsed_us={} result={outcome}",
            started.elapsed().as_micros()
        ));
        result
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        let deadline = Instant::now() + RPC_TIMEOUT;
        self.stop.store(true, Ordering::Release);
        let wake = TcpStream::connect_timeout(&self.address, RPC_TIMEOUT);
        self.diagnostics.record(format!(
            "cleanup wake result={:?}",
            wake.as_ref().map(|_| ())
        ));
        drop(wake);
        while !worker.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if !worker.is_finished() {
            // Rust cannot kill a thread. Report failure within the deadline and
            // keep its fixture/oracles alive in the worker. The enclosing exact
            // test process has a separate hard deadline; this is never success.
            return Err(format!(
                "RPC server did not stop within existing 3s cleanup bound; {}",
                self.diagnostics.report()
            ));
        }
        match worker.join() {
            Ok(()) => {
                self.diagnostics.joined.store(true, Ordering::Release);
                Ok(())
            }
            Err(panic) => {
                let reason = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string panic");
                Err(format!(
                    "RPC server panicked: {reason}; {}",
                    self.diagnostics.report()
                ))
            }
        }
    }
}
impl Drop for OwnedRpcServer {
    fn drop(&mut self) {
        if self.worker.is_none() {
            return;
        }
        let result = self.finish();
        if thread::panicking() {
            eprintln!("{}", self.diagnostics.report());
        }
        if let Err(error) = result {
            eprintln!("{error}");
            if !thread::panicking() {
                panic!("{error}");
            }
        }
    }
}
