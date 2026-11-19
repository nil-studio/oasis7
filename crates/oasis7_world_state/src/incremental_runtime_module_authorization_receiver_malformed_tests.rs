//! Generic-valid malicious children must fail the typed receiver.
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_record_tree::RecordTreeWritePolicyV1;
use crate::incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1};
use oasis7_formal_protocol::incremental_execution::TransitionVerifier;
fn malformed(mode: u8) {
    let backing = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = backing.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages(Rc::new(RefCell::new(BTreeMap::new())));
    let grants = BTreeMap::from([(
        "cap".into(),
        CapabilityGrant {
            name: "x".repeat(1 << 20),
            effect_kinds: vec!["a".into(), "b".into(), "c".into()],
            expiry: Some(5),
        },
    )]);
    let policies = crate::policy::PolicySet::allow_all();
    let mut writes = Vec::new();
    prepare_module_authorization_genesis_v1(
        &grants,
        &policies,
        &mut writer(&mut pages, &backing, &mut io(), &mut keys()),
        &mut work(),
        &mut |row: ReservedValue<ExecutionWriteV1>| {
            writes.push(row.mutation.clone());
            Ok(())
        },
    )
    .unwrap();
    drop(grants);
    drop(policies);
    let mut roots = RootSetV1::empty("module-point-test".into(), [2; 32]).unwrap();
    roots.state[3] = persist(roots.state[3].clone(), writes, &mut pages, &backing);
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let family = bootstrap(&roots, &pages, &memory, MemoryClass::Ordinary).unwrap();
    let source = pages.clone();
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut never = || false;
    let cancel = RefCell::new(&mut never);
    let error = RefCell::new(None);
    let state = tree::Source {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut q = io();
    let mut k = keys();
    let mut fixed = WorldFieldReadContextV1 {
        root: &roots.state[3],
        state: &state,
        values: ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut q,
        },
        key_work: &mut k,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled: || false,
    };
    let body = open_qualified_module_authorization_body_v1(
        ModuleAuthorizationRecordKeyV1::Grant("cap"),
        &roots,
        &family,
        &mut fixed,
    )
    .unwrap()
    .unwrap();
    let mut body_ctx = WorldFieldReadContextV1 {
        root: body.root(),
        state: &state,
        values: ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut q,
        },
        key_work: &mut k,
        semantic_bytes: 4096,
        record_bytes: 3 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled: || false,
    };
    let path = read::key(
        ModuleAuthorizationFieldV1::Capabilities,
        ModuleAuthorizationAddressV1::fixed("name"),
        &mut body_ctx.records(),
    )
    .unwrap();
    let forbidden = body_ctx.lookup(&path.0).unwrap().unwrap();
    let values = Guarded {
        pages: &source,
        forbidden,
    };
    let parent = ExecutionParent {
        commit: [1; 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 0,
        },
    };
    let live = parent.fence().unwrap();

    let _scratch = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [4; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 8,
            max_facts: 4,
            max_pending_node_bytes: 2 << 20,
        },
    )
    .unwrap();
    let owner = if mode == 5 {
        ModuleAuthorizationFieldV1::PolicyRules
    } else {
        ModuleAuthorizationFieldV1::Capabilities
    };
    let original = if mode == 5 {
        {
            let mut q = io();
            let mut k = keys();
            open_qualified_module_authorization_body_v1(
                ModuleAuthorizationRecordKeyV1::Policy(0),
                &roots,
                &family,
                &mut WorldFieldReadContextV1 {
                    root: &roots.state[3],
                    state: &state,
                    values: ValueReadContext {
                        source: &source,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut q,
                    },
                    key_work: &mut k,
                    semantic_bytes: 4096,
                    record_bytes: 3 << 20,
                    take_state_error: || error.borrow_mut().take(),
                    cancelled: || false,
                },
            )
            .unwrap()
            .unwrap()
        }
    } else {
        body
    };
    let mut child = original.root().clone();
    if mode == 7 {
        let meta = WorldFieldKeyV1::new(owner.descriptor(), WorldFieldRecordKindV1::Metadata, None)
            .unwrap();
        tx.write(StateNamespaceV1::Authorization, meta.as_bytes(), None)
            .unwrap();
    } else if mode != 6 {
        let (address, payload) = match mode {
            0 => (
                ModuleAuthorizationAddressV1::fixed("unknown"),
                Some(MemberPayloadRefV1::U64(1)),
            ),
            1 => (ModuleAuthorizationAddressV1::fixed("name"), None),
            2 => (
                ModuleAuthorizationAddressV1 {
                    column: "expiry",
                    index: Some(0),
                },
                Some(MemberPayloadRefV1::U64(9)),
            ),
            3 => (
                ModuleAuthorizationAddressV1::fixed("effect_kinds.length"),
                Some(MemberPayloadRefV1::U64(1)),
            ),
            4 => (
                ModuleAuthorizationAddressV1 {
                    column: "effect_kinds",
                    index: Some(1),
                },
                None,
            ),
            5 => (
                ModuleAuthorizationAddressV1::fixed("decision.kind"),
                Some(MemberPayloadRefV1::Text("deny")),
            ),
            _ => unreachable!(),
        };
        let mut vi = io();
        let mut vk = keys();
        let mut records = InventoryValueReader {
            values: &mut ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut vk,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            cancelled: &mut || false,
        };
        let key = read::key(owner, address, &mut records).unwrap();
        let old = tree::lookup(&child, &key.0, &mut records, genesis::LABELS).unwrap();
        let new = payload.map(|payload| {
            write_member_record_v1(
                genesis::body_descriptor(),
                WorldFieldRecordKindV1::Entry,
                &address.semantic(owner),
                payload,
                &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
            )
            .unwrap()
            .value_root
        });
        child = tree::edit(
            child,
            &key.0,
            (old, new),
            records.values,
            &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
            &mut || false,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: 1 << 20,
                labels: genesis::LABELS,
            },
        )
        .unwrap();
        if mode == 4 {
            // Keep the total leaf count consistent, while leaving ordinal 2
            // and deleting ordinal 1 from the claimed interval [0,2).
            let address = ModuleAuthorizationAddressV1::fixed("effect_kinds.length");
            let key = read::key(owner, address, &mut records).unwrap();
            let old = tree::lookup(&child, &key.0, &mut records, genesis::LABELS).unwrap();
            let new = write_member_record_v1(
                genesis::body_descriptor(),
                WorldFieldRecordKindV1::Entry,
                &address.semantic(owner),
                MemberPayloadRefV1::U64(2),
                &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
            )
            .unwrap();
            child = tree::edit(
                child,
                &key.0,
                (old, Some(new.value_root)),
                records.values,
                &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
                &mut || false,
                RecordTreeWritePolicyV1 {
                    max_pending_node_bytes: 1 << 20,
                    labels: genesis::LABELS,
                },
            )
            .unwrap();
        }
        let outer = if owner == ModuleAuthorizationFieldV1::Capabilities {
            genesis::root_row(
                owner,
                "cap",
                child,
                &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
            )
            .unwrap()
        } else {
            genesis::root_row(
                owner,
                &0u64,
                child,
                &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
            )
            .unwrap()
        };
        tx.write(
            StateNamespaceV1::Authorization,
            outer.field_key.as_bytes(),
            Some(outer.value_root),
        )
        .unwrap();
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    verifier
        .verify(&sealed.record)
        .expect("malicious owner data must remain generic-valid");
    let mut other = roots.clone();
    other.height += 1;
    let wrong = if mode == 6 {
        Some(bootstrap(&other, &pages, &memory, MemoryClass::Ordinary).unwrap())
    } else {
        None
    };
    let mut q = io();
    let mut k = keys();
    let result = accept_module_authorization_transition_v1(
        wrong.as_ref().unwrap_or(&family),
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[3],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut q,
            },
            key_work: &mut k,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            leaves_remaining: 1000,
            read_bytes_remaining: 4 << 20,
        },
        &mut read_work(),
    );
    assert!(
        matches!(
            result,
            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
        ),
        "mode {mode} must reject the closed schema, not exhaust IO/heap or fail a source read"
    );
}
#[test]
fn module_authorization_receiver_rejects_unknown_and_fixed_delete_or_indexed_add() {
    for m in 0..3 {
        malformed(m)
    }
}
#[test]
fn module_authorization_receiver_rejects_stale_vector_ordinal_and_hole() {
    for m in 3..5 {
        malformed(m)
    }
}
#[test]
fn module_authorization_receiver_rejects_decision_without_reason() {
    malformed(5)
}
#[test]
fn module_authorization_receiver_rejects_wrong_parent_and_metadata_delete() {
    for m in 6..8 {
        malformed(m)
    }
}
