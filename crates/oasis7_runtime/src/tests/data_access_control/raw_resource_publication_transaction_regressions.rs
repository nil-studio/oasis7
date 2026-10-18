use super::*;
use crate::{CausedBy, MaterialLedgerId, WorldError};

fn two_agents() -> World {
    let mut world = World::new();
    register_agent(&mut world, "owner");
    register_agent(&mut world, "grantee");
    world
        .set_agent_resource_balance("owner", ResourceKind::Electricity, 20)
        .unwrap();
    world
        .set_agent_resource_balance("owner", ResourceKind::Data, 10)
        .unwrap();
    world
}

fn authenticated_fixture() -> (World, DomainEvent) {
    let private_key = [40_u8; 32];
    let public_key = hex::encode(
        SigningKey::from_bytes(&private_key)
            .verifying_key()
            .to_bytes(),
    );
    let signature = oasis7_proto::collect_data_auth::sign_authorization(
        oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
        3,
        4,
        "player-40a",
        &public_key,
        1,
        &hex::encode(private_key),
    )
    .unwrap();
    let mut world = two_agents();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "owner".into(),
        player_id: "player-40a".into(),
        public_key: Some(public_key.clone()),
    });
    world.step().expect("bind authenticated collector");
    (
        world,
        DomainEvent::DataCollectedAuthenticated {
            collector_agent_id: "owner".into(),
            electricity_cost: 3,
            data_amount: 4,
            player_id: "player-40a".into(),
            public_key,
            nonce: 1,
            signature,
        },
    )
}

fn fixtures() -> Vec<(World, DomainEvent)> {
    let common = two_agents();
    let mut granted = common.clone();
    granted.submit_action(Action::GrantDataAccess {
        owner_agent_id: "owner".into(),
        grantee_agent_id: "grantee".into(),
    });
    granted.step().expect("seed permission for revoke");
    vec![
        (
            common.clone(),
            DomainEvent::ResourceTransferred {
                from_agent_id: "owner".into(),
                to_agent_id: "grantee".into(),
                kind: ResourceKind::Data,
                amount: 3,
            },
        ),
        (
            common.clone(),
            DomainEvent::DataCollected {
                collector_agent_id: "owner".into(),
                electricity_cost: 3,
                data_amount: 4,
            },
        ),
        authenticated_fixture(),
        (
            common,
            DomainEvent::DataAccessGranted {
                owner_agent_id: "owner".into(),
                grantee_agent_id: "grantee".into(),
            },
        ),
        (
            granted,
            DomainEvent::DataAccessRevoked {
                owner_agent_id: "owner".into(),
                grantee_agent_id: "grantee".into(),
            },
        ),
    ]
}

fn assert_world_unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.current_state_root_hash().unwrap(), root);
    for id in ["owner", "grantee"] {
        assert_eq!(
            world.state().agents[id].mailbox,
            before.state().agents[id].mailbox
        );
    }
}

