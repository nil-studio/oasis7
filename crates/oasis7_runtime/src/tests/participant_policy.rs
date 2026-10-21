use super::super::*;
use super::capability_grant_v2 as fixture;
use crate::service_admission::{participant::*,participant_policy::*,sign_read_request};
use oasis7_agent_api::game::*;
use std::collections::BTreeMap;
fn world()->World {
    let mut world=fixture::fixture_world();
    let private=hex::encode([72;32]);
    let public=sign_read_request("fixture",(),&private).unwrap().subject_public_key;
    world.submit_action(Action::ClaimStarterOc {agent_id:fixture::SUBJECT_ID.into(),player_id:"owner-7".into(),public_key:Some(public)});
    world.step().unwrap();
    let snapshot=world.snapshot_with_chain_resource_context(ChainResourceDerivationContext {
        world_id:fixture::WORLD_ID,chain_id:"fixture",genesis_ref:Some("genesis"),created_at_height:0,manifest_height:0,commit_block_hash:None,tick:world.state().time },"fixture","fixture");
    let mut world=World::from_snapshot(snapshot,world.journal().clone()).unwrap();
    world.bind_cognition_runtime(fixture::WORLD_ID,fixture::BRANCH_ID,4,Some(format!("blake3:{}","ab".repeat(32))),"verified",0).unwrap();
    world
}
fn policy()->ParticipantEntryPolicyV1 {ParticipantEntryPolicyV1 {api_rev:1,world_id:fixture::WORLD_ID.into(),genesis_digest:"genesis".into(),
    identity_namespace:GameIdentityNamespaceV1 {service_id:"identity".into(),environment_id:"private".into(),instance_epoch:DecimalU64(1)},
    issuer_public_key:sign_read_request("fixture",(),&hex::encode([71;32])).unwrap().subject_public_key,
    generation:1,max_grant_ticks:16,max_participants:10,registry_complete:true,enabled:true}}
