//! Real original field qualification/Current points; no fabricated native Q.
//! Full native23 aggregation is exercised separately by the Runtime GEN fixture.
use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_codec::TypedRecordWriteWorkV1;
use family::WorldNativeFieldReadWorkV1;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionError, ExecutionParent, TransactionLimits,
};
#[path = "incremental_world_native_family_test_support.rs"]
mod support;
use support::*;
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [61; 32],
        authority: AuthorityBinding {
            token: [62; 32],
            epoch: 0,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 256,
        max_writes: 16,
        max_facts: 4,
        max_pending_node_bytes: 8 << 20,
    }
}
fn mw() -> MetaProgressMutationWorkV1 {
    MetaProgressMutationWorkV1 {
        steps_remaining: 100_000,
        comparison_bytes_remaining: 32 << 20,
        max_entries: 1024,
        max_text_bytes: 4096,
    }
}
fn initial() -> crate::MetaProgressState {
    crate::MetaProgressState {
        agent_id: "embedded-other".into(),
        track_points: std::collections::BTreeMap::from([
            ("science".into(), 19),
            ("other".into(), 99),
        ]),
        total_points: 7,
        achievements: vec!["z".into(), "a".into(), "a".into()],
        unlocked_tiers: std::collections::BTreeMap::from([("other".into(), vec!["bronze".into()])]),
        last_granted_at: 3,
    }
}
#[test]
fn meta_current_create_update_preserves_outer_identity_and_requires_fresh_source() {
    for existing in [false, true] {
        let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
        let pages = Pages::with_limit(&memory, 32768);
        let unrelated = initial();
        let mut entries = vec![("unrelated".into(), unrelated.clone())];
        if existing {
            entries.push(("target".into(), initial()));
        }
        let source = rows::<MetaProgressFieldV1>(&memory, &pages, &entries);
        let ns = MetaProgressFieldV1::profile().unwrap().namespace().unwrap();
        let roots = install(&memory, &pages, ns, &source);
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = context(&memory, &pages, roots.state_root(ns), &mut quota, &mut keys);
        let q = family::qualify_world_native_field_v1::<MetaProgressFieldV1>(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut work(),
        )
        .unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent(&roots), [63; 32], &pages, &memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let (observed, fee) = read_current(
            "target",
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &guard,
            &live,
        )
        .unwrap();
        let observed = fee.retain(observed);
        assert_eq!(observed.value(), existing.then(initial).as_ref());
        let mut mutation = mw();
        let prepared = prepare_field_delta(
            &observed,
            "science",
            2,
            Some("a"),
            7,
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &mut mutation,
            &live,
        )
        .unwrap();
        // An actual unrelated transaction read changes revision; an Accepted
        // event later uses this same contract, but this data test mints no audit.
        tx.read(StateNamespaceV1::Scheduler, b"unrelated-read")
            .unwrap();
        let guard = tx.guard();
        let (current, fee) = read_current(
            "target",
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &guard,
            &live,
        )
        .unwrap();
        let current = fee.retain(current);
        {
            let mut writer = Writer(&pages);
            let mut output_io = io();
            let mut output_keys = SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            };
            let mut output = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut output_io,
                key_work: &mut output_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            };
            write_prepared_current_meta_progress_v1(
                &prepared,
                &current,
                &mut tx,
                &mut ctx,
                &mut output,
                &mut 8,
                &mut write_work(),
                &mut mutation,
                &live,
            )
            .unwrap();
        }
        let guard = tx.guard();
        let (actual, fee) = read_current(
            "target",
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &guard,
            &live,
        )
        .unwrap();
        let actual = fee.retain(actual);
        let expected = if existing {
            crate::MetaProgressState {
                agent_id: "embedded-other".into(),
                track_points: std::collections::BTreeMap::from([
                    ("science".into(), 21),
                    ("other".into(), 99),
                ]),
                total_points: 9,
                achievements: vec!["a".into(), "tier.science.bronze".into(), "z".into()],
                unlocked_tiers: std::collections::BTreeMap::from([
                    ("other".into(), vec!["bronze".into()]),
                    ("science".into(), vec!["bronze".into()]),
                ]),
                last_granted_at: 7,
            }
        } else {
            crate::MetaProgressState {
                agent_id: "target".into(),
                track_points: std::collections::BTreeMap::from([("science".into(), 2)]),
                total_points: 2,
                achievements: vec!["a".into()],
                unlocked_tiers: std::collections::BTreeMap::new(),
                last_granted_at: 7,
            }
        };
        assert_eq!(actual.value(), Some(&expected));
        let guard = tx.guard();
        let (other, fee) = read_current(
            "unrelated",
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &guard,
            &live,
        )
        .unwrap();
        let other = fee.retain(other);
        assert_eq!(other.value(), Some(&unrelated));
        // Reusing an earlier source after a real mutation must fail and poison,
        // even though the prepared DTO remains memory-safe and fully charged.
        assert!(current.validate(&mut tx, &memory, &live).is_err());
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
#[test]
fn meta_current_continuous_mutation_work_exhaustion_aborts_without_publication() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let source = rows::<MetaProgressFieldV1>(&memory, &pages, &[("target".into(), initial())]);
    let ns = MetaProgressFieldV1::profile().unwrap().namespace().unwrap();
    let roots = install(&memory, &pages, ns, &source);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(&memory, &pages, roots.state_root(ns), &mut quota, &mut keys);
    let q = family::qualify_world_native_field_v1::<MetaProgressFieldV1>(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent(&roots), [64; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let (current, fee) = read_current(
        "target",
        &q,
        &roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut work().typed,
        &guard,
        &live,
    )
    .unwrap();
    let current = fee.retain(current);
    let mut mutation = mw();
    mutation.steps_remaining = 0;
    assert!(matches!(
        prepare_field_delta(
            &current,
            "science",
            100,
            Some("earned"),
            9,
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &mut mutation,
            &live
        ),
        Err(WorldFieldOverlayErrorV1::Value(
            ValueTreeError::ResourceLimited(_)
        ))
    ));
    assert_eq!(mutation.steps_remaining, 0);
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn meta_current_cancelled_source_aborts_at_the_real_current_boundary() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let source = rows::<MetaProgressFieldV1>(&memory, &pages, &[("target".into(), initial())]);
    let ns = MetaProgressFieldV1::profile().unwrap().namespace().unwrap();
    let roots = install(&memory, &pages, ns, &source);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(&memory, &pages, roots.state_root(ns), &mut quota, &mut keys);
    let q = family::qualify_world_native_field_v1::<MetaProgressFieldV1>(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    drop(ctx);
    let mut tx =
        ExecutionTransaction::begin(parent(&roots), [65; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut ctx = WorldFieldReadContextV1 {
        root: roots.state_root(ns),
        state: &pages,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 1 << 20,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || true,
    };
    let result = tx.contribute(|tx| {
        read_current(
            "target",
            &q,
            &roots,
            tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &guard,
            &live,
        )
    });
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
    ));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}

#[test]
fn meta_generated_achievement_respects_same_text_cap_before_any_output() {
    for cap in [27, 28] {
        let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
        let pages = Pages::with_limit(&memory, 32768);
        let source = rows::<MetaProgressFieldV1>(&memory, &pages, &[]);
        let ns = MetaProgressFieldV1::profile().unwrap().namespace().unwrap();
        let roots = install(&memory, &pages, ns, &source);
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = context(&memory, &pages, roots.state_root(ns), &mut quota, &mut keys);
        let q = family::qualify_world_native_field_v1::<MetaProgressFieldV1>(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut work(),
        )
        .unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent(&roots), [66; 32], &pages, &memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let (current, fee) = read_current(
            "target",
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &guard,
            &live,
        )
        .unwrap();
        let current = fee.retain(current);
        let mut mutation = mw();
        mutation.max_text_bytes = cap;
        let count = pages.len();
        let track = "x".repeat(16);
        let result = prepare_field_delta(
            &current,
            &track,
            20,
            None,
            7,
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &mut mutation,
            &live,
        );
        assert_eq!(
            pages.len(),
            count,
            "preparation must not encode or publish any node"
        );
        if cap == 27 {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(
                    ValueTreeError::ResourceLimited("meta generated identifier bytes")
                ))
            ));
            assert_eq!(tx.ordered_fact_count(), 0);
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
        } else {
            let prepared = result.unwrap();
            assert_eq!(
                prepared.value().achievements,
                [format!("tier.{track}.bronze")]
            );
            let guard = tx.guard();
            let (fresh, fee) = read_current(
                "target",
                &q,
                &roots,
                &mut tx,
                &mut ctx,
                &mut scan(),
                &mut work().typed,
                &guard,
                &live,
            )
            .unwrap();
            let fresh = fee.retain(fresh);
            let mut writer = Writer(&pages);
            let mut output_io = io();
            let mut output_keys = SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            };
            let mut output = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut output_io,
                key_work: &mut output_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 2 << 20,
                    record_bytes: 3 << 20,
                },
                cancelled: || false,
            };
            write_prepared_current_meta_progress_v1(
                &prepared,
                &fresh,
                &mut tx,
                &mut ctx,
                &mut output,
                &mut 8,
                &mut write_work(),
                &mut mutation,
                &live,
            )
            .unwrap();
            let guard = tx.guard();
            let (actual, fee) = read_current(
                "target",
                &q,
                &roots,
                &mut tx,
                &mut ctx,
                &mut scan(),
                &mut work().typed,
                &guard,
                &live,
            )
            .unwrap();
            let actual = fee.retain(actual);
            assert_eq!(actual.value(), Some(prepared.value()));
            // The same cap can read and prepare the state it just produced.
            prepare_field_delta(
                &actual,
                &track,
                -1,
                None,
                8,
                &q,
                &roots,
                &mut tx,
                &mut ctx,
                &mut scan(),
                &mut work().typed,
                &mut mutation,
                &live,
            )
            .unwrap();
        }
    }
}
#[test]
fn meta_kernel_late_work_exhaustion_discards_private_partial_dto_and_aborts() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let source = rows::<MetaProgressFieldV1>(&memory, &pages, &[("target".into(), initial())]);
    let ns = MetaProgressFieldV1::profile().unwrap().namespace().unwrap();
    let roots = install(&memory, &pages, ns, &source);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(&memory, &pages, roots.state_root(ns), &mut quota, &mut keys);
    let q = family::qualify_world_native_field_v1::<MetaProgressFieldV1>(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let mut measured = None;
    for attempt in 0..2 {
        let mut tx = ExecutionTransaction::begin(
            parent(&roots),
            [67 + attempt; 32],
            &pages,
            &memory,
            limits(),
        )
        .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let (current, fee) = read_current(
            "target",
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &guard,
            &live,
        )
        .unwrap();
        let current = fee.retain(current);
        let mut mutation = mw();
        if let Some(used) = measured {
            mutation.steps_remaining = used - 1;
        }
        let before = mutation.steps_remaining;
        let count = pages.len();
        let result = prepare_field_delta(
            &current,
            "science",
            100,
            Some("earned"),
            9,
            &q,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work().typed,
            &mut mutation,
            &live,
        );
        assert_eq!(pages.len(), count);
        assert_eq!(tx.ordered_fact_count(), 0);
        if attempt == 0 {
            result.unwrap();
            let used = before - mutation.steps_remaining;
            assert!(used > 1);
            measured = Some(used);
        } else {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(
                    ValueTreeError::ResourceLimited(_)
                ))
            ));
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
        }
    }
}
