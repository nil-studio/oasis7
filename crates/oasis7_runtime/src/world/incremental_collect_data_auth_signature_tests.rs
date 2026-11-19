//! Signature proof only: an unbound or nonce-zero request is not authorized.
use super::*;
use crate::world::incremental_action_data::auth_signature::{
    CollectDataSignatureWorkV1, SelectedCollectDataSignatureV1,
    verify_selected_collect_data_signature_v1,
};
#[test]
fn selected_auth_signature_preserves_legacy_rejection_without_minting_binding() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    let private_key = [7u8; 32];
    let public_key = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&private_key)
            .verifying_key()
            .to_bytes(),
    );
    let signature = |cost, nonce| {
        oasis7_proto::collect_data_auth::sign_authorization(
            oasis7_proto::collect_data_auth::COLLECT_DATA_SUBMIT_OPERATION,
            cost,
            3,
            "player",
            &public_key,
            nonce,
            &hex::encode(private_key),
        )
        .unwrap()
    };
    let valid = signature(2, 1);
    let zero = signature(2, 0);
    let cases = [
        (2, public_key.clone(), 1, valid.clone(), true),
        (2, "not-hex".into(), 1, valid.clone(), false),
        (2, public_key.clone(), 0, zero, true),
        (3, public_key.clone(), 1, valid, false),
        (2, public_key.clone(), 1, "wrong-prefix".into(), false),
    ];
    let mut expected = Vec::new();
    for (cost, key, nonce, sig, is_valid) in cases {
        let id = world.submit_action(crate::Action::CollectDataAuthenticated {
            collector_agent_id: "unbound-collector".into(),
            electricity_cost: cost,
            data_amount: 3,
            player_id: "player".into(),
            public_key: key,
            nonce,
            signature: sig,
        });
        let envelope = world.pending_actions.iter().find(|e| e.id == id).unwrap();
        let oracle = world.action_to_event(envelope).unwrap();
        let crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { reason, .. }) =
            oracle
        else {
            panic!("unbound oracle must reject")
        };
        if is_valid {
            assert!(format!("{reason:?}").contains("requires exactly one starter OC"));
            expected.push(None);
        } else {
            expected.push(Some(reason));
        }
    }
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
    let mut work = CollectDataSignatureWorkV1 {
        checks_remaining: 5,
        bytes_remaining: 1 << 20,
    };
    for expected in expected {
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
        let checked = verify_selected_collect_data_signature_v1(
            &selected,
            &outcome,
            &mut tx,
            &memory,
            &mut work,
            &live,
            &mut || false,
        )
        .unwrap();
        match (checked, expected) {
            (SelectedCollectDataSignatureV1::Verified(proof), None) => {
                proof
                    .validate(&selected, &outcome, &mut tx, &memory, &live)
                    .unwrap();
                let fields = proof.fields();
                assert_eq!(fields.collector, "unbound-collector");
                assert_eq!(fields.player, "player");
                assert_eq!(fields.public_key, public_key);
                assert_eq!(fields.data_amount, 3);
                assert_eq!(fields.electricity_cost, 2);
                assert!(!fields.signature.is_empty());
                assert!(fields.nonce <= 1);
            }
            (SelectedCollectDataSignatureV1::Rejected(reason), Some(expected)) => {
                assert_eq!(&*reason, &expected)
            }
            _ => panic!("signature source result differs from original verifier"),
        }
        assert_eq!(
            tx.ordered_fact_count(),
            before,
            "signature check cannot publish a business Event"
        );
    }
    assert_eq!(work.checks_remaining, 0);
    assert!(
        take_pending_action(
            &mut pending,
            &mut tx,
            &mut read,
            &mut pending_work,
            &mut originals,
            &live
        )
        .unwrap()
        .is_none()
    );
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert!(
        sealed
            .record
            .facts
            .iter()
            .all(|f| f.kind == ExecutionFactKindV1::Input)
    );
    assert_eq!(sealed.record.facts.len(), 5);
}
