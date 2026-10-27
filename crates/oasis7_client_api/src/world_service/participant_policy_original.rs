//! Original policy evidence. Verification is not native-session or world-write admission.
use super::{PARTICIPANT_POLICY_PATH, ParticipantPolicyRequestV1};
use crate::world_service::{
    SignedReadRequest, SignedServiceResponse, WorldIdentity,
    private_goal::verify_owner_signature_strict, request_digest,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantPolicyOriginalV1 {
    pub request: SignedReadRequest<ParticipantPolicyRequestV1>,
    pub response: SignedServiceResponse<Value>,
}
impl ParticipantPolicyOriginalV1 {
    /// The nonce must come from the consumer's persisted, charged attempt; the
    /// World and key come from fixed trust configuration, never this packet.
    /// Payload policy, native binding and live authority need separate checks.
    pub fn verify_strict(
        &self,
        observation_nonce: &str,
        world: &WorldIdentity,
        world_public_key: &str,
    ) -> Result<(), String> {
        let request = &self.request.request;
        request.validate()?;
        if observation_nonce.len() != 64
            || !observation_nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || request.observation_nonce != observation_nonce
            || &request.expected_world != world
            || request.trust_config_ref != world_public_key
            || self.response.payload["policy"]["issuer_public_key"]
                != self.request.subject_public_key
        {
            return Err("participant policy original context mismatch".into());
        }
        verify_owner_signature_strict(
            PARTICIPANT_POLICY_PATH,
            request,
            &self.request.subject_public_key,
            &self.request.signature_hex,
        )?;
        if self.response.request_digest != request_digest(PARTICIPANT_POLICY_PATH, &self.request)? {
            return Err("participant policy original request mismatch".into());
        }
        verify_owner_signature_strict(
            PARTICIPANT_POLICY_PATH,
            &(&self.response.request_digest, &self.response.payload),
            world_public_key,
            &self.response.signature_hex,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_service::{sign_read_request, sign_service_response};
    use serde_json::json;
    fn original() -> ParticipantPolicyOriginalV1 {
        let world_key = hex::encode([41; 32]);
        let pin = sign_read_request("public", (), &world_key)
            .unwrap()
            .subject_public_key;
        let request = sign_read_request(
            PARTICIPANT_POLICY_PATH,
            ParticipantPolicyRequestV1 {
                contract_version: 1,
                expected_world: WorldIdentity {
                    world_id: "world".into(),
                    genesis_digest: "genesis".into(),
                },
                trust_config_ref: pin,
                observation_nonce: hex::encode([0xab; 32]),
            },
            &hex::encode([42; 32]),
        )
        .unwrap();
        let payload =
            json!({"policy":{"issuer_public_key":request.subject_public_key,"enabled":false}});
        let response = sign_service_response(
            PARTICIPANT_POLICY_PATH,
            request_digest(PARTICIPANT_POLICY_PATH, &request).unwrap(),
            payload,
            &world_key,
        )
        .unwrap();
        ParticipantPolicyOriginalV1 { request, response }
    }
    #[test]
    fn exact_original_binds_configured_world_pin_nonce_and_both_signatures() {
        let p = original();
        let r = &p.request.request;
        // A valid disabled-policy observation is evidence, never admission.
        p.verify_strict(&r.observation_nonce, &r.expected_world, &r.trust_config_ref)
            .unwrap();
        for mutation in 0..5 {
            let mut bad = p.clone();
            match mutation {
                0 => bad.request.request.observation_nonce = hex::encode([0xac; 32]),
                1 => bad.response.payload["policy"]["enabled"] = json!(true),
                2 => bad.request.signature_hex = "00".repeat(64),
                3 => bad.response.signature_hex = "00".repeat(64),
                _ => bad.response.payload["policy"]["issuer_public_key"] = json!("00".repeat(32)),
            }
            assert!(
                bad.verify_strict(&r.observation_nonce, &r.expected_world, &r.trust_config_ref)
                    .is_err()
            );
        }
        assert!(
            p.verify_strict(&r.observation_nonce, &r.expected_world, &"00".repeat(32))
                .is_err()
        );
        assert!(
            p.verify_strict(
                &r.observation_nonce.to_uppercase(),
                &r.expected_world,
                &r.trust_config_ref
            )
            .is_err()
        );
    }
}
