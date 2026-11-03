//! In-memory schema tests; these are not physical DistFS/cold receipts.
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1},
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_state::{StatePageWriter, StateRootV1, StateScanBudget},
    incremental_value::{
        ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
pub(super) type Nodes = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
#[derive(Clone)]
pub(super) struct Pages {
    pub(super) nodes: Rc<RefCell<Nodes>>,
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 65536);
        self.nodes.borrow_mut().insert(id, Arc::new(value));
        Ok(())
    }
}
impl StatePageWriter for Pages {}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let nodes = self.nodes.borrow();
        let bytes = nodes
            .get(id)
            .ok_or_else(|| ValueTreeError::Io(std::io::ErrorKind::NotFound.into()))?;
        if bytes.len() > max {
            return Err(ValueTreeError::Invalid("artifact test source bound"));
        }
        Ok(Arc::new(
            memory.reserve(bytes.len(), class)?.retain(bytes.to_vec()),
        ))
    }
}
pub(super) fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
pub(super) fn write_work() -> ArtifactConfigWriteWorkV1 {
    ArtifactConfigWriteWorkV1 {
        cells_remaining: 10_000,
        json: JsonWriteWorkV1 {
            cells_remaining: 10_000,
            json_nodes_remaining: 100,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
        max_pending_node_bytes: 1 << 20,
    }
}
fn produce(source: genesis::Body<'_>, memory: &MemoryBudget, pages: &mut Pages) -> StateRootV1 {
    genesis::body(
        source,
        &mut MemberRecordWriteContext {
            sink: pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut write_work(),
    )
    .unwrap()
}
fn qualify(
    kind: ArtifactConfigBodyKindV1,
    root: &StateRootV1,
    pages: &Pages,
) -> Result<(), ValueTreeError> {
    // Fresh small read pool; no original DTO passed to the qualifier.
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut cancelled = || false;
    let cancel = RefCell::new(&mut cancelled);
    let error = RefCell::new(None);
    let state = tree::Source {
        source: pages,
        memory: &memory,
        class: MemoryClass::Recovery,
        io: &state_io,
        cancelled: &cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut value_io = io();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root,
        state: &state,
        values: ValueReadContext {
            source: pages,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut value_io,
        },
        key_work: &mut key_work,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled: || false,
    };
    let proof = qualify_artifact_config_body_v1(
        kind,
        &mut context,
        &mut StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 10_000,
        },
        &mut ArtifactConfigReadWorkV1 {
            cells_remaining: 10_000,
            max_payload_bytes: 1 << 20,
            json: JsonReadWorkV1 {
                cells_remaining: 10_000,
                json_nodes_remaining: 100,
                max_depth: 16,
                max_payload_bytes: 1 << 20,
            },
        },
    )?;
    proof.validate_binding(kind, root)?;
    Ok(())
}

#[test]
fn raw_module_body_needs_no_dto_and_closes_options_nested_vectors_and_cap_keys() {
    let memory = MemoryBudget::new(192 << 20, 0).unwrap();
    // Admit the test store map/Arc metadata before retaining generated pages.
    let _index = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages {
        nodes: Rc::new(RefCell::new(BTreeMap::new())),
    };
    let record = ModuleRecord {
        manifest: tests::module_fixture(),
        registered_at: u64::MAX,
        registered_by: String::new(),
        audit_event_id: Some(0),
    };
    let root = produce(genesis::Body::Module(&record), &memory, &mut pages);
    drop(record);
    qualify(ArtifactConfigBodyKindV1::Module, &root, &pages).unwrap();
    assert!(matches!(
        qualify(ArtifactConfigBodyKindV1::Limits, &root, &pages),
        Err(ValueTreeError::Invalid(_))
    ));
    // Same-count corruption cannot use a fake cap_slots count to hide an orphan:
    // the original required record fields remain present, but the added leaf is
    // not a canonical cap member. The authenticated root is otherwise valid.
    let address = ArtifactConfigAddressV1 {
        section: "abi_contract.cap_slots",
        ordinal: None,
        inner_ordinal: None,
        member: None,
        column: "length",
    };
    let mut write_io = io();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let source = pages.clone();
    let mut write = MemberRecordWriteContext {
        sink: &mut pages,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let count = write_member_record_v1(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &address.semantic(),
        MemberPayloadRefV1::U64(2),
        &mut write,
    )
    .unwrap();
    let before_leaves = root.leaf_count;
    let old = write_member_record_v1(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &address.semantic(),
        MemberPayloadRefV1::U64(1),
        &mut write,
    )
    .unwrap();
    let changed = tree::edit(
        root,
        &count.field_key,
        (Some(old.value_root), Some(count.value_root)),
        &mut ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io(),
        },
        &mut write,
        &mut || false,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: 1 << 20,
            labels: genesis::LABELS,
        },
    )
    .unwrap();
    let extra = write_member_record_v1(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        "orphan",
        MemberPayloadRefV1::Text("unqualified"),
        &mut write,
    )
    .unwrap();
    let changed = tree::append(
        changed,
        &extra.field_key,
        extra.value_root,
        &mut write,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: 1 << 20,
            labels: genesis::LABELS,
        },
    )
    .unwrap();
    assert_eq!(changed.leaf_count, before_leaves + 1);
    assert!(matches!(
        qualify(ArtifactConfigBodyKindV1::Module, &changed, &pages),
        Err(ValueTreeError::Invalid(_))
    ));
}

