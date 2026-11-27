//! Typed namespace/Current primitive only; no accepted head or business authority.
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateNamespaceV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueIoBudget, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    WorldState,
    gameplay_state::{CrisisState, CrisisStatus},
    incremental_diplomacy::*,
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::SemanticKeyBudget,
};
#[path = "support/crisis_current_pages.rs"]
mod pages;
use pages::{Pages, Writer, io};
struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 32 << 20,
        leaves_remaining: 10_000,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    }
}
fn crisis() -> CrisisState {
    CrisisState {
        crisis_id: "embedded".into(),
        kind: "storm".into(),
        severity: 2,
        status: CrisisStatus::Active,
        opened_at: 1,
        expires_at: 20,
        resolver_agent_id: None,
        strategy: None,
        success: None,
        impact: 0,
        resolved_at: None,
    }
}
fn fixture_with_crisis(selected: CrisisState) -> Fixture {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let mut roots = RootSetV1::empty("crisis-data-only".into(), [71; 32]).unwrap();
    let mut state = WorldState::default();
    state.crises.insert("outer".into(), selected);
    let mut writer = Writer(&pages);
    let mut i = io();
    let mut k = keys();
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut i,
        key_work: &mut k,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::new();
    assert_eq!(
        prepare_diplomacy_genesis_v1(&state, &mut write, &mut 100_000, 8 << 20, &mut |row| {
            rows.push(row);
            Ok(())
        })
        .unwrap(),
        [0, 0, 1, 0]
    );
    drop(state); // No World/state hydration is used by the Current reader.
    let mut mutations: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
    mutations.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
        mutations,
        &pages,
        8 << 20,
    )
    .unwrap();
    for node in &delta.new_nodes {
        writer
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap() = delta.after;
    Fixture {
        memory,
        pages,
        roots,
    }
}
fn fixture() -> Fixture {
    fixture_with_crisis(crisis())
}
fn context<'a, C: FnMut() -> bool>(
    f: &'a Fixture,
    root: &'a StateRootV1,
    i: &'a mut ValueIoBudget,
    k: &'a mut SemanticKeyBudget,
    cancelled: C,
) -> WorldFieldReadContextV1<'a, Pages, Pages, impl FnMut() -> Option<ValueTreeError>, C> {
    WorldFieldReadContextV1 {
        root,
        state: &f.pages,
        values: ValueReadContext {
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            io: i,
        },
        key_work: k,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled,
    }
}
fn qualify(f: &Fixture) -> ReservedValue<DiplomacyFamilyQualificationV1> {
    let mut i = io();
    let mut k = keys();
    let mut ctx = context(
        f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut i,
        &mut k,
        || false,
    );
    qualify_diplomacy_family_v1(
        &f.roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 100_000,
        1 << 20,
    )
    .unwrap()
}
fn parent(f: &Fixture) -> ExecutionParent {
    // Only a primitive transaction fixture. This public DTO never qualifies an
    // Accepted head, production invocation, or durable/runtime capability.
    let digest = f.roots.digest().unwrap();
    ExecutionParent {
        roots: f.roots.clone(),
        commit: digest,
        authority: AuthorityBinding {
            token: digest,
            epoch: 0,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 64,
        max_writes: 32,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
fn fresh_open(id: &str) {
    let f = fixture();
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [91; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut i = io();
    let mut k = keys();
    let mut ctx = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut i,
        &mut k,
        || false,
    );
    let mut cells = 100_000;
    let mut inner = scan();
    let record = open_current_diplomacy_crisis_record_v1(
        id,
        &q,
        &f.roots,
        &mut tx,
        &mut ctx,
        &mut inner,
        &mut cells,
        1 << 20,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(record.is_some(), id == "outer");
    if let Some(record) = record {
        let mut i = io();
        let mut k = keys();
        let mut body = context(&f, record.root(), &mut i, &mut k, || false);
        let mut text = Vec::new();
        read_qualified_diplomacy_scalar_v1(
            id,
            DiplomacyFieldV1::Crises,
            DiplomacyScalarV1::Crisis(CrisisScalarV1::CrisisId),
            &record,
            &mut body,
            4096,
            &mut text,
        )
        .unwrap();
        assert_eq!(text, b"embedded");
        assert!(cells < 100_000);
    }
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}
#[test]
fn fresh_current_crisis_hit_accepts_its_own_first_reads() {
    fresh_open("outer");
}
#[test]
fn fresh_current_crisis_absence_accepts_its_own_first_reads() {
    fresh_open("missing");
}

#[test]
fn current_crisis_complete_paid_state_consumes_actual_pages_without_hydrating_world() {
    let f = fixture();
    let family = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [99; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut i = io();
    let mut k = keys();
    let mut ctx = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut i,
        &mut k,
        || false,
    );
    let before = f.memory.used();
    let full = read_current_diplomacy_crisis_state_v1(
        "outer",
        &family,
        &f.roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut 100_000,
        4096,
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    assert_eq!(&*full, &crisis());
    assert!(full.uses_budget(&f.memory));
    let during = f.memory.used();
    let paid = full.reserved_bytes();
    assert!(paid > 0);
    assert!(during >= before + paid);
    drop(full);
    assert_eq!(f.memory.used(), during - paid);
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
    // No new unregistered test target: the existing required WorldState
    // harness executes all crisis-specific rule cases on actual pages.
    check_current_crisis_rule_regressions();
    check_current_crisis_rule_cancel();
}

#[test]
fn current_crisis_reopens_typed_pending_edit_and_deletion() {
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1;
    let f = fixture();
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [92; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let mut i = io();
    let mut k = keys();
    let mut cells = 100_000;
    let mut inner = scan();
    let guard = tx.guard();
    let before_q = {
        let mut ctx = context(
            &f,
            f.roots.state_root(StateNamespaceV1::Business),
            &mut i,
            &mut k,
            || false,
        );
        open_current_diplomacy_crisis_record_v1(
            "outer",
            &q,
            &f.roots,
            &mut tx,
            &mut ctx,
            &mut inner,
            &mut cells,
            1 << 20,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap()
    };
    let before = crisis();
    let mut after = crisis();
    after.status = CrisisStatus::Resolved;
    after.resolver_agent_id = Some("resolver".into());
    after.strategy = Some("updated".into());
    after.success = Some(true);
    after.impact = 4;
    after.resolved_at = Some(11);
    let mut writer = Writer(&f.pages);
    let mut wi = io();
    let mut wk = keys();
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &f.memory,
        io: &mut wi,
        key_work: &mut wk,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let guard = tx.guard();
    let changed = {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut i,
            },
            key_work: &mut k,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        change_qualified_diplomacy_scalars_v1(
            DiplomacyScalarsChangeV1 {
                id: "outer",
                qualification: &before_q,
                expected: DiplomacySourceV1::Crisis(&before),
                after: DiplomacySourceV1::Crisis(&after),
                max_current_payload_bytes: 1 << 20,
                max_pending_node_bytes: 8 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap()
    };
    assert_ne!(before_q.root(), changed.root());
    let guard = tx.guard();
    let reopened = {
        let mut ctx = context(
            &f,
            f.roots.state_root(StateNamespaceV1::Business),
            &mut i,
            &mut k,
            || false,
        );
        open_current_diplomacy_crisis_record_v1(
            "outer",
            &q,
            &f.roots,
            &mut tx,
            &mut ctx,
            &mut inner,
            &mut cells,
            1 << 20,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap()
    };
    assert_eq!(reopened.root(), changed.root());
    let _text_fee = f.memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let mut text = Vec::new();
    {
        let mut body = context(&f, reopened.root(), &mut i, &mut k, || false);
        read_qualified_diplomacy_scalar_v1(
            "outer",
            DiplomacyFieldV1::Crises,
            DiplomacyScalarV1::Crisis(CrisisScalarV1::Strategy),
            &reopened,
            &mut body,
            4096,
            &mut text,
        )
        .unwrap();
    }
    assert_eq!(text, b"updated");
    // The same original transaction must expose the COMPLETE edited child,
    // not reconstruct the old outer/embedded record from a World snapshot.
    let full = {
        let mut body = context(
            &f,
            f.roots.state_root(StateNamespaceV1::Business),
            &mut i,
            &mut k,
            || false,
        );
        let guard = tx.guard();
        read_current_diplomacy_crisis_state_v1(
            "outer", &q, &f.roots, &mut tx, &mut body, &mut inner, &mut cells, 4096, &guard, &live,
        )
        .unwrap()
        .unwrap()
    };
    assert_eq!(&*full, &after);
    drop(full);
    let guard = tx.guard();
    {
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut i,
            },
            key_work: &mut k,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        delete_qualified_diplomacy_closed_v1(
            DiplomacyClosedDeleteV1 {
                id: "outer",
                qualification: &reopened,
                expected: DiplomacySourceV1::Crisis(&after),
                max_current_payload_bytes: 1 << 20,
            },
            &mut overlay,
            &guard,
            &live,
        )
        .unwrap();
    }
    let guard = tx.guard();
    {
        let mut ctx = context(
            &f,
            f.roots.state_root(StateNamespaceV1::Business),
            &mut i,
            &mut k,
            || false,
        );
        assert!(
            open_current_diplomacy_crisis_record_v1(
                "outer",
                &q,
                &f.roots,
                &mut tx,
                &mut ctx,
                &mut inner,
                &mut cells,
                1 << 20,
                &guard,
                &live
            )
            .unwrap()
            .is_none()
        );
    }
    assert!(cells < 100_000);
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}
#[test]
fn current_crisis_keeps_stale_input_guard_rejected_and_aborted() {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
    use oasis7_world_state::incremental_world_field_key::{
        WorldFieldKeyV1, WorldFieldRecordKindV1,
    };
    let f = fixture();
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [93; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let stale = tx.guard();
    let metadata = WorldFieldKeyV1::new(
        DiplomacyFieldV1::Crises.descriptor().unwrap(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .unwrap();
    tx.read(StateNamespaceV1::Business, metadata.as_bytes())
        .unwrap();
    let mut i = io();
    let mut k = keys();
    let mut ctx = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut i,
        &mut k,
        || false,
    );
    let result = open_current_diplomacy_crisis_record_v1(
        "outer",
        &q,
        &f.roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut 100_000,
        1 << 20,
        &stale,
        &live,
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Execution(
            ExecutionError::StaleGuard
        ))
    ));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
#[test]
fn current_crisis_cancelled_read_aborts() {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
    let f = fixture();
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [94; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut i = io();
    let mut k = keys();
    let mut ctx = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut i,
        &mut k,
        || true,
    );
    let result = open_current_diplomacy_crisis_record_v1(
        "outer",
        &q,
        &f.roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut 100_000,
        1 << 20,
        &guard,
        &live,
    );
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
fn current_crisis_foreign_parent_pool_aborts() {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
    let foreign = fixture();
    let q = qualify(&foreign);
    let f = fixture();
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [95; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut i = io();
    let mut k = keys();
    let mut ctx = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut i,
        &mut k,
        || false,
    );
    let result = open_current_diplomacy_crisis_record_v1(
        "outer",
        &q,
        &f.roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut 100_000,
        1 << 20,
        &guard,
        &live,
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
    ));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}

#[test]
fn current_crisis_missing_parent_merkle_page_preserves_source_error_and_aborts() {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
    let f = fixture();
    let qualification = qualify(&f);
    let parent = parent(&f);
    let root = f
        .roots
        .state_root(StateNamespaceV1::Business)
        .node_root
        .clone();
    let mut tx =
        ExecutionTransaction::begin(parent, [96; 32], &f.pages, &f.memory, limits()).unwrap();
    // An actual parent Merkle page disappears after qualification; the caller
    // must see its original source error rather than an absent Crisis record.
    f.pages
        .remove(blake3::Hash::from_hex(&root).unwrap().as_bytes());
    let guard = tx.guard();
    let live = tx.parent_fence();
    let mut io = io();
    let mut keys = keys();
    let mut read = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut io,
        &mut keys,
        || false,
    );
    let result = open_current_diplomacy_crisis_record_v1(
        "outer",
        &qualification,
        &f.roots,
        &mut tx,
        &mut read,
        &mut scan(),
        &mut 100_000,
        1 << 20,
        &guard,
        &live,
    );
    assert!(
        matches!(
            result,
            Err(WorldFieldOverlayErrorV1::Execution(ExecutionError::Tree(_)))
        ),
        "{result:?}"
    );
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}

#[test]
fn current_crisis_exhausted_typed_scan_is_not_a_valid_current_proof() {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
    let f = fixture();
    let qualification = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [97; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut io = io();
    let mut keys = keys();
    let mut read = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut io,
        &mut keys,
        || false,
    );
    let mut exhausted = StateScanBudget {
        nodes_remaining: 0,
        read_bytes_remaining: 0,
        leaves_remaining: 0,
    };
    let result = open_current_diplomacy_crisis_record_v1(
        "outer",
        &qualification,
        &f.roots,
        &mut tx,
        &mut read,
        &mut exhausted,
        &mut 100_000,
        1 << 20,
        &guard,
        &live,
    );
    assert!(
        matches!(
            result,
            Err(WorldFieldOverlayErrorV1::Value(
                ValueTreeError::ResourceLimited("diplomacy closure scan")
            ))
        ),
        "{result:?}"
    );
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}

fn decide_crisis_case(
    source: CrisisState,
    id: &str,
    strategy: &str,
    success: bool,
    now: u64,
    expected: Result<i64, CrisisResolutionDataRejectionV1>,
) {
    let f = fixture_with_crisis(source.clone());
    let parent_family = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [103; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut io = io();
    let mut keys = keys();
    let mut ctx = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut io,
        &mut keys,
        || false,
    );
    let before = f.memory.used();
    let actual = decide_current_crisis_resolution_data_v1(
        id,
        strategy,
        success,
        now,
        &parent_family,
        &f.roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut 100_000,
        4096,
        &guard,
        &live,
    )
    .unwrap();
    match (actual, expected) {
        (CrisisResolutionDataOutcomeV1::Rejected(actual), Err(expected)) => {
            assert_eq!(actual, expected);
        }
        (CrisisResolutionDataOutcomeV1::Ready(result), Ok(expected)) => {
            assert_eq!(result.current(), &source);
            assert_eq!(result.impact(), expected);
            assert!(result.uses_budget(&f.memory));
            assert!(f.memory.used() > before);
        }
        _ => panic!("crisis-only rule outcome differs from legacy"),
    }
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}

fn check_current_crisis_rule_regressions() {
    use CrisisResolutionDataRejectionV1 as Why;
    // Actual outer map key differs from the embedded crisis_id.
    decide_crisis_case(crisis(), " outer ", "  containment  ", true, 20, Ok(20));
    decide_crisis_case(crisis(), "outer", "containment", false, 20, Ok(-20));
    let mut minimum_severity = crisis();
    minimum_severity.severity = 0;
    decide_crisis_case(minimum_severity, "outer", "work", false, 20, Ok(-10));
    decide_crisis_case(crisis(), " ", "work", true, 20, Err(Why::EmptyId));
    decide_crisis_case(crisis(), "missing", "work", true, 20, Err(Why::Missing));
    decide_crisis_case(crisis(), "outer", "work", true, 21, Err(Why::Expired));
    decide_crisis_case(crisis(), "outer", " \t ", true, 20, Err(Why::EmptyStrategy));
    let mut resolved = crisis();
    resolved.status = CrisisStatus::Resolved;
    resolved.resolved_at = Some(5);
    decide_crisis_case(resolved, "outer", "work", true, 21, Err(Why::Inactive));
    let mut timed_out = crisis();
    timed_out.status = CrisisStatus::TimedOut;
    decide_crisis_case(timed_out, "outer", "work", false, 20, Err(Why::Inactive));
}

fn check_current_crisis_rule_cancel() {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;
    let f = fixture();
    let parent_family = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [104; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut io = io();
    let mut keys = keys();
    let mut ctx = context(
        &f,
        f.roots.state_root(StateNamespaceV1::Business),
        &mut io,
        &mut keys,
        || true,
    );
    let error = decide_current_crisis_resolution_data_v1(
        "outer", "work", true, 12, &parent_family, &f.roots, &mut tx, &mut ctx,
        &mut scan(), &mut 100_000, 4096, &guard, &live,
    );
    assert!(matches!(error, Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))));
    let guard = tx.guard();
    assert!(matches!(tx.seal(&guard, &live, 1), Err(ExecutionError::Aborted)));
}
