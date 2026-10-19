//! Real loopback protocol regressions; process-local environment, no accounts.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    process::Command,
    thread,
    time::{Duration, Instant},
};

const CHILD: &str = "OASIS7_TEST_WORLD_RUNTIME_TIMEOUT_CASE";

#[test]
fn runtime_control_timeout_environment_preserves_protocol_and_bounds() {
    if let Ok(case) = std::env::var(CHILD) {
        run_child(&case);
        return;
    }
    for (case, timeout) in [
        ("configured_slow", Some("4000")),
        ("fragmented_deadline", Some("100")),
        ("default_slow", None),
        ("minimum", Some("100")),
        ("maximum", Some("30000")),
        ("wrong_world", Some("4000")),
        ("non_loopback", Some("4000")),
        ("invalid", Some("")),
        ("invalid", Some("99")),
        ("invalid", Some("30001")),
        ("invalid", Some("+4000")),
        ("invalid", Some(" 4000")),
        ("invalid", Some("４０００")),
        ("invalid", Some("18446744073709551616")),
    ] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "hosted_identity::world::runtime_timeout_tests::runtime_control_timeout_environment_preserves_protocol_and_bounds",
                "--nocapture",
            ])
            .env(CHILD, case);
        match timeout {
            Some(value) => child.env("OASIS7_WORLD_SERVICE_TIMEOUT_MS", value),
            None => child.env_remove("OASIS7_WORLD_SERVICE_TIMEOUT_MS"),
        };
        if case == "invalid" {
            child.env("OASIS7_HOSTED_IDENTITY_V2_ENABLED", "1");
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "runtime timeout child {case} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        // An incorrect --exact name must not silently skip the regression.
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}

fn accept_bounded(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "runtime probe never connected");
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("runtime probe accept failed: {error}"),
        }
    }
}

fn run_child(case: &str) {
    if case == "non_loopback" {
        assert!(matches!(
            runtime::query("192.0.2.1:5023", "world", "player"),
            Err(StoreError::Rejected(reason)) if reason == "runtime_control_not_loopback"
        ));
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    if case == "invalid" {
        assert!(matches!(
            super::super::config::IdentityConfig::from_env(),
            Err(reason) if reason.contains("OASIS7_WORLD_SERVICE_TIMEOUT_MS")
        ));
        listener.set_nonblocking(true).unwrap();
        assert!(matches!(
            runtime::query(&address, "world", "player"),
            Err(StoreError::Unavailable(_))
        ));
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "invalid timeout must reject configuration before connecting"
        );
        return;
    }
    let wrong_world = case == "wrong_world";
    let slow = matches!(case, "configured_slow" | "default_slow");
    let fragmented = case == "fragmented_deadline";
    let (query_finished, finish_received) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let mut stream = accept_bounded(&listener);
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let hello: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(hello["type"], "hello");
        assert_eq!(
            hello["version"],
            oasis7_client_api::viewer_protocol::VIEWER_PROTOCOL_VERSION
        );
        writeln!(
            stream,
            "{}",
            json!({"type":"hello_ack","world_id":if wrong_world {"other-world"} else {"world"}})
        )
        .unwrap();
        if wrong_world {
            return;
        }
        line.clear();
        reader.read_line(&mut line).unwrap();
        let recovery: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(recovery["type"], "authoritative_recovery");
        assert_eq!(recovery["command"]["mode"], "reconnect_sync");
        assert_eq!(recovery["command"]["request"]["player_id"], "player");
        if slow {
            thread::sleep(Duration::from_millis(2_200));
        }
        let mut response = serde_json::to_vec(&json!({"type":"authoritative_recovery_ack","ack":{
            "status":"catch_up_ready","player_id":"player","message":"delta_replay_allowed",
            "snapshot_hash":"actual-loopback-probe","snapshot_height":1,
            "session_pubkey":null,"session_epoch":7
        }}))
        .unwrap();
        response.push(b'\n');
        if fragmented {
            for index in 0..3 {
                thread::sleep(Duration::from_millis(60));
                let start = response.len() * index / 3;
                let end = response.len() * (index + 1) / 3;
                // Late writes after the expected absolute timeout are harmless.
                if stream.write_all(&response[start..end]).is_err() {
                    break;
                }
            }
            // Keep both stream handles alive until query has returned. Thus
            // EOF/early server disconnect cannot satisfy the rejection test.
            finish_received
                .recv_timeout(Duration::from_secs(5))
                .expect("query must complete while the actual receiver stays open");
        } else {
            // A default-budget client deliberately times out before this reply.
            let _ = stream.write_all(&response);
        }
    });
    let result = runtime::query(&address, "world", "player");
    let _ = query_finished.send(());
    server
        .join()
        .expect("bounded actual protocol server completed");
    match case {
        "configured_slow" | "minimum" | "maximum" => {
            let fence = result.unwrap_or_else(|_| panic!("configured runtime read must succeed"));
            assert!(fence.key.is_none());
            assert_eq!(fence.epoch, 7);
        }
        "default_slow" | "fragmented_deadline" => {
            assert!(
                matches!(result, Err(StoreError::Unavailable(_))),
                "a fragmented ACK beyond the single configured deadline must not succeed"
            );
        }
        "wrong_world" => assert!(
            matches!(result, Err(StoreError::Rejected(reason)) if reason=="runtime_world_mismatch")
        ),
        _ => panic!("unknown runtime timeout child case"),
    }
}