pub(super) fn family_in_pool(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<
    (
        ReservedValue<ArtifactConfigFamilyQualificationV1>,
        ReservedValue<ArtifactConfigBodyQualificationV1>,
    ),
    ValueTreeError,
> {
    use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut cancelled = || false;
    let cancel = RefCell::new(&mut cancelled);
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
    let mut value_io = io();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Artifacts),
        state: &state,
        values: ValueReadContext {
            source: pages,
            memory,
            class,
            io: &mut value_io,
        },
        key_work: &mut key_work,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled: || false,
    };
    let mut scan = StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 10_000,
    };
    let proof = qualify_artifact_config_family_v1(
        roots,
        &mut context,
        &mut scan,
        &mut StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 10_000,
        },
        &mut ArtifactConfigReadWorkV1 {
            cells_remaining: 10_000,
            max_payload_bytes: 1 << 20,
            json: JsonReadWorkV1 {
                cells_remaining: 10_000,
                json_nodes_remaining: 100,
                max_depth: 16,
                max_payload_bytes: 1 << 20,
            },
        },
    )?;
    proof.validate_binding(&roots.digest().unwrap())?;
    let mut other_roots = roots.clone();
    other_roots.simulation_tick += 1;
    assert!(
        proof
            .validate_binding(&other_roots.digest().unwrap())
            .is_err()
    );
    let child = open_qualified_artifact_config_body_v1(
        ArtifactConfigFieldV1::RegistryManifest,
        ArtifactConfigRecordKeyV1::Member("raw external key"),
        roots,
        &proof,
        &mut context,
    )?
    .unwrap();
    assert_eq!(child.kind(), ArtifactConfigBodyKindV1::Module);
    assert!(child.uses_budget(memory));
    Ok((proof, child))
}

pub(super) fn qualify_family(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    pages: &Pages,
) -> Result<(), ValueTreeError> {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    family_in_pool(roots, pages, &memory, MemoryClass::Recovery).map(|_| ())
}

#[test]
fn recovery_family_cannot_open_an_ordinary_body() {
    use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
    let memory = MemoryBudget::new(192 << 20, 4 << 20).unwrap();
    let _index = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages {
        nodes: Rc::new(RefCell::new(BTreeMap::new())),
    };
    let (roots, _) = seed(&memory, &mut pages);
    let (recovery, recovery_body) =
        family_in_pool(&roots, &pages, &memory, MemoryClass::Recovery).unwrap();
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut cancelled = || false;
    let cancel = RefCell::new(&mut cancelled);
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
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let result = open_qualified_artifact_config_body_v1(
        ArtifactConfigFieldV1::RegistryManifest,
        ArtifactConfigRecordKeyV1::Member("raw external key"),
        &roots,
        &recovery,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Artifacts),
            state: &state,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        },
    );
    assert!(
        matches!(result, Err(ValueTreeError::Invalid(_))),
        "Recovery family must not authorize Ordinary access: {result:?}"
    );
    let result = open_artifact_config_json_v1(
        &recovery_body,
        ArtifactConfigAddressV1 {
            section: "subscriptions",
            ordinal: Some(0),
            inner_ordinal: None,
            member: None,
            column: "filters",
        },
        &mut WorldFieldReadContextV1 {
            root: recovery_body.root(),
            state: &state,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        },
        &mut JsonReadWorkV1 {
            cells_remaining: 10_000,
            json_nodes_remaining: 100,
            max_depth: 16,
            max_payload_bytes: 1 << 20,
        },
    );
    assert!(
        matches!(result, Err(ValueTreeError::Invalid(_))),
        "Recovery body must not authorize Ordinary JSON access: {result:?}"
    );
    // Recovery is still usable in its own class; a new independent Ordinary
    // qualification is the supported way to start ordinary successor work.
    let recovery_json = json_in_pool(
        &recovery_body,
        ArtifactConfigAddressV1 {
            section: "subscriptions",
            ordinal: Some(0),
            inner_ordinal: None,
            member: None,
            column: "filters",
        },
        &pages,
        &memory,
        MemoryClass::Recovery,
    );
    assert_eq!(recovery_json.allocation_class(), MemoryClass::Recovery);
    let (_, ordinary) = family_in_pool(&roots, &pages, &memory, MemoryClass::Ordinary).unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
}

