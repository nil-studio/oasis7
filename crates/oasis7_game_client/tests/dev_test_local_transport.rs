#![cfg(unix)]
use oasis7_game_client::dev_test_local_adapter as transport;
#[cfg(target_os = "linux")]
use std::{io::Write, os::unix::net::UnixStream, time::Duration};

#[cfg(target_os = "linux")]
#[test]
fn peer_identity_precedes_payload_parsing() {
    let (mut server, mut client) = UnixStream::pair().unwrap();
    let uid = unsafe { libc::geteuid() };
    client.write_all(b"not-json-and-no-token").unwrap();
    client.shutdown(std::net::Shutdown::Write).unwrap();
    let denied = transport::LocalPolicy {
        service_uid: uid,
        caller_uid: uid.wrapping_add(1),
        max_request_bytes: 64,
        timeout: Duration::from_secs(1),
    };
    assert_eq!(
        transport::receive(&mut server, &denied).unwrap_err(),
        "local caller denied"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn same_uid_policy_is_rejected() {
    let uid = unsafe { libc::geteuid() };
    let policy = transport::LocalPolicy {
        service_uid: uid,
        caller_uid: uid,
        max_request_bytes: 4,
        timeout: Duration::from_secs(1),
    };
    let (mut server, _client) = UnixStream::pair().unwrap();
    assert_eq!(
        transport::receive(&mut server, &policy).unwrap_err(),
        "invalid local isolation policy"
    );
}

#[test]
fn model_cannot_supply_identity_or_secret_paths() {
    for extra in ["caller_id", "credential", "config_path", "url"] {
        let mut request = serde_json::json!({"profile":"dev-test-accounts-v1", "audience":"oasis7-dev-test-tools", "method":"capabilities"});
        request
            .as_object_mut()
            .unwrap()
            .insert(extra.into(), serde_json::json!("forbidden"));
        assert!(serde_json::from_value::<transport::LocalRequest>(request).is_err());
    }
}