fn assert_fail_retry(index: usize) {
    let (mut world, event) = fixtures().swap_remove(index);
    let baseline = world.snapshot();
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(400)),
        )
        .expect_err("raw resource event must honor postprepare failure");
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { ref reason } if reason.contains("publication preparation"))
    );
    assert_world_unchanged(&world, &before, &root);

    let cause = Some(CausedBy::Action(401));
    world
        .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
        .expect("same-world retry");
    assert_eq!(
        world
            .journal()
            .events
            .last()
            .and_then(|event| event.caused_by.clone()),
        cause
    );
    assert_ne!(world.snapshot(), baseline);
    if index == 2 {
        assert_eq!(
            world.state().authenticated_collect_data_last_nonces["player-40a"]
                .values()
                .next(),
            Some(&1),
            "authenticated nonce advances exactly once on successful retry"
        );
    }
    let replay = World::from_snapshot(baseline, world.journal().clone()).expect("replay retry");
    assert_eq!(replay.snapshot(), world.snapshot());
    assert_eq!(
        replay.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn resource_transferred_raw_is_atomic_and_retryable() {
    assert_fail_retry(0);
}
#[test]
fn data_collected_raw_is_atomic_and_retryable() {
    assert_fail_retry(1);
}
#[test]
fn authenticated_data_collected_raw_is_atomic_and_retryable() {
    assert_fail_retry(2);
}
#[test]
fn data_access_granted_raw_is_atomic_and_retryable() {
    assert_fail_retry(3);
}
#[test]
fn data_access_revoked_raw_is_atomic_and_retryable() {
    assert_fail_retry(4);
}

fn assert_natural_error(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("natural validation error");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    assert_world_unchanged(&world, &before, &root);
}

#[test]
fn transfer_and_collection_validation_priority_is_stable() {
    let mut common = two_agents();
    assert_natural_error(
        common.clone(),
        DomainEvent::ResourceTransferred {
            from_agent_id: "missing-from".into(),
            to_agent_id: "missing-to".into(),
            kind: ResourceKind::Data,
            amount: 1,
        },
        "missing-from",
    );
    assert_natural_error(
        common.clone(),
        DomainEvent::ResourceTransferred {
            from_agent_id: "owner".into(),
            to_agent_id: "missing-to".into(),
            kind: ResourceKind::Data,
            amount: 11,
        },
        "missing-to",
    );
    common
        .set_agent_resource_balance("grantee", ResourceKind::Data, i64::MAX)
        .unwrap();
    assert_natural_error(
        common.clone(),
        DomainEvent::ResourceTransferred {
            from_agent_id: "owner".into(),
            to_agent_id: "grantee".into(),
            kind: ResourceKind::Data,
            amount: 1,
        },
        "transfer add failed",
    );
    assert_natural_error(
        common.clone(),
        DomainEvent::ResourceTransferred {
            from_agent_id: "owner".into(),
            to_agent_id: "grantee".into(),
            kind: ResourceKind::Data,
            amount: 11,
        },
        "transfer remove failed",
    );
    assert_natural_error(
        common.clone(),
        DomainEvent::DataCollected {
            collector_agent_id: "missing".into(),
            electricity_cost: 0,
            data_amount: 0,
        },
        "electricity_cost must be > 0",
    );
    assert_natural_error(
        common,
        DomainEvent::DataCollected {
            collector_agent_id: "owner".into(),
            electricity_cost: 21,
            data_amount: i64::MAX,
        },
        "electricity debit failed",
    );
}

#[test]
fn authenticated_and_access_compatibility_preserves_nonce_permissions_and_routing() {
    let (world, mut authenticated) = authenticated_fixture();
    if let DomainEvent::DataCollectedAuthenticated {
        nonce, signature, ..
    } = &mut authenticated
    {
        *nonce = 0;
        signature.clear();
    }
    assert_natural_error(world, authenticated, "invalid amount or nonce");

    let (world, mut bad_signature) = authenticated_fixture();
    if let DomainEvent::DataCollectedAuthenticated { signature, .. } = &mut bad_signature {
        *signature = "00".into();
    }
    assert_natural_error(world, bad_signature, "signature invalid");

    let (world, event) = authenticated_fixture();
    let mut state = world.state().clone();
    state.starter_oc_claims.clear();
    assert_natural_error(
        World::new_with_state(state),
        event.clone(),
        "requires exactly one starter OC",
    );

    let mut state = world.state().clone();
    let mut duplicate = state
        .starter_oc_claims
        .values()
        .next()
        .expect("seed claim")
        .clone();
    duplicate.agent_id = "grantee".into();
    state
        .starter_oc_claims
        .insert("duplicate-claim".into(), duplicate);
    assert_natural_error(
        World::new_with_state(state),
        event.clone(),
        "requires exactly one starter OC",
    );

    let mut insufficient = world;
    insufficient
        .set_agent_resource_balance("owner", ResourceKind::Electricity, 0)
        .unwrap();
    let nonces_before = insufficient
        .state()
        .authenticated_collect_data_last_nonces
        .clone();
    assert_natural_error(insufficient.clone(), event, "electricity debit failed");
    assert_eq!(
        insufficient.state().authenticated_collect_data_last_nonces,
        nonces_before,
        "resource failure cannot advance the authenticated nonce"
    );

    let mut world = two_agents();
    let before_owner_mailbox = world.state().agents["owner"].mailbox.len();
    let before_grantee_mailbox = world.state().agents["grantee"].mailbox.len();
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::DataAccessRevoked {
                owner_agent_id: "owner".into(),
                grantee_agent_id: "grantee".into(),
            }),
            None,
        )
        .expect("absent permission revoke is accepted");
    assert!(!world.state().data_access_permissions.contains_key("owner"));
    assert_eq!(
        world.state().agents["owner"].mailbox.len(),
        before_owner_mailbox + 1
    );
    assert_eq!(
        world.state().agents["grantee"].mailbox.len(),
        before_grantee_mailbox + 1
    );

    let activity_before = world.state().agents["owner"].last_active;
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::ResourceTransferred {
                from_agent_id: "owner".into(),
                to_agent_id: "owner".into(),
                kind: ResourceKind::Data,
                amount: i64::MIN,
            }),
            None,
        )
        .expect("self-transfer accepts arbitrary amount without changing balance");
    assert_eq!(
        world
            .agent_resource_balance("owner", ResourceKind::Data)
            .unwrap(),
        10
    );
    assert!(world.state().agents["owner"].last_active >= activity_before);
    assert_eq!(
        world.state().agents["owner"].mailbox.len(),
        before_owner_mailbox + 2
    );
    assert_eq!(
        world.state().agents["grantee"].mailbox.len(),
        before_grantee_mailbox + 1
    );

    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::DataAccessGranted {
                owner_agent_id: "owner".into(),
                grantee_agent_id: "owner".into(),
            }),
            None,
        )
        .expect("same-agent grant is accepted");
    assert!(!world.state().data_access_permissions.contains_key("owner"));

    assert_natural_error(
        two_agents(),
        DomainEvent::DataAccessGranted {
            owner_agent_id: "missing-owner".into(),
            grantee_agent_id: "missing-grantee".into(),
        },
        "missing-owner",
    );
}

