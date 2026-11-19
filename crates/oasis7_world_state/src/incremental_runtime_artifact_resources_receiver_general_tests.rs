use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_typed_record_test_support::{ClassedPages, Pages, Writer},
};
use oasis7_agent_api::{
    world_action::{MaterialLedgerId, ResourceKind},
    world_geometry::ChunkCoord,
    world_resource::*,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{collections::BTreeMap, sync::Arc};
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [2; 32],
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 4,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 128,
        max_writes: 16,
        max_facts: 4,
        max_pending_node_bytes: 8 << 20,
    }
}
fn digest(root: &StateRootV1) -> [u8; 32] {
    *blake3::Hash::from_hex(&root.digest().unwrap())
        .unwrap()
        .as_bytes()
}
fn fixture_chunk() -> ChainChunkResourceManifestEntry {
    ChainChunkResourceManifestEntry {
        schema_version: "schema".into(),
        world_id: "world".into(),
        chain_id: "chain".into(),
        world_seed: 7,
        chunk_generation_schema_version: "chunk schema".into(),
        coord: ChunkCoord { x: 1, y: 2, z: 3 },
        seed: 8,
        chunk_status: ChainChunkResourceStatus::Committed,
        fragment_count: 1,
        block_count: 1,
        fragment_refs: vec![ChainFragmentResourceRef::default()],
        chunk_budget_total_hash: "total".into(),
        chunk_budget_remaining_hash: "remaining".into(),
        total_by_element_g: BTreeMap::from([(FragmentElementKind::Iron, 100)]),
        remaining_by_element_g: BTreeMap::from([(FragmentElementKind::Iron, 80)]),
        commit_ref: ChainResourceCommitRef::default(),
        manifest_hash: "original".into(),
    }
}
struct Scope<'a, R> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
    values: &'a R,
}
impl<R: ValueNodeReader> Scope<'_, R> {
    fn accept(
        &self,
        family: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
        p: &ExecutionParent,
        record: &TransitionRecordV1,
        scan: &mut [StateScanBudget; RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1],
        max_payload: u64,
    ) -> Result<ReservedValue<RuntimeArtifactResourcesQualificationV1>, WorldFieldOverlayErrorV1>
    {
        let verifier = TransitionVerifier {
            parent: p,
            operation: [7; 32],
            source: self.pages,
            memory: self.memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let mut vi = io();
        let mut kw = keys();
        accept_runtime_artifact_resources_transition_v1(
            family,
            &verifier,
            record,
            &mut WorldFieldReadContextV1 {
                root: &record.after.state[5],
                state: self.pages,
                values: ValueReadContext {
                    source: self.values,
                    memory: self.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut vi,
                },
                key_work: &mut kw,
                semantic_bytes: 2 << 20,
                record_bytes: 4 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            scan,
            max_payload,
            &mut work(),
        )
    }
    fn body(
        &self,
        roots: &RootSetV1,
        family: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
        change: RuntimeArtifactBodyChangeV1<'_>,
    ) -> (ExecutionParent, ReservedValue<PreparedTransition>) {
        let p = parent(roots);
        let live = p.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(p.clone(), [7; 32], self.pages, self.memory, limits())
                .unwrap();
        let guard = tx.guard();
        let mut vi = io();
        let mut kw = keys();
        let mut wi = io();
        let mut wk = keys();
        let mut out = Writer(self.pages);
        assert!(
            change_runtime_artifact_body_overlay_v1(
                change,
                family,
                &mut WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: self.values,
                        memory: self.memory,
                        class: MemoryClass::Ordinary,
                        io: &mut vi
                    },
                    key_work: &mut kw,
                    semantic_bytes: 2 << 20,
                    record_bytes: 4 << 20,
                    take_state_error: || None,
                    cancelled: || false
                },
                &mut MemberRecordWriteContext {
                    sink: &mut out,
                    memory: self.memory,
                    io: &mut wi,
                    key_work: &mut wk,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 2 << 20,
                        payload_bytes: 2 << 20,
                        record_bytes: 4 << 20
                    },
                    cancelled: || false
                },
                &mut work(),
                &guard,
                &live
            )
            .unwrap()
        );
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, roots.simulation_tick + 1).unwrap();
        (p, sealed)
    }
    fn builtin(
        &self,
        roots: &RootSetV1,
        family: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
        change: RuntimeBuiltinEntryChangeV1<'_>,
    ) -> (ExecutionParent, ReservedValue<PreparedTransition>) {
        let p = parent(roots);
        let live = p.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(p.clone(), [7; 32], self.pages, self.memory, limits())
                .unwrap();
        let guard = tx.guard();
        let mut vi = io();
        let mut kw = keys();
        let mut wi = io();
        let mut wk = keys();
        let mut out = Writer(self.pages);
        assert!(
            change_runtime_builtin_entry_overlay_v1(
                change,
                family,
                &mut WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: self.values,
                        memory: self.memory,
                        class: MemoryClass::Ordinary,
                        io: &mut vi
                    },
                    key_work: &mut kw,
                    semantic_bytes: 2 << 20,
                    record_bytes: 4 << 20,
                    take_state_error: || None,
                    cancelled: || false
                },
                &mut MemberRecordWriteContext {
                    sink: &mut out,
                    memory: self.memory,
                    io: &mut wi,
                    key_work: &mut wk,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 2 << 20,
                        payload_bytes: 2 << 20,
                        record_bytes: 4 << 20
                    },
                    cancelled: || false
                },
                &mut work(),
                &guard,
                &live
            )
            .unwrap()
        );
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, roots.simulation_tick + 1).unwrap();
        (p, sealed)
    }
    fn publish(&self, p: &ExecutionParent, record: &TransitionRecordV1) -> RootSetV1 {
        let verified = TransitionVerifier {
            parent: p,
            operation: [7; 32],
            source: self.pages,
            memory: self.memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        }
        .verify(record)
        .unwrap();
        let mut out = Writer(self.pages);
        for node in &verified.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            out.put_node(
                id,
                self.memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
        }
        record.after.clone()
    }
    fn field(
        &self,
        roots: &RootSetV1,
        field: RuntimeArtifactFieldV1,
    ) -> Option<ReservedValue<RuntimeArtifactChildRootV1>> {
        let mut vi = io();
        let mut kw = keys();
        inspect_runtime_artifact_record_root_v1(
            field,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[5],
                state: self.pages,
                values: ValueReadContext {
                    source: self.values,
                    memory: self.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut vi,
                },
                key_work: &mut kw,
                semantic_bytes: 2 << 20,
                record_bytes: 4 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
        )
        .unwrap()
    }
    fn child<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &self,
        parent: RuntimeArtifactRecordV1,
        child: RuntimeArtifactRecordV1,
        root: &ReservedValue<RuntimeArtifactChildRootV1>,
        key: &K,
    ) -> ReservedValue<RuntimeArtifactChildRootV1> {
        let mut vi = io();
        let mut kw = keys();
        read_runtime_artifact_record_reference_v1(
            parent,
            child,
            root,
            key,
            &mut WorldFieldReadContextV1 {
                root: root.root(),
                state: self.pages,
                values: ValueReadContext {
                    source: self.values,
                    memory: self.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut vi,
                },
                key_work: &mut kw,
                semantic_bytes: 2 << 20,
                record_bytes: 4 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
        )
        .unwrap()
        .unwrap()
    }
    fn value<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &self,
        record: RuntimeArtifactRecordV1,
        root: &StateRootV1,
        key: &K,
    ) -> [u8; 32] {
        let mut vi = io();
        let mut kw = keys();
        let mut ctx = WorldFieldReadContextV1 {
            root,
            state: self.pages,
            values: ValueReadContext {
                source: self.values,
                memory: self.memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let path = ctx
            .key(record.descriptor(), WorldFieldRecordKindV1::Entry, key)
            .unwrap();
        ctx.lookup(&path.0).unwrap().unwrap()
    }
}
struct Unreadable;
impl ValueNodeReader for Unreadable {
    fn read_node(
        &self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
        _: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        panic!("NoOp opened a resource payload")
    }
}
#[test]
fn resource_receiver_accepts_none_noop_and_requires_explicit_ordinary_readmission() {
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let manifest = ChainResourceManifest::default();
    let builtin = Builtins(Default::default());
    let (pages, roots) = seed(&memory, &manifest, None, &builtin);
    let source = ClassedPages::new(&pages, &memory, MemoryClass::Recovery);
    let mut vi = io();
    let mut kw = keys();
    let recovery = qualify_runtime_artifact_resources_v1(
        &roots,
        &mut RuntimeArtifactResourcesReadContextV1 {
            state: &source,
            values: ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            max_payload_bytes: 2 << 20,
            scans: &mut scans(),
            take_state_error: || None,
            cancelled: || false,
        },
        &mut work(),
    )
    .unwrap();
    let p = parent(&roots);
    let live = p.fence().unwrap();
    let tx = ExecutionTransaction::begin(p.clone(), [7; 32], &pages, &memory, limits()).unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let scope = Scope {
        pages: &pages,
        memory: &memory,
        values: &Unreadable,
    };
    assert!(matches!(
        scope.accept(&recovery, &p, &sealed.record, &mut scans(), 0),
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
    ));
    let family =
        readmit_runtime_artifact_resources_ordinary_v1(&recovery, &roots, &memory, || false)
            .unwrap();
    let mut zero = std::array::from_fn(|_| StateScanBudget {
        nodes_remaining: 0,
        leaves_remaining: 0,
        read_bytes_remaining: 0,
    });
    let after = scope
        .accept(&family, &p, &sealed.record, &mut zero, 0)
        .unwrap();
    assert_eq!(after.record_counts(), [1, 0, 1]);
}
#[test]
fn resource_receiver_accepts_none_some_none_and_internal_option_changes() {
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let manifest = ChainResourceManifest::default();
    let builtin = Builtins(Default::default());
    let (pages, mut roots) = seed(&memory, &manifest, None, &builtin);
    let mut family = qualify(&pages, &roots, &memory).unwrap();
    let scope = Scope {
        pages: &pages,
        memory: &memory,
        values: &pages,
    };
    let mut delta = ChainResourceDelta::empty_for_manifest("world", &manifest.manifest_hash, 0, 0);
    let entries = vec![
        ChainResourceDeltaEntry::RuntimeResource {
            kind: ResourceKind::Electricity,
            delta: 1,
            resulting_balance: 1,
        },
        ChainResourceDeltaEntry::MaterialLedger {
            ledger_id: MaterialLedgerId::World,
            material_kind: "iron".into(),
            delta: 2,
            resulting_balance: 2,
        },
        ChainResourceDeltaEntry::ChunkResource {
            coord: ChunkCoord { x: 1, y: 2, z: 3 },
            element: FragmentElementKind::Iron,
            total_delta_g: 1,
            remaining_delta_g: 1,
            resulting_remaining_g: 1,
            remaining_after_hash: "remaining".into(),
            chunk_remaining_after_hash: "chunk".into(),
        },
    ];
    for stage in 0..4 {
        if stage == 1 {
            delta.entries = entries.clone();
            delta.action_id = Some(7);
            delta.event_id = Some(8);
            delta.commit_block_hash = Some("block".into());
            delta.tick = 1;
        }
        if stage == 2 {
            delta.entries.truncate(1);
            delta.action_id = None;
            delta.event_id = None;
            delta.commit_block_hash = None;
            delta.tick = 2;
        }
        let current = scope
            .field(&roots, RuntimeArtifactFieldV1::LatestChainResourceDelta)
            .as_ref()
            .map(|r| digest(r.root()));
        let (p, sealed) = scope.body(
            &roots,
            &family,
            RuntimeArtifactBodyChangeV1::ReplaceLatestDelta {
                expected_root: current,
                after: if stage == 3 { None } else { Some(&delta) },
            },
        );
        let after = scope
            .accept(&family, &p, &sealed.record, &mut scans(), 4096)
            .unwrap();
        assert_eq!(after.record_counts(), [1, u64::from(stage != 3), 1]);
        roots = scope.publish(&p, &sealed.record);
        family = after;
    }
    qualify(&pages, &roots, &memory).expect("saved None successor must independently reopen");
}
struct Guarded<'a> {
    pages: &'a Pages,
    forbidden: [[u8; 32]; 3],
}
impl ValueNodeReader for Guarded<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert!(
            !self.forbidden.contains(id),
            "resource receiver opened an unchanged MiB sibling"
        );
        ValueNodeReader::read_node(self.pages, id, max, memory, class)
    }
}
#[test]
fn resource_receiver_changes_all_nested_shapes_without_reading_large_unchanged_siblings() {
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(64 << 20, MemoryClass::Ordinary).unwrap();
    let mut manifest = ChainResourceManifest::default();
    manifest.generation_algorithm_id = "large original algorithm ".repeat(48_000);
    manifest
        .resource_balances
        .insert(ResourceKind::Electricity, 10);
    manifest.material_ledgers.insert(
        MaterialLedgerId::World,
        BTreeMap::from([("iron".into(), 5)]),
    );
    manifest
        .generated_chunks
        .insert("original chunk key".into(), fixture_chunk());
    let mut ids = BTreeMap::from([(
        "original artifact key".into(),
        ModuleArtifactIdentity {
            source_hash: "source".into(),
            build_manifest_hash: "build".into(),
            signer_node_id: "signer".into(),
            signature_scheme: "ed25519".into(),
            artifact_signature: "s".repeat(1 << 20),
        },
    )]);
    let tokens = vec!["h".repeat(1 << 20)];
    let builtin = Builtins(BTreeMap::from([(
        "set".into(),
        BTreeMap::from([("large".into(), (tokens.clone(), ids.clone()))]),
    )]));
    let (pages, mut roots) = seed(&memory, &manifest, None, &builtin);
    let mut family = qualify(&pages, &roots, &memory).unwrap();
    let raw = Scope {
        pages: &pages,
        memory: &memory,
        values: &pages,
    };
    let manifest_root = raw
        .field(&roots, RuntimeArtifactFieldV1::ChainResourceManifest)
        .unwrap();
    let builtin_root = raw
        .field(&roots, RuntimeArtifactFieldV1::BuiltinReleaseManifest)
        .unwrap();
    let set = raw.child(
        RuntimeArtifactRecordV1::BuiltinReleaseManifest,
        RuntimeArtifactRecordV1::BuiltinModuleSet,
        &builtin_root,
        &("module_sets", "set"),
    );
    let entry = raw.child(
        RuntimeArtifactRecordV1::BuiltinModuleSet,
        RuntimeArtifactRecordV1::BuiltinReleaseEntry,
        &set,
        &("modules", "large"),
    );
    let identity = raw.child(
        RuntimeArtifactRecordV1::BuiltinReleaseEntry,
        RuntimeArtifactRecordV1::ModuleArtifactIdentity,
        &entry,
        &("artifact_identities", "original artifact key"),
    );
    let guarded = Guarded {
        pages: &pages,
        forbidden: [
            raw.value(
                RuntimeArtifactRecordV1::ChainResourceManifest,
                manifest_root.root(),
                &"generation_algorithm_id",
            ),
            raw.value(
                RuntimeArtifactRecordV1::BuiltinReleaseEntry,
                entry.root(),
                &("hash_tokens", 0u64),
            ),
            raw.value(
                RuntimeArtifactRecordV1::ModuleArtifactIdentity,
                identity.root(),
                &"artifact_signature",
            ),
        ],
    };
    let scope = Scope {
        pages: &pages,
        memory: &memory,
        values: &guarded,
    };
    manifest
        .resource_balances
        .insert(ResourceKind::Electricity, 20);
    manifest
        .material_ledgers
        .get_mut(&MaterialLedgerId::World)
        .unwrap()
        .insert("iron".into(), 7);
    let chunk = manifest
        .generated_chunks
        .get_mut("original chunk key")
        .unwrap();
    chunk.commit_ref.event_id = Some(7);
    chunk.fragment_refs.push(ChainFragmentResourceRef {
        fragment_id: "new".into(),
        location_id: "site".into(),
        profile_hash: "profile".into(),
        budget_total_hash: "total".into(),
        budget_remaining_hash: "remaining".into(),
    });
    chunk.fragment_count = 2;
    chunk.chunk_status = ChainChunkResourceStatus::Exhausted;
    chunk
        .remaining_by_element_g
        .insert(FragmentElementKind::Iron, 70);
    let (p, sealed) = scope.body(
        &roots,
        &family,
        RuntimeArtifactBodyChangeV1::ReplaceManifest {
            expected_root: digest(manifest_root.root()),
            after: &manifest,
        },
    );
    let after = scope
        .accept(&family, &p, &sealed.record, &mut scans(), 4096)
        .unwrap();
    roots = scope.publish(&p, &sealed.record);
    family = after;
    ids.get_mut("original artifact key").unwrap().source_hash = "new source".into();
    let (p, sealed) = scope.builtin(
        &roots,
        &family,
        RuntimeBuiltinEntryChangeV1 {
            module_set: "set",
            module_id: "large",
            expected_entry_root: Some(digest(entry.root())),
            after: Some(BuiltinReleaseEntryRefV1 {
                hash_tokens: &tokens,
                artifact_identities: &ids,
            }),
        },
    );
    let after = scope
        .accept(&family, &p, &sealed.record, &mut scans(), 4096)
        .unwrap();
    roots = scope.publish(&p, &sealed.record);
    family = after;
    let small_tokens = vec!["small".into()];
    let empty_ids = BTreeMap::new();
    let (p, sealed) = scope.builtin(
        &roots,
        &family,
        RuntimeBuiltinEntryChangeV1 {
            module_set: "set",
            module_id: "other",
            expected_entry_root: None,
            after: Some(BuiltinReleaseEntryRefV1 {
                hash_tokens: &small_tokens,
                artifact_identities: &empty_ids,
            }),
        },
    );
    let after = scope
        .accept(&family, &p, &sealed.record, &mut scans(), 4096)
        .unwrap();
    roots = scope.publish(&p, &sealed.record);
    after
        .validate_binding(&roots.digest().unwrap(), MemoryClass::Ordinary)
        .unwrap();
    qualify(&pages, &roots, &memory)
        .expect("all nested original record shapes independently reopen");
}

#[path = "incremental_runtime_artifact_resources_receiver_malformed_tests.rs"]
mod malformed;
