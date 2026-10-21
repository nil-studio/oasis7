use super::*;
use crate::service_admission::{sign_read_request, SignedReadRequest};
use crate::service_admission::participant::*;
use oasis7_agent_api::game::*;
pub(super) fn fixture() -> World {
    let world=World::new_with_state(crate::WorldState::default());
    let snapshot=world.snapshot_with_chain_resource_context(crate::ChainResourceDerivationContext {
        world_id:"world",chain_id:"test-chain",genesis_ref:Some("genesis"),created_at_height:0,
        manifest_height:0,commit_block_hash:None,tick:0 }, "fixture","fixture");
    let mut world=World::from_snapshot(snapshot,world.journal().clone()).unwrap();
    world.bind_cognition_runtime("world","main",0,None,"pending",0).unwrap();
    world
}
pub(super) fn policy() -> ParticipantEntryPolicyV1 {
    ParticipantEntryPolicyV1 {api_rev:1,world_id:"world".into(),genesis_digest:"genesis".into(),identity_namespace:namespace(),
        issuer_public_key:sign_read_request("test",(),&hex::encode([71;32])).unwrap().subject_public_key,
        generation:1,max_grant_ticks:16,max_participants:2,registry_complete:true,enabled:true}
}
fn namespace()->GameIdentityNamespaceV1 {GameIdentityNamespaceV1 {service_id:"identity".into(),environment_id:"fixture".into(),instance_epoch:DecimalU64(1)}}
pub(super) fn request(account:&str,op:&str) -> (SignedReadRequest<GameParticipantRegisterV1>,SignedReadRequest<GameParticipantAdmissionV1>) {
    let private=hex::encode([72;32]);
    let public=sign_read_request("test",(),&private).unwrap().subject_public_key;
    let r=GameParticipantRegisterV1 {api_rev:1,client_operation_id:op.into(),identity_namespace:namespace(),account_id:account.into(),
        world_id:"world".into(),genesis_digest:"genesis".into(),owner_public_key:public,owner_key_version:DecimalU64(1)};
    let a=GameParticipantAdmissionV1 {api_rev:1,business_digest:business_digest(&r).unwrap(),identity_namespace:namespace(),account_id:account.into(),
        world_id:r.world_id.clone(),genesis_digest:r.genesis_digest.clone(),owner_public_key:r.owner_public_key.clone(),branch_id:"main".into(),
        reorg_epoch:DecimalU64(0),policy_generation:DecimalU64(1),valid_from_tick:DecimalU64(0),valid_until_tick:DecimalU64(16)};
    (sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN,r,&private).unwrap(),sign_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN,a,&hex::encode([71;32])).unwrap())
}
#[test]
fn default_closed_and_private_policy_requires_empty_known_complete_world() {
    let mut world=fixture(); let (r,a)=request("account","op");
    assert!(world.apply_participant_registration(&r,&a,"main",0).is_err());
    world.install_private_participant_fixture_policy(policy()).unwrap();
    let before=world.snapshot();
    assert!(world.install_private_participant_fixture_policy(policy()).is_err());
    assert_eq!(world.snapshot(),before);
    let mut legacy=fixture(); legacy.submit_action(crate::Action::RegisterAgent {agent_id:"legacy".into(),pos:oasis7_agent_api::world_action::GeoPos::new(0,0,0)});legacy.step().unwrap();
    assert!(legacy.install_private_participant_fixture_policy(policy()).is_err());
}
#[test]
fn participant_atomic_actual_agent_permanent_recovery_and_snapshot_replay() {
    let mut world=fixture();world.install_private_participant_fixture_policy(policy()).unwrap();
    let checkpoint=world.snapshot();let (r,a)=request("account","op");
    let receipt=world.apply_participant_registration(&r,&a,"main",0).unwrap();
    assert!(world.state().agents.contains_key(&receipt.agent_id));
    assert!(world.snapshot().pending_actions.is_empty());
    assert!(world.state().starter_oc_claims.is_empty());
    assert_eq!(crate::service_admission::agent_authority::owner_public_key(&world,&receipt.agent_id).unwrap(),r.subject_public_key);
    assert_eq!(world.current_agent_owner(&receipt.agent_id).unwrap().generation,1);
    assert_eq!(world.current_agent_identity(&receipt.agent_id).unwrap().generation,1);
    let registered_count=world.state().agents.len();
    assert_eq!(world.apply_participant_registration(&r,&a,"main",0).unwrap(),receipt);
    let (r2,a2)=request("account","different-op");
    assert_eq!(world.apply_participant_registration(&r2,&a2,"main",0).unwrap(),receipt);
    assert_eq!(world.state().agents.len(),registered_count);
    assert_eq!(world.capability_revocation_state().participants.operations.len(),2);
    let restored=World::from_snapshot(world.snapshot(),world.journal().clone()).unwrap();
    let replayed=World::from_snapshot(checkpoint,world.journal().clone()).unwrap();
    assert_eq!(restored.capability_revocation_state().participants,world.capability_revocation_state().participants);
    assert_eq!(replayed.capability_revocation_state().participants,world.capability_revocation_state().participants);
    assert_eq!(replayed.state(),world.state());
    assert_eq!(replayed.capability_authorization_root(),world.capability_authorization_root());
}
#[test]
fn participant_bad_proofs_scope_history_expiry_quota_and_owner_fail_closed() {
    let mut world=fixture();world.install_private_participant_fixture_policy(policy()).unwrap();
    let (r,a)=request("one","op");
    let before=world.snapshot();
    let mut forged=a.clone();forged.signature_hex="00".repeat(64);
    assert!(world.apply_participant_registration(&r,&forged,"main",0).is_err());
    for field in ["world","owner","branch","expired","window","namespace"] {
        let mut invalid=a.request.clone();
        match field {"world"=>invalid.world_id="foreign".into(),"owner"=>invalid.owner_public_key="00".repeat(32),
            "branch"=>invalid.branch_id="stale".into(),"expired"=>{invalid.valid_from_tick=DecimalU64(1);invalid.valid_until_tick=DecimalU64(2);},
            "window"=>invalid.valid_until_tick=DecimalU64(17),"namespace"=>invalid.identity_namespace.instance_epoch=DecimalU64(2),_=>unreachable!()}
        let invalid=sign_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN,invalid,&hex::encode([71;32])).unwrap();
        assert!(world.apply_participant_registration(&r,&invalid,"main",0).is_err(),"{field}");
        assert_eq!(world.snapshot(),before);
    }
    assert!(world.apply_participant_registration(&r,&a,"other",0).is_err());
    assert!(world.apply_participant_registration(&r,&a,"main",1).is_err());
    world.apply_participant_registration(&r,&a,"main",0).unwrap();
    let (r2,a2)=request("two","op");world.apply_participant_registration(&r2,&a2,"main",0).unwrap();
    let (r3,a3)=request("three","op");let before=world.snapshot();
    assert!(world.apply_participant_registration(&r3,&a3,"main",0).is_err());assert_eq!(world.snapshot(),before);
    // A valid foreign owner signature and fresh issuer proof cannot replace an existing owner.
    let mut changed=r.request.clone();let other=hex::encode([73;32]);changed.owner_public_key=sign_read_request("test",(),&other).unwrap().subject_public_key;
    let mut grant=a.request.clone();grant.owner_public_key=changed.owner_public_key.clone();grant.business_digest=business_digest(&changed).unwrap();
    let changed=sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN,changed,&other).unwrap();
    let grant=sign_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN,grant,&hex::encode([71;32])).unwrap();
    assert!(world.apply_participant_registration(&changed,&grant,"main",0).is_err());assert_eq!(world.snapshot(),before);
}

