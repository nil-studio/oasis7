//! Real selected signature -> Current claim -> replay CAS, compared to the legacy owner.
use super::*;
use crate::world::incremental_action_data::auth_current::*;
use crate::world::incremental_action_data::auth_signature::CollectDataSignatureWorkV1;
use oasis7_world_state::{
    incremental_authenticated_nonces::{AuthenticatedNonceV1, read_authenticated_nonce_overlay_v1},
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_world_native::StarterBindingIndexWorkV1,
};
#[test]
fn selected_authenticated_claim_and_nonce_match_legacy_and_consume_current_replay() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = transfer_resource_world();
    let key = |seed: u8| {
        hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_bytes(),
        )
    };
    let pk = key(7);
    let duplicate_key = key(8);
    let unbound_key = key(9);
    for (outer, agent, public_key) in [
        ("outer-different-from-player", "sender", pk.as_str()),
        ("duplicate-a", "sender", duplicate_key.as_str()),
        ("duplicate-b", "receiver", duplicate_key.as_str()),
    ] {
        world.state.starter_oc_claims.insert(
            outer.into(),
            oasis7_world_state::gameplay_state::StarterOcClaimState {
                agent_id: agent.into(),
                player_id: "player".into(),
                public_key: Some(public_key.into()),
                ..Default::default()
            },
        );
    }
    for (seed, public_key, collector, nonce, bad) in [
        (9, unbound_key.as_str(), "sender", 1, false),
        (7, pk.as_str(), "sender", 1, true),
        (7, pk.as_str(), "sender", 0, false),
        (7, pk.as_str(), "sender", 1, false),
        (7, pk.as_str(), "sender", 1, false),
        (7, pk.as_str(), "sender", 2, false),
        (7, pk.as_str(), "receiver", 3, false),
        (8, duplicate_key.as_str(), "sender", 1, false),
        (7, pk.as_str(), "sender", 3, false),
    ] {
        let signature = if bad {
            "invalid".into()
        } else {
            oasis7_proto::collect_data_auth::sign_authorization(
                oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
                1,
                1,
                "player",
                public_key,
                nonce,
                &hex::encode([seed; 32]),
            )
            .unwrap()
        };
        world.submit_action(crate::Action::CollectDataAuthenticated {
            collector_agent_id: collector.into(),
            electricity_cost: 1,
            data_amount: 1,
            player_id: "player".into(),
            public_key: public_key.into(),
            nonce,
            signature,
        });
    }
    let mut oracle = World::new_with_state(world.state.clone());
    oracle.state.time = 1;
    let mut expected = Vec::new();
    let mut expected_nonce = Vec::new();
    for envelope in &world.pending_actions {
        let body = oracle.action_to_event(envelope).unwrap();
        oracle
            .append_event_for_test(body.clone(), Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected.push(match body {
            crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                reason, ..
            }) => Some(reason),
            _ => None,
        });
        expected_nonce.push(
            oracle
                .state
                .authenticated_collect_data_last_nonces
                .get("player")
                .and_then(|m| m.get(&pk))
                .copied(),
        );
    }
    assert_eq!(expected.iter().filter(|e| e.is_none()).count(), 3);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    drop(oracle);
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
        [141; 32],
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
            &mut rules::directory_work(),
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
    let mut read_io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut auth_io = genesis::io();
    let mut auth_keys = genesis::keys();
    let mut auth_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut auth_io,
        },
        key_work: &mut auth_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut work = CollectDataAuthorizationWorkV1 {
        signature: CollectDataSignatureWorkV1 {
            checks_remaining: 9,
            bytes_remaining: 1 << 20,
        },
        claims: StarterBindingIndexWorkV1 {
            max_rows: 256,
            max_row_text_bytes: 64 << 10,
            max_pending_rows: 1024,
            steps_remaining: 100_000,
            bytes_remaining: 64 << 20,
            comparison_bytes_remaining: 64 << 20,
            moved_slots_remaining: 1_000_000,
            pending_entries_remaining: 100_000,
        },
        native: genesis::current_work(&fixture.roots).native.native,
        scan: genesis::scan(),
    };
    let mut sink = fixture.cas.clone();
    let mut output_io = genesis::io();
    let mut output_keys = genesis::keys();
    let mut writer = MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut output_io,
        key_work: &mut output_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 16,
            record_bytes: 16 << 20,
        },
        cancelled: || false,
    };
    let mut consumed = 0;
    for (expected, nonce) in expected.into_iter().zip(expected_nonce) {
        let selected = take_pending_action(
            &mut pending,
            &mut tx,
            &mut read,
            &mut pending_work,
            &mut originals,
            &live,
        )
        .unwrap()
        .unwrap();
        let outcome = rules::outcome(
            &fixture,
            &reader,
            &mut tx,
            &selected,
            &directory,
            &counter,
            &mut originals,
            &memory,
            0,
            &live,
        );
        let before = tx.ordered_fact_count();
        let checked = authorize_selected_collect_data_v1(
            &selected,
            &outcome,
            fixture.current.native().native().starter_binding_index(),
            fixture.current.native().controls(),
            &fixture.roots,
            &mut tx,
            &mut auth_read,
            &mut work,
            &live,
        )
        .unwrap();
        match (checked, expected) {
            (SelectedCollectDataAuthorizationV1::Rejected(reason), Some(expected)) => {
                assert_eq!(&*reason, &expected)
            }
            (SelectedCollectDataAuthorizationV1::Authorized(proof), None) => {
                proof
                    .validate(&selected, &outcome, &mut tx, &memory, &live)
                    .unwrap();
                let mut overlay = WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &reader,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *auth_read.values.io,
                    },
                    key_work: &mut *auth_read.key_work,
                    semantic_bytes: 4096,
                    record_bytes: 16 << 20,
                    take_state_error: || reader.cas.error(),
                    cancelled: || false,
                };
                proof
                    .consume_nonce(&selected, &outcome, &mut overlay, &mut writer, &live)
                    .unwrap();
                consumed += 1;
            }
            _ => panic!("Current authentication differs from independent legacy owner"),
        }
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *auth_read.values.io,
            },
            key_work: &mut *auth_read.key_work,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        let current = read_authenticated_nonce_overlay_v1("player", &pk, &mut overlay).unwrap();
        assert_eq!(
            current,
            nonce
                .map(AuthenticatedNonceV1::Value)
                .unwrap_or(AuthenticatedNonceV1::PlayerAbsent)
        );
        assert_eq!(
            tx.ordered_fact_count(),
            before,
            "auth/nonce work is not a business Event"
        );
    }
    assert_eq!(consumed, 3);
    assert_eq!(work.signature.checks_remaining, 0);
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(sealed.record.facts.len(), 9);
    assert!(
        sealed
            .record
            .facts
            .iter()
            .all(|f| f.kind == ExecutionFactKindV1::Input)
    );
}
