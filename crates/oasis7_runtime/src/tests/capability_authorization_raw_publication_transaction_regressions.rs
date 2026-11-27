use super::super::*;
use oasis7_wasm_abi::{CapabilityAudience, CapabilityPresenter, CapabilitySubject};

fn fixture() -> World {
    super::capability_grant_v2::fixture_world()
}

fn context() -> CapabilityInvocationContext {
    CapabilityInvocationContext {
        grant_id: "grant-raw-context".into(),
        subject: CapabilitySubject::System {
            system_id: "system.raw-context".into(),
            epoch: 0,
        },
        presenter: CapabilityPresenter {
            presenter_id: "provider.raw-context".into(),
            presenter_kind: "provider".into(),
            session_id: Some("session.raw-context".into()),
            attestation_ref: None,
        },
        audience: CapabilityAudience {
            world_id: super::capability_grant_v2::WORLD_ID.into(),
            branch_id: super::capability_grant_v2::BRANCH_ID.into(),
            finality_epoch: 4,
            target_kind: "world".into(),
            target_id: None,
        },
        catalog_snapshot_id: "catalog.raw-context".into(),
        module_id: super::capability_grant_v2::MODULE_ID.into(),
        module_version: super::capability_grant_v2::MODULE_VERSION.into(),
        response_nonce: "response.raw-context".into(),
    }
}

fn raw_events(world: &World) -> Vec<CapabilityAuthorizationEvent> {
    let issuer = super::capability_grant_v2::ISSUER_ID;
    let record = world.capability_revocation_state().authority_records[issuer].clone();
    let proof = world
        .capability_revocation_state()
        .authority_finality_proofs[issuer]
        .clone();
    let context = context();
    let mut context_capture = world.clone();
    context_capture
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::SystemIdentityInstalled {
                    system_id: "system.raw".into(),
                    epoch: 0,
                },
            ),
            None,
        )
        .expect("install live system identity for context capture");
    context_capture
        .install_capability_invocation_context(context.clone())
        .expect("capture valid context event");
    let context_event = context_capture
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::CapabilityAuthorization(
                event @ CapabilityAuthorizationEvent::InvocationContextInstalled { .. },
            ) => Some(event.clone()),
            _ => None,
        })
        .expect("captured context event");
    let subject = world
        .capability_budget_accounts()
        .values()
        .next()
        .expect("fixture budget")
        .subject
        .clone();
    let account = CapabilityBudgetAccount {
        subject,
        grant_id: "grant-raw-budget".into(),
        remaining_units: 31,
        reserved_units: 0,
        spent_units: 0,
    };
    let mut budget_capture = world.clone();
    budget_capture
        .install_capability_budget_account(account.clone())
        .expect("capture valid budget event");
    let budget_event = match &budget_capture
        .journal()
        .events
        .last()
        .expect("budget event")
        .body
    {
        WorldEventBody::CapabilityAuthorization(event) => event.clone(),
        other => panic!("unexpected budget event: {other:?}"),
    };
    let grant = super::capability_grant_v2::signed_grant(super::capability_grant_v2::grant_json(
        serde_json::json!({
            "grant_nonce": "raw-publication-grant"
        }),
    ));
    vec![
        CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, proof },
        CapabilityAuthorizationEvent::AgentIdentityInstalled {
            agent_id: super::capability_grant_v2::SUBJECT_ID.into(),
            identity: CapabilityAgentIdentity {
                owner_binding: "owner-7".into(),
                generation: 2,
            },
        },
        CapabilityAuthorizationEvent::SystemIdentityInstalled {
            system_id: "system.raw".into(),
            epoch: 0,
        },
        context_event,
        budget_event,
        CapabilityAuthorizationEvent::GrantRegistered { grant },
    ]
}

