use super::*;
use crate::tests::agent_cognition_runtime_hardening::WORLD_ID;
use oasis7_proto::agency_control::{
    AGENCY_CONTROL_REQUEST_TYPE, AgencyControlOperation, AgencyControlRequest,
};
use oasis7_proto::player_auth::{sign_agency_control_auth_proof, sign_agent_chat_auth_proof};
use std::cell::Cell;

struct Registry {
    public: String,
    calls: Cell<usize>,
}
impl OwnerControlHostAuthority for Registry {
    fn validate_registered_owner(
        &self,
        player: &str,
        public: &str,
        agent: &str,
        world: &str,
        epoch: u64,
    ) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        if player != "player"
            || public != self.public
            || agent != AGENT_ID
            || world != WORLD_ID
            || epoch != 0
        {
            return Err("current registered key or binding rejected".into());
        }
        Ok(())
    }
}
fn keys() -> (String, String) {
    let private = [61u8; 32];
    let key = ed25519_dalek::SigningKey::from_bytes(&private);
    (
        hex::encode(key.verifying_key().to_bytes()),
        hex::encode(private),
    )
}
fn registry() -> Registry {
    Registry {
        public: keys().0,
        calls: Cell::new(0),
    }
}
fn request(command: AgencyControlOperation, nonce: u64) -> AgencyControlRequest {
    let (public, private) = keys();
    let mut request = AgencyControlRequest {
        frame_type: AGENCY_CONTROL_REQUEST_TYPE.into(),
        request_id: format!("signed-{nonce}"),
        player_id: "player".into(),
        public_key: public.clone(),
        agent_id: AGENT_ID.into(),
        world_id: WORLD_ID.into(),
        reorg_epoch: 0,
        command,
        auth: None,
    };
    request.auth = Some(
        sign_agency_control_auth_proof(
            &request.request_id,
            &request.player_id,
            &request.public_key,
            &request.agent_id,
            &request.world_id,
            request.reorg_epoch,
            &serde_json::to_value(&request.command).unwrap(),
            nonce,
            &public,
            &private,
        )
        .unwrap(),
    );
    request
}
fn install() -> AgencyControlOperation {
    let g = grant();
    AgencyControlOperation::InstallDelegation {
        grant_id: g.grant_id,
        object_id: g.object_id,
        action_kinds: g.action_kinds,
        period_id: g.period_id,
        valid_from_tick: g.valid_from_tick,
        valid_until_tick: g.valid_until_tick,
        limit_units: g.limit_units,
    }
}
#[test]
fn signed_delegation_rechecks_signature_and_current_registry_without_mutation() {
    let mut world = fixture(); // Historical None owner key remains governed by the actual host policy.
    assert!(
        world.state().starter_oc_claims[AGENT_ID]
            .public_key
            .is_none()
    );
    let signed = request(install(), 1);
    let before = serde_json::to_vec(&world).unwrap();
    let denied = Registry {
        public: "revoked-old-key".into(),
        calls: Cell::new(0),
    };
    assert!(
        world
            .apply_signed_delegation_control(&signed, &denied)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    let host = registry();
    let mut tampered = signed.clone();
    if let AgencyControlOperation::InstallDelegation { limit_units, .. } = &mut tampered.command {
        *limit_units += 1;
    }
    assert!(
        world
            .apply_signed_delegation_control(&tampered, &host)
            .is_err()
    );
    assert_eq!(
        host.calls.get(),
        0,
        "tampered wire data must fail before host policy"
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    world
        .apply_signed_delegation_control(&signed, &host)
        .unwrap();
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].grant,
        grant()
    );
}
#[test]
fn signed_owner_control_preserves_exact_retry_and_rejects_conflict_and_revoked_revision() {
    let mut world = fixture();
    let host = registry();
    world
        .apply_signed_delegation_control(&request(install(), 1), &host)
        .unwrap();
    let intent = accepted(&mut world);
    let command = AgencyControlOperation::OverridePendingIntent {
        control_id: "exact-control".into(),
        intent_id: intent.intent_id.clone(),
        request_digest: intent.request_digest.clone(),
        grant_id: "grant".into(),
        expected_grant_revision: 1,
    };
    let signed = request(command.clone(), 2);
    let disposition = world.apply_signed_owner_control(&signed, &host).unwrap();
    assert_eq!(
        world
            .signed_owner_control_replay(&signed, &host)
            .unwrap()
            .unwrap()
            .1,
        disposition
    );
    let before = serde_json::to_vec(&world).unwrap();
    let mut different = command.clone();
    if let AgencyControlOperation::OverridePendingIntent { request_digest, .. } = &mut different {
        *request_digest = "different-exact-intent".into();
    }
    assert!(
        world
            .apply_signed_owner_control(&request(different, 3), &host)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    world
        .apply_signed_delegation_control(
            &request(
                AgencyControlOperation::RevokeDelegation {
                    grant_id: "grant".into(),
                    expected_revision: 1,
                },
                4,
            ),
            &host,
        )
        .unwrap();
    assert_eq!(
        world
            .signed_owner_control_replay(&signed, &host)
            .unwrap()
            .unwrap()
            .1,
        disposition,
        "exact committed control retry remains original after revocation"
    );
    let mut stale = command;
    if let AgencyControlOperation::OverridePendingIntent { control_id, .. } = &mut stale {
        *control_id = "new-stale-control".into();
    }
    let before = serde_json::to_vec(&world).unwrap();
    assert!(
        world
            .apply_signed_owner_control(&request(stale, 5), &host)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
}
#[test]
fn signed_local_chat_keeps_none_key_policy_and_rejects_forged_payload() {
    let mut world = fixture();
    let host = registry();
    let (public, private) = keys();
    let mut chat = oasis7_proto::viewer::AgentChatRequest {
        agent_id: AGENT_ID.into(),
        message: "public signed local intent".into(),
        player_id: Some("player".into()),
        public_key: Some(public.clone()),
        auth: None,
        intent_tick: Some(world.state().time),
        intent_seq: Some(10),
        world_id: Some(WORLD_ID.into()),
        reorg_epoch: Some(0),
        authority_scope: Some("player_agent_chat".into()),
        replaces_intent_id: None,
        canonical_authority: None,
    };
    chat.auth = Some(sign_agent_chat_auth_proof(&chat, 10, &public, &private).unwrap());
    let before = serde_json::to_vec(&world).unwrap();
    let denied = Registry {
        public: "unregistered".into(),
        calls: Cell::new(0),
    };
    assert!(
        world
            .record_signed_local_agent_chat(&chat, &denied)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    let mut tampered = chat.clone();
    tampered.message.push('!');
    assert!(
        world
            .record_signed_local_agent_chat(&tampered, &host)
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    assert!(
        !world
            .record_signed_local_agent_chat(&chat, &host)
            .unwrap()
            .is_replay()
    );
    assert!(
        world
            .record_signed_local_agent_chat(&chat, &host)
            .unwrap()
            .is_replay()
    );
}