fn proof(world:&mut World,policy:&ParticipantEntryPolicyV1,migrate:bool)->ParticipantPolicyProofV1 {
    let mut entries=Vec::new();
    if migrate {
        let private=hex::encode([72;32]);let public=sign_read_request("fixture",(),&private).unwrap().subject_public_key;
        let entry=ParticipantMigrationBindingV1 {registration:GameParticipantRegisterV1 {api_rev:1,client_operation_id:"original-import".into(),identity_namespace:policy.identity_namespace.clone(),
            account_id:"legacy-account".into(),world_id:policy.world_id.clone(),genesis_digest:policy.genesis_digest.clone(),owner_public_key:public,owner_key_version:DecimalU64(1)},
            agent_id:fixture::SUBJECT_ID.into(),owner_binding:"owner-7".into(),owner_generation:1};
        entries.push(sign_read_request(PARTICIPANT_MIGRATION_OWNER_DOMAIN,entry,&private).unwrap());
    }
    let manifest=ParticipantMigrationManifestV1 {api_rev:1,policy_digest:policy_digest(policy).unwrap(),world_id:policy.world_id.clone(),genesis_digest:policy.genesis_digest.clone(),
        branch_id:fixture::BRANCH_ID.into(),reorg_epoch:0,identity_namespace:policy.identity_namespace.clone(),source_snapshot_epoch:1,source_watermark:"db-seq-10".into(),
        source_snapshot_digest:format!("blake3:{}","a".repeat(64)),complete:true,previous_registry_digest:world.participant_registry_digest().unwrap(),
        legacy_owner_state_digest:world.participant_legacy_owner_state_digest().unwrap(),entries};
    let certificate=world.capability_revocation_state().authority_finality_proofs.get(fixture::ISSUER_ID).unwrap().certificate.clone();
    let mut proof=ParticipantPolicyProofV1 {manifest:sign_read_request(PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,manifest,&hex::encode([71;32])).unwrap(),certificate,signatures:BTreeMap::new()};
    authorize(world,policy,&mut proof);proof
}
fn resign(policy:&ParticipantEntryPolicyV1,proof:&mut ParticipantPolicyProofV1) {
    let domain=if policy.generation==1 {PARTICIPANT_POLICY_INSTALL_DOMAIN}else{PARTICIPANT_POLICY_UPDATE_DOMAIN};
    proof.manifest=sign_read_request(PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,proof.manifest.request.clone(),&hex::encode([71;32])).unwrap();
    let payload=policy_signing_payload(policy,proof);
    proof.signatures.clear();
    for (node,key) in [(fixture::ISSUER_ID,fixture::capability_issuer_signing_key()),(fixture::FINALITY_SIGNER_2,fixture::capability_finality_signing_key_2())] {
        let signed=sign_read_request(domain,payload.clone(),&hex::encode(key.to_bytes())).unwrap();proof.signatures.insert(node.into(),signed.signature_hex);
    }
}
#[test]
fn participant_governed_migration_preserves_existing_agent_assets_and_replays() {
    let mut world=world();let before=world.state().clone();let policy=policy();let proof=proof(&mut world,&policy,true);let checkpoint=world.snapshot();
    world.install_participant_policy_with_finality_proof(policy.clone(),proof.clone()).unwrap();
    let local=crate::world::DevLocalParticipantAuthority::new(oasis7_proto::storage_profile::StorageProfile::DevLocal,
        &world.release_security_policy().clone(),sign_read_request("pin",(),&hex::encode([77;32])).unwrap().subject_public_key).unwrap();
    assert!(world.clone().with_devlocal_participant_authority(local).is_err());

    assert_eq!(world.state(),&before);
    let imported=world.capability_revocation_state().participants.bindings.values().next().unwrap();
    assert_eq!(imported.agent_id,fixture::SUBJECT_ID);assert!(imported.imported);
    assert_eq!(world.current_agent_owner(fixture::SUBJECT_ID).unwrap().player_id,"owner-7");
    let restored=World::from_snapshot(world.snapshot(),world.journal().clone()).unwrap();
    let replayed=World::from_snapshot(checkpoint,world.journal().clone()).unwrap();
    assert_eq!(restored.capability_revocation_state().participants,world.capability_revocation_state().participants);
    assert_eq!(replayed.capability_revocation_state().participants,world.capability_revocation_state().participants);
    let mut closed=policy;closed.generation=2;closed.enabled=false;
    let update=proof_for_update(&mut world,&closed);
    world.install_participant_policy_with_finality_proof(closed,update).unwrap();
    assert_eq!(world.capability_revocation_state().participants.bindings.len(),1);
    assert!(!world.capability_revocation_state().participants.policy.as_ref().unwrap().enabled);
    World::from_snapshot(world.snapshot(),world.journal().clone()).unwrap();
}
fn proof_for_update(world:&mut World,p:&ParticipantEntryPolicyV1)->ParticipantPolicyProofV1 {proof(world,p,false)}
#[test]
fn participant_governed_empty_attestation_admits_existing_world_and_bad_evidence_is_atomic() {
    let mut world=world();let policy=policy();let valid=proof(&mut world,&policy,false);let before=world.snapshot();
    for kind in ["quorum","issuer","incomplete","watermark","stale","duplicate"] {
        let mut invalid=valid.clone();
        match kind {
            "quorum"=>{invalid.signatures.pop_first();},
            "issuer"=>invalid.manifest.signature_hex="00".repeat(64),
            "incomplete"=>{invalid.manifest.request.complete=false;resign(&policy,&mut invalid);},
            "watermark"=>{invalid.manifest.request.source_watermark.clear();resign(&policy,&mut invalid);},
            "stale"=>{invalid.manifest.request.legacy_owner_state_digest="stale".into();resign(&policy,&mut invalid);},
            "duplicate"=>{invalid.manifest.request.entries.clear();invalid.manifest.request.source_snapshot_epoch=0;resign(&policy,&mut invalid);},
            _=>unreachable!() }
        assert!(world.install_participant_policy_with_finality_proof(policy.clone(),invalid).is_err(),"{kind}");
        assert_eq!(world.snapshot(),before);
    }
    world.install_participant_policy_with_finality_proof(policy,valid).unwrap();
    assert_eq!(world.state().agents.len(),1);assert!(world.capability_revocation_state().participants.bindings.is_empty());
}