pub(super) fn json_in_pool(
    body: &ReservedValue<ArtifactConfigBodyQualificationV1>,
    address: ArtifactConfigAddressV1<'_>,
    pages: &Pages,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> ReservedValue<ArtifactConfigJsonQualificationV1> {
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut cancelled = || false;
    let cancel = RefCell::new(&mut cancelled);
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
    let mut value_io = io();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    open_artifact_config_json_v1(
        body,
        address,
        &mut WorldFieldReadContextV1 {
            root: body.root(),
            state: &state,
            values: ValueReadContext {
                source: pages,
                memory,
                class,
                io: &mut value_io,
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        },
        &mut JsonReadWorkV1 {
            cells_remaining: 10_000,
            json_nodes_remaining: 1000,
            max_depth: 16,
            max_payload_bytes: 1 << 20,
        },
    )
    .unwrap()
}

pub(super) fn seed(
    memory: &MemoryBudget,
    pages: &mut Pages,
) -> (
    oasis7_formal_protocol::incremental_execution::RootSetV1,
    BTreeMap<Vec<u8>, [u8; 32]>,
) {
    use oasis7_formal_protocol::{
        incremental_execution::RootSetV1, incremental_state::StateNamespaceV1,
    };
    let _bookkeeping = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let record = ModuleRecord {
        manifest: tests::module_fixture(),
        registered_at: u64::MAX,
        registered_by: String::new(),
        audit_event_id: Some(0),
    };
    let registry = ModuleRegistry {
        records: BTreeMap::from([("raw external key".to_owned(), record)]),
        active: BTreeMap::from([("unrelated id".to_owned(), "unregistered@version".to_owned())]),
    };
    let manifest = crate::Manifest {
        version: 0,
        content: serde_json::json!({"原值":"界"}),
    };
    let mut writes = Vec::with_capacity(32);
    let mut write_io = io();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: pages,
        memory,
        io: &mut write_io,
        key_work: &mut key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    prepare_artifact_config_genesis_v1(
        &ArtifactConfigSourceRefV1 {
            manifest: &manifest,
            registry: &registry,
            limits: &ModuleLimits::unbounded(),
            security: ArtifactSecurityPolicyRefV1 {
                allow_builtin_manifest_fallback: false,
                allow_identity_hash_signature: true,
                allow_local_finality_signing: false,
                allow_runtime_source_compile: true,
            },
        },
        &mut write,
        &mut write_work(),
        &mut |value| {
            assert!(writes.len() < 32);
            writes.push(value);
            Ok(())
        },
    )
    .unwrap();
    let mut roots = RootSetV1::empty("artifact test".to_owned(), [0; 32]).unwrap();
    let mut outer = roots.state_root(StateNamespaceV1::Artifacts).clone();
    let mut by_key = BTreeMap::new();
    for value in writes {
        let binding = WorldFieldKeyV1::from_bytes(&value.mutation.key).unwrap();
        let field = ArtifactConfigFieldV1::ALL
            .into_iter()
            .find(|field| field.name() == binding.name)
            .unwrap();
        let key =
            WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key).unwrap();
        assert_eq!(key.as_bytes(), value.mutation.key);
        assert_eq!(value.namespace, StateNamespaceV1::Artifacts);
        let id = value.mutation.after.unwrap();
        by_key.insert(value.mutation.key.clone(), id);
        outer = tree::append(
            outer,
            &key,
            id,
            &mut write,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: 1 << 20,
                labels: genesis::LABELS,
            },
        )
        .unwrap();
    }
    assert_eq!(outer.leaf_count, 16);
    roots.state[5] = outer;
    drop(registry);
    drop(manifest);
    (roots, by_key)
}

#[test]
fn full_genesis_qualifies_exact_eight_fields_and_rejects_split_registry_columns() {
    let memory = MemoryBudget::new(192 << 20, 0).unwrap();
    let _index = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let _bookkeeping = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages {
        nodes: Rc::new(RefCell::new(BTreeMap::new())),
    };
    let (mut roots, by_key) = seed(&memory, &mut pages);
    qualify_family(&roots, &pages).unwrap();
    // A second fully valid Module body bound to just one original column must
    // not be accepted as a complete registry record.
    let changed_record = ModuleRecord {
        manifest: tests::module_fixture(),
        registered_at: 0,
        registered_by: String::new(),
        audit_event_id: Some(0),
    };
    let child = produce(genesis::Body::Module(&changed_record), &memory, &mut pages);
    let source = pages.clone();
    let mut write_io = io();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut pages,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let field = ArtifactConfigFieldV1::RegistryRegisteredBy;
    let row = genesis::root_row(
        field.descriptor(),
        "raw external key",
        field.schema(),
        child,
        &mut write,
    )
    .unwrap();
    roots.state[5] = tree::edit(
        roots.state[5].clone(),
        &row.field_key,
        (Some(by_key[row.field_key.as_bytes()]), Some(row.value_root)),
        &mut ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io(),
        },
        &mut write,
        &mut || false,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: 1 << 20,
            labels: genesis::LABELS,
        },
    )
    .unwrap();
    assert!(matches!(
        qualify_family(&roots, &pages),
        Err(ValueTreeError::Invalid(_))
    ));
}
