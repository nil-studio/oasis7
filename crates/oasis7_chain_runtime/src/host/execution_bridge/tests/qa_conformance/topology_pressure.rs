//! Distinct Linux network namespaces exercise the unchanged production listener.
use super::*;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

fn coordination() -> PathBuf {
    PathBuf::from(std::env::var("PRE2_PRESSURE_COORD").unwrap())
}
fn line(socket: &mut BufReader<TcpStream>, value: &str) {
    writeln!(socket.get_mut(), "{value}").unwrap();
    socket.get_mut().flush().unwrap();
}
fn receive(socket: &mut BufReader<TcpStream>) -> String {
    let mut value = String::new();
    assert!(
        socket.read_line(&mut value).unwrap() > 0,
        "pressure worker disconnected"
    );
    value.trim().to_owned()
}
fn exchange(endpoint: &str) -> String {
    let mut socket = TcpStream::connect(endpoint).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let _ = socket.write_all(b"POST /v1/world/describe HTTP/1.1\r\nContent-Length: 0\r\n\r\n");
    let mut bytes = Vec::new();
    if let Err(error) = socket.read_to_end(&mut bytes) {
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
    String::from_utf8(bytes).unwrap()
}
fn witness(active: usize, peers: usize) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_millis(600);
    loop {
        let actual = fs::read(coordination().join("admission.json"))
            .ok()
            .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok());
        if let Some(value) = actual
            && value["active"] == active
            && value["peers"].as_object().unwrap().len() == peers
        {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "actual admitted permits never reached {active}/{peers}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn client() -> RemoteWorldServiceClient {
    RemoteWorldServiceClient::new(WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: "http://world-service:4200".into(),
        trusted_service_public_key: sign_read_request("service", (), &hex::encode([9u8; 32]))
            .unwrap()
            .subject_public_key,
        expected_world: WorldIdentity {
            world_id: "w1".into(),
            genesis_digest: "fixture-genesis-v1".into(),
        },
        scope_id: "agent:agent-a".into(),
        read_private_key_hex: hex::encode([7u8; 32]),
        timeout: Duration::from_secs(1),
        max_response_bytes: 1_048_576,
    })
    .unwrap()
}
#[test]
#[ignore = "fixed Linux artifact production listener process"]
fn pressure_service_entry() {
    let fixture = Fixture::with_controlled_commits(true);
    let before = fixture
        .driver
        .lock()
        .unwrap()
        .state
        .last_applied_committed_height;
    fs::write(
        coordination().join("service-public-key"),
        sign_read_request("service", (), &hex::encode([9u8; 32]))
            .unwrap()
            .subject_public_key,
    )
    .unwrap();
    let mut server = crate::status_admission::start_test_server(
        &fixture.root,
        "http://0.0.0.0:4200",
        fixture.node.clone(),
        crate::feedback_submit_api::FeedbackSubmitSigner {
            private_key_hex: hex::encode([9u8; 32]),
            public_key_hex: sign_read_request("service", (), &hex::encode([9u8; 32]))
                .unwrap()
                .subject_public_key,
        },
    )
    .unwrap();
    fs::write(
        coordination().join("service-ready"),
        b"production-status-loop",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    while !coordination().join("done").exists() {
        assert!(
            Instant::now() < deadline,
            "pressure coordinator did not finish"
        );
        thread::sleep(Duration::from_millis(10));
    }
    crate::status_server_support::stop_chain_status_server(&mut server);
    witness(0, 0);
    assert_eq!(
        fixture
            .driver
            .lock()
            .unwrap()
            .state
            .last_applied_committed_height,
        before
    );
    println!("PRE2_PRESSURE_CANONICAL_UNCHANGED_PASSED");
}
#[test]
#[ignore = "one distinct Docker source IP, four genuine incomplete connections"]
fn pressure_source_entry() {
    let id = std::env::var("PRE2_PRESSURE_SOURCE").unwrap();
    let mut control = BufReader::new(TcpStream::connect("pressure-control:4300").unwrap());
    control
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    line(&mut control, &id);
    let mut held = Vec::new();
    loop {
        match receive(&mut control).as_str() {
            "hold" => {
                assert!(held.is_empty());
                for _ in 0..4 {
                    let mut socket = TcpStream::connect("world-service:4200").unwrap();
                    socket
                        .write_all(b"POST /v1/world/describe HTTP/1.1\r\nContent-Length: 100\r\n")
                        .unwrap();
                    held.push(socket);
                }
                line(
                    &mut control,
                    &format!("held:{}", held[0].local_addr().unwrap().ip()),
                );
            }
            "peer" => {
                let response = exchange("world-service:4200");
                assert!(response.starts_with("HTTP/1.1 429"), "{response}");
                assert!(response.to_ascii_lowercase().contains("retry-after: 1"));
                line(&mut control, "peer429");
            }
            "release" => {
                held.clear();
                line(&mut control, "released");
            }
            "quit" => {
                assert!(held.is_empty());
                line(&mut control, "joined");
                break;
            }
            other => panic!("unknown control command {other}"),
        }
    }
}
fn snapshot() {
    let deadline = Instant::now() + Duration::from_secs(10);
    let socket = loop {
        if let Ok(socket) = TcpStream::connect("application:4100") {
            break socket;
        }
        assert!(
            Instant::now() < deadline,
            "shipped Viewer listener unavailable"
        );
        thread::sleep(Duration::from_millis(25));
    };
    let mut socket = BufReader::new(socket);
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    for request in [
        serde_json::json!({"type":"hello_v2","client":"PRE2 pressure","version":2,"capabilities":[]}),
        serde_json::json!({"type":"subscribe","streams":["snapshot"],"event_kinds":[]}),
        serde_json::json!({"type":"request_snapshot"}),
    ] {
        line(&mut socket, &request.to_string());
    }
    loop {
        let message: serde_json::Value = serde_json::from_str(&receive(&mut socket)).unwrap();
        if message["type"] == "snapshot" {
            break;
        }
    }
}
#[test]
#[ignore = "ninth distinct source controls real global32 pressure and shipped Viewer"]
fn pressure_controller_entry() {
    let listener = TcpListener::bind("0.0.0.0:4300").unwrap();
    listener.set_nonblocking(true).unwrap();
    fs::write(coordination().join("controller-ready"), b"ready").unwrap();
    let mut sources = std::collections::BTreeMap::new();
    let sources_deadline = Instant::now() + Duration::from_secs(60);
    for _ in 0..8 {
        let (socket, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < sources_deadline,
                        "eight pressure sources never became ready"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("pressure controller accept: {error}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut socket = BufReader::new(socket);
        let id = receive(&mut socket);
        assert!(sources.insert(id, socket).is_none());
    }
    let client = client();
    client.describe().unwrap();
    snapshot();
    let first = sources.values_mut().next().unwrap();
    line(first, "hold");
    assert!(receive(first).starts_with("held:"));
    witness(4, 1);
    line(first, "peer");
    assert_eq!(receive(first), "peer429");
    client.describe().unwrap();
    line(first, "release");
    assert_eq!(receive(first), "released");
    witness(0, 0);
    let begun = Instant::now();
    for socket in sources.values_mut() {
        line(socket, "hold");
    }
    let mut ips = std::collections::BTreeSet::new();
    for socket in sources.values_mut() {
        assert!(ips.insert(receive(socket)));
    }
    let saturated = witness(32, 8);
    assert!(
        saturated["peers"]
            .as_object()
            .unwrap()
            .values()
            .all(|v| v == 4)
    );
    let response = exchange("world-service:4200");
    assert!(response.starts_with("HTTP/1.1 503"), "{response}");
    assert!(response.to_ascii_lowercase().contains("retry-after: 1"));
    assert!(
        begun.elapsed() < Duration::from_secs(2),
        "missed unchanged ingress deadline"
    );
    let first = sources.values_mut().next().unwrap();
    line(first, "release");
    assert_eq!(receive(first), "released");
    witness(28, 7);
    client.describe().unwrap();
    for socket in sources.values_mut().skip(1) {
        line(socket, "release");
        assert_eq!(receive(socket), "released");
    }
    witness(0, 0);
    snapshot();
    for socket in sources.values_mut() {
        line(socket, "quit");
        assert_eq!(receive(socket), "joined");
    }
    fs::write(coordination().join("pressure-result.json"),serde_json::to_vec(&serde_json::json!({
        "global32":"passed","independent_source":"passed","actual_saturation":saturated,
        "distinct_source_reports":ips,"same_peer429":true,"ninth_source503":true,"retry_after":1,
        "signed_describe_recovered":true,"shipped_snapshot_recovered":true,"real_multinode":"not_run"})).unwrap()).unwrap();
    fs::write(coordination().join("done"), b"joined").unwrap();
    println!("PRE2_GLOBAL32_INDEPENDENT_SOURCE_PRESSURE_PASSED");
}
