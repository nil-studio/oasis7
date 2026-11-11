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
    gameplay_state::{
        EconomicContractFulfillmentKind, EconomicContractState, EconomicContractStatus,
    },
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
fn contract() -> EconomicContractState {
    EconomicContractState {
        contract_id: "embedded-contract".into(),
        creator_agent_id: "creator".into(),
        counterparty_agent_id: "counterparty".into(),
        fulfillment_kind: EconomicContractFulfillmentKind::Service,
        settlement_kind: oasis7_agent_api::world_action::ResourceKind::Data,
        settlement_amount: 35,
        reputation_stake: 7,
        expires_at: 20,
        description: "retained description".into(),
        status: EconomicContractStatus::Open,
        accepted_at: None,
        settled_at: None,
        settlement_success: None,
        transfer_amount: 0,
        tax_amount: 0,
        settlement_notes: Some("retained note".into()),
    }
}
fn fixture() -> Fixture {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let mut roots = RootSetV1::empty("contract-data-only".into(), [71; 32]).unwrap();
    let mut state = WorldState::default();
    state.economic_contracts.insert("outer".into(), contract());
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
        [0, 0, 0, 1]
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
fn fresh_contract(id: &str) {
    let f = fixture();
    let q = qualify(&f);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [96; 32], &f.pages, &f.memory, limits()).unwrap();
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
    let result = open_current_diplomacy_contract_record_v1(
        id,
        &q,
        &f.roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut 100_000,
        1 << 20,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(result.is_some(), id == "outer");
    if let Some(record) = result {
        let mut i = io();
        let mut k = keys();
        let mut body = context(&f, record.root(), &mut i, &mut k, || false);
        let _fee = f.memory.reserve(4096, MemoryClass::Ordinary).unwrap();
        let mut out = Vec::new();
        read_qualified_diplomacy_scalar_v1(
            id,
            DiplomacyFieldV1::EconomicContracts,
            DiplomacyScalarV1::Contract(ContractScalarV1::ContractId),
            &record,
            &mut body,
            4096,
            &mut out,
        )
        .unwrap();
        assert_eq!(out, b"embedded-contract");
        assert!(
            read_qualified_diplomacy_scalar_v1(
                id,
                DiplomacyFieldV1::Crises,
                DiplomacyScalarV1::Crisis(CrisisScalarV1::CrisisId),
                &record,
                &mut body,
                4096,
                &mut Vec::new()
            )
            .is_err()
        );
    }
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}
#[test]
fn contract_current_fresh_hit_keeps_embedded_id_and_rejects_other_field() {
    fresh_contract("outer");
}
#[test]
fn contract_current_fresh_absence_is_none() {
    fresh_contract("missing");
}

#[test]
fn contract_current_reopens_typed_pending_edit_and_deletion() {
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
        open_current_diplomacy_contract_record_v1(
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
    let before = contract();
    let mut after = contract();
    after.status = EconomicContractStatus::Accepted;
    after.accepted_at = Some(11);
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
                expected: DiplomacySourceV1::Contract(&before),
                after: DiplomacySourceV1::Contract(&after),
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
        open_current_diplomacy_contract_record_v1(
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
            DiplomacyFieldV1::EconomicContracts,
            DiplomacyScalarV1::Contract(ContractScalarV1::Status),
            &reopened,
            &mut body,
            4096,
            &mut text,
        )
        .unwrap();
    }
    assert_eq!(text, b"accepted");
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
                expected: DiplomacySourceV1::Contract(&after),
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
            open_current_diplomacy_contract_record_v1(
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
