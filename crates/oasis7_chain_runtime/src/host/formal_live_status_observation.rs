//! Operator-side verification of challenge-bound Host diagnostics.
use super::formal_live_config::MAX_NODE_ID_BYTES;
use super::formal_live_service::{
    CHAIN_STATUS_PATH, FormalLiveStatusV1, http::FormalLiveStatusChallengeV1,
};
use oasis7::world_service::{
    authority::{request_digest, verify_service_response},
    wire::SignedServiceResponse,
};
use oasis7_client_api::world_service::WorldIdentity;
use serde::{Deserialize, Serialize};
use std::{io::Read, time::Duration};

const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StatusPins {
    pub(super) status_url: String,
    pub(super) service_public_key: String,
    pub(super) expected_world: WorldIdentity,
    pub(super) expected_node_id: String,
}

#[derive(Serialize)]
pub(super) struct StatusObservation {
    api_version: &'static str,
    observation_scope: &'static str,
    authority_qualification: &'static str,
    world: WorldIdentity,
    node_id: String,
    host_state: String,
    node_running: bool,
    host_reports_ready: bool,
    committed_position: Option<u64>,
    committed_record_id: Option<String>,
    runtime_error_present: bool,
}

pub(super) fn read_pins(input: impl Read) -> Result<StatusPins, String> {
    const MAX_PINS_BYTES: u64 = 16 * 1024;
    let mut bytes = Vec::new();
    input.take(MAX_PINS_BYTES + 1).read_to_end(&mut bytes)
        .map_err(|_| "formal status observation input read failed")?;
    if bytes.len() as u64 > MAX_PINS_BYTES {
        return Err("formal status observation input exceeds bound".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid formal status observation input".into())
}

pub(super) fn observe(pins: &StatusPins) -> Result<StatusObservation, String> {
    let url = reqwest::Url::parse(&pins.status_url).map_err(|_| "invalid formal status URL")?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != CHAIN_STATUS_PATH
    {
        return Err("invalid formal status URL".into());
    }
    pins.expected_world
        .validate()
        .map_err(|_| "invalid formal status world pin")?;
    if pins.expected_node_id.trim().is_empty()
        || pins.expected_node_id.len() > MAX_NODE_ID_BYTES
        || pins.expected_node_id.chars().any(char::is_control)
        || pins.service_public_key.len() != 64
        || !pins
            .service_public_key
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("invalid formal status identity pin".into());
    }

    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| "formal status observation nonce unavailable")?;
    let challenge = FormalLiveStatusChallengeV1 {
        observation_nonce: hex::encode(nonce),
    };
    let http = oasis7_game_client::formal_read_http_client(TIMEOUT)
        .map_err(|_| "formal status HTTP setup failed")?;
    let response = http
        .post(url)
        .json(&challenge)
        .send()
        .map_err(|_| "formal status request failed")?;
    if response.status() != reqwest::StatusCode::OK {
        return Err("formal status observation unavailable".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "formal status response read failed")?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("formal status response exceeds bound".into());
    }
    let signed = serde_json::from_slice(&bytes).map_err(|_| "invalid formal status response")?;
    verify(pins, &challenge, signed)
}

fn verify(
    pins: &StatusPins,
    challenge: &FormalLiveStatusChallengeV1,
    signed: SignedServiceResponse<FormalLiveStatusV1>,
) -> Result<StatusObservation, String> {
    let digest = request_digest(CHAIN_STATUS_PATH, challenge)?;
    verify_service_response(
        CHAIN_STATUS_PATH,
        &digest,
        &signed,
        &pins.service_public_key,
    )
    .map_err(|_| "formal status signature or observation nonce rejected")?;
    let status = signed.payload;
    if status.api_version != "oasis7.formal-live-status/v1"
        || status.runtime_profile != "formal_live_v1"
        || status.world != pins.expected_world
        || status.node_id != pins.expected_node_id
        || !matches!(
            status.state.as_str(),
            "starting" | "stopped" | "degraded" | "ready" | "stopping"
        )
        || status.ready != status.can_propose
        || status.ready != (status.state == "ready")
        || (status.ready && (!status.node_running || status.committed_position.is_none()))
        || status.committed_position.is_some() != status.committed_record_id.is_some()
        || status
            .committed_record_id
            .as_ref()
            .is_some_and(|id| id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("formal status identity or observation shape rejected".into());
    }
    Ok(StatusObservation {
        api_version: "oasis7.formal-live-status-observation/v1",
        observation_scope: "authenticated_host_diagnostics_only",
        authority_qualification: "unknown",
        world: status.world,
        node_id: status.node_id,
        host_state: status.state,
        node_running: status.node_running,
        host_reports_ready: status.ready,
        committed_position: status.committed_position,
        committed_record_id: status.committed_record_id,
        runtime_error_present: status.last_error.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7::world_service::authority::sign_service_response;
    use ed25519_dalek::SigningKey;

    #[test]
    fn status_observer_stdin_pins_are_bounded() {
        let bytes = br#"{"status_url":"http://127.0.0.1:8000/v1/chain/status","service_public_key":"key","expected_world":{"world_id":"world","genesis_digest":"genesis"},"expected_node_id":"node"}"#;
        let pins = read_pins(bytes.as_slice()).unwrap();
        assert_eq!(pins.expected_node_id, "node");
        let oversized = std::io::repeat(b' ').take(16 * 1024 + 1);
        assert!(read_pins(oversized).is_err());
    }

    #[test]
    fn fixed_legacy_signature_and_other_challenge_do_not_verify() {
        let signer = SigningKey::from_bytes(&[19; 32]);
        let world = WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "a".repeat(64),
        };
        let pins = StatusPins {
            status_url: String::new(),
            service_public_key: hex::encode(signer.verifying_key().to_bytes()),
            expected_world: world.clone(),
            expected_node_id: "observer-target".into(),
        };
        let status = FormalLiveStatusV1 {
            api_version: "oasis7.formal-live-status/v1".into(),
            runtime_profile: "formal_live_v1".into(),
            state: "stopped".into(),
            ready: false,
            can_propose: false,
            node_id: pins.expected_node_id.clone(),
            world,
            node_running: false,
            tick_count: 0,
            committed_position: None,
            committed_record_id: None,
            last_error: None,
            effect_delivery: None,
        };
        let challenge = FormalLiveStatusChallengeV1 { observation_nonce: "b".repeat(64) };
        let other = FormalLiveStatusChallengeV1 { observation_nonce: "c".repeat(64) };
        let signing_key = hex::encode(signer.to_bytes());

        for digest in [
            request_digest(CHAIN_STATUS_PATH, &()).unwrap(),
            request_digest(CHAIN_STATUS_PATH, &other).unwrap(),
        ] {
            let signed =
                sign_service_response(CHAIN_STATUS_PATH, digest, status.clone(), &signing_key)
                    .unwrap();
            assert!(verify(&pins, &challenge, signed).is_err());
        }

        let digest = request_digest(CHAIN_STATUS_PATH, &challenge).unwrap();
        let signed = sign_service_response(CHAIN_STATUS_PATH, digest, status, &signing_key).unwrap();
        assert!(verify(&pins, &challenge, signed).is_ok());
    }
}
