use super::*;
use crate::world_service::{request_digest, sign_service_response, signing_bytes};
use serde_json::json;
fn request() -> ParticipantOwnerViewRequestV1 {
    let ns = json!({"service_id":"identity","environment_id":"test","instance_epoch":"1"});
    serde_json::from_value(json!({
        "owner":{"request":{"api_rev":1,"client_operation_id":"register","identity_namespace":ns,"account_id":"account","world_id":"world","genesis_digest":"genesis","owner_public_key":"ab".repeat(32),"owner_key_version":"1"},"subject_public_key":"ab".repeat(32),"signature_hex":"00".repeat(64)},
        "admission":{"request":{"api_rev":1,"business_digest":"digest","identity_namespace":ns,"account_id":"account","world_id":"world","genesis_digest":"genesis","owner_public_key":"ab".repeat(32),"branch_id":"main","reorg_epoch":"0","policy_generation":"1","valid_from_tick":"0","valid_until_tick":"100"},"subject_public_key":"cd".repeat(32),"signature_hex":"00".repeat(64)},
        "read":{"request":{"contract_version":1,"observation_nonce":"ef".repeat(32),"world":{"world_id":"world","genesis_digest":"genesis"},"scope_id":"agent:agent","min_commit":null,"fixed_commit":null,"deadline_unix_ms":null},"subject_public_key":"ab".repeat(32),"signature_hex":"00".repeat(64)}
    })).unwrap()
}
#[derive(Serialize)]
struct Legacy<'a> {
    owner: &'a SignedReadRequest<GameParticipantRegisterV1>,
    admission: &'a SignedReadRequest<GameParticipantAdmissionV1>,
    read: &'a SignedReadRequest<ReadWorldViewRequest>,
}
#[test]
fn absent_profile_preserves_legacy_json_cbor_digest_and_signature() {
    let req = request();
    let old = Legacy {
        owner: &req.owner,
        admission: &req.admission,
        read: &req.read,
    };
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&req).unwrap()
    );
    assert_eq!(
        signing_bytes(PARTICIPANT_OWNER_VIEW_PATH, &old).unwrap(),
        signing_bytes(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap()
    );
    let old_digest = request_digest(PARTICIPANT_OWNER_VIEW_PATH, &old).unwrap();
    assert_eq!(
        old_digest,
        request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap()
    );
    let old_signed = sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        old_digest,
        json!({"legacy":true}),
        &"09".repeat(32),
    )
    .unwrap();
    let new_signed = sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap(),
        json!({"legacy":true}),
        &"09".repeat(32),
    )
    .unwrap();
    assert_eq!(old_signed, new_signed);
}
#[test]
fn explicit_profile_is_bound_and_unknown_profile_rejected() {
    let mut req = request();
    let old = request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap();
    req.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerControlV1);
    assert_ne!(
        old,
        request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap()
    );
    let mut value = serde_json::to_value(req).unwrap();
    value["response_profile"] = json!("unknown");
    assert!(serde_json::from_value::<ParticipantOwnerViewRequestV1>(value).is_err());
}

#[test]
fn starter_owner_observation_rejects_cross_owner_claim_and_first_production() {
    use oasis7_agent_api::world_action::*;
    let owner: oasis7_proto::viewer::CanonicalAgentOwnerFenceV1 = serde_json::from_value(json!({
        "agent_id":"agent", "player_id":"owner", "public_key":"ab".repeat(32),
        "world_id":"world", "reorg_epoch":0, "current_intent_id":null,
        "canonical_authority":{"branch_id":"main","agent_identity_generation":2}
    }))
    .unwrap();
    let claim: OrdinaryStarterClaimV1 = serde_json::from_value(json!({
        "world_binding":"bound-world", "owner_binding":"owner", "owner_generation":1,
        "agent_id":"agent", "offer_id":"offer", "offer_revision":1, "offer_digest":"digest",
        "slot_id":"slot", "site_id":"site", "factory_id":"factory", "location_id":"location",
        "profile_id":"profile", "profile_revision":1, "recipe_id":"recipe", "materials":{}, "electricity":26,
        "receipt":{"receipt_key":"receipt", "request_id":"request", "actor_id":"agent", "committed_at":10,
            "command":{"kind":"Claim","data":{"offer_id":"offer","offer_revision":1,"slot_id":"slot","offer_digest":"digest","owner_binding":"owner","owner_generation":1}}},
        "first_production":{"owner_binding":"owner","agent_id":"agent","site_id":"site","factory_id":"factory", "profile_id":"profile","profile_revision":1,"recipe_id":"recipe","job_id":1,"settled_at":20,
            "output_ledger": MaterialLedgerId::site("site"),"accepted_batches":1,"consume":[],"power_required":8,"produce":[],"byproducts":[]}
    })).unwrap();
    let status = OrdinaryStarterStatusV1 {
        world_binding: Some("bound-world".into()),
        owner_binding: Some("owner".into()),
        owner_generation: Some(2),
        claim: Some(claim),
        ..Default::default()
    };
    assert!(
        validate_starter_owner_observation(&status, &owner).is_ok(),
        "historical claim generation remains original"
    );
    for field in 0..7 {
        let mut invalid = status.clone();
        match field {
            0 => invalid.owner_binding = Some("other".into()),
            1 => invalid.owner_generation = Some(3),
            2 => invalid.world_binding = Some("other-world".into()),
            3 => invalid.claim.as_mut().unwrap().agent_id = "other-agent".into(),
            4 => invalid.claim.as_mut().unwrap().owner_binding = "other-owner".into(),
            5 => {
                invalid
                    .claim
                    .as_mut()
                    .unwrap()
                    .first_production
                    .as_mut()
                    .unwrap()
                    .factory_id = "other-factory".into()
            }
            _ => {
                invalid
                    .claim
                    .as_mut()
                    .unwrap()
                    .first_production
                    .as_mut()
                    .unwrap()
                    .owner_binding = "other-owner".into()
            }
        }
        assert!(
            validate_starter_owner_observation(&invalid, &owner).is_err(),
            "field {field}"
        );
    }
}
