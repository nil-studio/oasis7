use super::*;
use crate::incremental_world_event_decode::{
    WorldEventDecodeWorkV1, decode_world_event_original_v1,
};
use crate::{DomainEvent, WorldEvent};
use oasis7_formal_protocol::{
    controlled_authority::formal::{self, test_fixtures::incremental},
    incremental_material::promote_candidate_event_fact_v1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
fn canonical() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 32 << 20,
        max_depth: 64,
        max_map_keys: 1024,
        max_key_bytes: 4096,
    }
}
fn work() -> MailboxWorldEventOriginalWorkV1 {
    MailboxWorldEventOriginalWorkV1 {
        objects_remaining: 10,
        original_bytes_remaining: 16 << 20,
        max_original_bytes: 1 << 20,
        canonical: canonical(),
    }
}
fn decode(event: &WorldEvent, memory: &MemoryBudget) -> Arc<ReservedValue<DecodedWorldEventV1>> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(event).unwrap();
    Arc::new(
        decode_world_event_original_v1(
            &bytes,
            memory,
            MemoryClass::Ordinary,
            &mut WorldEventDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: 16 << 20,
                    values_remaining: 100_000,
                    max_depth: 64,
                },
                canonical: canonical(),
                max_original_bytes: 1 << 20,
                max_heap_bytes: 32 << 20,
            },
            || false,
        )
        .unwrap(),
    )
}
fn event() -> WorldEvent {
    WorldEvent {
        id: 17,
        time: 29,
        caused_by: Some(crate::CausedBy::Action(7)),
        body: WorldEventBody::Domain(DomainEvent::BodyAttributesRejected {
            agent_id: "original agent".into(),
            reason: "original reason 雪".into(),
        }),
    }
}
#[test]
fn mailbox_requires_real_accepted_world_event_and_preserves_envelope_identity() {
    let memory = MemoryBudget::new(128 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let event = event();
    let signed =
        crate::incremental_domain_event_proof::test_support::candidate_provenance(&event, &memory);
    let accepted =
        formal::verify_formal_qualified(&signed.proof, &signed.authority, None, None).unwrap();
    let roots = incremental::setup_with_memory(memory.clone()).roots;
    let scope = MailboxProvenanceScopeV1::Accepted {
        head: &accepted,
        roots: &roots,
    };
    let raw = promote_candidate_event_fact_v1(&signed.raw, &accepted, &memory, || false).unwrap();
    let id = raw.location().original;
    let mut budget = work();
    let before = budget.canonical.visits_remaining;
    let token = verify_mailbox_world_event_original_v1::<std::convert::Infallible>(
        decode(&event, &memory),
        MailboxWorldEventOccurrenceV1::Accepted(raw),
        &scope,
        &memory,
        MemoryClass::Ordinary,
        &mut budget,
        || false,
    )
    .unwrap();
    assert_eq!(token.event(), &event);
    assert_eq!(token.event().caused_by, Some(crate::CausedBy::Action(7)));
    assert_eq!(token.original_id(), id);
    assert!(budget.canonical.visits_remaining < before);
    CheckedMailboxWorldEventOriginalV1::validate(
        &token,
        &id,
        scope.anchor(&memory, MemoryClass::Ordinary).unwrap(),
        &memory,
        MemoryClass::Ordinary,
    )
    .unwrap();
    assert!(
        CheckedMailboxWorldEventOriginalV1::validate(
            &token,
            &[9; 32],
            scope.anchor(&memory, MemoryClass::Ordinary).unwrap(),
            &memory,
            MemoryClass::Ordinary
        )
        .is_err()
    );
    drop(token);
    let mut changed = event.clone();
    changed.caused_by = None;
    let raw = promote_candidate_event_fact_v1(&signed.raw, &accepted, &memory, || false).unwrap();
    let mut budget = work();
    let before = budget.canonical.visits_remaining;
    assert!(
        verify_mailbox_world_event_original_v1::<std::convert::Infallible>(
            decode(&changed, &memory),
            MailboxWorldEventOccurrenceV1::Accepted(raw),
            &scope,
            &memory,
            MemoryClass::Ordinary,
            &mut budget,
            || false
        )
        .is_err()
    );
    assert!(
        budget.canonical.visits_remaining < before,
        "failed semantic work must remain consumed"
    );
    let raw = promote_candidate_event_fact_v1(&signed.raw, &accepted, &memory, || false).unwrap();
    let mut limited = work();
    limited.original_bytes_remaining = 0;
    assert!(
        verify_mailbox_world_event_original_v1::<std::convert::Infallible>(
            decode(&event, &memory),
            MailboxWorldEventOccurrenceV1::Accepted(raw),
            &scope,
            &memory,
            MemoryClass::Ordinary,
            &mut limited,
            || false
        )
        .is_err()
    );
}
#[test]
fn mailbox_world_event_rejects_another_genuine_accepted_record_and_cancellation() {
    let memory = MemoryBudget::new(128 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let event = event();
    let first =
        crate::incremental_domain_event_proof::test_support::candidate_provenance(&event, &memory);
    let accepted =
        formal::verify_formal_qualified(&first.proof, &first.authority, None, None).unwrap();
    let mut another = event.clone();
    another.time += 1;
    let second = crate::incremental_domain_event_proof::test_support::candidate_provenance(
        &another, &memory,
    );
    let different =
        formal::verify_formal_qualified(&second.proof, &second.authority, None, None).unwrap();
    assert_ne!(
        accepted.verified_record_id(),
        different.verified_record_id()
    );
    let roots = incremental::setup_with_memory(memory.clone()).roots;
    let raw = promote_candidate_event_fact_v1(&first.raw, &accepted, &memory, || false).unwrap();
    assert!(
        verify_mailbox_world_event_original_v1::<std::convert::Infallible>(
            decode(&event, &memory),
            MailboxWorldEventOccurrenceV1::Accepted(raw),
            &MailboxProvenanceScopeV1::Accepted {
                head: &different,
                roots: &roots
            },
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        )
        .is_err()
    );
    let raw = promote_candidate_event_fact_v1(&first.raw, &accepted, &memory, || false).unwrap();
    assert!(
        verify_mailbox_world_event_original_v1::<std::convert::Infallible>(
            decode(&event, &memory),
            MailboxWorldEventOccurrenceV1::Accepted(raw),
            &MailboxProvenanceScopeV1::Accepted {
                head: &accepted,
                roots: &roots
            },
            &memory,
            MemoryClass::Ordinary,
            &mut work(),
            || true
        )
        .is_err()
    );
}
