use super::*;
use ed25519_dalek::SigningKey;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpStream;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub(super) const VIEWER_AGENT_DECISION_SOURCE_ENV: &str = "OASIS7_AGENT_DECISION_SOURCE";
pub(super) const VIEWER_AGENT_PROVIDER_BACKEND_ENV: &str = "OASIS7_AGENT_PROVIDER_BACKEND";
pub(super) const VIEWER_AGENT_PROVIDER_CONTRACT_ENV: &str = "OASIS7_AGENT_PROVIDER_CONTRACT";
pub(super) const VIEWER_AGENT_PROVIDER_TRANSPORT_ENV: &str = "OASIS7_AGENT_PROVIDER_TRANSPORT";
pub(super) const VIEWER_AGENT_PROVIDER_URL_ENV: &str = "OASIS7_AGENT_PROVIDER_URL";
pub(super) const VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV: &str = "OASIS7_AGENT_PROVIDER_AUTH_TOKEN";
pub(super) const VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV: &str =
    "OASIS7_AGENT_PROVIDER_CONNECT_TIMEOUT_MS";
pub(super) const VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV: &str =
    "OASIS7_AGENT_PROVIDER_DECISION_TIMEOUT_MS";
pub(super) const VIEWER_AGENT_PROVIDER_PROFILE_ENV: &str = "OASIS7_AGENT_PROVIDER_PROFILE";
pub(super) const VIEWER_AGENT_EXECUTION_LANE_ENV: &str = "OASIS7_AGENT_EXECUTION_LANE";
pub(super) const VIEWER_AGENT_PROVIDER_MODE_ENV: &str = "OASIS7_AGENT_PROVIDER_MODE";
pub(super) const RUNTIME_AGENT_CHAT_ECHO_ENV: &str = "OASIS7_RUNTIME_AGENT_CHAT_ECHO";
pub(super) const RUNTIME_LIVE_LLM_TIMEOUT_ENV: &str = "OASIS7_RUNTIME_LIVE_LLM_TIMEOUT_MS";
pub(super) const HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV: &str =
    "OASIS7_HOSTED_STRONG_AUTH_PUBLIC_KEY";

pub(super) fn test_signer(seed: u8) -> (String, String) {
    let private_key = [seed; 32];
    let signing_key = SigningKey::from_bytes(&private_key);
    (
        hex::encode(signing_key.verifying_key().to_bytes()),
        hex::encode(private_key),
    )
}

pub(super) fn removed_old_brand_runtime_live_env(suffix: &str) -> String {
    ["AGENT", "WORLD", suffix].join("_")
}

pub(super) fn lock_test_llm_env() -> std::sync::MutexGuard<'static, ()> {
    let guard = runtime_provider_env_lock().lock().expect("env lock");
    clear_runtime_provider_env();
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_MODEL, "gpt-4o-mini");
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(
            crate::simulator::ENV_LLM_BASE_URL,
            "https://api.openai.com/v1",
        );
    }
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_API_KEY, "test-api-key");
    }
    guard
}

pub(super) fn clear_runtime_provider_env() {
    let removed_old_brand_envs = [
        removed_old_brand_runtime_live_env("AGENT_DECISION_SOURCE"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_BACKEND"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_CONTRACT"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_TRANSPORT"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_URL"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_AUTH_TOKEN"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_CONNECT_TIMEOUT_MS"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_DECISION_TIMEOUT_MS"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_PROFILE"),
        removed_old_brand_runtime_live_env("AGENT_EXECUTION_LANE"),
        removed_old_brand_runtime_live_env("AGENT_PROVIDER_MODE"),
        removed_old_brand_runtime_live_env("RUNTIME_AGENT_CHAT_ECHO"),
    ];
    for env_name in [
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
        RUNTIME_AGENT_CHAT_ECHO_ENV,
        RUNTIME_LIVE_LLM_TIMEOUT_ENV,
    ] {
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            oasis7::env_mut::remove_var(env_name);
        }
    }
    for env_name in removed_old_brand_envs {
        // SAFETY: This test/setup code mutates process environment in a controlled scope.
        unsafe {
            oasis7::env_mut::remove_var(env_name);
        }
    }
}

pub(super) fn runtime_provider_env_lock() -> &'static Mutex<()> {
    super::super::canonical_runtime_provider_env_lock()
}

pub(super) fn clear_hosted_strong_auth_env() {
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::remove_var(HOSTED_STRONG_AUTH_GRANT_PUBLIC_KEY_ENV);
    }
}

pub(super) fn hosted_strong_auth_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(super) fn lock_test_hosted_strong_auth_env() -> std::sync::MutexGuard<'static, ()> {
    let guard = hosted_strong_auth_env_lock().lock().expect("env lock");
    clear_hosted_strong_auth_env();
    guard
}

pub(super) fn test_now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(super) fn connect_runtime_live_client(
    addr: &str,
) -> (BufReader<TcpStream>, BufWriter<TcpStream>) {
    let stream = TcpStream::connect(addr).expect("connect runtime live");
    stream.set_nodelay(true).expect("set_nodelay");
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .expect("set_read_timeout");
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .expect("set_write_timeout");
    let reader_stream = stream.try_clone().expect("clone stream");
    let writer_stream = stream;
    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(writer_stream);
    serde_json::to_writer(
        &mut writer,
        &ViewerRequest::Hello {
            client: "runtime-live-test".to_string(),
            version: VIEWER_PROTOCOL_VERSION,
        },
    )
    .expect("write hello");
    writer.write_all(b"\n").expect("write hello newline");
    writer.flush().expect("flush hello");

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).expect("read hello ack");
        assert!(bytes > 0, "runtime live socket closed before hello ack");
        if line.trim().is_empty() {
            continue;
        }
        let response: ViewerResponse = serde_json::from_str(line.trim()).expect("parse response");
        if matches!(response, ViewerResponse::HelloAck { .. }) {
            break;
        }
    }

    (reader, writer)
}