#[test]
fn legacy_material_forms_are_preserved_on_failure_and_normalized_on_retry() {
    let world_key = MaterialLedgerId::world();
    let unrelated_key = MaterialLedgerId::site("unrelated-40a");
    for mode in 0..3 {
        let mut state = two_agents().state().clone();
        state.materials.insert("legacy".into(), 7);
        state
            .material_ledgers
            .insert(unrelated_key.clone(), [("ore".into(), 9)].into());
        match mode {
            0 => {
                state.material_ledgers.remove(&world_key);
            }
            1 => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), Default::default());
            }
            2 => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), [("canonical".into(), 11)].into());
            }
            _ => unreachable!(),
        }
        let mut world = World::new_with_state(state);
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        let event = DomainEvent::ResourceTransferred {
            from_agent_id: "owner".into(),
            to_agent_id: "owner".into(),
            kind: ResourceKind::Data,
            amount: i64::MIN,
        };
        world.fail_next_append_after_publication_prepare_for_test();
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .expect_err("legacy material publication honors failpoint");
        assert_world_unchanged(&world, &before, &root);
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .expect("retry normalizes materials");
        assert_eq!(world.state().material_ledgers[&unrelated_key]["ore"], 9);
        assert_eq!(
            world.state().materials,
            world.state().material_ledgers[&world_key]
        );
        if mode == 2 {
            assert_eq!(world.state().materials["canonical"], 11);
            assert!(!world.state().materials.contains_key("legacy"));
        } else {
            assert_eq!(world.state().materials["legacy"], 7);
        }
    }
}
