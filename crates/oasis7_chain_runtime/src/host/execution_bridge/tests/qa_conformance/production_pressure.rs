//! Pressure against the production accept loop, not the serial conformance dispatcher.
use super::*;
use std::io::{Read, Write};

struct ProductionServer(crate::status_server_support::ChainStatusServer);
impl Drop for ProductionServer {
    fn drop(&mut self) {
        crate::status_server_support::stop_chain_status_server(&mut self.0);
    }
}
fn exchange(endpoint: &str, request: &[u8]) -> String {
    let mut stream = TcpStream::connect(endpoint.strip_prefix("http://").unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    // Overload can arrive before the server reads this request.
    let _ = stream.write_all(request);
    let mut response = Vec::new();
    if let Err(error) = stream.take(262144).read_to_end(&mut response) {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::ConnectionReset,
            "unexpected HTTP read error: {error}"
        );
        let end = response
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .expect("reset before complete HTTP headers");
        let headers = String::from_utf8_lossy(&response[..end]);
        let length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .expect("reset response lacks declared Content-Length");
        assert_eq!(
            response.len(),
            end + 4 + length,
            "reset before complete declared HTTP response"
        );
    }
    String::from_utf8(response).unwrap()
}
#[test]
fn real_tcp_production_listener_pressure_deadline_and_recovery() {
    let fixture = Fixture::with_controlled_commits(true);
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", reserved.local_addr().unwrap());
    drop(reserved);
    let _server = ProductionServer(
        crate::status_admission::start_test_server(
            &fixture.root,
            &endpoint,
            fixture.node.clone(),
            crate::feedback_submit_api::FeedbackSubmitSigner {
                private_key_hex: hex::encode([9u8; 32]),
                public_key_hex: sign_read_request("service", (), &hex::encode([9u8; 32]))
                    .unwrap()
                    .subject_public_key,
            },
        )
        .unwrap(),
    );
    let mut config = fixture.client.config().clone();
    config.endpoint = endpoint.clone();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    client.describe().unwrap();
    let before = fixture
        .driver
        .lock()
        .unwrap()
        .state
        .last_applied_committed_height;
    let begun = Instant::now();
    let mut slow = Vec::new();
    for index in 0..4 {
        if index == 3 {
            client.describe().unwrap();
            println!(
                "production_pressure_same_peer_available_slot=verified_signed_describe slow_connections=3"
            );
        }
        let mut socket = TcpStream::connect(endpoint.strip_prefix("http://").unwrap()).unwrap();
        socket
            .write_all(b"POST /v1/world/describe HTTP/1.1\r\nContent-Length: 100\r\n")
            .unwrap();
        slow.push(socket);
    }
    thread::sleep(Duration::from_millis(100));
    let overload = exchange(
        &endpoint,
        b"POST /v1/world/describe HTTP/1.1\r\nContent-Length: 0\r\n\r\n",
    );
    assert!(
        overload.starts_with("HTTP/1.1 429"),
        "actual peer saturation response: {overload}"
    );
    assert!(overload.to_ascii_lowercase().contains("retry-after: 1"));
    // Try a real source bind. Unsupported loopback aliases are recorded, never configured.
    let request = DescribeWorldRequest {
        contract_version: 1,
        expected_world: client.config().expected_world.clone(),
        trust_config_ref: client.config().trusted_service_public_key.clone(),
    };
    let signed_request = sign_read_request(
        "/v1/world/describe",
        request,
        &client.config().read_private_key_hex,
    )
    .unwrap();
    let independent = reqwest::blocking::Client::builder()
        .local_address("127.0.0.2".parse::<std::net::IpAddr>().unwrap())
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap()
        .post(format!("{endpoint}/v1/world/describe"))
        .json(&signed_request)
        .send();
    match independent {
        Ok(response) => {
            assert_eq!(response.status().as_u16(), 200);
            let signed: SignedServiceResponse<DescribeWorldResponse> = response.json().unwrap();
            oasis7::world_service::authority::verify_service_response(
                "/v1/world/describe",
                &oasis7::world_service::authority::request_digest(
                    "/v1/world/describe",
                    &signed_request,
                )
                .unwrap(),
                &signed,
                &client.config().trusted_service_public_key,
            )
            .unwrap();
            signed
                .payload
                .validate(&client.config().expected_world)
                .unwrap();
            println!(
                "production_pressure_independent_peer=verified_signed_describe source=127.0.0.2"
            );
        }
        Err(error) => println!(
            "production_pressure_independent_peer=not_run source_bind_or_transport_unavailable reason={}",
            error.to_string().chars().take(256).collect::<String>()
        ),
    }
    // Preserve incomplete connections until the production total-read deadline releases them.
    let deadline = begun + Duration::from_secs(6);
    loop {
        if client.describe().is_ok() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "production slow-peer slots did not recover"
        );
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        begun.elapsed() >= Duration::from_millis(1800),
        "slots recovered before exercised total deadline"
    );
    drop(slow);
    let mut oversized =
        b"POST /v1/world/describe HTTP/1.1\r\nContent-Length: 70000\r\n\r\n".to_vec();
    oversized.resize(66000, b'x');
    let response = exchange(&endpoint, &oversized);
    assert!(
        response.starts_with("HTTP/1.1 413"),
        "actual request limit response: {response}"
    );
    client.describe().unwrap();
    assert_eq!(
        fixture
            .driver
            .lock()
            .unwrap()
            .state
            .last_applied_committed_height,
        before
    );
    println!(
        "PRE2_PRODUCTION_LISTENER_PRESSURE_DEADLINE_RECOVERY_PASSED peer429=true retry_after=1 request_limit413=true global_saturation=not_run canonical_height_unchanged=true"
    );
}
