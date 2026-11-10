use super::*;
fn create(
    f: &Fixture,
    q: &ReservedValue<DiplomacyFamilyQualificationV1>,
    tx: &mut ExecutionTransaction<'_, Pages>,
    id: &str,
    source: &AllianceState,
    cells: &mut u64,
) -> Result<ReservedValue<DiplomacyRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let live = tx.parent_fence();
    let guard = tx.guard();
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
    super::super::super::super::create::create_qualified_alliance_v1(
        super::super::super::super::create::AllianceCreateV1 {
            id,
            source,
            family: q,
            max_pending_node_bytes: 8 << 20,
        },
        &mut overlay,
        &mut write,
        &guard,
        &live,
        cells,
    )
}
#[test]
fn atomic_alliance_lifecycle_rejects_overwrite_budget_and_full_expected_mismatch() {
    for case in 0..4 {
        let source = alliance("embedded", &["alice", "bob"]);
        let f = fixture(
            &BTreeMap::from([("outer".into(), source.clone())]),
            &BTreeMap::new(),
        );
        let q = qualify(&f);
        let mut tx =
            ExecutionTransaction::begin(parent(&f.roots), [52; 32], &f.pages, &f.memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        if case < 2 {
            assert!(
                create(
                    &f,
                    &q,
                    &mut tx,
                    if case == 0 { "outer" } else { "new" },
                    &source,
                    &mut if case == 1 { 0 } else { 100000 }
                )
                .is_err()
            );
        } else {
            let proof = {
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
                    "outer",
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
            let mut wrong = source.clone();
            if case == 2 {
                wrong.members[1] = "wrong-member".into();
            } else {
                wrong.charter = "wrong-charter".into();
            }
            let mut io = io();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            };
            let guard = tx.guard();
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
            assert!(
                super::super::super::super::delete::delete_qualified_alliance_v1(
                    super::super::super::super::delete::DiplomacyClosedDeleteV1 {
                        id: "outer",
                        qualification: &proof,
                        expected: DiplomacySourceV1::Alliance(&wrong),
                        max_current_payload_bytes: 1 << 20
                    },
                    &mut overlay,
                    &guard,
                    &live,
                    &mut 100000
                )
                .is_err()
            );
        }
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err(), "case {case}");
    }
}
fn change_war(
    f: &Fixture,
    q: &ReservedValue<DiplomacyFamilyQualificationV1>,
    tx: &mut ExecutionTransaction<'_, Pages>,
    column: WarScalarV1,
    expected: DiplomacyScalarValueV1<'_>,
    after: DiplomacyScalarValueV1<'_>,
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
            DiplomacyFieldV1::Wars,
            "war-outer",
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
    let guard = tx.guard();
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
    change_qualified_diplomacy_scalar_v1(
        DiplomacyScalarChangeV1 {
            field: DiplomacyFieldV1::Wars,
            id: "war-outer",
            column: DiplomacyScalarV1::War(column),
            qualification: &proof,
            expected: Some(expected),
            after: Some(after),
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
fn current_active_war_inner_delta_and_parent_nonmatching_child_are_not_recovered() {
    let f = fixture(
        &BTreeMap::from([
            ("a".into(), alliance("embedded-a", &["alice"])),
            ("b".into(), alliance("embedded-b", &["bob"])),
        ]),
        &BTreeMap::from([("war-outer".into(), war("embedded-war", "a", "a", true))]),
    );
    let q = qualify(&f);
    // Removing an unrelated physical child must not matter to an ordinary
    // relation query. Querying that actual child later must fail, not trust
    // the cached projection as proof of source availability.
    let irrelevant = q
        .relations
        .rows
        .iter()
        .find(|row| row.field == DiplomacyFieldV1::Alliances && row.outer.as_str() == "b")
        .unwrap();
    f.pages.remove(
        blake3::Hash::from_hex(&irrelevant.child.node_root)
            .unwrap()
            .as_bytes(),
    );
    let p = parent(&f.roots);
    let live = p.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(p, [53; 32], &f.pages, &f.memory, limits()).unwrap();
    let mut w = work();
    assert_eq!(
        observed(&f, &q, &mut tx, "alice", false, &mut w)
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["a"]
    );
    assert_eq!(
        observed(&f, &q, &mut tx, "a", true, &mut w)
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["war-outer"]
    );
    change_war(
        &f,
        &q,
        &mut tx,
        WarScalarV1::Active,
        DiplomacyScalarValueV1::Bool(true),
        DiplomacyScalarValueV1::Bool(false),
        &mut w,
    );
    assert_eq!(
        observed(&f, &q, &mut tx, "a", true, &mut w)
            .unwrap()
            .ids()
            .len(),
        0
    );
    change_war(
        &f,
        &q,
        &mut tx,
        WarScalarV1::AggressorAllianceId,
        DiplomacyScalarValueV1::Text("a"),
        DiplomacyScalarValueV1::Text("c"),
        &mut w,
    );
    change_war(
        &f,
        &q,
        &mut tx,
        WarScalarV1::DefenderAllianceId,
        DiplomacyScalarValueV1::Text("a"),
        DiplomacyScalarValueV1::Text("c"),
        &mut w,
    );
    change_war(
        &f,
        &q,
        &mut tx,
        WarScalarV1::Active,
        DiplomacyScalarValueV1::Bool(false),
        DiplomacyScalarValueV1::Bool(true),
        &mut w,
    );
    assert_eq!(
        observed(&f, &q, &mut tx, "a", true, &mut w)
            .unwrap()
            .ids()
            .len(),
        0
    );
    assert_eq!(
        observed(&f, &q, &mut tx, "c", true, &mut w)
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec!["war-outer"]
    );
    assert!(observed(&f, &q, &mut tx, "bob", false, &mut w).is_err());
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}

#[test]
fn diplomacy_outer_semantic_limit_is_independent_of_inner_payload_limit() {
    let outer = "outer/".repeat(80);
    let mut f = fixture(
        &BTreeMap::from([(outer.clone(), alliance("embedded", &["alice"]))]),
        &BTreeMap::new(),
    );
    let q = {
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
        // All original inner payloads fit eight bytes. The original outer
        // semantic limit is separately one MiB, not this payload limit.
        qualify_diplomacy_family_v1(&f.roots, &mut ctx, &mut scan(), &mut scan(), &mut 100000, 8)
            .unwrap()
    };
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [55; 32], &f.pages, &f.memory, limits())
            .unwrap();
    assert_eq!(
        observed(&f, &q, &mut tx, "alice", false, &mut work())
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec![outer.as_str()]
    );
    let added = format!("{outer}/added");
    let source = alliance("embedded", &["alice"]);
    create(&f, &q, &mut tx, &added, &source, &mut 100000).unwrap();
    let p = parent(&f.roots);
    let live = p.fence().unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    persist(&f, &sealed.new_nodes);
    let accepted = {
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
            operation: [55; 32],
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        super::super::super::super::receiver::accept_diplomacy_transition_v1(
            &q,
            &verifier,
            &sealed.record,
            &mut ctx,
            &mut 100000,
            8,
        )
        .unwrap()
    };
    f.roots = sealed.record.after.clone();
    let mut tx =
        ExecutionTransaction::begin(parent(&f.roots), [56; 32], &f.pages, &f.memory, limits())
            .unwrap();
    assert_eq!(
        observed(&f, &accepted, &mut tx, "alice", false, &mut work())
            .unwrap()
            .ids()
            .collect::<Vec<_>>(),
        vec![outer.as_str(), added.as_str()]
    );
}
