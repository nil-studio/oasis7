//! Genuine nonempty parent and Current fixtures, using the original owner codec.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Pages, Writer, io};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits, TransitionVerifier,
    },
    incremental_state::{StateDeltaV1, StateScanBudget, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
use std::collections::BTreeMap;
struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
}
fn alliance(id: &str, members: &[&str]) -> AllianceState {
    AllianceState {
        alliance_id: id.into(),
        members: members.iter().map(|s| s.to_string()).collect(),
        charter: "charter".into(),
        formed_by_agent_id: "alice".into(),
        formed_at: 1,
    }
}
fn war(id: &str, a: &str, d: &str, active: bool) -> WarState {
    WarState {
        war_id: id.into(),
        initiator_agent_id: "alice".into(),
        aggressor_alliance_id: a.into(),
        defender_alliance_id: d.into(),
        objective: "objective".into(),
        intensity: 1,
        active,
        declared_mobilization_electricity_cost: 0,
        declared_mobilization_data_cost: 0,
        max_duration_ticks: 10,
        aggressor_score: 0,
        defender_score: 0,
        concluded_at: None,
        winner_alliance_id: None,
        loser_alliance_id: None,
        settlement_summary: None,
        participant_outcomes: vec![],
        declared_at: 1,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> DiplomacyIndexWorkV1 {
    DiplomacyIndexWorkV1 {
        cells_remaining: 100000,
        comparison_bytes_remaining: 16 << 20,
        max_rows: 64,
        max_members: 64,
        max_text_bytes: 4096,
        max_pending_rows: 32,
        pending_entries_remaining: 1000,
    }
}
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [31; 32],
        authority: AuthorityBinding {
            token: [32; 32],
            epoch: 0,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 4096,
        max_writes: 64,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
fn context<'a>(
    f: &'a Fixture,
    root: &'a StateRootV1,
    io: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root,
        state: &f.pages,
        values: ValueReadContext {
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work: keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn persist(f: &Fixture, nodes: &[oasis7_formal_protocol::incremental_state::StateNodeWriteV1]) {
    for node in nodes {
        Writer(&f.pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                f.memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
}
fn fixture(
    alliances: &BTreeMap<String, AllianceState>,
    wars: &BTreeMap<String, WarState>,
) -> Fixture {
    let memory = MemoryBudget::new(512 << 20, 64 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let mut f = Fixture {
        memory,
        pages,
        roots: RootSetV1::empty("diplomacy-index".into(), [71; 32]).unwrap(),
    };
    let _phase = f.memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::new();
    let mut writer = Writer(&f.pages);
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &f.memory,
        io: &mut io,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    for field in DiplomacyFieldV1::ALL {
        let row = crate::incremental_field_presence::write_field_metadata_v1(
            field.descriptor().unwrap(),
            &mut crate::incremental_field_presence::PresenceRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: crate::incremental_field_presence::PresenceRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        crate::incremental_field_write::emit(row.field_key, row.value_root, &mut write, &mut |r| {
            rows.push(r);
            Ok(())
        })
        .unwrap();
    }
    let mut cells = 100000;
    for (id, source) in alliances {
        let body = genesis::body(
            DiplomacySourceV1::Alliance(source),
            &mut write,
            &mut cells,
            8 << 20,
        )
        .unwrap();
        let row =
            write_diplomacy_root_reference_v1(DiplomacyFieldV1::Alliances, id, &body, &mut write)
                .unwrap();
        crate::incremental_field_write::emit(row.field_key, row.value_root, &mut write, &mut |r| {
            rows.push(r);
            Ok(())
        })
        .unwrap();
    }
    for (id, source) in wars {
        let body = genesis::body(
            DiplomacySourceV1::War(source),
            &mut write,
            &mut cells,
            8 << 20,
        )
        .unwrap();
        let row = write_diplomacy_root_reference_v1(DiplomacyFieldV1::Wars, id, &body, &mut write)
            .unwrap();
        crate::incremental_field_write::emit(row.field_key, row.value_root, &mut write, &mut |r| {
            rows.push(r);
            Ok(())
        })
        .unwrap();
    }
    let mut mutations: Vec<StateWriteV1> = rows.iter().map(|r| r.mutation.clone()).collect();
    mutations.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        mutations,
        &f.pages,
        8 << 20,
    )
    .unwrap();
    persist(&f, &delta.new_nodes);
    *f.roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap() = delta.after;
    f
}
fn qualify(f: &Fixture) -> ReservedValue<DiplomacyFamilyQualificationV1> {
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(
        f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut io,
        &mut keys,
    );
    qualify_diplomacy_family_v1(
        &f.roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 100000,
        1 << 20,
    )
    .unwrap()
}
fn observed(
    f: &Fixture,
    q: &ReservedValue<DiplomacyFamilyQualificationV1>,
    tx: &mut ExecutionTransaction<'_, Pages>,
    target: &str,
    war: bool,
    w: &mut DiplomacyIndexWorkV1,
) -> Result<ReservedValue<DiplomacyRelationObservationV1>, WorldFieldOverlayErrorV1> {
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(
        f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut io,
        &mut keys,
    );
    let live = tx.parent_fence().clone();
    let guard = tx.guard();
    if war {
        observe_current_active_wars_v1(target, q, &f.roots, tx, &mut ctx, w, &guard, &live)
    } else {
        observe_current_alliance_memberships_v1(target, q, &f.roots, tx, &mut ctx, w, &guard, &live)
    }
}
#[test]
fn genuine_diplomacy_index_preserves_outer_multimatch_and_active_wars() {
    let alliances = BTreeMap::from([
        ("z-outer".into(), alliance("embedded-z", &["alice", "bob"])),
        (
            "a-outer".into(),
            alliance("embedded-a", &["alice", "alice"]),
        ),
    ]);
    let wars = BTreeMap::from([
        (
            "ended".into(),
            war("embedded-ended", "a-outer", "z-outer", false),
        ),
        (
            "active".into(),
            war("embedded-active", "a-outer", "z-outer", true),
        ),
        (
            "both".into(),
            war("embedded-both", "a-outer", "a-outer", true),
        ),
    ]);
    let f = fixture(&alliances, &wars);
    let q = qualify(&f);
    assert_eq!(q.record_counts(), [2, 3, 0, 0]);
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [33; 32], &f.pages, &f.memory, limits())
            .unwrap();
    let mut w = work();
    for target in ["missing", "bob", "alice"] {
        let expected: Vec<_> = alliances
            .iter()
            .filter(|(_, a)| a.members.iter().any(|m| m == target))
            .map(|(id, _)| id.as_str())
            .collect();
        let got = observed(&f, &q, &mut tx, target, false, &mut w).unwrap();
        assert_eq!(got.ids().collect::<Vec<_>>(), expected);
        got.validate(&mut tx, &f.memory, &parent(&f.roots).fence().unwrap())
            .unwrap();
    }
    let expected: Vec<_> = wars
        .iter()
        .filter(|(_, war)| {
            war.active
                && (war.aggressor_alliance_id == "a-outer" || war.defender_alliance_id == "a-outer")
        })
        .map(|(id, _)| id.as_str())
        .collect();
    let got = observed(&f, &q, &mut tx, "a-outer", true, &mut w).unwrap();
    assert_eq!(got.ids().collect::<Vec<_>>(), expected);
}
#[test]
fn genuine_diplomacy_create_members_delete_current_and_verified_successor() {
    let f = fixture(
        &BTreeMap::from([("old".into(), alliance("embedded-old", &["alice"]))]),
        &BTreeMap::new(),
    );
    let q = qualify(&f);
    let p = parent(&f.roots);
    let live = p.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(p.clone(), [34; 32], &f.pages, &f.memory, limits()).unwrap();
    let old_observation = observed(&f, &q, &mut tx, "alice", false, &mut work()).unwrap();
    let created = alliance("embedded-new", &["alice", "carol"]);
    let proof = {
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut wio = crate::incremental_typed_record_test_support::io();
        let mut wkeys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut writer = Writer(&f.pages);
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &f.memory,
            io: &mut wio,
            key_work: &mut wkeys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        super::super::super::create::create_qualified_alliance_v1(
            super::super::super::create::AllianceCreateV1 {
                id: "new",
                source: &created,
                family: &q,
                max_pending_node_bytes: 8 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
            &mut 100000,
        )
        .unwrap()
    };
    assert_eq!(
        observed(&f, &q, &mut tx, "alice", false, &mut work())
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["new", "old"]
    );
    // The stale observation is checked in its own transaction below: rejecting
    // it here would correctly poison this transaction before deletion.
    {
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        super::super::super::delete::delete_qualified_alliance_v1(
            super::super::super::delete::DiplomacyClosedDeleteV1 {
                id: "new",
                qualification: &proof,
                expected: DiplomacySourceV1::Alliance(&created),
                max_current_payload_bytes: 1 << 20,
            },
            &mut overlay,
            &guard,
            &live,
            &mut 100000,
        )
        .unwrap();
    }
    assert_eq!(
        observed(&f, &q, &mut tx, "alice", false, &mut work())
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["old"]
    );
    drop(old_observation);
    // Delete an actual parent record; the verified receiver must update the
    // same relation index, rather than resurrecting its cached membership.
    let oldproof = {
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = context(
            &f,
            f.roots.state_root(StateNamespaceV1::Business),
            &mut io,
            &mut keys,
        );
        let guard = tx.guard();
        open_current_diplomacy_relation_record_v1(
            DiplomacyFieldV1::Alliances,
            "old",
            &q,
            &f.roots,
            &mut tx,
            &mut ctx,
            &mut work(),
            &guard,
            &live,
        )
        .unwrap()
        .unwrap()
    };
    {
        let expected = alliance("embedded-old", &["alice"]);
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        super::super::super::delete::delete_qualified_alliance_v1(
            super::super::super::delete::DiplomacyClosedDeleteV1 {
                id: "old",
                qualification: &oldproof,
                expected: DiplomacySourceV1::Alliance(&expected),
                max_current_payload_bytes: 1 << 20,
            },
            &mut overlay,
            &guard,
            &live,
            &mut 100000,
        )
        .unwrap();
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    persist(&f, &sealed.new_nodes);
    let after = {
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = context(
            &f,
            sealed.record.after.state_root(StateNamespaceV1::Business),
            &mut io,
            &mut keys,
        );
        let verifier = TransitionVerifier {
            parent: &p,
            operation: [34; 32],
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        super::super::super::receiver::accept_diplomacy_transition_v1(
            &q,
            &verifier,
            &sealed.record,
            &mut ctx,
            &mut 100000,
            1 << 20,
        )
        .unwrap()
    };
    assert_eq!(after.record_counts(), [0, 0, 0, 0]);
    assert_eq!(after.relations.record_count(), 0);
}

#[test]
fn current_diplomacy_relation_errors_poison_and_preserve_continuous_work() {
    for case in 0..10 {
        let f = fixture(
            &BTreeMap::from([("outer".into(), alliance("embedded", &["alice"]))]),
            &BTreeMap::new(),
        );
        let q = qualify(&f);
        let mut tx =
            ExecutionTransaction::begin(parent(&f.roots), [41; 32], &f.pages, &f.memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let mut w = work();
        let mut roots = f.roots.clone();
        if case == 0 {
            w.cells_remaining = 0;
        }
        if case == 1 {
            tx.write(StateNamespaceV1::Scheduler, b"changed", Some([3; 32]))
                .unwrap();
        }
        if case == 2 {
            roots.world_id = "foreign-root".into();
        }
        if case == 3 {
            w.comparison_bytes_remaining = 0;
        }
        if case == 4 {
            w.pending_entries_remaining = 0;
            tx.write(StateNamespaceV1::Business, b"unrelated", Some([4; 32]))
                .unwrap();
        }
        if case == 5 {
            let key = WorldFieldKeyV1::new(
                DiplomacyFieldV1::Alliances.descriptor().unwrap(),
                WorldFieldRecordKindV1::Metadata,
                None,
            )
            .unwrap();
            tx.write(StateNamespaceV1::Business, key.as_bytes(), None)
                .unwrap();
        }
        if case == 6 {
            let row = &q.relations.rows[0];
            f.pages.remove(
                blake3::Hash::from_hex(&row.child.node_root)
                    .unwrap()
                    .as_bytes(),
            );
        }
        if case == 7 {
            let row = &q.relations.rows[0];
            tx.write(
                StateNamespaceV1::Artifacts,
                row.key.as_bytes(),
                Some(row.member),
            )
            .unwrap();
        }
        let guard = if case == 1 { guard } else { tx.guard() };
        let calls = std::cell::Cell::new(0usize);
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: f.roots.state_root(StateNamespaceV1::Business),
            state: &f.pages,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || {
                if case == 8 {
                    Some(ValueTreeError::Invalid("genuine fixture source failure"))
                } else {
                    None
                }
            },
            cancelled: || {
                let n = calls.get() + 1;
                calls.set(n);
                case == 9 && n >= 10
            },
        };
        let before = w.cells_remaining;
        let result = observe_current_alliance_memberships_v1(
            "alice", &q, &roots, &mut tx, &mut ctx, &mut w, &guard, &live,
        );
        assert!(result.is_err(), "case {case}");
        assert!(w.cells_remaining <= before);
        if case == 9 {
            assert!(calls.get() >= 10);
        }
        let guard = tx.guard();
        assert!(
            tx.seal(&guard, &live, 1).is_err(),
            "case {case} must poison"
        );
    }
}
#[test]
fn current_diplomacy_observation_rejects_stale_or_foreign_transaction() {
    let f = fixture(
        &BTreeMap::from([("outer".into(), alliance("embedded", &["alice"]))]),
        &BTreeMap::new(),
    );
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [42; 32], &f.pages, &f.memory, limits())
            .unwrap();
    let live = tx.parent_fence();
    let observation = observed(&f, &q, &mut tx, "alice", false, &mut work()).unwrap();
    tx.write(StateNamespaceV1::Scheduler, b"revision", Some([5; 32]))
        .unwrap();
    assert!(observation.validate(&mut tx, &f.memory, &live).is_err());
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
    let mut foreign =
        ExecutionTransaction::begin(parent(&f.roots), [43; 32], &f.pages, &f.memory, limits())
            .unwrap();
    assert!(
        observation
            .validate(&mut foreign, &f.memory, &live)
            .is_err()
    );
    let guard = foreign.guard();
    assert!(foreign.seal(&guard, &live, 1).is_err());
}

#[allow(clippy::too_many_arguments)]
fn change_member(
    f: &Fixture,
    q: &ReservedValue<DiplomacyFamilyQualificationV1>,
    tx: &mut ExecutionTransaction<'_, Pages>,
    id: &str,
    length: u64,
    ordinal: u64,
    expected: Option<&str>,
    after: Option<&str>,
    work: &mut DiplomacyIndexWorkV1,
) {
    let live = tx.parent_fence();
    let proof = {
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = context(
            f,
            f.roots.state_root(StateNamespaceV1::Business),
            &mut io,
            &mut keys,
        );
        let guard = tx.guard();
        open_current_diplomacy_relation_record_v1(
            DiplomacyFieldV1::Alliances,
            id,
            q,
            &f.roots,
            tx,
            &mut ctx,
            work,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap()
    };
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut wio = crate::incremental_typed_record_test_support::io();
    let mut wkeys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut writer = Writer(&f.pages);
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &f.memory,
        io: &mut wio,
        key_work: &mut wkeys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let guard = overlay.transaction.guard();
    change_qualified_diplomacy_member_v1(
        DiplomacyMemberChangeV1 {
            field: DiplomacyFieldV1::Alliances,
            id,
            qualification: &proof,
            expected_length: length,
            ordinal,
            expected: expected.map(DiplomacyMemberV1::Alliance),
            after: after.map(DiplomacyMemberV1::Alliance),
            max_current_payload_bytes: 1 << 20,
            max_pending_node_bytes: 8 << 20,
        },
        &mut overlay,
        &mut write,
        &guard,
        &live,
    )
    .unwrap();
}
#[test]
fn current_diplomacy_member_inner_delta_add_replace_remove() {
    let f = fixture(
        &BTreeMap::from([
            ("a".into(), alliance("embedded-a", &["alice", "bob"])),
            ("b".into(), alliance("embedded-b", &["alice"])),
        ]),
        &BTreeMap::new(),
    );
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [45; 32], &f.pages, &f.memory, limits())
            .unwrap();
    let mut w = work();
    change_member(
        &f,
        &q,
        &mut tx,
        "a",
        2,
        0,
        Some("alice"),
        Some("carol"),
        &mut w,
    );
    assert_eq!(
        observed(&f, &q, &mut tx, "alice", false, &mut w)
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["b"]
    );
    change_member(&f, &q, &mut tx, "a", 2, 2, None, Some("alice"), &mut w);
    assert_eq!(
        observed(&f, &q, &mut tx, "alice", false, &mut w)
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    change_member(&f, &q, &mut tx, "a", 3, 2, Some("alice"), None, &mut w);
    change_member(&f, &q, &mut tx, "b", 1, 0, Some("alice"), None, &mut w);
    assert_eq!(
        observed(&f, &q, &mut tx, "alice", false, &mut w)
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        Vec::<&str>::new()
    );
    assert_eq!(
        observed(&f, &q, &mut tx, "carol", false, &mut w)
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["a"]
    );
    assert!(w.cells_remaining < 100000 && w.comparison_bytes_remaining < 16 << 20);
}

struct RecoveryReader<'a>(&'a Fixture);
struct RecoveryNode(std::sync::Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for RecoveryNode {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for RecoveryReader<'_> {
    type Node = RecoveryNode;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.0.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let node = MerkleNodeReader::read_node(&self.0.pages, id, max)?;
        node.map(|node| {
            let bytes = node.as_ref();
            let fee = self
                .0
                .memory
                .reserve(bytes.len() + 128, MemoryClass::Recovery)
                .map_err(|e| e.to_string())?;
            Ok(RecoveryNode(std::sync::Arc::new(
                fee.retain(bytes.to_vec()),
            )))
        })
        .transpose()
    }
}
impl ValueNodeReader for RecoveryReader<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(&self.0.memory) {
            return Err(ValueTreeError::Invalid("recovery fixture pool/class"));
        }
        let node =
            ValueNodeReader::read_node(&self.0.pages, id, max, memory, MemoryClass::Ordinary)?;
        let fee = memory.reserve(node.len() + 128, MemoryClass::Recovery)?;
        Ok(std::sync::Arc::new(fee.retain(node.as_slice().to_vec())))
    }
}
#[test]
fn diplomacy_real_recovery_readmission_is_paid_cancellable_and_independent() {
    let f = fixture(
        &BTreeMap::from([("outer".into(), alliance("embedded", &["alice", "bob"]))]),
        &BTreeMap::from([("war".into(), war("embedded-war", "outer", "other", true))]),
    );
    let source = RecoveryReader(&f);
    let recovery = {
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: f.roots.state_root(StateNamespaceV1::Business),
            state: &source,
            values: ValueReadContext {
                source: &source,
                memory: &f.memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        qualify_diplomacy_family_v1(
            &f.roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 100000,
            1 << 20,
        )
        .unwrap()
    };
    let before = f.memory.used();
    assert!(
        super::super::readmit_diplomacy_family_ordinary_with_work_v1(
            &recovery,
            &f.memory,
            &mut work(),
            || true
        )
        .is_err()
    );
    assert_eq!(f.memory.used(), before);
    let calls = std::cell::Cell::new(0usize);
    assert!(
        super::super::readmit_diplomacy_family_ordinary_with_work_v1(
            &recovery,
            &f.memory,
            &mut work(),
            || {
                let n = calls.get() + 1;
                calls.set(n);
                n >= 3
            }
        )
        .is_err()
    );
    assert_eq!(f.memory.used(), before);
    let mut none = work();
    none.cells_remaining = 0;
    assert!(
        super::super::readmit_diplomacy_family_ordinary_with_work_v1(
            &recovery,
            &f.memory,
            &mut none,
            || false
        )
        .is_err()
    );
    assert_eq!(f.memory.used(), before);
    let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    assert!(
        super::super::readmit_diplomacy_family_ordinary_with_work_v1(
            &recovery,
            &foreign,
            &mut work(),
            || false
        )
        .is_err()
    );
    let derived = super::super::readmit_diplomacy_family_ordinary_cancellable_v1(
        &recovery,
        &f.memory,
        || false,
    )
    .unwrap();
    assert_eq!(derived.record_counts(), recovery.record_counts());
    assert!(!std::sync::Arc::ptr_eq(
        &recovery.relations.rows[0],
        &derived.relations.rows[0]
    ));
    drop(derived);
    assert_eq!(f.memory.used(), before);
    let ordinary = super::super::readmit_diplomacy_family_ordinary_with_work_v1(
        &recovery,
        &f.memory,
        &mut work(),
        || false,
    )
    .unwrap();
    assert!(f.memory.used() > before);
    assert!(!std::sync::Arc::ptr_eq(
        &recovery.relations.rows[0],
        &ordinary.relations.rows[0]
    ));
    assert!(!std::sync::Arc::ptr_eq(
        &recovery.relations.rows[0].outer,
        &ordinary.relations.rows[0].outer
    ));
    drop(recovery);
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [46; 32], &f.pages, &f.memory, limits())
            .unwrap();
    assert_eq!(
        observed(&f, &ordinary, &mut tx, "alice", false, &mut work())
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["outer"]
    );
    assert!(
        super::super::readmit_diplomacy_family_ordinary_with_work_v1(
            &ordinary,
            &f.memory,
            &mut work(),
            || false
        )
        .is_err()
    );
}

#[test]
fn empty_diplomacy_index_and_foreign_pool_observation_fail_closed() {
    let f = fixture(&BTreeMap::new(), &BTreeMap::new());
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [47; 32], &f.pages, &f.memory, limits())
            .unwrap();
    let result = observed(&f, &q, &mut tx, "missing", false, &mut work()).unwrap();
    assert_eq!(result.ids().len(), 0);
    let foreign = fixture(&BTreeMap::new(), &BTreeMap::new());
    let foreign_q = qualify(&foreign);
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [48; 32], &f.pages, &f.memory, limits())
            .unwrap();
    let live = tx.parent_fence();
    assert!(observed(&f, &foreign_q, &mut tx, "missing", false, &mut work()).is_err());
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}

#[path = "incremental_diplomacy_index_safety_tests.rs"]
mod safety;
