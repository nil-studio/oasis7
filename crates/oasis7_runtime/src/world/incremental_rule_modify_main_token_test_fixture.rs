//! Main-token DATA and observations for the existing actual worker fixture.
//! This neither replaces the native Host closure fixture nor supplies a Rule
//! result: the parent fixture invokes its original worker and reads originals.
use super::*;
use oasis7_world_state::{
    incremental_world_balances::{
        WorldBalanceFieldV1, WorldBalanceReadV1, read_world_balance_overlay_v1,
    },
    incremental_world_number_maps::{
        WorldNumberMapFieldV1, WorldNumberMapKeyRefV1, WorldNumberV1,
        read_world_number_map_overlay_v1,
    },
};

pub(super) fn action() -> crate::Action {
    crate::Action::TransferMainToken {
        from_account_id: "sender".into(),
        to_account_id: "receiver".into(),
        amount: 3,
        nonce: 1,
        asset_id: None,
        memo: None,
        chain_id: None,
        network_id: None,
        tx_version: None,
        tx_type: None,
        valid_until_unix_ms: None,
        max_fee: None,
        fee_asset_id: None,
        application_payload_hash: None,
        client_request_id: None,
    }
}
pub(super) fn event() -> crate::WorldEventBody {
    crate::WorldEventBody::Domain(crate::DomainEvent::MainTokenTransferred {
        from_account_id: "sender".into(),
        to_account_id: "receiver".into(),
        amount: 3,
        nonce: 1,
        asset_id: None,
        memo: None,
    })
}
pub(super) fn seed(world: &mut World) {
    // Distinct pre-existing activity catches both advancing to event time and
    // incorrectly resetting an account's corresponding Agent body to zero.
    for (agent, last_active) in [("sender", 3), ("receiver", 7)] {
        world.state.agents.get_mut(agent).unwrap().last_active = last_active;
    }
    world.set_main_token_config(crate::main_token::MainTokenConfig {
        initial_supply: 8,
        ..crate::main_token::MainTokenConfig::default()
    });
    world.set_main_token_supply(crate::main_token::MainTokenSupplyState {
        total_supply: 8,
        circulating_supply: 8,
        total_issued: 0,
        total_burned: 0,
    });
    world
        .set_main_token_account_balance("sender", 8, 0)
        .unwrap();
}

/// Run the original World publication before the bounded worker invocation.
/// These scalar expectations never provide state or authority to that worker.
pub(super) fn legacy_activity(world: &mut World) -> [u64; 2] {
    let agents = ["sender", "receiver"];
    let before = agents.map(|agent| world.state.agents[agent].last_active);
    assert_eq!(before, [3, 7]);
    assert_eq!(world.state.time, 11);
    assert!(
        agents
            .iter()
            .all(|agent| world.state.agents[*agent].mailbox.is_empty())
    );
    let selected = world.pending_actions.front().unwrap().clone();
    assert!(World::should_emit_action_accepted(&selected.action));
    let accepted = crate::DomainEvent::ActionAccepted {
        action_id: selected.id,
        action_kind: crate::world::module_runtime_labels::action_kind_label(&selected.action)
            .into(),
        actor_id: selected.action.actor_id().unwrap().into(),
        eta_ticks: 0,
        notes: vec!["accepted_for_gameplay_processing".into()],
    };
    world
        .append_event(
            crate::WorldEventBody::Domain(accepted.clone()),
            Some(crate::CausedBy::Action(selected.id)),
        )
        .unwrap();
    assert_eq!(
        agents.map(|agent| world.state.agents[agent].last_active),
        before
    );
    world
        .append_event(event(), Some(crate::CausedBy::Action(selected.id)))
        .unwrap();
    let crate::WorldEventBody::Domain(transfer) = event() else {
        unreachable!("the fixed main-token fixture has a Domain body")
    };
    assert_eq!(
        world.state.agents["sender"]
            .mailbox
            .iter()
            .collect::<Vec<_>>(),
        vec![&accepted, &transfer],
        "legacy MainToken routes Accepted then transfer only to the sender",
    );
    assert!(world.state.agents["receiver"].mailbox.is_empty());
    let after = agents.map(|agent| world.state.agents[agent].last_active);
    // Unlike Resource/Material transfers, neither selective Accepted nor the
    // original monetary reducer changes Agent activity for a token account.
    assert_eq!(after, before, "legacy MainToken preserves Agent activity");
    after
}

pub(super) fn assert_current<'a>(
    reader: &Reader<'a>,
    transaction: &mut ExecutionTransaction<'_, Reader<'a>>,
    memory: &MemoryBudget,
) {
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = WorldFieldOverlayContextV1 {
        transaction,
        values: ValueReadContext {
            source: reader,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    for (account, expected) in [("sender", 5), ("receiver", 3)] {
        let mut identity = Vec::new();
        let value = read_world_balance_overlay_v1(
            WorldBalanceFieldV1::MainToken,
            account,
            &mut read,
            1024,
            &mut identity,
        )
        .unwrap();
        assert_eq!(identity, account.as_bytes());
        assert!(matches!(value, Some(WorldBalanceReadV1::MainToken {
            liquid_balance, vested_balance: 0, restricted_starter_claim_balance: 0, ..
        }) if liquid_balance == expected));
        let nonce = read_world_number_map_overlay_v1(
            WorldNumberMapFieldV1::MainTokenTransferNonces,
            WorldNumberMapKeyRefV1::Text(account),
            &mut read,
        )
        .unwrap();
        assert_eq!(
            nonce,
            if account == "sender" {
                Some(WorldNumberV1::Unsigned(1))
            } else {
                None
            }
        );
    }
}
