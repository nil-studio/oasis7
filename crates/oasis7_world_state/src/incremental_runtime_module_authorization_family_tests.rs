use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
    incremental_state::{StateDeltaV1, StateWriteV1},
};
fn bootstrap(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<ModuleAuthorizationFamilyQualificationV1>, ValueTreeError> {
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut stop = || false;
    let cancel = RefCell::new(&mut stop);
    let error = RefCell::new(None);
    let state = tree::Source {
        source: pages,
        memory,
        class,
        io: &state_io,
        cancelled: &cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut io = io();
    let mut keys = keys();
    qualify_module_authorization_family_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &state,
            values: ValueReadContext {
                source: pages,
                memory,
                class,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 10_000,
        },
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 10_000,
        },
        &mut ModuleAuthorizationReadWorkV1 {
            cells_remaining: 10_000,
            max_payload_bytes: 2 << 20,
        },
    )
}
fn persist(
    root: StateRootV1,
    rows: Vec<StateWriteV1>,
    pages: &mut Pages,
    memory: &MemoryBudget,
) -> StateRootV1 {
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut stop = || false;
    let cancel = RefCell::new(&mut stop);
    let error = RefCell::new(None);
    let source = pages.clone();
    let state = tree::Source {
        source: &source,
        memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut rows = rows;
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(root, rows, &state, 4 << 20).unwrap();
    for node in delta.new_nodes {
        let id = hex::decode(node.digest).unwrap().try_into().unwrap();
        pages
            .put_node(
                id,
                memory
                    .reserve(node.bytes.capacity(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes),
            )
            .unwrap();
    }
    delta.after
}
#[test]
fn full_saved_authorization_family_preserves_outer_identity_and_requires_explicit_readmission() {
    let backing = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = backing.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages(Rc::new(RefCell::new(BTreeMap::new())));
    let grants = BTreeMap::from([
        (
            String::new(),
            CapabilityGrant {
                name: "different embedded name".into(),
                effect_kinds: vec!["x.*".into()],
                expiry: Some(0),
            },
        ),
        (
            " cap ".into(),
            CapabilityGrant {
                name: String::new(),
                effect_kinds: vec![],
                expiry: None,
            },
        ),
    ]);
    let policies = crate::policy::PolicySet {
        rules: vec![
            PolicyRule {
                when: PolicyWhen {
                    effect_kind: None,
                    origin_kind: None,
                    cap_name: None,
                },
                decision: PolicyDecision::Allow,
            },
            PolicyRule {
                when: PolicyWhen {
                    effect_kind: Some("x".into()),
                    origin_kind: Some(OriginKind::Module),
                    cap_name: Some("".into()),
                },
                decision: PolicyDecision::Deny {
                    reason: String::new(),
                },
            },
        ],
    };
    let mut writes = Vec::new();
    prepare_module_authorization_genesis_v1(
        &grants,
        &policies,
        &mut writer(&mut pages, &backing, &mut io(), &mut keys()),
        &mut work(),
        &mut |write: ReservedValue<ExecutionWriteV1>| {
            assert_eq!(write.namespace, StateNamespaceV1::Authorization);
            writes.push(write.mutation.clone());
            Ok(())
        },
    )
    .unwrap();
    drop(grants);
    drop(policies);
    let mut roots = RootSetV1::empty("module-authorization-test".into(), [2; 32]).unwrap();
    roots.state[3] = persist(roots.state[3].clone(), writes, &mut pages, &backing);
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let family = bootstrap(&roots, &pages, &memory, MemoryClass::Ordinary).unwrap();
    assert_eq!(family.grant_count(), 2);
    assert_eq!(family.policy_length(), 2);
    let recovered = bootstrap(&roots, &pages, &memory, MemoryClass::Recovery).unwrap();
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut stop = || false;
    let cancel = RefCell::new(&mut stop);
    let error = RefCell::new(None);
    let state = tree::Source {
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut q = io();
    let mut k = keys();
    let mut ctx = WorldFieldReadContextV1 {
        root: &roots.state[3],
        state: &state,
        values: ValueReadContext {
            source: &pages,
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
    assert!(matches!(
        open_qualified_module_authorization_body_v1(
            ModuleAuthorizationRecordKeyV1::Grant(""),
            &roots,
            &recovered,
            &mut ctx
        ),
        Err(ValueTreeError::Invalid(_))
    ));
    let child = open_qualified_module_authorization_body_v1(
        ModuleAuthorizationRecordKeyV1::Grant(""),
        &roots,
        &family,
        &mut ctx,
    )
    .unwrap()
    .unwrap();
    assert_eq!(child.field(), ModuleAuthorizationFieldV1::Capabilities);
    assert!(
        open_qualified_module_authorization_body_v1(
            ModuleAuthorizationRecordKeyV1::Grant("different embedded name"),
            &roots,
            &family,
            &mut ctx
        )
        .unwrap()
        .is_none()
    );
    assert!(
        open_qualified_module_authorization_body_v1(
            ModuleAuthorizationRecordKeyV1::Policy(2),
            &roots,
            &family,
            &mut ctx
        )
        .unwrap()
        .is_none()
    );
    let available = (4 << 20) - memory.used();
    let hold = memory.reserve(available, MemoryClass::Ordinary).unwrap();
    assert!(matches!(
        readmit_module_authorization_family_ordinary_v1(&recovered, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(hold);
    let ordinary =
        readmit_module_authorization_family_ordinary_v1(&recovered, &roots, &memory, || false)
            .unwrap();
    drop(recovered);
    assert!(ordinary.uses_budget(&memory));
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(family.validate_binding(&wrong).is_err());
    // A real generic-valid removal leaves an ordered hole: never substitute
    // a successful read of ordinal1 for the required dense [0,length) scope.
    let path = family::record_key(
        ModuleAuthorizationRecordKeyV1::Policy(0),
        &mut ctx.records(),
    )
    .unwrap();
    let before = ctx.lookup(&path.0).unwrap().unwrap();
    let mut broken = roots.clone();
    broken.state[3] = persist(
        broken.state[3].clone(),
        vec![StateWriteV1 {
            key: path.0.as_bytes().to_vec(),
            before: Some(before),
            after: None,
        }],
        &mut pages,
        &backing,
    );
    assert!(matches!(
        bootstrap(&broken, &pages, &memory, MemoryClass::Ordinary),
        Err(ValueTreeError::Invalid(_))
    ));
}
