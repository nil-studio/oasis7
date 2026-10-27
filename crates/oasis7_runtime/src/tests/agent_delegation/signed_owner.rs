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

#[test]
fn signed_local_chat_preserves_registered_ordinary_agent_without_starter_claim() {
    let mut world = fixture();
    let mut state = world.state().clone();
    state.starter_oc_claims.remove(AGENT_ID);
    world = World::new_with_state(state);
    let (public, private) = keys();
    let mut chat = oasis7_proto::viewer::AgentChatRequest {
        agent_id: AGENT_ID.into(),
        message: "registered ordinary agent".into(),
        player_id: Some("player".into()),
        public_key: Some(public.clone()),
        auth: None,
        intent_tick: Some(world.state().time),
        intent_seq: Some(12),
        world_id: Some(WORLD_ID.into()),
        reorg_epoch: Some(0),
        authority_scope: Some("player_agent_chat".into()),
        replaces_intent_id: None,
        canonical_authority: None,
    };
    chat.auth = Some(sign_agent_chat_auth_proof(&chat, 12, &public, &private).unwrap());
    assert!(
        !world
            .record_signed_local_agent_chat(&chat, &registry())
            .unwrap()
            .is_replay()
    );
}

#[test]
fn signed_replaced_recipe_grant_survives_restore_and_only_debits_replacement_once() {
    let mut world = recipe_fixture();
    // Publish the fixture resource changes before exercising snapshot recovery.
    world.step().unwrap();
    let host = registry();
    let install_recipe = |id: &str| AgencyControlOperation::InstallDelegation {
        grant_id: id.into(),
        object_id: "factory".into(),
        action_kinds: vec!["ScheduleRecipe".into()],
        period_id: "period".into(),
        valid_from_tick: 0,
        valid_until_tick: 100,
        limit_units: 12,
    };
    world
        .apply_signed_delegation_control(&request(install_recipe("grant-a"), 1), &host)
        .unwrap();
    world
        .apply_signed_delegation_control(
            &request(
                AgencyControlOperation::RevokeDelegation {
                    grant_id: "grant-a".into(),
                    expected_revision: 1,
                },
                2,
            ),
            &host,
        )
        .unwrap();
    world
        .apply_signed_delegation_control(&request(install_recipe("grant-b"), 3), &host)
        .unwrap();
    // Restore in the same world identity used by the signed owner and cognition
    // request fixtures, rather than snapshot()'s generic fallback world ID.
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: WORLD_ID,
            chain_id: "runtime-chain",
            genesis_ref: None,
            created_at_height: world.journal().len() as u64,
            manifest_height: world.journal().len() as u64,
            commit_block_hash: None,
            tick: world.state().time,
        },
        world.current_manifest_hash().unwrap(),
        world.current_manifest_hash().unwrap(),
    );
    world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let quote = world
        .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 1)
        .unwrap();
    assert_eq!(quote.authorization.grant.grant_id, "grant-b");
    assert_eq!(quote.authorization.cost_units, 8);
    assert!(quote.rejection.is_none());
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    let prepared = prepare(
        &mut world,
        "replacement-production",
        quote.action,
        8,
        Default::default(),
    );
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    let receipts = world.agent_causal_receipts(AGENT_ID).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].disposition, "applied");
    assert_eq!(
        receipts[0].authorization.as_ref().unwrap().grant.grant_id,
        "grant-b"
    );
    let balances = world.state().material_ledgers.clone();
    let electricity = world.state().agents[AGENT_ID].state.resources.clone();
    world = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(world.agent_causal_receipts(AGENT_ID).unwrap(), receipts);
    assert_eq!(world.state().material_ledgers, balances);
    assert_eq!(world.state().agents[AGENT_ID].state.resources, electricity);
    let rows = world.agent_delegation_authorizations(AGENT_ID).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].grant.grant_id, "grant-a");
    assert!(rows[0].grant.revoked);
    assert_eq!(rows[0].grant.revision, 2);
    assert_eq!(rows[0].spent_units, 0);
    assert_eq!(rows[1].grant.grant_id, "grant-b");
    assert!(!rows[1].grant.revoked);
    assert_eq!(rows[1].spent_units, 8);
    // A started recipe is not a completed product. Advance and check real output.
    for _ in 0..12 {
        world.step().unwrap();
    }
    let output = &world.state().factories["factory"].output_ledger;
    assert_eq!(
        world.state().material_ledgers[output].get("iron_ingot"),
        Some(&3)
    );
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[1].spent_units,
        8
    );
}