fn authorize(world:&mut World,policy:&ParticipantEntryPolicyV1,proof:&mut ParticipantPolicyProofV1) {
    use ed25519_dalek::Signer;
    // Sign issuer snapshot first; its exact bytes are approved by governance.
    proof.manifest=sign_read_request(PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,proof.manifest.request.clone(),&hex::encode([71;32])).unwrap();
    let proposal=world.propose_manifest_update(Manifest {version:world.manifest().version+1,content:serde_json::json!({
        "participant_policy_authorization":policy_proposal_commitment(policy,&proof.manifest).unwrap()})},"tester").unwrap();
    world.shadow_proposal(proposal).unwrap();world.approve_proposal(proposal,"tester",ProposalDecision::Approve).unwrap();
    let hash=match &world.proposals().get(&proposal).unwrap().status {ProposalStatus::Approved {manifest_hash,..}=>manifest_hash.clone(),_=>unreachable!()};
    let snapshot=world.governance_finality_epoch_snapshots().get(&4).unwrap();
    let height=world.journal().events.len() as u64+1;
    let mut certificate=GovernanceFinalityCertificate {proposal_id:proposal,manifest_hash:hash,consensus_height:height,epoch_id:4,
        validator_set_hash:snapshot.validator_set_hash.clone(),stake_root:snapshot.stake_root.clone(),threshold_bps:snapshot.threshold_bps,
        min_unique_signers:snapshot.effective_min_unique_signers(),threshold:snapshot.effective_min_unique_signers(),signatures:BTreeMap::new()};
    for (node,key) in [(fixture::ISSUER_ID,fixture::capability_issuer_signing_key()),(fixture::FINALITY_SIGNER_2,fixture::capability_finality_signing_key_2())] {
        let bytes=GovernanceFinalityCertificate::signing_payload_v1(proposal,&certificate.manifest_hash,height,4,&certificate.validator_set_hash,&certificate.stake_root,
            certificate.threshold_bps,certificate.min_unique_signers,node);
        certificate.signatures.insert(node.into(),format!("{}{}",GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,hex::encode(key.sign(&bytes).to_bytes())));
    }
    proof.certificate=certificate;resign(policy,proof);
}

#[test]
fn participant_governed_authorized_bad_migration_still_checks_live_uniqueness_and_owner() {
    for kind in ["duplicate-account","duplicate-agent","wrong-owner","wrong-generation","unknown-agent","stale-branch","stale-registry"] {
        let mut world=world();let policy=policy();let mut proof=proof(&mut world,&policy,true);
        match kind {
            "duplicate-account"=>{let duplicate=proof.manifest.request.entries[0].clone();proof.manifest.request.entries.push(duplicate);},
            "duplicate-agent"=>{let mut duplicate=proof.manifest.request.entries[0].request.clone();duplicate.registration.account_id="second-account".into();
                duplicate.registration.client_operation_id="second-operation".into();proof.manifest.request.entries.push(sign_read_request(PARTICIPANT_MIGRATION_OWNER_DOMAIN,duplicate,&hex::encode([72;32])).unwrap());},
            "wrong-owner"=>{let entry=&mut proof.manifest.request.entries[0];let mut body=entry.request.clone();body.owner_binding="other-owner".into();*entry=sign_read_request(PARTICIPANT_MIGRATION_OWNER_DOMAIN,body,&hex::encode([72;32])).unwrap();},
            "wrong-generation"=>{let entry=&mut proof.manifest.request.entries[0];let mut body=entry.request.clone();body.owner_generation=2;*entry=sign_read_request(PARTICIPANT_MIGRATION_OWNER_DOMAIN,body,&hex::encode([72;32])).unwrap();},
            "unknown-agent"=>{let entry=&mut proof.manifest.request.entries[0];let mut body=entry.request.clone();body.agent_id="unknown".into();*entry=sign_read_request(PARTICIPANT_MIGRATION_OWNER_DOMAIN,body,&hex::encode([72;32])).unwrap();},
            "stale-branch"=>proof.manifest.request.branch_id="stale".into(),
            "stale-registry"=>proof.manifest.request.previous_registry_digest="stale".into(),
            _=>unreachable!()
        }
        authorize(&mut world,&policy,&mut proof);
        let before=world.snapshot();let journal=world.journal().clone();
        assert!(world.install_participant_policy_with_finality_proof(policy,proof).is_err(),"{kind}");
        assert_eq!(world.snapshot(),before);assert_eq!(world.journal(),&journal);
    }
}

#[test]
fn participant_policy_recomputes_approved_proposal_manifest_hash() {
    let mut world=world();let policy=policy();let proof=proof(&mut world,&policy,false);
    let mut snapshot=world.snapshot();
    snapshot.proposals.get_mut(&proof.certificate.proposal_id).unwrap().manifest.version+=1;
    let mut corrupt=World::from_snapshot(snapshot,world.journal().clone()).unwrap();
    let before=corrupt.snapshot();
    let error=corrupt.install_participant_policy_with_finality_proof(policy,proof).unwrap_err();
    assert!(format!("{error:?}").contains("approved proposal manifest hash mismatch"));
    assert_eq!(corrupt.snapshot(),before);
}

