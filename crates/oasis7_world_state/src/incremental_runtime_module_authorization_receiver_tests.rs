use super::*;
use oasis7_formal_protocol::incremental_execution::TransitionVerifier;
#[test]
fn module_authorization_changed_receiver_skips_original_large_name_and_checks_final_child() {
    let backing = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = backing.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages(Rc::new(RefCell::new(BTreeMap::new())));
    let grants = BTreeMap::from([(
        "cap".into(),
        CapabilityGrant {
            name: "x".repeat(1 << 20),
            effect_kinds: vec!["existing".into()],
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
    let guard = tx.guard();
    let mut q = io();
    let mut k = keys();
    let mut work = read_work();
    change_module_authorization_cell_v1(
        ModuleAuthorizationCellChangeV1 {
            request: ModuleAuthorizationCellRequestV1 {
                key: ModuleAuthorizationRecordKeyV1::Grant("cap"),
                address: ModuleAuthorizationAddressV1::fixed("expiry"),
                family: &family,
                body: &body,
                max_payload_bytes: 8,
            },
            expected: ModuleAuthorizationScalarRefV1::Unsigned(5),
            after: ModuleAuthorizationScalarRefV1::Unsigned(6),
            max_pending_node_bytes: 1 << 20,
        },
        &mut WorldFieldOverlayContextV1 {
            transaction: &mut tx,
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
        &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
        &mut work,
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
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
    let mut q = io();
    let mut k = keys();
    let accepted = accept_module_authorization_transition_v1(
        &family,
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
    )
    .unwrap();
    accepted.validate_binding(&sealed.record.after).unwrap();
    assert_eq!(accepted.grant_count(), 1);
    assert!(
        q.read_bytes_remaining > 63 << 20,
        "receiver streamed the unchanged MiB name"
    );
}

#[path = "incremental_runtime_module_authorization_receiver_malformed_tests.rs"]
mod malformed_tests;
