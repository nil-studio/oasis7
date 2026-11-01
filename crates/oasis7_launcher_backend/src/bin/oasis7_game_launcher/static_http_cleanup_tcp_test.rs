use super::*;
// Test seam uses the actual socket parser, fixed dispatcher and response writer.
pub(crate) fn cleanup_dispatch_for_test(
    identity: Arc<Mutex<Option<super::hosted_identity::IdentityService>>>,
    target: &str,
    headers: &str,
    body: &[u8],
) -> super::hosted_identity::IdentityReply {
    cleanup_dispatch_with_root_for_test(identity, target, headers, body, None)
}
// The real handler runs in its own socket thread. Install only that thread's
// fixture CA, rather than a process-global root or environment trust bypass.
pub(crate) fn cleanup_dispatch_with_root_for_test(
    identity: Arc<Mutex<Option<super::hosted_identity::IdentityService>>>,
    target: &str,
    headers: &str,
    body: &[u8],
    fixture_root: Option<Vec<u8>>,
) -> super::hosted_identity::IdentityReply {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Native fixture listener");
    let address = listener.local_addr().expect("Native fixture address");
    let runtime = StaticHttpRuntimeConfig {
        live_bind: String::new(),
        viewer_runtime_config_json: String::new(),
        identity,
        model_client: None,
    };
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("one Native fixture connection");
        let dispatch = || {
            handle_http_connection(
                stream,
                Path::new("."),
                &runtime,
                false,
                None,
                DeploymentMode::HostedPublicJoin,
                &Arc::new(Mutex::new(HostedPlayerSessionIssuer::default())),
                &Arc::new(Mutex::new(HostedAccountIdentityBroker::disabled())),
            )
            .expect("actual Native HTTP handler")
        };
        if let Some(root) = fixture_root {
            super::hosted_identity::IdentityService::with_execution_callback_fixture_root(
                &root, dispatch,
            );
        } else {
            dispatch();
        }
    });
    let mut client = TcpStream::connect(address).expect("Native fixture client");
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("Native response bound");
    let (_, remaining_headers) = headers.split_once("\r\n").expect("fixture request line");
    let remaining_headers = remaining_headers
        .strip_suffix("\r\n\r\n")
        .expect("fixture header terminator");
    write!(client, "POST {target} HTTP/1.1\r\n{remaining_headers}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len())
        .expect("Native request headers");
    client.write_all(body).expect("Native request body");
    client.flush().expect("Native request flush");
    let mut response = Vec::new();
    client
        .read_to_end(&mut response)
        .expect("Native response bytes");
    server.join().expect("Native fixture handler joined");
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("Native response headers");
    let header = std::str::from_utf8(&response[..split]).expect("Native response encoding");
    let status = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .expect("Native response status");
    super::hosted_identity::IdentityReply {
        status,
        body: serde_json::from_slice(&response[split + 4..]).expect("Native response JSON"),
        cookies: Vec::new(),
    }
}
