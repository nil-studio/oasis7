use super::pos;
use crate::{
    Action, DomainEvent, EconomicContractFulfillmentKind, EconomicContractStatus, RejectReason,
    World, WorldEventBody,
};
use ed25519_dalek::SigningKey;
use oasis7_agent_api::world_action::ResourceKind;

#[path = "data_access_control/raw_resource_publication_transaction_regressions.rs"]
mod raw_resource_publication_transaction_regressions;

fn register_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
}

fn assert_latest_rule_denied_contains(world: &World, needle: &str) {
    let body = world
        .journal()
        .events
        .iter()
        .rev()
        .map(|event| &event.body)
        .find(|body| {
            matches!(
                body,
                WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
            )
        })
        .expect("latest rejected action");
    match body {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
            ..
        }) => assert!(
            notes.iter().any(|value| value.contains(needle)),
            "expected RuleDenied containing '{needle}', got {notes:?}"
        ),
        other => panic!("expected ActionRejected RuleDenied, got {other:?}"),
    }
}

#[test]
fn collect_data_consumes_electricity_and_adds_data() {
    let mut world = World::new();
    register_agent(&mut world, "collector");
    world
        .set_agent_resource_balance("collector", ResourceKind::Electricity, 20)
        .expect("seed electricity");

    world.submit_action(Action::CollectData {
        collector_agent_id: "collector".to_string(),
        electricity_cost: 7,
        data_amount: 11,
    });
    world.step().expect("collect data");

    assert_eq!(
        world
            .agent_resource_balance("collector", ResourceKind::Electricity)
            .expect("collector electricity"),
        13
    );
    assert_eq!(
        world
            .agent_resource_balance("collector", ResourceKind::Data)
            .expect("collector data"),
        11
    );

    match &world.journal().events.last().expect("collect event").body {
        WorldEventBody::Domain(DomainEvent::DataCollected {
            collector_agent_id,
            electricity_cost,
            data_amount,
        }) => {
            assert_eq!(collector_agent_id, "collector");
            assert_eq!(*electricity_cost, 7);
            assert_eq!(*data_amount, 11);
        }
        other => panic!("expected DataCollected, got {other:?}"),
    }
}