fn assert_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(
        world.capability_revocation_state(),
        before.capability_revocation_state()
    );
    assert_eq!(world.capability_grants_v2(), before.capability_grants_v2());
    assert_eq!(
        world.capability_nonce_records(),
        before.capability_nonce_records()
    );
    assert_eq!(
        world.capability_authorization_receipts(),
        before.capability_authorization_receipts()
    );
    assert_eq!(
        world.capability_invocation_contexts(),
        before.capability_invocation_contexts()
    );
    assert_eq!(
        world.capability_budget_accounts(),
        before.capability_budget_accounts()
    );
    assert_eq!(
        world.capability_effect_receipt_links(),
        before.capability_effect_receipt_links()
    );
    assert_eq!(
        world.capability_authorization_root(),
        before.capability_authorization_root()
    );
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        (
            world.snapshot().last_event_id,
            world.snapshot().event_id_era
        ),
        (
            before.snapshot().last_event_id,
            before.snapshot().event_id_era
        )
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.state().time, before.state().time);
    assert_eq!(world.current_state_root_hash().expect("root"), root);
}

#[test]
fn six_raw_capability_events_fail_atomically_after_prepare() {
    for event in raw_events(&fixture()) {
        let mut world = fixture();
        let before = world.clone();
        let root = world.current_state_root_hash().expect("root before");
        world.fail_next_append_after_publication_prepare_for_test();
        let error = world
            .append_event_for_test(
                WorldEventBody::CapabilityAuthorization(event),
                Some(CausedBy::Action(38)),
            )
            .expect_err("raw capability event must honor post-prepare failure");
        assert!(
            matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
        );
        assert_unchanged(&world, &before, &root);
    }
}

#[test]
fn legacy_authority_rejections_precede_and_preserve_failpoint() {
    let mut world = fixture();
    let issuer = super::capability_grant_v2::ISSUER_ID;
    let record = world.capability_revocation_state().authority_records[issuer].clone();
    let proof = world
        .capability_revocation_state()
        .authority_finality_proofs[issuer]
        .clone();
    let before = world.clone();
    let root = world.current_state_root_hash().expect("root before");
    world.fail_next_append_after_publication_prepare_for_test();
    for event in [
        CapabilityAuthorizationEvent::AuthorityInstalled {
            record: record.clone(),
        },
        CapabilityAuthorizationEvent::AuthorityInstalledWithFinality {
            record,
            certificate: proof.certificate,
        },
    ] {
        let error = world
            .append_event_for_test(WorldEventBody::CapabilityAuthorization(event), None)
            .expect_err("legacy authority event rejected");
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { ref reason } if reason.contains("no replayable finality") || reason.contains("no binding proof"))
        );
        assert_unchanged(&world, &before, &root);
    }
    let valid = CapabilityAuthorizationEvent::SystemIdentityInstalled {
        system_id: "system.failpoint-probe".into(),
        epoch: 0,
    };
    let error = world
        .append_event_for_test(WorldEventBody::CapabilityAuthorization(valid), None)
        .expect_err("legacy rejects must not consume failpoint");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
    );
    assert_unchanged(&world, &before, &root);
}

