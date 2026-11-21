//! Loopback-only diagnostic consumer. Never grants or changes Node admission.
use oasis7_node::NodeRuntime;
use serde::Serialize;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};

const PATH: &str = "/v1/bft/status";
const BUDGET: Duration = Duration::from_millis(100);
const MAX_REQUEST: usize = 2048;
const MAX_RESPONSE: usize = 16 * 1024;

pub(super) struct Listener {
    socket: TcpListener,
    incarnation: String,
    authority: String,
}

#[derive(Serialize)]
struct Response {
    api_version: &'static str,
    runtime_profile: &'static str,
    incarnation: String,
    running: bool,
    blocker: Option<&'static str>,
    qualification: &'static str,
    protected_services_readiness: &'static str,
    can_propose: bool,
    observation: Option<oasis7_node::NodeBftObservationV1>,
}

impl Listener {
    pub(super) fn bind(address: SocketAddr) -> Result<Self, String> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err("header Node status requires an explicit loopback port".into());
        }
        let mut random = [0_u8; 32];
        getrandom::fill(&mut random).map_err(|_| "header Node status identity unavailable")?;
        let socket = TcpListener::bind(address).map_err(|_| "header Node status bind failed")?;
        socket
            .set_nonblocking(true)
            .map_err(|_| "header Node status listener setup failed")?;
        Ok(Self {
            socket,
            incarnation: hex::encode(random),
            authority: address.to_string(),
        })
    }

    /// At most one bounded read-only request per serve iteration. The consensus
    /// worker is independent; a client cannot stall it or mutate its state.
    pub(super) fn poll(&self, runtime: &NodeRuntime) {
        let Ok((mut stream, peer)) = self.socket.accept() else {
            return;
        };
        if !peer.ip().is_loopback() || !valid_request(&mut stream, &self.authority) {
            return;
        }
        let status = runtime.bft_v3_status();
        let running = status.as_ref().is_some_and(|s| s.running);
        let blocker = if !running {
            Some("runtime_not_running")
        } else if status.as_ref().is_some_and(|s| s.blocked_reason.is_some()) {
            Some("runtime_blocked")
        } else {
            None
        };
        let response = Response {
            api_version: "oasis7.bft-node-observation/v1",
            runtime_profile: "bft/version=1",
            incarnation: self.incarnation.clone(),
            running,
            blocker,
            // Checkpoint facts do not attest backend protection, current lease,
            // private closure or current installed membership eligibility.
            qualification: "unknown",
            protected_services_readiness: "unknown",
            can_propose: false,
            observation: status.and_then(|s| s.observation),
        };
        let Ok(body) = serde_json::to_vec(&response) else {
            return;
        };
        if body.len() > MAX_RESPONSE {
            return;
        }
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        // Complete short writes within one absolute budget; never renew it.
        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(&body);
        let deadline = Instant::now() + BUDGET;
        let mut sent = 0;
        while sent < bytes.len() {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return;
            };
            if remaining.is_zero() || stream.set_write_timeout(Some(remaining)).is_err() {
                return;
            }
            match stream.write(&bytes[sent..]) {
                Ok(0) | Err(_) => return,
                Ok(n) => sent += n,
            }
        }
    }
}

fn valid_request(stream: &mut TcpStream, authority: &str) -> bool {
    let deadline = Instant::now() + BUDGET;
    let mut buffer = [0_u8; MAX_REQUEST];
    let mut used = 0;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return false;
        };
        if remaining.is_zero() || stream.set_read_timeout(Some(remaining)).is_err() {
            return false;
        }
        match stream.read(&mut buffer[used..]) {
            Ok(0) | Err(_) => return false,
            Ok(n) => used += n,
        }
        if buffer[..used].ends_with(b"\r\n\r\n") {
            return request_bytes_valid(&buffer[..used], authority);
        }
        if used == MAX_REQUEST {
            return false;
        }
    }
}

fn request_bytes_valid(bytes: &[u8], authority: &str) -> bool {
    if bytes.windows(4).position(|w| w == b"\r\n\r\n") != bytes.len().checked_sub(4) {
        return false;
    }
    let Ok(request) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut lines = request.split("\r\n");
    if lines.next() != Some(&format!("GET {PATH} HTTP/1.1")) {
        return false;
    }
    // No request body, upgrade, proxy forwarding or pipelining. This is not a
    // general HTTP API and does not share the private read service's authority.
    let mut host_seen = false;
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        // A space before ':' must not turn forbidden request framing or Origin
        // into an unknown header. Only RFC token bytes are valid header names;
        // control characters are never valid in a status request's values.
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || !value
                .bytes()
                .all(|b| b == b'\t' || (0x20..=0x7e).contains(&b))
        {
            return false;
        }
        if name.eq_ignore_ascii_case("host") {
            if host_seen || value.trim() != authority {
                return false;
            }
            host_seen = true;
        }
        if name.eq_ignore_ascii_case("origin")
            || name.eq_ignore_ascii_case("transfer-encoding")
            || name.eq_ignore_ascii_case("upgrade")
            || (name.eq_ignore_ascii_case("content-length") && value.trim() != "0")
        {
            return false;
        }
    }
    host_seen
}