#[test]
fn participant_owner_conflicts_and_generation_revoke_current_access() {
    let mut world=fixture();world.install_private_participant_fixture_policy(policy()).unwrap();
    let (r,a)=request("account","op");let receipt=world.apply_participant_registration(&r,&a,"main",0).unwrap();
    let mut conflicting=world.clone();
    conflicting.state.starter_oc_claims.insert(receipt.agent_id.clone(),crate::gameplay_state::StarterOcClaimState {
        agent_id:receipt.agent_id.clone(),player_id:"foreign-owner".into(),public_key:Some(r.subject_public_key.clone()),..Default::default() });
    assert!(conflicting.current_agent_owner(&receipt.agent_id).is_none());
    assert!(crate::service_admission::agent_authority::owner_public_key(&conflicting,&receipt.agent_id).is_err());
    assert!(validate_admission(&conflicting,&r,&a,"main",0).is_err());
    // An economic claimer is never implicitly a participant's account owner.
    let mut conflicting=world.clone();
    let claim:crate::AgentClaimState=serde_json::from_value(serde_json::json!({
        "target_agent_id":receipt.agent_id,"claim_owner_id":"foreign-agent","claimed_at":0,
        "last_upkeep_epoch":0,"status":"active"})).unwrap();
    conflicting.state.agent_claims.insert(receipt.agent_id.clone(),claim);
    assert!(conflicting.current_agent_owner(&receipt.agent_id).is_none());
    world.install_capability_agent_identity(&receipt.agent_id,&receipt.owner_binding,2).unwrap();
    assert!(world.current_agent_owner(&receipt.agent_id).is_none());
    assert!(validate_admission(&world,&r,&a,"main",0).is_err());
}

#[test]
fn participant_two_concurrent_operations_share_one_canonical_binding() {
    let mut world=fixture();world.install_private_participant_fixture_policy(policy()).unwrap();
    let first=request("same-account","op-a");let second=request("same-account","op-b");
    let world=std::sync::Arc::new(std::sync::Mutex::new(world));
    let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers=[first,second].map(|(r,a)| {
        let world=world.clone();let barrier=barrier.clone();
        std::thread::spawn(move || {barrier.wait();world.lock().unwrap().apply_participant_registration(&r,&a,"main",0).unwrap()})
    });
    let [one,two]=workers.map(|w|w.join().unwrap());
    assert_eq!(one,two);
    let world=world.lock().unwrap();assert_eq!(world.state().agents.len(),1);
    assert_eq!(world.capability_revocation_state().participants.operations.len(),2);
}
#[test]
fn participant_registration_publication_failure_leaves_no_agent_identity_or_receipt() {
    let mut world=fixture();world.install_private_participant_fixture_policy(policy()).unwrap();
    let (r,a)=request("account","op");let before=world.snapshot();let journal=world.journal().clone();
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(world.apply_participant_registration(&r,&a,"main",0).is_err());
    assert_eq!(world.snapshot(),before);assert_eq!(world.journal(),&journal);
    assert!(world.state().agents.is_empty());assert!(world.capability_revocation_state().participants.bindings.is_empty());
}