#[test]
fn grant_registration_preserves_validation_priority_and_immutable_registry() {
    let cases = [
        (
            serde_json::json!({"status": "draft", "issuer": {"issuer_id": "missing"}}),
            "only finalized and verified grants",
        ),
        (
            serde_json::json!({"grant_nonce": "missing-issuer", "issuer": {"issuer_id": "missing"}}),
            "issuer has no finalized authority record",
        ),
        (
            serde_json::json!({"grant_nonce": "missing-parent", "parent_grant_id": "absent-parent", "delegation_depth": 1}),
            "parent grant is not in the durable registry",
        ),
    ];
    for (overrides, expected) in cases {
        let mut world = fixture();
        let grant = super::capability_grant_v2::signed_grant(
            super::capability_grant_v2::grant_json(overrides),
        );
        let before = world.clone();
        let root = world.current_state_root_hash().expect("root before");
        let error = world
            .append_event_for_test(
                WorldEventBody::CapabilityAuthorization(
                    CapabilityAuthorizationEvent::GrantRegistered { grant },
                ),
                None,
            )
            .expect_err("invalid grant must be rejected before publication");
        assert!(
            matches!(error, WorldError::CapabilityAuthorizationDenied { ref reason } if reason.contains(expected)),
            "unexpected error: {error:?}"
        );
        assert_unchanged(&world, &before, &root);
    }

    let revoked = super::capability_grant_v2::signed_grant(super::capability_grant_v2::grant_json(
        serde_json::json!({
            "grant_nonce": "revoked-priority"
        }),
    ));
    let mut world = super::capability_grant_v2::fixture_world_with_revocations(
        std::collections::BTreeSet::from([revoked.grant_id.clone()]),
    );
    let before = world.clone();
    let root = world.current_state_root_hash().expect("root before");
    let error = world
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::GrantRegistered { grant: revoked },
            ),
            None,
        )
        .expect_err("revoked grant must be rejected before parent or publication");
    assert!(
        matches!(error, WorldError::CapabilityAuthorizationDenied { ref reason } if reason.contains("grant is revoked or superseded"))
    );
    assert_unchanged(&world, &before, &root);

    let grant = super::capability_grant_v2::signed_grant(super::capability_grant_v2::grant_json(
        serde_json::json!({
            "grant_nonce": "immutable-conflict"
        }),
    ));
    let mut world = fixture();
    world.seed_capability_grant_for_test(
        grant.grant_id.clone(),
        serde_json::json!({"different": true}),
    );
    let before = world.clone();
    let root = world.current_state_root_hash().expect("root before");
    let error = world
        .append_event_for_test(
            WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::GrantRegistered { grant },
            ),
            None,
        )
        .expect_err("grant body is immutable by stable id");
    assert!(
        matches!(error, WorldError::CapabilityAuthorizationDenied { ref reason } if reason.contains("immutable grant body changed"))
    );
    assert_unchanged(&world, &before, &root);
}

#[test]
fn successful_raw_sequence_preserves_cause_and_replays_maps_and_root() {
    let mut world = fixture();
    let raw_baseline = world.snapshot();
    let manifest_hash = util::hash_json(&raw_baseline.manifest).expect("hash replay manifest");
    let baseline = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: super::capability_grant_v2::WORLD_ID,
            chain_id: "runtime-chain",
            genesis_ref: None,
            created_at_height: raw_baseline.journal_len as u64,
            manifest_height: raw_baseline.journal_len as u64,
            commit_block_hash: None,
            tick: raw_baseline.state.time,
        },
        manifest_hash.clone(),
        manifest_hash,
    );
    let cause = Some(CausedBy::Action(380));
    for event in raw_events(&world) {
        world
            .append_event_for_test(
                WorldEventBody::CapabilityAuthorization(event),
                cause.clone(),
            )
            .expect("raw capability success");
    }
    assert!(
        world
            .journal()
            .events
            .iter()
            .rev()
            .take(6)
            .all(|event| event.caused_by == cause)
    );
    let replay = World::from_snapshot(baseline, world.journal().clone())
        .expect("replay raw capability sequence");
    assert_eq!(
        replay.capability_revocation_state(),
        world.capability_revocation_state()
    );
    assert_eq!(replay.capability_grants_v2(), world.capability_grants_v2());
    assert_eq!(
        replay.capability_invocation_contexts(),
        world.capability_invocation_contexts()
    );
    assert_eq!(
        replay.capability_budget_accounts(),
        world.capability_budget_accounts()
    );
    assert_eq!(
        replay.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replay.current_state_root_hash().expect("replay root"),
        world.current_state_root_hash().expect("live root")
    );
    staged_capability_grant_revoke_and_trusted_command_prepare_and_rollback();
}