#[test]
fn authenticated_collect_data_advances_consensus_nonce_and_rejects_replay() {
    let private_key = [41_u8; 32];
    let public_key = hex::encode(
        SigningKey::from_bytes(&private_key)
            .verifying_key()
            .to_bytes(),
    );
    let signature = oasis7_proto::collect_data_auth::sign_authorization(
        oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
        7,
        11,
        "player-auth",
        public_key.as_str(),
        9,
        hex::encode(private_key).as_str(),
    )
    .expect("sign authenticated collection");
    let mut world = World::new();
    register_agent(&mut world, "collector-auth");
    world
        .set_agent_resource_balance("collector-auth", ResourceKind::Electricity, 30)
        .expect("seed electricity");
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "collector-auth".to_string(),
        player_id: "player-auth".to_string(),
        public_key: Some(public_key.clone()),
    });
    world.step().expect("claim collector");

    let action = Action::CollectDataAuthenticated {
        collector_agent_id: "collector-auth".to_string(),
        electricity_cost: 7,
        data_amount: 11,
        player_id: "player-auth".to_string(),
        public_key: public_key.clone(),
        nonce: 9,
        signature: signature.clone(),
    };
    world.submit_action(action.clone());
    world.step().expect("authenticated collection");
    assert_eq!(
        world.state().authenticated_collect_data_last_nonces["player-auth"][public_key.as_str()],
        9
    );
    assert_eq!(
        world
            .agent_resource_balance("collector-auth", ResourceKind::Electricity)
            .expect("collector electricity"),
        23
    );

    world.submit_action(action);
    world.step().expect("replay deterministically rejected");
    assert_latest_rule_denied_contains(&world, "authenticated collect_data nonce replay");
    assert_eq!(
        world
            .agent_resource_balance("collector-auth", ResourceKind::Electricity)
            .expect("collector electricity"),
        23
    );

    let lower_signature = oasis7_proto::collect_data_auth::sign_authorization(
        oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
        7,
        11,
        "player-auth",
        public_key.as_str(),
        8,
        hex::encode(private_key).as_str(),
    )
    .expect("sign lower nonce collection");
    world.submit_action(Action::CollectDataAuthenticated {
        collector_agent_id: "collector-auth".to_string(),
        electricity_cost: 7,
        data_amount: 11,
        player_id: "player-auth".to_string(),
        public_key: public_key.clone(),
        nonce: 8,
        signature: lower_signature,
    });
    world
        .step()
        .expect("lower nonce deterministically rejected");
    assert_latest_rule_denied_contains(&world, "authenticated collect_data nonce replay");

    let signature_10 = oasis7_proto::collect_data_auth::sign_authorization(
        oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
        7,
        11,
        "player-auth",
        public_key.as_str(),
        10,
        hex::encode(private_key).as_str(),
    )
    .expect("sign tamper baseline");
    let baseline = Action::CollectDataAuthenticated {
        collector_agent_id: "collector-auth".to_string(),
        electricity_cost: 7,
        data_amount: 11,
        player_id: "player-auth".to_string(),
        public_key: public_key.clone(),
        nonce: 10,
        signature: signature_10,
    };
    let mut tampered = Vec::new();
    for field in [
        "collector",
        "cost",
        "yield",
        "player",
        "key",
        "nonce",
        "signature",
    ] {
        let mut action = baseline.clone();
        let Action::CollectDataAuthenticated {
            collector_agent_id,
            electricity_cost,
            data_amount,
            player_id,
            public_key,
            nonce,
            signature,
        } = &mut action
        else {
            unreachable!()
        };
        match field {
            "collector" => *collector_agent_id = "other-agent".to_string(),
            "cost" => *electricity_cost = 8,
            "yield" => *data_amount = 12,
            "player" => *player_id = "other-player".to_string(),
            "key" => *public_key = hex::encode([42_u8; 32]),
            "nonce" => *nonce = 11,
            "signature" => signature.push('0'),
            _ => unreachable!(),
        }
        tampered.push(action);
    }
    for action in tampered {
        world.submit_action(action);
        world
            .step()
            .expect("tampered action deterministically rejected");
        assert_latest_rule_denied_contains(&world, "authenticated collect_data");
        assert_eq!(
            world
                .agent_resource_balance("collector-auth", ResourceKind::Electricity)
                .expect("collector electricity"),
            23
        );
        assert_eq!(
            world.state().authenticated_collect_data_last_nonces["player-auth"]
                [public_key.as_str()],
            9
        );
    }
}

#[test]
fn collect_data_rejects_when_electricity_is_insufficient() {
    let mut world = World::new();
    register_agent(&mut world, "collector");
    world
        .set_agent_resource_balance("collector", ResourceKind::Electricity, 3)
        .expect("seed electricity");

    world.submit_action(Action::CollectData {
        collector_agent_id: "collector".to_string(),
        electricity_cost: 5,
        data_amount: 8,
    });
    world.step().expect("collect should be rejected");

    match &world.journal().events.last().expect("reject event").body {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason:
                RejectReason::InsufficientResource {
                    agent_id,
                    kind,
                    requested,
                    available,
                },
            ..
        }) => {
            assert_eq!(agent_id, "collector");
            assert_eq!(*kind, ResourceKind::Electricity);
            assert_eq!(*requested, 5);
            assert_eq!(*available, 3);
        }
        other => panic!("expected InsufficientResource rejection, got {other:?}"),
    }

    assert_eq!(
        world
            .agent_resource_balance("collector", ResourceKind::Electricity)
            .expect("collector electricity"),
        3
    );
    assert_eq!(
        world
            .agent_resource_balance("collector", ResourceKind::Data)
            .expect("collector data"),
        0
    );
}