#[test]
fn signed_delegation_typed_outcome_and_readonly_replay_share_identity() {
    let mut world = fixture();
    let host = registry();
    let signed = request(install(), 1);
    let before = serde_json::to_vec(&world).unwrap();
    assert_eq!(
        world
            .signed_delegation_control_replay(&signed, &host)
            .unwrap(),
        DelegationReplayMatch::NoMatch
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
    let applied = world
        .apply_signed_delegation_control(&signed, &host)
        .unwrap();
    assert_eq!(applied.operation, DelegationControlOperation::Install);
    assert_eq!(applied.disposition, DelegationControlDisposition::Applied);
    assert_eq!(applied.authorization.grant, grant());
    let after = serde_json::to_vec(&world).unwrap();
    assert_eq!(
        world
            .signed_delegation_control_replay(&signed, &host)
            .unwrap(),
        DelegationReplayMatch::ExactReplay(Box::new(applied.authorization.clone()))
    );
    let replay = world
        .apply_signed_delegation_control(&signed, &host)
        .unwrap();
    assert_eq!(
        replay.disposition,
        DelegationControlDisposition::ExactReplay
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), after);
}

#[test]
fn delegation_readonly_replay_reauthenticates_and_preserves_action_order_and_source() {
    let mut world = fixture();
    let host = registry();
    let mut command = install();
    if let AgencyControlOperation::InstallDelegation { action_kinds, .. } = &mut command {
        *action_kinds = vec!["MoveAgent".into(), "TransferMaterial".into()];
    }
    let signed = request(command.clone(), 1);
    world
        .apply_signed_delegation_control(&signed, &host)
        .unwrap();
    let baseline = serde_json::to_vec(&world).unwrap();
    let calls = host.calls.get();
    assert!(matches!(
        world
            .signed_delegation_control_replay(&signed, &host)
            .unwrap(),
        DelegationReplayMatch::ExactReplay(_)
    ));
    assert_eq!(host.calls.get(), calls + 1);
    for field in [
        "request", "player", "key", "agent", "world", "epoch", "command",
    ] {
        let mut tampered = signed.clone();
        match field {
            "request" => tampered.request_id.push('x'),
            "player" => tampered.player_id.push('x'),
            "key" => tampered.public_key = "00".repeat(32),
            "agent" => tampered.agent_id.push('x'),
            "world" => tampered.world_id.push('x'),
            "epoch" => tampered.reorg_epoch += 1,
            "command" => {
                if let AgencyControlOperation::InstallDelegation { limit_units, .. } =
                    &mut tampered.command
                {
                    *limit_units += 1;
                }
            }
            _ => unreachable!(),
        }
        let calls = host.calls.get();
        assert_eq!(
            world
                .signed_delegation_control_replay(&tampered, &host)
                .unwrap_err()
                .stage,
            DelegationControlStage::Authentication,
            "{field}"
        );
        assert_eq!(host.calls.get(), calls, "signature before host: {field}");
    }
    let rejected_host = Registry {
        public: "rotated-key".into(),
        calls: Cell::new(0),
    };
    assert_eq!(
        world
            .signed_delegation_control_replay(&signed, &rejected_host)
            .unwrap_err()
            .stage,
        DelegationControlStage::Authentication
    );
    if let AgencyControlOperation::InstallDelegation { action_kinds, .. } = &mut command {
        action_kinds.reverse();
    }
    let reordered = request(command, 2);
    assert_eq!(
        world
            .signed_delegation_control_replay(&reordered, &host)
            .unwrap(),
        DelegationReplayMatch::IdentityConflict
    );
    assert_eq!(
        world
            .apply_signed_delegation_control(&reordered, &host)
            .unwrap_err()
            .stage,
        DelegationControlStage::Identity
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), baseline);
    let mut snapshot = world.snapshot();
    snapshot
        .state
        .starter_oc_claims
        .get_mut(AGENT_ID)
        .unwrap()
        .claimed_at += 1;
    let changed_source = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(
        changed_source
            .signed_delegation_control_replay(&signed, &host)
            .unwrap(),
        DelegationReplayMatch::IdentityConflict
    );
    let mut snapshot = world.snapshot();
    snapshot
        .state
        .starter_oc_claims
        .get_mut(AGENT_ID)
        .unwrap()
        .player_id = "new-owner".into();
    let changed_owner = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(
        changed_owner
            .signed_delegation_control_replay(&signed, &host)
            .unwrap_err()
            .stage,
        DelegationControlStage::Authentication
    );
}