fn staged_capability_grant_revoke_and_trusted_command_prepare_and_rollback() {
    use crate::gameplay_state::StarterOcClaimState; use crate::world::agent_delegation::StagedAgentDelegationGrant;
    use crate::world::agent_delegation::controls::{StagedAgentDelegationDenial, StagedAgentOwnerControl};
    use crate::world::capability_authorization_publication::{StagedCapabilityGrantV2, StagedCapabilityRevokeV2};
    use crate::world::capability_authz::transaction::StagedTrustedModuleCommandWithContext; use crate::world::owner_delegation_control::StagedOwnerDelegationControl;
    use crate::world::transition::{BorrowedWorldView, DurableCommitConfirmation, StagedTransition, TransitionContext, TransitionReject};
    use ed25519_dalek::Signer; use oasis7_proto::agency_control::{AGENCY_CONTROL_REQUEST_TYPE, AgencyControlOperation, AgencyControlRequest};
    use oasis7_proto::player_auth::sign_agency_control_auth_proof; use serde_json::json; use std::collections::BTreeSet;
    let (mut world, ctx) = { let w = fixture(); let t = w.state().time; (w, TransitionContext::new(t, None)) };
    let grant = super::capability_grant_v2::signed_grant(super::capability_grant_v2::grant_json(json!({"grant_nonce": "staged-grant-adapter-regression"})));
    let mut bad_grant = grant.clone(); bad_grant.status = "pending".into(); let view = BorrowedWorldView::new(&world).unwrap(); assert!(StagedCapabilityGrantV2 { grant: bad_grant }.prepare(&view, &ctx).is_err());
    let snap = world.snapshot(); let prep_grant = StagedCapabilityGrantV2 { grant: grant.clone() }.prepare(&view, &ctx).unwrap();
    assert!(matches!(prep_grant.clone().install_confirmed(&mut world, DurableCommitConfirmation::StatusUnknown { reason: "io".into() }), Err(TransitionReject::StatusUnknownFenced { .. }))); assert_eq!(world.snapshot(), snap); prep_grant.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap();
    super::capability_grant_v2::install_budget_for_grant(&mut world, &grant, 100);
    let catalog = super::capability_grant_v2::prepared_catalog(&world, &grant, super::capability_grant_v2::catalog_json(json!({}))); let response = super::capability_grant_v2::prepared_response(super::capability_grant_v2::response_json(json!({})), &catalog);
    let inv_ctx = CapabilityInvocationContext { grant_id: grant.grant_id.clone(), subject: grant.subject.clone(), presenter: response.presenter.clone(), audience: grant.audience.clone(), catalog_snapshot_id: catalog.snapshot_id.clone(), module_id: response.selected_entry.module_id.clone(), module_version: response.selected_entry.module_version.clone(), response_nonce: response.response_nonce.clone() };
    let mut bad_cmd_grant = grant.clone(); bad_cmd_grant.status = "revoked".into(); assert!(StagedTrustedModuleCommandWithContext::new(bad_cmd_grant, catalog.clone(), response.clone(), inv_ctx.clone(), &mut super::capability_grant_v2::RecordingSandbox::default()).prepare(&BorrowedWorldView::new(&world).unwrap(), &ctx).is_err());
    let snap_cmd = world.snapshot(); let mut sb = super::capability_grant_v2::RecordingSandbox::default(); let cmd_stage = StagedTrustedModuleCommandWithContext::new(grant.clone(), catalog, response, inv_ctx, &mut sb);
    let prep_cmd = cmd_stage.prepare(&BorrowedWorldView::new(&world).unwrap(), &ctx).unwrap(); assert_eq!(cmd_stage.take_receipt().unwrap().decision, "accepted");
    assert!(matches!(prep_cmd.install_confirmed(&mut world, DurableCommitConfirmation::StatusUnknown { reason: "io".into() }), Err(TransitionReject::StatusUnknownFenced { .. }))); assert_eq!(world.snapshot(), snap_cmd);
    let mut record = world.capability_revocation_state().authority_records.get(super::capability_grant_v2::ISSUER_ID).unwrap().clone(); record.revocation_epoch += 1; record.revoked_grant_ids = BTreeSet::from([grant.grant_id.clone()]);
    let mut proof = world.capability_revocation_state().authority_finality_proofs.get(super::capability_grant_v2::ISSUER_ID).unwrap().clone(); proof.binding = CapabilityAuthorityFinalityBinding::from_record(&record).unwrap(); proof.signatures.clear();
    for (id, k) in [(super::capability_grant_v2::ISSUER_ID, super::capability_grant_v2::capability_issuer_signing_key()), (super::capability_grant_v2::FINALITY_SIGNER_2, super::capability_grant_v2::capability_finality_signing_key_2())] { let sig = k.sign(&proof.signing_payload_v1(id).unwrap()); proof.signatures.insert(id.into(), format!("{}{}", CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1, hex::encode(sig.to_bytes()))); }
    record.revoked_grant_ids.remove(&grant.grant_id); let view = BorrowedWorldView::new(&world).unwrap(); assert!(StagedCapabilityRevokeV2 { grant_id: String::new(), record: record.clone(), proof: proof.clone() }.prepare(&view, &ctx).is_err());
    let prep_rev = StagedCapabilityRevokeV2 { grant_id: grant.grant_id.clone(), record, proof }.prepare(&view, &ctx).unwrap(); let snap_rev = world.snapshot();
    assert!(matches!(prep_rev.clone().install_confirmed(&mut world, DurableCommitConfirmation::StatusUnknown { reason: "io".into() }), Err(TransitionReject::StatusUnknownFenced { .. }))); assert_eq!(world.snapshot(), snap_rev); prep_rev.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap(); assert!(world.capability_revocation_state().revoked_grant_ids.contains(&grant.grant_id));
    struct TestRegistry { public: String }
    impl OwnerControlHostAuthority for TestRegistry { fn validate_registered_owner(&self, p: &str, k: &str, a: &str, w: &str, e: u64) -> Result<(), String> { if p != "player" || k != self.public || a != super::agent_cognition_runtime_hardening::AGENT_ID || w != super::agent_cognition_runtime_hardening::WORLD_ID || e != 0 { Err("rejected".into()) } else { Ok(()) } } }
    let (agent_id, world_id) = (super::agent_cognition_runtime_hardening::AGENT_ID, super::agent_cognition_runtime_hardening::WORLD_ID);
    let mut w = World::new(); w.submit_action(Action::RegisterAgent { agent_id: agent_id.into(), pos: super::pos(0, 0) }); w.step().unwrap();
    let mut st = w.state().clone(); st.starter_oc_claims.insert(agent_id.into(), StarterOcClaimState { agent_id: agent_id.into(), player_id: "player".into(), public_key: None, amount: 1, claimed_at: 0, source_treasury_bucket_id: None });
    let mut w = World::new_with_state(st); w.bind_cognition_runtime(world_id, "main", 0, None, "pending", 0).unwrap(); w.record_agent_chat_intent("player", agent_id, 1, "move").unwrap(); let intent = w.state().agents.get(agent_id).unwrap().intent.clone().unwrap();
    let dgrant = AgentDelegationGrantV1 { grant_id: "staged-del-grant".into(), source_id: format!("starter_claim:{agent_id}:0"), issuer_id: "player".into(), owner_id: "player".into(), organization_id: None, agent_id: agent_id.into(), object_id: agent_id.into(), action_kinds: vec!["MoveAgent".into()], revision: 1, period_id: "period".into(), valid_from_tick: 0, valid_until_tick: 100, limit_units: 3, resource_kind: "electricity".into(), revoked: false };
    let mut bad_owner = dgrant.clone(); bad_owner.owner_id = "intruder".into(); assert!(StagedAgentDelegationGrant::new(bad_owner).prepare(&BorrowedWorldView::new(&w).unwrap(), &ctx).is_err());
    let gs = StagedAgentDelegationGrant::new(dgrant.clone()); let pg = gs.prepare(&BorrowedWorldView::new(&w).unwrap(), &ctx).unwrap(); assert!(gs.take_authorization().is_some());
    let snap = w.snapshot(); assert!(matches!(pg.install_confirmed(&mut w, DurableCommitConfirmation::StatusUnknown { reason: "io".into() }), Err(TransitionReject::StatusUnknownFenced { .. }))); assert_eq!(w.snapshot(), snap);
    StagedAgentDelegationGrant::new(dgrant.clone()).prepare(&BorrowedWorldView::new(&w).unwrap(), &ctx).unwrap().install_confirmed(&mut w, DurableCommitConfirmation::Confirmed).unwrap();
    let private = [61u8; 32]; let public = hex::encode(ed25519_dalek::SigningKey::from_bytes(&private).verifying_key().to_bytes()); let host = TestRegistry { public: public.clone() };
    let mut req = AgencyControlRequest { frame_type: AGENCY_CONTROL_REQUEST_TYPE.into(), request_id: "signed-revoke-1".into(), player_id: "player".into(), public_key: public, agent_id: agent_id.into(), world_id: world_id.into(), reorg_epoch: 0, command: AgencyControlOperation::RevokeDelegation { grant_id: dgrant.grant_id.clone(), expected_revision: 1 }, auth: None };
    req.auth = Some(sign_agency_control_auth_proof(&req.request_id, &req.player_id, &req.public_key, &req.agent_id, &req.world_id, 0, &serde_json::to_value(&req.command).unwrap(), 1, &req.public_key, &hex::encode(private)).unwrap());
    let os = StagedOwnerDelegationControl::new(&req, &host); let po = os.prepare(&BorrowedWorldView::new(&w).unwrap(), &ctx).unwrap(); assert!(os.take_outcome().is_some());
    let snap = w.snapshot(); assert!(matches!(po.install_confirmed(&mut w, DurableCommitConfirmation::StatusUnknown { reason: "io".into() }), Err(TransitionReject::StatusUnknownFenced { .. }))); assert_eq!(w.snapshot(), snap);
    let ctrl = AgentOwnerControlV1 { control_id: "ctrl-1".into(), agent_id: agent_id.into(), intent_id: intent.intent_id.clone(), request_digest: intent.request_digest.clone(), grant_id: Some(dgrant.grant_id), expected_grant_revision: Some(1), kind: AgentOwnerControlKindV1::Interrupt };
    assert!(StagedAgentOwnerControl::new("intruder", ctrl.clone()).prepare(&BorrowedWorldView::new(&w).unwrap(), &ctx).is_err());
    let cs = StagedAgentOwnerControl::new("player", ctrl); let pc = cs.prepare(&BorrowedWorldView::new(&w).unwrap(), &ctx).unwrap(); assert!(cs.take_disposition().is_some());
    assert!(matches!(pc.install_confirmed(&mut w, DurableCommitConfirmation::StatusUnknown { reason: "io".into() }), Err(TransitionReject::StatusUnknownFenced { .. }))); assert_eq!(w.snapshot(), snap);
    let err = WorldError::ResourceBalanceInvalid { reason: "agent_delegation:grant_revoked".into() };
    StagedAgentDelegationDenial { agent_id: agent_id.into(), context: AgentDecisionCausalContextV1 { intent_id: Some(intent.intent_id), ..Default::default() }, error: &err }.prepare(&BorrowedWorldView::new(&w).unwrap(), &ctx).unwrap().install_confirmed(&mut w, DurableCommitConfirmation::Confirmed).unwrap();
    assert_eq!(w.state().agents.get(agent_id).unwrap().intent.as_ref().unwrap().status, "rejected");
}
