//! Sequence DATA and original Normal builder regressions. These do not stand
//! in for final-source note capture, terminal sealing or accepted publication.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, bounded_context::FormalBoundedWorkV1,
};
use oasis7_world_state::{
    DomainEvent, WorldEventBody,
    incremental_world_event_decode::{WorldEventDecodeWorkV1, decode_world_event_original_v1},
};

fn append_rejection(world: &mut crate::World, agent: &str) {
    world
        .append_event(
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id: 7,
                reason: crate::RejectReason::AgentNotFound {
                    agent_id: agent.into(),
                },
            }),
            None,
        )
        .unwrap();
}

fn decode_work() -> WorldEventDecodeWorkV1 {
    WorldEventDecodeWorkV1 {
        structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 8 << 20,
            values_remaining: 100_000,
            max_depth: 32,
        },
        canonical: oasis7_wasm_abi::CanonicalCborWorkV1 {
            visits_remaining: 100_000,
            bytes_remaining: 8 << 20,
            max_depth: 32,
            max_map_keys: 64,
            max_key_bytes: 4096,
        },
        max_original_bytes: 1 << 20,
        max_heap_bytes: 32 << 20,
    }
}

fn assert_context_error(error: BodyMutationError, note: &str) {
    let BodyMutationError::Original(error) = error else {
        panic!("expected original Common refusal: {error:?}");
    };
    assert!(matches!(
        error.downcast_ref::<FormalError>(),
        Some(FormalError::Invalid(actual)) if actual == note
    ));
}

#[test]
fn normal_event_builder_preserves_original_sequence_and_exact_byte_debit() {
    normal_event_sequence_case();
}

pub(in crate::world) fn normal_event_sequence_case() {
    let memory = MemoryBudget::new(128 << 20, 0).unwrap();
    let _input = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let (mut world, parent, pages) = crate::world::incremental_body::tests::fixture(&memory);
    append_rejection(&mut world, "original-first");
    append_rejection(&mut world, "原 second");
    assert_eq!(world.journal().events.len(), 2);
    let tx = ExecutionTransaction::begin(
        parent.clone(),
        [113; 32],
        &pages,
        &memory,
        crate::world::incremental_body::tests::limits(),
    )
    .unwrap();
    let live = parent.fence().unwrap();
    let work = FormalBoundedWorkV1::new(1_000_000, 16 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut decode = decode_work();
    for count in [0, world.journal().events.len()] {
        let events = &world.journal().events[..count];
        let wire = oasis7_world_state::codec::journal_events_cbor(events).unwrap();
        let baseline = memory.used();
        let guard = tx.guard();
        let mut builder = NormalEventCommitmentBuilderV1::new(count, guard, &mut common).unwrap();
        for event in events {
            let raw = oasis7_wasm_abi::encode_canonical_cbor(event).unwrap();
            let decoded = decode_world_event_original_v1(
                &raw,
                &memory,
                MemoryClass::Ordinary,
                &mut decode,
                || false,
            )
            .unwrap();
            builder.push(decoded, &mut common).unwrap();
        }
        let before = work.remaining_read_bytes();
        let committed = builder.finish(&mut common).unwrap();
        assert_eq!(committed.digest(), blake3::hash(&wire).as_bytes());
        assert_eq!(committed.count(), count);
        assert_eq!(
            before - work.remaining_read_bytes(),
            (size_of::<NormalEventCommitmentV1>() + 2 * wire.len() + 32) as u64
        );
        committed
            .validate_capture(&tx, &tx.guard(), &mut common, &live)
            .unwrap();
        let mut visited = 0;
        committed
            .visit_appended_events_v1(&tx, &mut common, &live, |event, _| {
                assert_eq!(event, &events[visited]);
                visited += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(visited, count);
        drop(committed);
        assert_eq!(memory.used(), baseline);
    }
}

#[test]
fn event_sequence_hash_preserves_late_cancellation_and_final_byte_refusal() {
    event_sequence_hash_refusals_case();
}

pub(in crate::world) fn event_sequence_hash_refusals_case() {
    let memory = MemoryBudget::new(32 << 20, 0).unwrap();
    let _input = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = crate::World::new();
    append_rejection(&mut world, &"原".repeat(8192));
    let events = &world.journal().events;
    assert_eq!(events.len(), 1);
    let wire = oasis7_world_state::codec::journal_events_cbor(events).unwrap();
    let exact_bytes = (2 * wire.len() + 32) as u64;
    let baseline = memory.used();

    let work = FormalBoundedWorkV1::new(100_000, exact_bytes).unwrap();
    let mut stop = || exact_bytes - work.remaining_read_bytes() >= 2 * 8192;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let error = hash_world_event_sequence_bounded_v1(events.iter(), &mut common).unwrap_err();
    assert_context_error(error, "bounded archive read cancelled");
    assert!(work.remaining_read_bytes() > 32);
    assert!(work.remaining_read_bytes() < exact_bytes);
    assert_eq!(memory.used(), baseline);

    let work = FormalBoundedWorkV1::new(100_000, exact_bytes - 1).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let error = hash_world_event_sequence_bounded_v1(events.iter(), &mut common).unwrap_err();
    assert_context_error(error, "bounded archive read bytes");
    assert_eq!(work.remaining_read_bytes(), 0);
    assert!(work.remaining_steps() < 100_000);
    assert_eq!(memory.used(), baseline);

    let work = FormalBoundedWorkV1::new(100_000, exact_bytes).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    assert_eq!(
        hash_world_event_sequence_bounded_v1(events.iter(), &mut common).unwrap(),
        *blake3::hash(&wire).as_bytes()
    );
    assert_eq!(work.remaining_read_bytes(), 0);
    assert_eq!(memory.used(), baseline);
}
