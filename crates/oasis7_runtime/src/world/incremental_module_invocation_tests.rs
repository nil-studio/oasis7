//! Independent old World selection oracle and real original streaming consumer.
use super::*;
use crate::world::incremental_module_invocation::*;
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_material::*, incremental_state::*, incremental_value::*,
};
use oasis7_world_state::{
    incremental_field_member::*, incremental_field_overlay::*, incremental_field_read::*,
    incremental_json_records::*, incremental_runtime_artifact_config::*,
    incremental_runtime_artifact_content::*,
};
use std::io::{self, Write};

struct Originals<'a> {
    memory: &'a MemoryBudget,
    bytes: Arc<[u8]>,
    calls: usize,
    fail: bool,
    stop: Option<Rc<Cell<bool>>>,
}
struct ForbidValues<'a> {
    source: &'a Values,
    roots: Vec<[u8; 32]>,
}
struct StateCarrier(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for StateCarrier {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct StateSource<'a> {
    source: &'a Values,
    memory: &'a MemoryBudget,
    class: MemoryClass,
}
impl MerkleNodeReader for StateSource<'_> {
    type Node = StateCarrier;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let id: [u8; 32] = hex::decode(id)
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|_| "state digest".to_owned())?;
        self.source
            .read_node(&id, max, self.memory, self.class)
            .map(|v| Some(StateCarrier(v)))
            .map_err(|e| e.to_string())
    }
}
struct CountWriter<'a> {
    inner: &'a mut Values,
    puts: usize,
}
impl ValueNodeWriter for CountWriter<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.puts += 1;
        self.inner.put_node(id, bytes)
    }
}
impl ValueNodeReader for ForbidValues<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert!(!self.roots.contains(id), "unused instance payload read");
        self.source.read_node(id, max, memory, class)
    }
}
impl OriginalObjectReader for Originals<'_> {
    type Error = io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        panic!("invocation must use streamed verified original")
    }
}
impl OriginalObjectStreamReader for Originals<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        out: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        self.calls += 1;
        assert!(memory.same_pool(self.memory));
        assert_eq!(id, blake3::hash(&self.bytes).as_bytes());
        assert_eq!(max, self.bytes.len() as u64);
        if self.fail {
            return Err(io::ErrorKind::ConnectionReset.into());
        }
        for chunk in self.bytes.chunks(3) {
            out.write_all(chunk)?;
        }
        if let Some(stop) = &self.stop {
            stop.set(true);
        }
        Ok(self.bytes.len() as u64)
    }
}
fn quota() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    }
}
fn read_work() -> ArtifactConfigReadWorkV1 {
    ArtifactConfigReadWorkV1 {
        cells_remaining: 100_000,
        max_payload_bytes: 2 << 20,
        json: JsonReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 10_000,
            max_depth: 32,
            max_payload_bytes: 2 << 20,
        },
    }
}
fn write_work() -> ArtifactConfigWriteWorkV1 {
    ArtifactConfigWriteWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 1 << 20,
        json: JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 10_000,
            max_json_depth: 32,
            max_pending_node_bytes: 1 << 20,
        },
    }
}
fn manifest() -> crate::ModuleManifest {
    crate::ModuleManifest {
        module_id: "internal ID differs".into(),
        name: "selected".into(),
        version: "internal version".into(),
        kind: crate::ModuleKind::Reducer,
        role: crate::ModuleRole::Domain,
        wasm_hash: "arbitrary preserved artifact key".into(),
        interface_version: "wasm-1".into(),
        abi_contract: crate::ModuleAbiContract::default(),
        exports: vec!["reduce".into()],
        subscriptions: vec![],
        required_caps: vec![],
        artifact_identity: None,
        limits: crate::ModuleLimits::unbounded(),
    }
}
fn config_limits() -> ModuleInvocationPreparationLimitsV1 {
    ModuleInvocationPreparationLimitsV1 {
        selection_heap_bytes: 128 << 10,
        max_text_bytes: 4096,
        manifest: SelectedModuleManifestLimitsV1 {
            heap_bytes: 256 << 10,
            json_heap_bytes: 128 << 10,
            max_collection_items: 1000,
        },
        manifest_json_bytes_remaining: 1 << 20,
    }
}
fn writer<'a>(
    values: &'a mut Values,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    keys: &'a mut oasis7_world_state::incremental_semantic_key::SemanticKeyBudget,
) -> MemberRecordWriteContext<'a, Values, impl FnMut() -> bool> {
    MemberRecordWriteContext {
        sink: values,
        memory,
        io,
        key_work: keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 8192,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    }
}
fn keys() -> oasis7_world_state::incremental_semantic_key::SemanticKeyBudget {
    oasis7_world_state::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    }
}
fn mirror(pages: &mut Pages, values: &Values) {
    for (id, bytes) in values.pages.borrow().iter() {
        if bytes.starts_with(b"oasis7.iws.state-page.v1\0") {
            pages.0.insert(hex::encode(id), bytes.to_vec());
        }
    }
}
#[test]
fn root_invocation_matches_old_world_selection_and_real_original_without_world_cache() {
    let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
    // Explicit fixture-container charge, outside operation measurements.
    let _fixture = memory.reserve(96 << 20, MemoryClass::Ordinary).unwrap();
    let (mut world, mut parent, mut pages, mut values) = setup(&memory);
    world.module_registry.records.clear();
    world.module_registry.active.clear();
    world.module_artifact_bytes.clear();
    world.module_artifacts.clear();
    let wasm: Arc<[u8]> = Arc::from(&b"\0asm\x01\0\0\0"[..]);
    let manifest = manifest();
    for id in ["module", "global", "blocked"] {
        world.module_registry.records.insert(
            crate::ModuleRegistry::record_key(id, "2.0"),
            crate::ModuleRecord {
                manifest: manifest.clone(),
                registered_at: 9,
                registered_by: "r".repeat(1 << 20),
                audit_event_id: Some(0),
            },
        );
        world.module_registry.active.insert(id.into(), "2.0".into());
    }
    world
        .module_registry
        .active
        .insert("broken".into(), "missing".into());
    world
        .module_artifact_bytes
        .insert(manifest.wasm_hash.clone(), wasm.clone());
    // Deliberately no module_artifacts membership: actual lookup uses bytes map.
    let mut active = instance();
    active.active = true;
    world
        .state
        .module_instances
        .insert("map-key".into(), active);
    let mut inactive = instance();
    inactive.module_id = "blocked".into();
    world
        .state
        .module_instances
        .insert("inactive-key".into(), inactive);
    seed_instances(&world, &mut parent, &mut pages, &mut values, &memory);
    let ids = [
        "map-key",
        "inactive-key",
        "blocked",
        "global",
        "missing",
        "broken",
    ];
    let expected = ids.map(|id| {
        world.active_module_invocation_for_id(id).map(|v| {
            v.map(|v| {
                (
                    v.instance_id,
                    v.module_id,
                    v.install_target,
                    serde_json::to_vec(&v.manifest).unwrap(),
                    crate::util::hash_json(&v.manifest).unwrap(),
                )
            })
        })
    });
    let mut writes = Vec::new();
    let mut emit = |write: ReservedValue<ExecutionWriteV1>| {
        writes.push((*write).clone());
        Ok::<_, ValueTreeError>(())
    };
    world
        .prepare_incremental_artifact_config_genesis_v1(
            &mut writer(&mut values, &memory, &mut quota(), &mut keys()),
            &mut write_work(),
            &mut emit,
        )
        .unwrap();
    world
        .prepare_iws_runtime_artifact_content_genesis_v1(
            &mut writer(&mut values, &memory, &mut quota(), &mut keys()),
            &mut 100,
            &mut RuntimeArtifactOriginalWorkV1 {
                objects_remaining: 100,
                bytes_remaining: 1 << 20,
                max_object_bytes: 1 << 20,
            },
            &mut |original, bytes| {
                assert_eq!(original.id, *blake3::hash(bytes).as_bytes());
                Ok(())
            },
            &mut emit,
        )
        .unwrap();
    oasis7_world_state::incremental_installed_module_targets::prepare_installed_module_targets_genesis_v1(
        &world.state,&mut writer(&mut values,&memory,&mut quota(),&mut keys()),&mut 100,&mut emit).unwrap();
    for ns in [
        Ns::Business,
        Ns::Allocator,
        Ns::Scheduler,
        Ns::Authorization,
        Ns::Effects,
        Ns::Artifacts,
    ] {
        let mut selected: Vec<_> = writes
            .iter()
            .filter(|w| w.namespace == ns)
            .map(|w| w.mutation.clone())
            .collect();
        selected.sort_by(|left, right| left.key.cmp(&right.key));
        if selected.is_empty() {
            continue;
        }
        let delta = StateDeltaV1::build(
            parent.roots.state_root(ns).clone(),
            selected,
            &pages,
            16 << 20,
        )
        .unwrap();
        parent.roots.state[ns as usize - 1] = delta.after;
        for node in delta.new_nodes {
            pages.0.insert(node.digest, node.bytes);
        }
    }
    mirror(&mut pages, &values);
    for (id, bytes) in &pages.0 {
        let id = hex::decode(id).unwrap().try_into().unwrap();
        values
            .put_node(
                id,
                memory
                    .reserve(bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(bytes.clone()),
            )
            .unwrap();
    }
    let source = values.clone();
    let state = StateSource {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
    };
    let instances = recovery::qualify(
        &parent.roots,
        &pages,
        &values,
        &memory,
        MemoryClass::Ordinary,
    );
    let mut original = Originals {
        memory: &memory,
        bytes: wasm.clone(),
        calls: 0,
        fail: false,
        stop: None,
    };
    let mut q = quota();
    let mut k = keys();
    let mut read = WorldFieldReadContextV1 {
        root: parent.roots.state_root(Ns::Artifacts),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut q,
        },
        key_work: &mut k,
        semantic_bytes: 8192,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let config = qualify_artifact_config_family_v1(
        &parent.roots,
        &mut read,
        &mut scan(),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    let content = qualify_runtime_artifact_content_v1(
        &parent.roots,
        &mut read,
        &mut scan(),
        &mut original,
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 100,
            bytes_remaining: 1 << 20,
            max_object_bytes: 1 << 20,
        },
    )
    .unwrap();
    drop(world);
    let mut probe =
        ExecutionTransaction::begin(parent.clone(), [9; 32], &state, &memory, limits()).unwrap();
    let forbidden = [F::WasmHash, F::Owner, F::InstalledAt]
        .into_iter()
        .map(|field| {
            probe
                .read(
                    Ns::Business,
                    ModuleInstanceKeyV1::new("map-key", field)
                        .unwrap()
                        .as_bytes(),
                )
                .unwrap()
                .unwrap()
        })
        .collect();
    let selected_values = ForbidValues {
        source: &source,
        roots: forbidden,
    };
    for (id, expected) in ids.into_iter().zip(expected) {
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 128,
                max_writes: 16,
                max_facts: 16,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        let guard = tx.guard();
        let before = original.calls;
        let mut io = quota();
        let mut keys = keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &selected_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 8192,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let result = prepare_root_module_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id: id,
                parent_roots: &parent.roots,
                instances: &instances,
                config: &config,
                content: &content,
                current_active: None,
                current_record: None,
                guard: &guard,
                live: &live,
            },
            &state,
            &mut overlay,
            &mut scan(),
            &mut read_work(),
            &mut original,
            &mut RuntimeArtifactOriginalWorkV1 {
                objects_remaining: 1,
                bytes_remaining: 1 << 20,
                max_object_bytes: 1 << 20,
            },
            &mut config_limits(),
        );
        match expected {
            Ok(Some((iid, mid, target, bytes, hash))) => {
                let result = result.unwrap().unwrap();
                assert_eq!(result.instance_id(), iid);
                assert_eq!(result.module_id(), mid);
                assert_eq!(result.install_target(), &target);
                assert_eq!(serde_json::to_vec(result.manifest()).unwrap(), bytes);
                assert_eq!(hex::encode(result.manifest_sha256()), hash);
                assert_eq!(result.wasm(), &*wasm);
                assert_eq!(original.calls - before, 1);
            }
            Ok(None) => {
                assert!(result.unwrap().is_none());
                assert_eq!(original.calls, before);
            }
            Err(_) => {
                assert!(result.is_err());
                let guard = tx.guard();
                assert!(tx.seal(&guard, &live, 0).is_err());
                assert_eq!(original.calls, before);
            }
        }
    }
    // Genuine typed same-TX record update, then independent new-record create.
    // Both use the returned current-body scopes. A 165-byte read ceiling makes
    // re-bootstrap of the unrelated 1MiB registered_by column fail explicitly.
    {
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 256,
                max_writes: 32,
                max_facts: 16,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        let mut io = quota();
        let mut key_work = keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &selected_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut key_work,
            semantic_bytes: 8192,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let key = crate::ModuleRegistry::record_key("module", "2.0");
        let body = open_artifact_config_body_overlay_v1(
            ArtifactConfigSelectedBodyRequestV1 {
                field: ArtifactConfigFieldV1::RegistryManifest,
                key: ArtifactConfigRecordKeyV1::Member(&key),
                family: &config,
                parent_roots: &parent.roots,
                current: None,
            },
            &state,
            &mut overlay,
            &mut scan(),
            &mut read_work(),
        )
        .unwrap()
        .unwrap();
        let guard = overlay.transaction.guard();
        let changed = change_qualified_artifact_config_cell_v1(
            ArtifactConfigCellChangeV1 {
                request: ArtifactConfigCellRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryRegisteredAt,
                    key: ArtifactConfigRecordKeyV1::Member(&key),
                    address: ArtifactConfigAddressV1 {
                        section: "record",
                        ordinal: None,
                        inner_ordinal: None,
                        member: None,
                        column: "registered_at",
                    },
                    family: &config,
                    qualification: &body,
                    max_payload_bytes: 8,
                },
                expected: ArtifactConfigScalarRefV1::Unsigned(9),
                after: ArtifactConfigScalarRefV1::Unsigned(10),
                max_pending_node_bytes: 1 << 20,
            },
            &mut overlay,
            &mut writer(&mut values, &memory, &mut quota(), &mut keys()),
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        let mut bounded = read_work();
        bounded.max_payload_bytes = 165;
        let guard = overlay.transaction.guard();
        let prepared = prepare_root_module_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id: "map-key",
                parent_roots: &parent.roots,
                instances: &instances,
                config: &config,
                content: &content,
                current_active: None,
                current_record: Some(&changed),
                guard: &guard,
                live: &live,
            },
            &state,
            &mut overlay,
            &mut scan(),
            &mut bounded,
            &mut original,
            &mut RuntimeArtifactOriginalWorkV1 {
                objects_remaining: 1,
                bytes_remaining: 1 << 20,
                max_object_bytes: 1 << 20,
            },
            &mut config_limits(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(prepared.instance_id(), "module#stable-2");
        let new_record = crate::ModuleRecord {
            manifest: manifest.clone(),
            registered_at: 11,
            registered_by: "new caller input".repeat(1 << 16),
            audit_event_id: None,
        };
        let new_key = crate::ModuleRegistry::record_key("newglobal", "2.0");
        let guard = overlay.transaction.guard();
        let created = change_qualified_artifact_config_record_v1(
            ArtifactConfigRecordChangeV1 {
                field: ArtifactConfigFieldV1::RegistryManifest,
                key: ArtifactConfigRecordKeyV1::Member(&new_key),
                family: &config,
                expected: None,
                after: Some(ArtifactConfigBodyRefV1::Module(&new_record)),
            },
            &mut overlay,
            &mut writer(&mut values, &memory, &mut quota(), &mut keys()),
            &mut write_work(),
            &guard,
            &live,
        )
        .unwrap();
        assert!(created.changed);
        let guard = overlay.transaction.guard();
        let active = change_qualified_artifact_config_record_v1(
            ArtifactConfigRecordChangeV1 {
                field: ArtifactConfigFieldV1::RegistryActive,
                key: ArtifactConfigRecordKeyV1::Member("newglobal"),
                family: &config,
                expected: None,
                after: Some(ArtifactConfigBodyRefV1::Active("2.0")),
            },
            &mut overlay,
            &mut writer(&mut values, &memory, &mut quota(), &mut keys()),
            &mut write_work(),
            &guard,
            &live,
        )
        .unwrap();
        assert!(active.changed);
        let guard = overlay.transaction.guard();
        let prepared = prepare_root_module_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id: "newglobal",
                parent_roots: &parent.roots,
                instances: &instances,
                config: &config,
                content: &content,
                current_active: active.qualification.as_ref(),
                current_record: created.qualification.as_ref(),
                guard: &guard,
                live: &live,
            },
            &state,
            &mut overlay,
            &mut scan(),
            &mut bounded,
            &mut original,
            &mut RuntimeArtifactOriginalWorkV1 {
                objects_remaining: 1,
                bytes_remaining: 1 << 20,
                max_object_bytes: 1 << 20,
            },
            &mut config_limits(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(prepared.instance_id(), "newglobal");
        assert_eq!(
            hex::encode(prepared.manifest_sha256()),
            crate::util::hash_json(&manifest).unwrap()
        );
        assert_eq!(prepared.wasm(), &*wasm);
    }
    // A genuinely successful typed point/put precedes every negative prepare.
    // Same parent/root scopes remain valid while the one transaction overlays
    // the unused installed-at scalar; preparation must neither read it nor
    // allow a later source/class/guard/limit failure to seal that prefix.
    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    // A separate full Recovery bootstrap is not part of an ordinary point's
    // budget. Its real private mint class must not silently become Ordinary.
    let recovery_config = {
        let recovery_state = StateSource {
            source: &source,
            memory: &memory,
            class: MemoryClass::Recovery,
        };
        let mut io = quota();
        let mut keys = keys();
        let mut read = WorldFieldReadContextV1 {
            root: parent.roots.state_root(Ns::Artifacts),
            state: &recovery_state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 8192,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        qualify_artifact_config_family_v1(
            &parent.roots,
            &mut read,
            &mut scan(),
            &mut scan(),
            &mut read_work(),
        )
        .unwrap()
    };
    for mode in 0..8 {
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [5; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 128,
                max_writes: 16,
                max_facts: 16,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        let stale = tx.guard();
        let mut counted = CountWriter {
            inner: &mut values,
            puts: 0,
        };
        assert!(
            crate::change_module_instance_field_v1(
                crate::ModuleInstanceFieldChangeV1 {
                    qualification: &instances,
                    map_key: "map-key",
                    field: F::InstalledAt,
                    expected: S::U64(u64::MAX),
                    after: S::U64(u64::MAX - 1),
                    max_scalar_bytes: 4096
                },
                &mut tx,
                &source,
                &mut BodyMutationContext {
                    sink: &mut counted,
                    memory: &memory,
                    io: &mut quota(),
                    cancelled: || false
                }
            )
            .unwrap()
        );
        assert!(counted.puts > 0, "typed prefix must perform actual puts");
        let guard = if mode == 2 { stale } else { tx.guard() };
        let mut wrong = parent.roots.clone();
        wrong.simulation_tick += 1;
        let stop = Rc::new(Cell::new(false));
        let mut original = Originals {
            memory: if mode == 5 { &foreign } else { &memory },
            bytes: wasm.clone(),
            calls: 0,
            fail: mode == 0,
            stop: (mode == 1).then(|| stop.clone()),
        };
        let mut io = quota();
        let mut keys = keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &selected_values,
                memory: &memory,
                class: if mode == 7 {
                    MemoryClass::Recovery
                } else {
                    MemoryClass::Ordinary
                },
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 8192,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || stop.get(),
        };
        let mut limits = config_limits();
        if mode == 4 {
            limits.manifest_json_bytes_remaining = 0;
        }
        let result = prepare_root_module_invocation_v1(
            ModuleInvocationPreparationRequestV1 {
                invocation_id: "map-key",
                parent_roots: if mode == 3 { &wrong } else { &parent.roots },
                instances: &instances,
                config: if mode == 6 { &recovery_config } else { &config },
                content: &content,
                current_active: None,
                current_record: None,
                guard: &guard,
                live: &live,
            },
            &state,
            &mut overlay,
            &mut scan(),
            &mut read_work(),
            &mut original,
            &mut RuntimeArtifactOriginalWorkV1 {
                objects_remaining: 1,
                bytes_remaining: 1 << 20,
                max_object_bytes: 1 << 20,
            },
            &mut limits,
        );
        match (mode, result) {
            (
                0,
                Err(ModuleInvocationPreparationErrorV1::Original(
                    RuntimeArtifactOriginalErrorV1::Source(e),
                )),
            ) => assert_eq!(e.kind(), io::ErrorKind::ConnectionReset),
            (
                1,
                Err(ModuleInvocationPreparationErrorV1::Original(
                    RuntimeArtifactOriginalErrorV1::Value(ValueTreeError::Cancelled),
                )),
            ) => {}
            (2, Err(ModuleInvocationPreparationErrorV1::Body(BodyMutationError::Execution(_)))) => {
            }
            (3, Err(ModuleInvocationPreparationErrorV1::Value(ValueTreeError::Invalid(_)))) => {}
            (
                4,
                Err(ModuleInvocationPreparationErrorV1::Value(ValueTreeError::ResourceLimited(_))),
            ) => {}
            (
                5,
                Err(ModuleInvocationPreparationErrorV1::Original(
                    RuntimeArtifactOriginalErrorV1::Value(ValueTreeError::Invalid(_)),
                )),
            ) => {}
            (6 | 7, Err(ModuleInvocationPreparationErrorV1::Value(ValueTreeError::Invalid(_)))) => {
            }
            (mode, _) => panic!("wrong preparation failure mode {mode}"),
        }
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}
