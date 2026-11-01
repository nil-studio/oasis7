use super::test_support::provenance;
use super::*;
use std::cell::Cell;

fn event() -> WorldEvent {
    WorldEvent {
        id: u64::MAX,
        time: 91,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::ActionAccepted {
            action_id: 23,
            action_kind: " raw/界\0 ".into(),
            actor_id: "actor".into(),
            eta_ticks: u64::MAX,
            notes: vec!["".into(), "same".into(), "same".into()],
        }),
    }
}
fn work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 1 << 20,
        max_depth: 64,
        max_map_keys: 32,
        max_key_bytes: 128,
    }
}
fn request<'a>(
    event: &'a WorldEvent,
    raw: &'a ReservedValue<VerifiedEventFactOccurrenceV1>,
) -> DomainEventProofRequestV1<'a> {
    DomainEventProofRequestV1 {
        event,
        occurrence: raw,
        expected_head_record: raw.head_record_id(),
        expected_roots: raw.roots_id(),
    }
}

#[test]
fn concrete_domain_matches_signed_history_and_keeps_original_borrow_and_charge() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = provenance(&event, &memory);
    let baseline = memory.used();
    let mut ctx = DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    let proof = verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx).unwrap();
    assert!(std::ptr::eq(proof.event(), &event));
    assert_eq!(
        proof.domain(),
        match &event.body {
            WorldEventBody::Domain(d) => d,
            _ => unreachable!(),
        }
    );
    assert_eq!(proof.occurrence().location(), raw.location());
    assert!(proof.uses_budget(&memory));
    assert!(!ctx.is_poisoned());
    assert!(ctx.work.as_ref().unwrap().visits_remaining < 1_000_000);
    assert_eq!(memory.used(), baseline + proof.reserved_bytes());
    drop(proof);
    assert_eq!(memory.used(), baseline);
}

#[test]
fn current_fact_observations_feed_the_same_concrete_domain_and_mailbox_consumer() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = test_support::current_provenance(&event, &memory);
    let mut context =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    let proof = verify_domain_event_occurrence_v1(&request(&event, &raw), &mut context).unwrap();
    let id = verified_mailbox_domain_original_v1(
        &MailboxDomainProofRequestV1 {
            domain: proof.domain(),
            proof: &proof,
            expected_original: raw.location().original,
            expected_head_record: raw.head_record_id(),
            expected_roots: raw.roots_id(),
        },
        &mut context,
    )
    .unwrap();
    assert_eq!(
        id,
        *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap()).as_bytes()
    );
    assert!(!context.is_poisoned());
}

#[test]
fn domain_identity_type_pool_and_same_length_content_mismatches_poison() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = provenance(&event, &memory);
    for mode in 0..5 {
        let mut different = event.clone();
        let mut req = request(&different, &raw);
        match mode {
            0 => req.expected_head_record[0] ^= 1,
            1 => req.expected_roots[0] ^= 1,
            _ => {}
        }
        // IDs/time are part of the WorldEvent original, even with the same Domain.
        if mode == 2 {
            different.id ^= 1;
            req = request(&different, &raw);
        }
        if mode == 3 {
            if let WorldEventBody::Domain(DomainEvent::ActionAccepted { actor_id, .. }) =
                &mut different.body
            {
                *actor_id = "Actor".into();
            }
            req = request(&different, &raw);
        }
        let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let budget = if mode == 4 { &foreign } else { &memory };
        let mut ctx =
            DomainEventProofContextV1::new(budget, MemoryClass::Ordinary, work(), || false);
        assert!(verify_domain_event_occurrence_v1(&req, &mut ctx).is_err());
        assert!(ctx.is_poisoned());
        assert!(matches!(
            verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx),
            Err(CanonicalCborStreamError::Poisoned)
        ));
    }
}

#[test]
fn domain_continuous_allowances_and_final_cancel_cannot_mint_a_witness() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = provenance(&event, &memory);
    let calls = Cell::new(0u64);
    let mut ctx = DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || {
        calls.set(calls.get() + 1);
        false
    });
    drop(verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx).unwrap());
    let n = calls.get();
    assert!(n > 5);
    let remaining = ctx.work.as_ref().unwrap().visits_remaining;
    let used = 1_000_000 - remaining;
    let mut limited = work();
    limited.visits_remaining = used;
    let mut ctx = DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, limited, || false);
    drop(verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx).unwrap());
    assert!(matches!(
        verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx),
        Err(CanonicalCborStreamError::ResourceLimited(_))
    ));
    assert!(ctx.is_poisoned());
    for stop in [1, n - 1, n] {
        calls.set(0);
        let mut ctx =
            DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || {
                calls.set(calls.get() + 1);
                calls.get() >= stop
            });
        assert!(matches!(
            verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx),
            Err(CanonicalCborStreamError::Cancelled)
        ));
        assert!(ctx.is_poisoned());
    }
}

