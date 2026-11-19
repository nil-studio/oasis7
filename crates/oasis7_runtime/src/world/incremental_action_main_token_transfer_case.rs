//! Actual FIFO/Rule/Current owner tests. The World oracle independently checks
//! event publication, balances, nonces and optional sender mailbox effects.
use super::rules::{
    FixtureInputV1, NonmovementExecutionFixtureV1, NonmovementFixtureWorkV1, directory_work,
    evaluate,
};
use super::*;
use crate::main_token::{MainTokenConfig, MainTokenSupplyState};
use crate::world::incremental_action_data::main_token_transfer::MainTokenTransferWorkV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_world_state::{
    incremental_event_fields::{MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1},
    incremental_world_balances::{
        WorldBalanceFieldV1, WorldBalanceReadV1, read_world_balance_overlay_v1,
    },
    incremental_world_number_maps::{
        WorldNumberMapFieldV1, WorldNumberMapKeyRefV1, WorldNumberV1,
        read_world_number_map_overlay_v1,
    },
};

fn action(from: &str, to: &str, amount: u64, nonce: u64) -> crate::Action {
    crate::Action::TransferMainToken {
        from_account_id: from.into(),
        to_account_id: to.into(),
        amount,
        nonce,
        asset_id: Some(" main_token ".into()),
        memo: Some(" bounded transfer ".into()),
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
fn seed() -> World {
    let mut world = World::new();
    for id in ["alice", "bob"] {
        let mut state = oasis7_world_state::agent_state::AgentState::new(
            id,
            oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        );
        state
            .resources
            .set(
                oasis7_agent_api::world_action::ResourceKind::Electricity,
                10,
            )
            .unwrap();
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(state, 0));
    }
    world.set_main_token_config(MainTokenConfig {
        initial_supply: 1000,
        ..MainTokenConfig::default()
    });
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 1000,
        circulating_supply: 1000,
        total_issued: 0,
        total_burned: 0,
    });
    world
        .set_main_token_account_balance("alice", 900, 0)
        .unwrap();
    world.set_main_token_account_balance("bob", 100, 0).unwrap();
    world
}
fn work(common: &FormalBoundedWorkV1) -> MainTokenTransferWorkV1 {
    MainTokenTransferWorkV1 {
        steps_remaining: 10_000,
        bytes_remaining: 16 << 20,
        pending_entries_remaining: 100_000,
        max_identity_bytes: 1024,
        core: common.identity(),
    }
}

#[derive(Clone, Copy)]
enum Failure {
    None,
    Original,
    LocalWork,
    ForeignCommon,
}