#[test]
fn delegation_signed_runtime_profile_does_not_inherit_storage_actions() {
    for (kind, allowed) in [("WorldRule", true), ("BuildFactory", false)] {
        let mut world = fixture();
        let mut command = install();
        if let AgencyControlOperation::InstallDelegation { action_kinds, .. } = &mut command {
            *action_kinds = vec![kind.into()];
        }
        let result = world.apply_signed_delegation_control(&request(command, 1), &registry());
        if allowed {
            assert_eq!(result.unwrap().authorization.grant.action_kinds, vec![kind]);
        } else {
            assert_eq!(result.unwrap_err().stage, DelegationControlStage::Scope);
        }
    }
}

#[test]
fn delegation_revoke_exact_replay_preserves_saturating_revision_boundary() {
    let mut world = fixture();
    let mut old = grant();
    old.revision = u64::MAX;
    old.revoked = true;
    world.install_agent_delegation_grant(old.clone()).unwrap();
    let command = AgencyControlOperation::RevokeDelegation {
        grant_id: old.grant_id.clone(),
        expected_revision: u64::MAX,
    };
    let signed = request(command, 1);
    let before = serde_json::to_vec(&world).unwrap();
    let outcome = world
        .apply_signed_delegation_control(&signed, &registry())
        .unwrap();
    assert_eq!(
        outcome.disposition,
        DelegationControlDisposition::ExactReplay
    );
    assert_eq!(outcome.authorization.grant, old);
    assert_eq!(
        world
            .signed_delegation_control_replay(&signed, &registry())
            .unwrap(),
        DelegationReplayMatch::ExactReplay(Box::new(outcome.authorization))
    );
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
}

#[test]
fn delegation_orphan_spend_rejects_unrepresentable_signed_result_before_publication() {
    let original = fixture();
    let mut snapshot = original.snapshot();
    let grant = grant();
    let key = serde_json::to_string(&(
        &grant.grant_id,
        &grant.source_id,
        &grant.object_id,
        &grant.period_id,
    ))
    .unwrap();
    snapshot.cognition["agent_delegation"] =
        json!({"grants": {}, "spent": {key: grant.limit_units + 1}});
    let mut world = World::from_snapshot(snapshot, original.journal().clone()).unwrap();
    let mut legacy = world.clone();
    // The RED experiment asserted rejection here and failed: the old unit
    // mutation succeeds, then cannot project its newly installed grant.
    legacy
        .install_agent_delegation_grant(grant.clone())
        .unwrap();
    assert!(legacy.agent_delegation_authorizations(AGENT_ID).is_err());
    let root = std::env::temp_dir().join(format!(
        "oasis7-orphan-spend-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    world.save_to_dir(&root).unwrap();
    let persisted = std::fs::read(root.join("snapshot.json")).unwrap();
    let journal = std::fs::read(root.join("journal.json")).unwrap();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let signed = request(install(), 1);
    assert_eq!(
        world
            .signed_delegation_control_replay(&signed, &registry())
            .unwrap(),
        DelegationReplayMatch::NoMatch
    );
    let error = world
        .apply_signed_delegation_control(&signed, &registry())
        .unwrap_err();
    assert_eq!(error.stage, DelegationControlStage::Apply);
    assert_eq!(
        error.source,
        WorldError::ResourceBalanceInvalid {
            reason: "agent_delegation:budget_corrupt".into()
        }
    );
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert_eq!(
        std::fs::read(root.join("snapshot.json")).unwrap(),
        persisted
    );
    assert_eq!(std::fs::read(root.join("journal.json")).unwrap(), journal);
}
