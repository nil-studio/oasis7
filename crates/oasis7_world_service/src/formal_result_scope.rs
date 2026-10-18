//! Bootstrap receipts belong to their authenticated original sender. Agent
//! ownership is established by a later action, not by first registration.
use super::*;

pub(super) fn for_request(
    world: &World,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<String, String> {
    if let WorldServicePayloadV1::GameplayJson(bytes) = &request.signed_payload
        && let Ok(action) =
            serde_json::from_slice::<oasis7_proto::viewer::GameplayActionRequest>(bytes)
        && action.action_id == "claim_first_agent"
        && action.target_agent_id == "starter-agent-0"
    {
        let derived = correlation::derive_correlation(
            request.correlation.key.world.clone(),
            &request.signed_payload,
        )?;
        if derived != request.correlation {
            return Err("bootstrap result differs from its authenticated original request".into());
        }
        // Public-scope result proofs still require the read signer to equal
        // the committed original subject, both at service and verifier.
        return Ok("public".into());
    }
    Ok(format!(
        "agent:{}",
        outcome_agent_id(world, &request.signed_payload)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn request(action_id: &str) -> SubmitIntentRequest<WorldServicePayloadV1> {
        let key = SigningKey::from_bytes(&[11; 32]);
        let public = hex::encode(key.verifying_key().to_bytes());
        let mut action = oasis7_proto::viewer::GameplayActionRequest {
            action_id: action_id.into(),
            target_agent_id: "starter-agent-0".into(),
            actor_agent_id: None,
            player_id: "owner".into(),
            public_key: Some(public.clone()),
            auth: None,
        };
        action.auth = Some(
            oasis7_proto::player_auth::sign_gameplay_action_auth_proof(
                &action,
                1,
                &public,
                &hex::encode(key.to_bytes()),
            )
            .unwrap(),
        );
        let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&action).unwrap());
        let world = WorldIdentity {
            world_id: "bootstrap".into(),
            genesis_digest: "11".repeat(32),
        };
        SubmitIntentRequest {
            contract_version: 1,
            correlation: correlation::derive_correlation(world, &payload).unwrap(),
            signed_payload: payload,
            deadline_unix_ms: None,
        }
    }

    #[test]
    fn formal_first_agent_receipt_is_readable_only_by_its_original_sender_without_owner_grant() {
        let request = request("claim_first_agent");
        let world = oasis7_runtime::World::new();
        let result = CanonicalIntentResultV1 {
            request: request.clone(),
            action_id: 1,
            committed_height: 1,
            receipt: serde_json::json!({"agent_registered": true}),
            rejected: None,
        };
        let leaf = outcome_leaf(&world, &result, 2).unwrap();
        assert_eq!(leaf.visibility_scope, "public");
        let sender = &request.correlation.key.verified_subject;
        authorize_world_result_reader(&world, &leaf, "public", sender).unwrap();
        let other = hex::encode(SigningKey::from_bytes(&[12; 32]).verifying_key().to_bytes());
        assert!(authorize_world_result_reader(&world, &leaf, "public", &other).is_err());
        let mut tampered = request;
        tampered.correlation.key.verified_subject = other;
        assert!(for_request(&world, &tampered).is_err());
    }

    #[test]
    fn formal_ordinary_gameplay_results_keep_current_agent_authorization() {
        let request = request("claim_starter_oc");
        assert_eq!(
            for_request(&oasis7_runtime::World::new(), &request).unwrap(),
            format!("agent:{}", "starter-agent-0")
        );
    }
}