#[test]
fn data_transfer_requires_access_grant() {
    let mut world = World::new();
    register_agent(&mut world, "owner");
    register_agent(&mut world, "buyer");
    world
        .set_agent_resource_balance("owner", ResourceKind::Data, 10)
        .expect("seed owner data");

    world.submit_action(Action::EmitResourceTransfer {
        from_agent_id: "owner".to_string(),
        to_agent_id: "buyer".to_string(),
        kind: ResourceKind::Data,
        amount: 4,
    });
    world.step().expect("reject transfer without access grant");
    assert_latest_rule_denied_contains(&world, "missing access grant");

    assert_eq!(
        world
            .agent_resource_balance("owner", ResourceKind::Data)
            .expect("owner data"),
        10
    );
    assert_eq!(
        world
            .agent_resource_balance("buyer", ResourceKind::Data)
            .expect("buyer data"),
        0
    );

    world.submit_action(Action::GrantDataAccess {
        owner_agent_id: "owner".to_string(),
        grantee_agent_id: "buyer".to_string(),
    });
    world.step().expect("grant data access");

    world.submit_action(Action::EmitResourceTransfer {
        from_agent_id: "owner".to_string(),
        to_agent_id: "buyer".to_string(),
        kind: ResourceKind::Data,
        amount: 4,
    });
    world.step().expect("transfer with access grant");

    assert_eq!(
        world
            .agent_resource_balance("owner", ResourceKind::Data)
            .expect("owner data"),
        6
    );
    assert_eq!(
        world
            .agent_resource_balance("buyer", ResourceKind::Data)
            .expect("buyer data"),
        4
    );

    world.submit_action(Action::RevokeDataAccess {
        owner_agent_id: "owner".to_string(),
        grantee_agent_id: "buyer".to_string(),
    });
    world.step().expect("revoke data access");

    world.submit_action(Action::EmitResourceTransfer {
        from_agent_id: "owner".to_string(),
        to_agent_id: "buyer".to_string(),
        kind: ResourceKind::Data,
        amount: 1,
    });
    world.step().expect("reject transfer after revoke");
    assert_latest_rule_denied_contains(&world, "missing access grant");
}

#[test]
fn economic_contract_data_settlement_requires_access_grant() {
    let mut world = World::new();
    register_agent(&mut world, "owner");
    register_agent(&mut world, "buyer");
    world
        .set_agent_resource_balance("owner", ResourceKind::Data, 50)
        .expect("seed owner data");

    let expires_at = world.state().time.saturating_add(10);
    world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "owner".to_string(),
        contract_id: "contract.data.access".to_string(),
        counterparty_agent_id: "buyer".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Data,
        settlement_amount: 20,
        reputation_stake: 4,
        expires_at,
        description: "data delivery".to_string(),
    });
    world.step().expect("open economic contract");
    world.submit_action(Action::AcceptEconomicContract {
        accepter_agent_id: "buyer".to_string(),
        contract_id: "contract.data.access".to_string(),
    });
    world.step().expect("accept economic contract");

    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "owner".to_string(),
        contract_id: "contract.data.access".to_string(),
        success: true,
        notes: "attempt settle without grant".to_string(),
    });
    world
        .step()
        .expect("reject settlement without access grant");
    assert_latest_rule_denied_contains(&world, "missing access grant");

    let contract = world
        .state()
        .economic_contracts
        .get("contract.data.access")
        .expect("contract exists");
    assert_eq!(contract.status, EconomicContractStatus::Accepted);
    assert_eq!(contract.settled_at, None);
    assert_eq!(contract.settlement_success, None);

    world.submit_action(Action::GrantDataAccess {
        owner_agent_id: "owner".to_string(),
        grantee_agent_id: "buyer".to_string(),
    });
    world.step().expect("grant data access");

    world.submit_action(Action::SettleEconomicContract {
        operator_agent_id: "owner".to_string(),
        contract_id: "contract.data.access".to_string(),
        success: true,
        notes: "settle with access grant".to_string(),
    });
    world.step().expect("settle economic contract");

    let contract = world
        .state()
        .economic_contracts
        .get("contract.data.access")
        .expect("settled contract exists");
    assert_eq!(contract.status, EconomicContractStatus::Settled);
    assert_eq!(contract.transfer_amount, 20);
    assert_eq!(contract.settlement_success, Some(true));
    assert_eq!(
        world
            .agent_resource_balance("owner", ResourceKind::Data)
            .expect("owner data"),
        30
    );
    assert_eq!(
        world
            .agent_resource_balance("buyer", ResourceKind::Data)
            .expect("buyer data"),
        20
    );
}