#[test]
fn authenticated_non_domain_original_is_not_a_domain_witness() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let mut event = event();
    event.body = WorldEventBody::ModuleEmitted(oasis7_wasm_abi::ModuleEmitEvent {
        module_id: "module".into(),
        trace_id: "trace".into(),
        kind: "Domain".into(),
        payload: serde_json::json!({"type":"ActionAccepted","data":{"actor_id":"actor"}}),
    });
    let raw = provenance(&event, &memory);
    let mut ctx = DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx),
        Err(CanonicalCborStreamError::Invalid(
            "Event original is not Domain"
        ))
    ));
    assert!(ctx.is_poisoned());
}

#[test]
fn concrete_domain_exhaustion_leaks_no_charge_or_reusable_context() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = provenance(&event, &memory);
    let baseline = memory.used();
    for mode in 0..6 {
        let mut limits = work();
        match mode {
            0 => limits.bytes_remaining = 0,
            1 => limits.max_depth = 0,
            2 => limits.max_map_keys = 0,
            3 => limits.max_key_bytes = 0,
            4 => limits.visits_remaining = 0,
            _ => (),
        }
        let held = if mode == 5 {
            Some(
                memory
                    .reserve((4 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap(),
            )
        } else {
            None
        };
        let used = memory.used();
        let mut ctx =
            DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, limits, || false);
        assert!(
            matches!(
                verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx),
                Err(CanonicalCborStreamError::ResourceLimited(_))
            ),
            "mode={mode}"
        );
        assert!(ctx.is_poisoned());
        assert_eq!(memory.used(), used);
        drop(held);
        assert_eq!(memory.used(), baseline);
    }
}

#[test]
fn mailbox_requires_same_domain_original_and_head_and_spends_continuous_work() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = provenance(&event, &memory);
    let mut ctx = DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    let proof = verify_domain_event_occurrence_v1(&request(&event, &raw), &mut ctx).unwrap();
    let before = ctx.work.as_ref().unwrap().visits_remaining;
    let req = MailboxDomainProofRequestV1 {
        domain: proof.domain(),
        proof: &proof,
        expected_original: raw.location().original,
        expected_head_record: raw.head_record_id(),
        expected_roots: raw.roots_id(),
    };
    assert_eq!(
        verified_mailbox_domain_original_v1(&req, &mut ctx).unwrap(),
        raw.location().original
    );
    assert!(ctx.work.as_ref().unwrap().visits_remaining < before);
    for mode in 0..5 {
        let changed = DomainEvent::ActionAccepted {
            action_id: 23,
            action_kind: " raw/界\0 ".into(),
            actor_id: "Actor".into(),
            eta_ticks: u64::MAX,
            notes: vec!["".into(), "same".into(), "same".into()],
        };
        let mut req = MailboxDomainProofRequestV1 {
            domain: proof.domain(),
            proof: &proof,
            expected_original: raw.location().original,
            expected_head_record: raw.head_record_id(),
            expected_roots: raw.roots_id(),
        };
        match mode {
            0 => req.domain = &changed,
            1 => req.expected_original[0] ^= 1,
            2 => req.expected_head_record[0] ^= 1,
            3 => req.expected_roots[0] ^= 1,
            _ => (),
        }
        let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let mut ctx = DomainEventProofContextV1::new(
            if mode == 4 { &foreign } else { &memory },
            MemoryClass::Ordinary,
            work(),
            || false,
        );
        assert!(matches!(
            verified_mailbox_domain_original_v1(&req, &mut ctx),
            Err(CanonicalCborStreamError::Invalid(_))
        ));
        assert!(ctx.is_poisoned());
    }
}

#[test]
fn mailbox_mutation_cancel_is_checked_during_matching_and_at_final_boundary() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = provenance(&event, &memory);
    let mut source =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    let proof = verify_domain_event_occurrence_v1(&request(&event, &raw), &mut source).unwrap();
    let req = MailboxDomainProofRequestV1 {
        domain: proof.domain(),
        proof: &proof,
        expected_original: raw.location().original,
        expected_head_record: raw.head_record_id(),
        expected_roots: raw.roots_id(),
    };
    let calls = Cell::new(0u64);
    let mut ctx = DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    verified_mailbox_domain_original_with_cancel_v1(&req, &mut ctx, &mut || {
        calls.set(calls.get() + 1);
        false
    })
    .unwrap();
    let n = calls.get();
    assert!(n > 20);
    for stop in [1, 20, n] {
        calls.set(0);
        let mut ctx =
            DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
        assert!(matches!(
            verified_mailbox_domain_original_with_cancel_v1(&req, &mut ctx, &mut || {
                calls.set(calls.get() + 1);
                calls.get() >= stop
            }),
            Err(CanonicalCborStreamError::Cancelled)
        ));
        assert!(ctx.is_poisoned());
    }
}

#[test]
fn domain_semantics_can_use_recovery_reserve_without_ordinary_headroom() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let raw = provenance(&event, &memory);
    let ordinary = memory
        .reserve((4 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    let mut denied =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        verify_domain_event_occurrence_v1(&request(&event, &raw), &mut denied),
        Err(CanonicalCborStreamError::ResourceLimited(_))
    ));
    let mut recovery =
        DomainEventProofContextV1::new(&memory, MemoryClass::Recovery, work(), || false);
    let proof = verify_domain_event_occurrence_v1(&request(&event, &raw), &mut recovery).unwrap();
    assert_eq!(proof.occurrence().location(), raw.location());
    drop(proof);
    drop(ordinary);
}