fn run(case: Failure) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = seed();
    let actions = if matches!(case, Failure::None) {
        vec![
            action(" alice ", "bob", 250, 1),
            action("alice", "bob", 250, 2),
            action("alice", "bob", 1, 2),
            action("alice", "bob", 500, 3),
            action("alice", "carol", 5, 3),
            action("alice", "alice", 1, 4),
            action("alice", "bob", 0, 4),
            action("carol", "alice", 1, 1),
        ]
    } else {
        vec![action("alice", "bob", 250, 1)]
    };
    for action in actions {
        world.submit_action(action);
    }
    let mut oracle = World::new_with_state(world.state.clone());
    oracle.state.time = 1;
    for envelope in &world.pending_actions {
        let body = oracle.action_to_event(envelope).unwrap();
        oracle
            .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
    }
    let expected = oracle.journal.events.iter().cloned().collect::<Vec<_>>();
    let expected_balances = oracle.state.main_token_balances.clone();
    let expected_nonces = oracle.state.main_token_transfer_nonces.clone();
    let expected_mailboxes = ["alice", "bob"].map(|id| (id, oracle.state.agents[id].mailbox.len()));
    if matches!(case, Failure::None) {
        assert_eq!(expected_balances["alice"].liquid_balance, 396);
        assert_eq!(expected_balances["bob"].liquid_balance, 600);
        assert_eq!(expected_balances["carol"].liquid_balance, 4);
        assert_eq!(expected_nonces["alice"], 3);
        assert_eq!(expected_nonces["carol"], 1);
        assert!(matches!(
            expected[2].body,
            crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { .. })
        ));
        assert!(matches!(
            expected[3].body,
            crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { .. })
        ));
        assert_eq!(oracle.main_token_supply().total_supply, 1000);
    }
    drop(oracle);
    let limits = MaterialFixtureLimitsV1::for_state(&world.state);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut originals = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [157; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_READS,
            max_writes: 1024,
            max_facts: 64,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
    let directory = {
        let mut io = genesis::io();
        let mut keys = genesis::keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        build_accepted_invocation_directory_v1(
            &fixture.runtime,
            &fixture.head,
            fixture.current.raw().instances(),
            fixture.current.runtime().config(),
            &reader,
            &mut overlay,
            &mut directory_work(),
            &live,
        )
        .unwrap()
    };
    let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
    let guard = tx.guard();
    let mut pending =
        oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(),
            &fixture.roots,
            &mut tx,
            &memory,
            &guard,
            &live,
            &mut pending_work.index,
            || false,
        )
        .unwrap();
    let mut sink = fixture.cas.clone();
    let mut io = genesis::io();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut io,
        cancelled: || false,
    };
    let mut canonical = genesis::canonical();
    let guard = tx.guard();
    let step = IwsStepContext::begin_counted(
        &mut tx,
        0,
        &guard,
        &live,
        &reader,
        &mut mutation,
        &mut canonical,
        &counter,
    )
    .unwrap();
    let common = FormalBoundedWorkV1::new(100_000, 64 << 20).unwrap();
    let foreign = FormalBoundedWorkV1::new(100_000, 64 << 20).unwrap();
    let mut work = work(&common);
    if matches!(case, Failure::LocalWork) {
        work.steps_remaining = 0;
    }
    if matches!(case, Failure::ForeignCommon) {
        work.core = foreign.identity();
    }
    let mut audit = super::audit_work();
    let mut failure = None;
    for event in &expected {
        let mut persist =
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, cancelled: &mut dyn FnMut() -> bool| {
                let size = usize::try_from(stream.original_bytes()).unwrap(); assert!(size <= 64 << 10);
                let _fee = memory.reserve(size + std::mem::size_of::<Vec<u8>>(), MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                let mut bytes = Vec::with_capacity(size); stream.write_to(&mut bytes)?; assert!(!cancelled());
                if matches!(case, Failure::Original) && matches!(serde_cbor::from_slice::<crate::WorldEvent>(&bytes).unwrap().body, crate::WorldEventBody::Domain(crate::DomainEvent::MainTokenTransferred { .. })) {
                    return Err(BodyMutationError::Invalid("main token original injected"));
                }
                assert_eq!(fixture.objects.put(&bytes), stream.original_id()); Ok(())
            })
            };
        let input = if matches!(case, Failure::None) {
            FixtureInputV1::Pending(&mut pending, &mut pending_work)
        } else {
            FixtureInputV1::PendingFallible(&mut pending, &mut pending_work, &mut failure, None)
        };
        let now = step.time();
        let (_, completed) = evaluate(
            &fixture,
            &reader,
            step.transaction,
            input,
            &directory,
            &counter,
            &mut originals,
            &memory,
            now,
            &live,
            None,
            Some(NonmovementExecutionFixtureV1 {
                limits,
                persist: &mut persist,
                work: NonmovementFixtureWorkV1::MainTokenTransfer(&mut work, &common),
                audit: &mut audit,
            }),
        );
        if !matches!(case, Failure::None) {
            assert!(completed.is_none());
            break;
        }
        let Some(CompletedNonmovementActionV1::MainTokenTransfer(result)) = completed else {
            panic!("actual MainTokenTransfer completion missing")
        };
        result.validate(step.transaction).unwrap();
        assert_eq!(result.event(), event);
        let encoded = oasis7_wasm_abi::encode_canonical_cbor(event).unwrap();
        assert_eq!(
            fixture.objects.original(blake3::hash(&encoded).as_bytes()),
            encoded
        );
    }
    if !matches!(case, Failure::None) {
        let error = failure.expect("original typed execution error");
        let body = error.body_error_for_fixture().expect("actual owner error");
        match case {
            Failure::Original => assert!(matches!(
                body,
                BodyMutationError::Invalid("main token original injected")
            )),
            Failure::LocalWork => assert!(matches!(
                body,
                BodyMutationError::Value(ValueTreeError::ResourceLimited(
                    "main token transfer work"
                ))
            )),
            Failure::ForeignCommon => assert!(matches!(
                body,
                BodyMutationError::Invalid("main token transfer Common identity")
            )),
            Failure::None => unreachable!(),
        }
        if !matches!(case, Failure::ForeignCommon) {
            assert!(common.remaining_steps() < 100_000);
        }
        drop(step);
        let guard = tx.guard();
        assert!(
            tx.seal(&guard, &live, 1).is_err(),
            "partial monetary writes never seal"
        );
        return;
    }
    assert!(work.steps_remaining < 10_000);
    assert!(common.remaining_steps() < 100_000);
    assert_eq!(step.journal_height().unwrap(), expected.len() as u64);
    let mut read_io = genesis::io();
    let mut read_keys = genesis::keys();
    {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        for (id, expected) in expected_balances {
            let mut identity = Vec::new();
            let actual = read_world_balance_overlay_v1(
                WorldBalanceFieldV1::MainToken,
                &id,
                &mut overlay,
                1024,
                &mut identity,
            )
            .unwrap()
            .unwrap();
            assert_eq!(identity, id.as_bytes());
            let WorldBalanceReadV1::MainToken {
                identity_utf8_bytes,
                liquid_balance,
                vested_balance,
                restricted_starter_claim_balance,
            } = actual
            else {
                panic!("actual main-token row kind")
            };
            assert_eq!(
                (
                    identity_utf8_bytes,
                    liquid_balance,
                    vested_balance,
                    restricted_starter_claim_balance
                ),
                (
                    id.len() as u64,
                    expected.liquid_balance,
                    expected.vested_balance,
                    expected.restricted_starter_claim_balance
                )
            );
            let nonce = read_world_number_map_overlay_v1(
                WorldNumberMapFieldV1::MainTokenTransferNonces,
                WorldNumberMapKeyRefV1::Text(&id),
                &mut overlay,
            )
            .unwrap();
            assert_eq!(
                nonce,
                expected_nonces
                    .get(&id)
                    .copied()
                    .map(WorldNumberV1::Unsigned)
            );
        }
    }
    for (id, expected_len) in expected_mailboxes {
        let key = MailboxKeyV1::new(id, None).unwrap();
        let actual = MailboxFrontierV1::from_canonical_bytes(
            &read_fixed::<MAILBOX_FRONTIER_BYTES, _, _, _, _>(
                step.transaction,
                StateNamespaceV1::Scheduler,
                key.as_bytes(),
                step.values,
                step.mutation,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(actual.tail - actual.head, expected_len as u64);
    }
    drop(step);
    let guard = tx.guard();
    assert!(
        tx.seal(&guard, &live, 1).is_ok(),
        "real successful queue remains sealable"
    );
}

pub(in crate::world::incremental_genesis) fn fifo() {
    run(Failure::None);
}
pub(in crate::world::incremental_genesis) fn original_failure() {
    run(Failure::Original);
}
pub(in crate::world::incremental_genesis) fn work_exhaustion() {
    run(Failure::LocalWork);
}
pub(in crate::world::incremental_genesis) fn foreign_common() {
    run(Failure::ForeignCommon);
}