#[test]
fn participant_instance_epoch_is_freshness_fence_not_new_account_qualification() {
    let mut world=world();let initial=policy();let proof=proof(&mut world,&initial,false);
    world.install_participant_policy_with_finality_proof(initial.clone(),proof).unwrap();
    let key=hex::encode([72;32]);let public=sign_read_request("fixture",(),&key).unwrap().subject_public_key;
    let original=GameParticipantRegisterV1 {api_rev:1,client_operation_id:"original".into(),identity_namespace:initial.identity_namespace.clone(),account_id:"new-account".into(),
        world_id:initial.world_id.clone(),genesis_digest:initial.genesis_digest.clone(),owner_public_key:public,owner_key_version:DecimalU64(1)};
    let owner=sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN,original.clone(),&key).unwrap();
    let mut pending=original.clone();pending.account_id="pending-account".into();pending.client_operation_id="pending-original".into();
    let pending_digest=business_digest(&pending).unwrap();
    let pending_owner=sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN,pending.clone(),&key).unwrap();
    let grant=|policy:&ParticipantEntryPolicyV1,request:&GameParticipantRegisterV1,tick|sign_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN,GameParticipantAdmissionV1 {
        api_rev:1,business_digest:business_digest(request).unwrap(),identity_namespace:policy.identity_namespace.clone(),account_id:request.account_id.clone(),
        world_id:request.world_id.clone(),genesis_digest:request.genesis_digest.clone(),owner_public_key:request.owner_public_key.clone(),branch_id:fixture::BRANCH_ID.into(),reorg_epoch:DecimalU64(0),
        policy_generation:DecimalU64(policy.generation),valid_from_tick:DecimalU64(tick),valid_until_tick:DecimalU64(tick+16)},&hex::encode([71;32])).unwrap();
    let old_grant=grant(&initial,&original,world.state().time);
    let receipt=world.apply_participant_registration(&owner,&old_grant,fixture::BRANCH_ID,0).unwrap();let count=world.state().agents.len();
    let mut next=initial;next.generation=2;next.identity_namespace.instance_epoch=DecimalU64(2);
    let proof=proof_for_update(&mut world,&next);world.install_participant_policy_with_finality_proof(next.clone(),proof).unwrap();
    assert!(world.apply_participant_registration(&owner,&old_grant,fixture::BRANCH_ID,0).is_err());
    let fresh=grant(&next,&original,world.state().time);
    assert_eq!(world.apply_participant_registration(&owner,&fresh,fixture::BRANCH_ID,0).unwrap(),receipt);
    let mut current=original.clone();current.identity_namespace=next.identity_namespace.clone();
    assert_eq!(account_key(&current).unwrap(),account_key(&original).unwrap());assert_eq!(operation_key(&current).unwrap(),operation_key(&original).unwrap());
    let changed_owner=sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN,current.clone(),&key).unwrap();
    let changed_grant=grant(&next,&current,world.state().time);
    assert!(world.apply_participant_registration(&changed_owner,&changed_grant,fixture::BRANCH_ID,0).unwrap_err().contains("digest conflict"));
    current.client_operation_id="new-operation".into();let fresh=grant(&next,&current,world.state().time);let owner=sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN,current,&key).unwrap();
    assert_eq!(world.apply_participant_registration(&owner,&fresh,fixture::BRANCH_ID,0).unwrap(),receipt);
    assert_eq!(world.state().agents.len(),count);assert_eq!(world.current_agent_owner(&receipt.agent_id).unwrap().public_key,Some(receipt.owner_public_key));
    let pending_grant=grant(&next,&pending,world.state().time);
    let pending_receipt=world.apply_participant_registration(&pending_owner,&pending_grant,fixture::BRANCH_ID,0).unwrap();
    assert_eq!(pending_receipt.business_digest,pending_digest);assert_eq!(pending_owner.request,pending);
    assert_eq!(world.state().agents.len(),count+1);
    let mut future=pending;future.client_operation_id="future-epoch".into();future.identity_namespace.instance_epoch=DecimalU64(3);
    let future_grant=grant(&next,&future,world.state().time);let future_owner=sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN,future,&key).unwrap();
    assert!(world.apply_participant_registration(&future_owner,&future_grant,fixture::BRANCH_ID,0).is_err());
    World::from_snapshot(world.snapshot(),world.journal().clone()).unwrap();
}
