use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn read_probe_request(stream: &mut TcpStream) -> Vec<u8> {
    let started_at = Instant::now();
    let mut request = vec![0_u8; 1024];
    loop {
        match stream.read(&mut request) {
            Ok(bytes) => {
                request.truncate(bytes);
                return request;
            }
            Err(err)
                if err.kind() == ErrorKind::WouldBlock
                    && started_at.elapsed() < Duration::from_millis(500) =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(err) => panic!("read request: {err}"),
        }
    }
}

pub(super) struct RuntimeProviderProbeServer {
    shutdown: mpsc::Sender<()>,
    serve: Option<thread::JoinHandle<()>>,
}

impl RuntimeProviderProbeServer {
    pub(super) fn join(mut self) -> thread::Result<()> {
        let _ = self.shutdown.send(());
        self.serve.take().expect("provider server thread").join()
    }
}

impl Drop for RuntimeProviderProbeServer {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(serve) = self.serve.take() {
            let _ = serve.join();
        }
    }
}

pub(super) fn spawn_runtime_provider_probe_server() -> (String, RuntimeProviderProbeServer) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    listener
        .set_nonblocking(true)
        .expect("set test listener nonblocking");
    let bind = listener.local_addr().expect("listener addr");
    let (shutdown, stopped) = mpsc::channel();
    let serve = thread::spawn(move || {
        while stopped.try_recv() == Err(mpsc::TryRecvError::Empty) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    // Bound cleanup if a client connects but never sends its
                    // request; fixture lifetime itself belongs to the test.
                    stream
                        .set_read_timeout(Some(Duration::from_millis(500)))
                        .expect("set provider request read timeout");
                    let request = read_probe_request(&mut stream);
                    let request_text = String::from_utf8_lossy(&request);
                    let body = if request_text.contains("GET /v1/provider/info") {
                        r#"{"provider_id":"provider_local_bridge","name":"Provider Local Bridge","version":"0.1.0","protocol_version":"world-simulator-provider-loopback-http-v1","chain_resource_manifest_schema_version":"oasis7.world_resource_manifest.v1","chain_resource_delta_schema_version":"oasis7.world_resource_delta.v1","capabilities":["decision","feedback"],"supported_action_sets":["wait","wait_ticks","move_agent","speak_to_nearby","inspect_target","simple_interact"]}"#
                    } else {
                        r#"{"ok":true,"status":"ready","uptime_ms":42,"last_error":null,"queue_depth":0}"#
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => panic!("accept probe connection: {err}"),
            }
        }
    });
    (
        format!("http://{bind}"),
        RuntimeProviderProbeServer {
            shutdown,
            serve: Some(serve),
        },
    )
}
