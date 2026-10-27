use super::fixtures;
use crate::{WorldServiceClientConfig, WorldServiceClientError, WorldServiceTransport};
use oasis7_client_api::world_service::{FORMAL_BFT_READ_PATH_V2, WorldIdentity};
use std::io::{Read, Write};
use std::time::Duration;

fn private_transport(endpoint: &str) -> WorldServiceTransport {
    WorldServiceTransport::new(WorldServiceClientConfig {
        endpoint: endpoint.into(),
        trusted_service_public_key: fixtures::public(72),
        expected_world: WorldIdentity {
            world_id: "fixture-world".into(),
            genesis_digest: fixtures::h(75),
        },
        scope_id: "agent:reader".into(),
        read_private_key_hex: hex::encode([73; 32]),
        timeout: Duration::from_secs(2),
        max_response_bytes: oasis7_client_api::world_service::MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V2,
    })
    .unwrap()
}

#[test]
fn private_formal_bft_transport_rejects_remote_plaintext() {
    for endpoint in ["http://192.0.2.1:9080", "http://localhost:9080"] {
        let mut transport = private_transport(endpoint);
        assert!(
            matches!(
                transport.use_private_connection(),
                Err(WorldServiceClientError::Configuration(reason))
                    if reason.contains("private goals require HTTPS or a literal loopback HTTP address")
            ),
            "endpoint={endpoint}"
        );
    }
}

#[test]
fn private_formal_bft_transport_accepts_https_and_literal_loopback() {
    for endpoint in [
        "https://service.example",
        "http://127.0.0.1:9080",
        "http://[::1]:9080",
    ] {
        private_transport(endpoint)
            .private_connection()
            .unwrap_or_else(|error| panic!("endpoint={endpoint}, error={error}"));
    }
}

#[test]
fn private_formal_bft_transport_does_not_follow_redirects() {
    let redirect_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    redirect_listener.set_nonblocking(true).unwrap();
    let redirect_address = redirect_listener.local_addr().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut byte = [0; 1];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or_default();
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{redirect_address}/redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });
    let mut transport = private_transport(&endpoint);
    transport.use_private_connection().unwrap();

    let result = transport.call::<_, serde_json::Value>(FORMAL_BFT_READ_PATH_V2, &());
    server.join().unwrap();
    assert!(matches!(
        result,
        Err(WorldServiceClientError::Rejected { status: 307, .. })
    ));
    assert!(matches!(
        redirect_listener.accept(),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
    ));
}
