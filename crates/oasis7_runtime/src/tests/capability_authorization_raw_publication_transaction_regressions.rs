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
}