#[cfg(test)]
pub(crate) fn collect_for_test(runtime: &NodeRuntime) -> serde_json::Value {
    use std::process::{Command, Stdio};
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = socket.local_addr().unwrap();
    socket.set_nonblocking(true).unwrap();
    let listener = Listener {
        socket,
        incarnation: "a".repeat(64),
        authority: address.to_string(),
    };
    let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
    let code = r#"
import importlib.util,json,sys,time
spec=importlib.util.spec_from_file_location('live_collector',sys.argv[1])
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
status,error=m.fetch(sys.argv[2])
assert error == "success", error
profile=m.adapter.project(status,'http',time.time_ns()//1_000_000)
print(json.dumps({'profile':profile,'markdown':m.adapter.markdown(profile)}))
"#;
    let mut child = Command::new("python3")
        .arg("-B")
        .arg("-c")
        .arg(code)
        .arg(scripts.join("p2p-triad-http-profile.py"))
        .arg(format!("http://{address}{PATH}"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("live diagnostic collector timed out");
        }
        listener.poll(runtime);
        std::thread::sleep(Duration::from_millis(1));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_listener_serves_bounded_unavailable_observation_and_stops() {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        socket.set_nonblocking(true).unwrap();
        let listener = Listener {
            socket,
            incarnation: "a".repeat(64),
            authority: address.to_string(),
        };
        let runtime = NodeRuntime::new(
            oasis7_node::NodeConfig::new("diagnostic", "world", oasis7_node::NodeRole::Sequencer)
                .unwrap(),
        );
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .write_all(b"GET /v1/bft/status HTTP/1.1\r\n")
                .unwrap();
            std::thread::sleep(Duration::from_millis(5));
            stream
                .write_all(format!("Host: {address}\r\n\r\n").as_bytes())
                .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !client.is_finished() {
            assert!(Instant::now() < deadline);
            listener.poll(&runtime);
            std::thread::sleep(Duration::from_millis(1));
        }
        let response = client.join().unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        assert!(headers.starts_with("HTTP/1.1 200 OK"));
        assert!(headers.contains("Cache-Control: no-store"));
        let value: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(value["can_propose"], false);
        assert_eq!(value["qualification"], "unknown");
        assert_eq!(value["blocker"], "runtime_not_running");
        assert!(value["observation"].is_null());
        drop(listener);
        assert!(TcpStream::connect(address).is_err());
    }

    #[test]
    fn live_listener_rejects_malformed_headers_and_recovers() {
        use std::net::Shutdown;

        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        socket.set_nonblocking(true).unwrap();
        let listener = Listener {
            socket,
            incarnation: "a".repeat(64),
            authority: address.to_string(),
        };
        let runtime = NodeRuntime::new(
            oasis7_node::NodeConfig::new("diagnostic", "world", oasis7_node::NodeRole::Sequencer)
                .unwrap(),
        );
        let send = |headers: &str| {
            let mut stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let raw = format!(
                "GET /v1/bft/status HTTP/1.1\r\nHost: {address}\r\n{headers}\r\n"
            );
            stream.write_all(raw.as_bytes()).unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            listener.poll(&runtime);
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            response
        };

        // Each of these previously passed as an unrelated header, allowing a
        // read-only status response to malformed or ambiguously framed input.
        for malformed in [
            "Content-Length : 9\r\n",
            "Transfer-Encoding : chunked\r\n",
            "Origin : https://untrusted.invalid\r\n",
            "X-Probe: bad\0value\r\n",
            "X-Probe: bad\nvalue\r\n",
        ] {
            assert!(send(malformed).is_empty(), "accepted {malformed:?}");
        }
        let valid = send("Accept: application/json\r\n");
        assert!(valid.starts_with(b"HTTP/1.1 200 OK\r\n"));
        let response = String::from_utf8(valid).unwrap();
        assert!(response.contains(r#""qualification":"unknown""#));
        assert!(response.contains(r#""can_propose":false"#));
    }

    #[test]
    fn status_endpoint_rejects_non_loopback_and_ambiguous_requests() {
        assert!(Listener::bind("0.0.0.0:8901".parse().unwrap()).is_err());
        assert!(Listener::bind("127.0.0.1:0".parse().unwrap()).is_err());
        assert!(request_bytes_valid(
            b"GET /v1/bft/status HTTP/1.1\r\nHost: 127.0.0.1:8901\r\n\r\n",
            "127.0.0.1:8901"
        ));
        for bytes in [b"POST /v1/bft/status HTTP/1.1\r\n\r\n".as_slice(),
            b"GET /v1/bft/status?x=1 HTTP/1.1\r\n\r\n",
            b"GET /v1/bft/status HTTP/1.1\r\nHost: attacker.invalid\r\n\r\n",
            b"GET /v1/bft/status HTTP/1.1\r\nHost: 127.0.0.1:8901\r\nHost: 127.0.0.1:8901\r\n\r\n",
            b"GET /v1/bft/status HTTP/1.1\r\nHost: 127.0.0.1:8901\r\nOrigin: https://attacker.invalid\r\n\r\n",
            b"GET /v1/bft/status HTTP/1.1\r\nContent-Length: 10\r\n\r\n",
            b"GET /v1/bft/status HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"] {
            assert!(!request_bytes_valid(bytes, "127.0.0.1:8901"));
        }
    }
}
