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
fn candidate_domain_is_checked_before_acceptance_and_only_exact_head_promotes() {
    use oasis7_formal_protocol::{
        controlled_authority::formal, incremental_material::promote_candidate_event_fact_v1,
    };
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    // No verify_formal_qualified has run on this Event's candidate record.
    let pending = test_support::candidate_provenance(&event, &memory);
    let req = CandidateDomainEventProofRequestV1 {
        event: &event,
        occurrence: &pending.raw,
        expected_candidate_record: pending.raw.candidate_record_id(),
        expected_roots: pending.raw.roots_id(),
    };
    let mut context =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    let checked = check_candidate_domain_event_occurrence_v1(&req, &mut context).unwrap();
    assert!(std::ptr::eq(checked.event(), &event));
    assert_eq!(
        checked.occurrence().candidate_record_id(),
        pending.raw.candidate_record_id()
    );
    // Acceptance and promotion happen only after candidate semantic verification.
    let head =
        formal::verify_formal_qualified(&pending.proof, &pending.authority, None, None).unwrap();
    let accepted_raw =
        promote_candidate_event_fact_v1(&pending.raw, &head, &memory, || false).unwrap();
    let promote = CandidateDomainPromotionRequestV1 {
        checked: &checked,
        accepted_occurrence: &accepted_raw,
        expected_head_record: accepted_raw.head_record_id(),
        expected_roots: accepted_raw.roots_id(),
    };
    context.work.as_mut().unwrap().bytes_remaining = 0;
    context.work.as_mut().unwrap().visits_remaining = 1;
    let baseline = memory.used();
    let accepted = promote_candidate_domain_event_occurrence_v1(&promote, &mut context).unwrap();
    assert!(std::ptr::eq(accepted.event(), &event));
    assert_eq!(
        accepted.occurrence().head_record_id(),
        pending.raw.candidate_record_id()
    );
    assert_eq!(context.work.as_ref().unwrap().visits_remaining, 0);
    assert_eq!(context.work.as_ref().unwrap().bytes_remaining, 0);
    assert_eq!(memory.used(), baseline + accepted.reserved_bytes());
    let mut mailbox =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert_eq!(
        verified_mailbox_domain_original_v1(
            &MailboxDomainProofRequestV1 {
                domain: checked.domain(),
                proof: &accepted,
                expected_original: pending.raw.location().original,
                expected_head_record: accepted_raw.head_record_id(),
                expected_roots: accepted_raw.roots_id(),
            },
            &mut mailbox
        )
        .unwrap(),
        pending.raw.location().original
    );
    drop(accepted);
    assert_eq!(memory.used(), baseline);
    assert!(matches!(
        promote_candidate_domain_event_occurrence_v1(&promote, &mut context),
        Err(CanonicalCborStreamError::ResourceLimited(_))
    ));
    assert!(context.is_poisoned());

    let mut changed = event.clone();
    changed.id -= 1;
    let mut wrong_bytes =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        check_candidate_domain_event_occurrence_v1(
            &CandidateDomainEventProofRequestV1 {
                event: &changed,
                ..req
            },
            &mut wrong_bytes
        ),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
    assert!(wrong_bytes.is_poisoned());
    let mut wrong_record =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        check_candidate_domain_event_occurrence_v1(
            &CandidateDomainEventProofRequestV1 {
                expected_candidate_record: [91; 32],
                ..req
            },
            &mut wrong_record
        ),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let mut wrong_pool =
        DomainEventProofContextV1::new(&foreign, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        check_candidate_domain_event_occurrence_v1(&req, &mut wrong_pool),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
    let mut wrong_pool =
        DomainEventProofContextV1::new(&foreign, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        promote_candidate_domain_event_occurrence_v1(&promote, &mut wrong_pool),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
    let other_raw = test_support::inherited_provenance(&event, &memory);
    let mut other_head =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        promote_candidate_domain_event_occurrence_v1(
            &CandidateDomainPromotionRequestV1 {
                checked: &checked,
                accepted_occurrence: &other_raw,
                expected_head_record: other_raw.head_record_id(),
                expected_roots: other_raw.roots_id(),
            },
            &mut other_head
        ),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
    for final_boundary in [false, true] {
        let mut calls = 0;
        let mut cancel =
            DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || {
                calls += 1;
                calls == if final_boundary { 2 } else { 1 }
            });
        let used = memory.used();
        assert!(matches!(
            promote_candidate_domain_event_occurrence_v1(&promote, &mut cancel),
            Err(CanonicalCborStreamError::Cancelled)
        ));
        assert!(cancel.is_poisoned());
        assert_eq!(memory.used(), used);
    }
}

#[test]
fn inherited_domain_reuses_exact_immutable_semantics_across_signed_successor() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let event = event();
    let old = provenance(&event, &memory);
    let next = test_support::inherited_provenance(&event, &memory);
    assert_ne!(old.head_record_id(), next.head_record_id());
    assert_ne!(old.roots_id(), next.roots_id());
    assert_ne!(old.head_body_id(), next.head_body_id());
    assert_eq!(old.event_body_id(), next.event_body_id());
    assert_eq!(old.location(), next.location());
    let mut context =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    let proof = verify_domain_event_occurrence_v1(&request(&event, &old), &mut context).unwrap();
    let inheritance = DomainEventInheritanceRequestV1 {
        previous: &proof,
        occurrence: &next,
        expected_head_record: next.head_record_id(),
        expected_roots: next.roots_id(),
    };
    // Zero bytes and exactly one metadata visit cannot re-encode the event.
    context.work.as_mut().unwrap().visits_remaining = 1;
    context.work.as_mut().unwrap().bytes_remaining = 0;
    let baseline = memory.used();
    let inherited = inherit_domain_event_occurrence_v1(&inheritance, &mut context).unwrap();
    assert!(std::ptr::eq(inherited.event(), &event));
    assert_eq!(
        inherited.occurrence().head_record_id(),
        next.head_record_id()
    );
    assert_eq!(inherited.domain(), proof.domain());
    assert_eq!(memory.used(), baseline + inherited.reserved_bytes());
    assert_eq!(context.work.as_ref().unwrap().visits_remaining, 0);
    assert_eq!(context.work.as_ref().unwrap().bytes_remaining, 0);
    drop(inherited);
    assert_eq!(memory.used(), baseline);
    assert!(matches!(
        inherit_domain_event_occurrence_v1(&inheritance, &mut context),
        Err(CanonicalCborStreamError::ResourceLimited(
            "Domain inheritance work"
        ))
    ));
    assert!(context.is_poisoned());
    assert!(matches!(
        inherit_domain_event_occurrence_v1(&inheritance, &mut context),
        Err(CanonicalCborStreamError::Poisoned)
    ));

    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let mut wrong_pool =
        DomainEventProofContextV1::new(&foreign, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        inherit_domain_event_occurrence_v1(&inheritance, &mut wrong_pool),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
    for final_boundary in [false, true] {
        let mut calls = 0;
        let mut cancelled =
            DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || {
                calls += 1;
                calls == if final_boundary { 2 } else { 1 }
            });
        let used = memory.used();
        assert!(matches!(
            inherit_domain_event_occurrence_v1(&inheritance, &mut cancelled),
            Err(CanonicalCborStreamError::Cancelled)
        ));
        assert!(cancelled.is_poisoned());
        assert_eq!(memory.used(), used);
    }
    let mut wrong_head =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        inherit_domain_event_occurrence_v1(
            &DomainEventInheritanceRequestV1 {
                expected_head_record: old.head_record_id(),
                ..inheritance
            },
            &mut wrong_head
        ),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
    let mut changed = event.clone();
    changed.id -= 1;
    let wrong = test_support::inherited_provenance(&changed, &memory);
    let mut mismatch =
        DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
    assert!(matches!(
        inherit_domain_event_occurrence_v1(
            &DomainEventInheritanceRequestV1 {
                previous: &proof,
                occurrence: &wrong,
                expected_head_record: wrong.head_record_id(),
                expected_roots: wrong.roots_id(),
            },
            &mut mismatch
        ),
        Err(CanonicalCborStreamError::Invalid(_))
    ));
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
