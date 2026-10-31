#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits,
    },
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateScanBudget,
        StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState,
    incremental_field_member::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_effects::*,
    incremental_world_rule_organization::root::*,
    incremental_world_rule_organization::*,
    world_rules::{
        OrganizationCaseV1, OrganizationDispositionPhaseV1, OrganizationProtectionPhaseV1,
        WorldRulesStateV1,
    },
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        ..fixture::physical()
    }
}
#[allow(dead_code)]
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;
const CASE: &str = "outer-case";
const OLD: &str = "old-member";
fn empty_case() -> OrganizationCaseV1 {
    OrganizationCaseV1 {
        id: "different-inner-case".into(),
        organization_id: "org".into(),
        founder_id: "founder".into(),
        members: Default::default(),
        policy_version: u32::MAX,
        revision: 1,
        phase: OrganizationProtectionPhaseV1::Active,
        disposition: OrganizationDispositionPhaseV1::None,
        notified_at: None,
        protected_at: None,
        grants: Default::default(),
        requests: Default::default(),
        stage_receipts: Vec::new(),
    }
}
fn text(v: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(v.len() as u64).to_be_bytes());
    b.extend_from_slice(v.as_bytes());
    b
}
fn pair(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    let mut out = vec![5];
    out.extend(a);
    out.extend(b);
    out
}
fn member_key() -> WorldFieldKeyV1 {
    let mut b = b"oasis7.iws.semantic-key.v1\0".to_vec();
    b.extend(pair(
        pair(
            pair(text("organization_cases"), text(CASE)),
            text("members"),
        ),
        text(OLD),
    ));
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&b).as_bytes()),
    )
    .unwrap()
}
fn parent(root: &StateRootV1) -> ExecutionParent {
    let mut roots = RootSetV1::empty("org-creation".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    ExecutionParent {
        commit: [4; 32],
        roots,
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 1_000_000,
    }
}
fn relative_member_key(member: &str) -> WorldFieldKeyV1 {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend(pair(pair(vec![0], text("members")), text(member)));
    WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&bytes).as_bytes()),
    )
    .unwrap()
}
#[path = "support/incremental_world_rule_organization_root_faults.rs"]
mod faults;
use faults::{BatchObserver, StateArcMode, StateArcReader, verify_missing_original_pages};
fn creation(
    orphan: bool,
    compression: PageCompression,
    wire_limit: u64,
    state_mode: StateArcMode,
    cancel_after_batch: bool,
) {
    let huge = wire_limit > 165;
    let max_payload = if huge { wire_limit } else { 128 };
    let max_record = 8192.max(max_payload + 4096);
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store =
        IncrementalPageStore::create(&directory.0.join("pages"), memory.clone(), compression)
            .unwrap();
    let mut original = empty_case();
    original.members.insert(OLD.into());
    let source = WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            organization_cases: [(CASE.into(), original)].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    };
    let parent_rows = produce(&store, &memory, &source);
    assert_eq!(parent_rows.len(), 3);
    let mut rows = parent_rows.clone();
    if orphan {
        // Historical valid-case -> fixed-strip -> flat orphan revival is
        // preserved at the immutable fffa/1785 proof refs and original red log.
        // The retired flat producer is not needed to forge its original leaf.
        let target = member_key();
        let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
        semantic.extend(pair(
            pair(
                pair(text("organization_cases"), text(CASE)),
                text("members"),
            ),
            text(OLD),
        ));
        let mut frame = b"oasis7.iws.field-member.v1\0".to_vec();
        frame.extend_from_slice(&(target.as_bytes().len() as u16).to_be_bytes());
        frame.push(1);
        frame.extend_from_slice(&(semantic.len() as u64).to_be_bytes());
        frame.extend_from_slice(&0u64.to_be_bytes());
        frame.extend_from_slice(target.as_bytes());
        frame.extend(semantic);
        let value = write_value(
            &mut frame.as_slice(),
            frame.len() as u64,
            &mut store.value_session(physical()),
            &memory,
            &mut logical(),
            || false,
        )
        .unwrap()
        .digest()
        .unwrap();
        rows.push((target.as_bytes().to_vec(), value));
    }
    let before = build(&store, &memory, &rows);
    assert_eq!(before.leaf_count, if orphan { 4 } else { 3 });

    let parent = parent(&before);
    let roots = parent.roots.clone();
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let foreign = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let adversarial = StateArcReader {
        source: &values,
        foreign: &foreign,
        mode: state_mode,
        supplied: std::cell::Cell::new(0),
    };
    let mut writer = BatchObserver::new(store.value_session(physical()), cancel_after_batch);
    let batch_control = writer.control.clone();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 64,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &adversarial,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: max_record,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut logical(),
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        limits: MemberRecordLimitsV1 {
            payload_bytes: wire_limit,
            record_bytes: max_record,
            ..limits()
        },
        cancelled: || {
            batch_control
                .cancelled
                .load(std::sync::atomic::Ordering::SeqCst)
        },
    };
    let guard = read.transaction.guard();
    assert!(
        change_world_rule_effect_overlay_v1(
            WorldRuleEffectChangeV1 {
                key: "p",
                expected: WorldRuleEffectExpectedV1::Value("original"),
                after: Some("prefix"),
                max_current_payload_bytes: 128
            },
            &mut read,
            &mut write,
            &guard,
            &live
        )
        .unwrap()
    );
    let qualified = read.transaction.contribute(
        |_| -> Result<_, oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1> {
            let mut fixed = WorldFieldReadContextV1 {
                root: &before,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: max_record,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            Ok(qualify_organization_family_v1(
                &roots,
                &mut fixed,
                &mut scan(),
                &mut scan(),
                &mut 100_000,
                max_payload,
            )?)
        },
    );
    if orphan {
        let error = qualified.unwrap_err();
        assert!(
            error.to_string().contains("flat orphan descendant"),
            "{error}"
        );
        let guard = tx.guard();
        assert!(
            tx.seal(&guard, &live, 1).is_err(),
            "qualification failure must poison the earlier typed prefix"
        );
        return;
    }
    let qualified = qualified.unwrap();
    assert_eq!(qualified.record_counts(), [0, 0]);
    let mut empty = empty_case();
    if huge {
        empty.founder_id = "x".repeat(wire_limit as usize);
    }
    let guard = read.transaction.guard();
    let mut cells = 1000;
    let created = create_qualified_organization_empty_v1(
        OrganizationEmptyRootCreateV1 {
            owner: OrganizationRootKeyV1::Case(CASE),
            source: WorldRuleOrganizationScalarsRefV1::Case(&empty),
            parent: &qualified,
            max_pending_node_bytes: 4 << 20,
        },
        &mut read,
        &mut write,
        &guard,
        &live,
        &mut cells,
    );
    if cancel_after_batch {
        assert!(matches!(
            created,
            Err(
                oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                    ValueTreeError::Cancelled
                )
            )
        ));
        let pages = batch_control
            .state_pages
            .load(std::sync::atomic::Ordering::SeqCst);
        let direct = batch_control
            .direct_puts
            .load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            pages > 256 && direct > 0,
            "real fresh-store state batches and typed value puts must have happened"
        );
        assert_eq!(
            write.io.nodes_remaining,
            100_000 - pages - direct,
            "packing must preserve per-page logical billing"
        );
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
        batch_control
            .cancelled
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let mut retry = ExecutionTransaction::begin(
            parent.clone(),
            [7; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 128,
                max_writes: 64,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut retry_read = WorldFieldOverlayContextV1 {
            transaction: &mut retry,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: max_record,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let guard = retry_read.transaction.guard();
        let retried = create_qualified_organization_empty_v1(
            OrganizationEmptyRootCreateV1 {
                owner: OrganizationRootKeyV1::Case(CASE),
                source: WorldRuleOrganizationScalarsRefV1::Case(&empty),
                parent: &qualified,
                max_pending_node_bytes: 4 << 20,
            },
            &mut retry_read,
            &mut write,
            &guard,
            &live,
            &mut 1000,
        )
        .unwrap();
        assert_eq!(retried.root().leaf_count, 15);
        let guard = retry.guard();
        assert!(
            retry.seal(&guard, &live, 1).is_ok(),
            "the original authoritative root must permit the legal retry"
        );
        return;
    }
    if wire_limit < 165 {
        assert!(matches!(
            created,
            Err(
                oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(
                    MemberRecordError::Value(ValueTreeError::ResourceLimited(
                        "member record lengths"
                    ))
                )
            )
        ));
        assert_eq!(
            cells, 985,
            "all 15 body members precede the fixed outer reference admission"
        );
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
        return;
    }
    let body = created.unwrap();
    let guard = read.transaction.guard();
    let changed_result = change_qualified_organization_member_v1(
        OrganizationRootMemberChangeV1 {
            case: CASE,
            member: "new-member",
            expected: false,
            after: true,
            qualification: &body,
            max_pending_node_bytes: 4 << 20,
        },
        &mut read,
        &mut write,
        &guard,
        &live,
    );
    if state_mode != StateArcMode::Good {
        assert!(
            adversarial.supplied.get() > 0,
            "the forged carrier must reach an inner state read"
        );
        let rejected = matches!(
            changed_result,
            Err(
                oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                    ValueTreeError::Invalid("organization state page allocation")
                )
            )
        );
        let guard = tx.guard();
        let sealable = tx.seal(&guard, &live, 1).is_ok();
        assert!(
            rejected && !sealable,
            "state carrier {state_mode:?}: rejected={rejected}, sealable={sealable}"
        );
        return;
    }
    let changed = changed_result.unwrap().unwrap();
    let changed = faults::change_policy_without_text(
        &changed, &state, &mut read, &mut write, &live, max_record,
    );
    let changed = receipt_points::receipts(changed, &state, &mut read, &mut write, &live);
    assert_eq!(
        changed
            .root()
            .get(relative_member_key(OLD).as_bytes(), &state)
            .unwrap(),
        None,
        "old flat member must not be inherited"
    );
    assert!(
        changed
            .root()
            .get(relative_member_key("new-member").as_bytes(), &state)
            .unwrap()
            .is_some()
    );
    let guard = read.transaction.guard();
    assert!(
        change_qualified_organization_member_v1(
            OrganizationRootMemberChangeV1 {
                case: CASE,
                member: "new-member",
                expected: true,
                after: true,
                qualification: &changed,
                max_pending_node_bytes: 4 << 20
            },
            &mut read,
            &mut write,
            &guard,
            &live
        )
        .unwrap()
        .is_none()
    );
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(prepared.record.writes.len(), 2);
    oasis7_formal_protocol::incremental_execution::TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
    use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
    for node in &prepared.new_nodes {
        writer
            .put_node(
                *blake3::hash(&node.bytes).as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let after_roots = prepared.record.after.clone();
    let after_record = prepared.record.clone();
    drop(writer);
    drop(values);
    drop(state);
    drop(store);
    drop(prepared);
    let cold = IncrementalPageStore::open(
        &directory.0.join("pages"),
        memory.clone(),
        compression,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let cold_state = cold
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let cold_values = cold.value_session(physical());
    let after = after_roots
        .state
        .iter()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap();
    let mut fixed = WorldFieldReadContextV1 {
        root: after,
        state: &cold_state,
        values: ValueReadContext {
            source: &cold_values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut logical(),
        },
        key_work: &mut SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        },
        semantic_bytes: 1024,
        record_bytes: max_record,
        take_state_error: || cold_state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let accepted = accept_organization_transition_v1(
        &qualified,
        &oasis7_formal_protocol::incremental_execution::TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &cold_state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        },
        &after_record,
        &mut fixed,
        &mut scan(),
        &mut 100_000,
        max_payload,
    )
    .unwrap();
    assert_eq!(accepted.record_counts(), [1, 0]);
    accepted
        .validate_binding(&after_roots.digest().unwrap())
        .unwrap();
    let restored = qualify_organization_family_v1(
        &after_roots,
        &mut fixed,
        &mut scan(),
        &mut scan(),
        &mut 100_000,
        max_payload,
    )
    .unwrap();
    assert_eq!(restored.record_counts(), [1, 0]);
    faults::assert_cold_policy(&after_roots, &restored, &mut fixed);
    let inner = inspect_organization_root_v1(OrganizationRootKeyV1::Case(CASE), &mut fixed)
        .unwrap()
        .unwrap();
    assert_eq!(
        inner
            .get(relative_member_key(OLD).as_bytes(), &cold_state)
            .unwrap(),
        None
    );
    assert!(
        inner
            .get(relative_member_key("new-member").as_bytes(), &cold_state)
            .unwrap()
            .is_some()
    );
    if huge {
        verify_missing_original_pages(
            &cold,
            &memory,
            &directory.0.join("pages"),
            &after_roots,
            &inner,
            max_payload,
            max_record,
        );
    }
}
#[test]
fn qualified_parent_rejects_original_orphan_and_poisons_prior_prefix() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        creation(true, compression, 165, StateArcMode::Good, false);
    }
}
#[test]
fn fresh_inner_case_accepts_same_transaction_member_and_cold_root_only_recovery() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        creation(false, compression, 165, StateArcMode::Good, false);
    }
}

#[test]
fn fixed_record_reference_requires_exact_165_byte_payload_admission() {
    creation(false, PageCompression::Raw, 164, StateArcMode::Good, false);
}

#[test]
fn root_only_recovery_requires_outer_unit_and_last_long_text_original_pages() {
    creation(
        false,
        PageCompression::Raw,
        (2 * oasis7_formal_protocol::incremental_value::VALUE_CHUNK_BYTES + 512) as u64,
        StateArcMode::Good,
        false,
    );
}

#[test]
fn foreign_state_page_arc_must_reject_and_poison_typed_prefix() {
    creation(
        false,
        PageCompression::Raw,
        165,
        StateArcMode::Foreign,
        false,
    );
}
#[test]
fn undercharged_state_page_arc_must_reject_and_poison_typed_prefix() {
    creation(
        false,
        PageCompression::Raw,
        165,
        StateArcMode::Undercharged,
        false,
    );
}

#[test]
fn post_batch_real_put_cancellation_poisons_and_original_root_can_retry() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        creation(false, compression, 165, StateArcMode::Good, true);
    }
}

#[path = "support/incremental_world_rule_organization_root_receipts.rs"]
mod receipt_points;
