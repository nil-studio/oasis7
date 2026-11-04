use super::*;
use crate::{
    incremental_field_member::*,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer},
};
use oasis7_agent_api::world_resource::{ChainResourceDelta, ChainResourceManifest};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::collections::BTreeMap;
pub(super) fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
pub(super) fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    }
}
pub(super) fn scans() -> [StateScanBudget; RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1] {
    std::array::from_fn(|_| StateScanBudget {
        nodes_remaining: 2_000_000,
        leaves_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
    })
}
pub(super) fn work() -> RuntimeArtifactResourcesWorkV1 {
    RuntimeArtifactResourcesWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
    }
}
/// Only the original borrowed adapter is supplied. No shadow manifest codec.
type Entry = (Vec<String>, BTreeMap<String, ModuleArtifactIdentity>);
type ModuleSet = BTreeMap<String, Entry>;
pub(super) struct Builtins(pub BTreeMap<String, ModuleSet>);
pub(super) struct Set<'a>(&'a ModuleSet);
impl BuiltinReleaseModuleSetSourceV1 for Set<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn for_each_module<'a, E, F>(&'a self, mut visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, BuiltinReleaseEntryRefV1<'a>) -> Result<(), E>,
    {
        for (k, (tokens, ids)) in self.0 {
            visit(
                k,
                BuiltinReleaseEntryRefV1 {
                    hash_tokens: tokens,
                    artifact_identities: ids,
                },
            )?
        }
        Ok(())
    }
}
impl BuiltinReleaseManifestSourceV1 for Builtins {
    type ModuleSet<'a> = Set<'a>;
    fn len(&self) -> usize {
        self.0.len()
    }
    fn for_each_module_set<'a, E, F>(&'a self, mut visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, Self::ModuleSet<'a>) -> Result<(), E>,
    {
        for (k, v) in &self.0 {
            visit(k, Set(v))?
        }
        Ok(())
    }
}
pub(super) fn seed(
    memory: &MemoryBudget,
    manifest: &ChainResourceManifest,
    delta: Option<&ChainResourceDelta>,
    builtin: &Builtins,
) -> (Pages, RootSetV1) {
    let pages = Pages::with_limit(memory, 65_536);
    let _phase = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let mut out = Writer(&pages);
    let mut vi = io();
    let mut kw = keys();
    let mut rows = Vec::with_capacity(6);
    prepare_runtime_artifact_resources_genesis_v1(
        &RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: manifest,
            latest_chain_resource_delta: delta,
            builtin_release_manifest: builtin,
        },
        &mut MemberRecordWriteContext {
            sink: &mut out,
            memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 2 << 20,
                payload_bytes: 2 << 20,
                record_bytes: 4 << 20,
            },
            cancelled: || false,
        },
        &mut work(),
        &mut |row: ReservedValue<ExecutionWriteV1>| {
            assert!(rows.len() < 6);
            rows.push(row.mutation.clone());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(rows.len(), 6);
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
        rows,
        &pages,
        8 << 20,
    )
    .unwrap();
    for n in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes();
        out.put_node(
            id,
            memory
                .reserve(n.bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(n.bytes.clone()),
        )
        .unwrap();
    }
    let mut roots = RootSetV1::empty("runtime-artifact-resources".into(), [8; 32]).unwrap();
    roots.state[5] = delta.after;
    (pages, roots)
}
pub(super) fn qualify(
    pages: &Pages,
    roots: &RootSetV1,
    memory: &MemoryBudget,
) -> Result<ReservedValue<RuntimeArtifactResourcesQualificationV1>, ValueTreeError> {
    let mut vi = io();
    let mut kw = keys();
    qualify_runtime_artifact_resources_v1(
        roots,
        &mut RuntimeArtifactResourcesReadContextV1 {
            state: pages,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
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
}

pub(super) fn rewrite<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
    memory: &MemoryBudget,
    pages: &Pages,
    root: &StateRootV1,
    record: RuntimeArtifactRecordV1,
    key: &K,
    payload: MemberPayloadRefV1<'_>,
) -> StateRootV1 {
    let _phase = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut vi = io();
    let mut kw = keys();
    let mut cancel = || false;
    let mut values = ValueReadContext {
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut vi,
    };
    let mut records = crate::incremental_inventory_value_read::InventoryValueReader {
        values: &mut values,
        key_work: &mut kw,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        cancelled: &mut cancel,
    };
    let path = records
        .key(record.descriptor(), WorldFieldRecordKindV1::Entry, key)
        .unwrap();
    let labels = crate::incremental_record_tree::RecordTreeLabelsV1 {
        read: "resource test read",
        pending: "resource test pending",
        tree: "resource test tree",
        digest: "resource test digest",
        read_allowance: "resource test allowance",
        admission: "resource test admission",
        bound: "resource test bound",
        producer_pool: "resource test pool",
        page: "resource test page",
        write_allowance: "resource test write",
    };
    let old = crate::incremental_record_tree::lookup(root, &path.0, &mut records, labels).unwrap();
    let mut out = Writer(pages);
    let mut wi = io();
    let mut wk = keys();
    let mut ctx = MemberRecordWriteContext {
        sink: &mut out,
        memory,
        io: &mut wi,
        key_work: &mut wk,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 2 << 20,
            payload_bytes: 2 << 20,
            record_bytes: 4 << 20,
        },
        cancelled: || false,
    };
    let after = write_member_record_v1(
        record.descriptor(),
        WorldFieldRecordKindV1::Entry,
        key,
        payload,
        &mut ctx,
    )
    .unwrap();
    crate::incremental_record_tree::edit(
        root.clone(),
        &path.0,
        (old, Some(after.value_root)),
        records.values,
        &mut ctx,
        &mut || false,
        crate::incremental_record_tree::RecordTreeWritePolicyV1 {
            max_pending_node_bytes: 8 << 20,
            labels,
        },
    )
    .unwrap()
}
pub(super) fn root_ref<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
    schema: [u8; 32],
    key: &K,
    root: StateRootV1,
    memory: &MemoryBudget,
) -> oasis7_formal_protocol::incremental_record_root::RecordRootRefV1 {
    let key = crate::incremental_semantic_key::hash_semantic_key_v1(
        key,
        memory,
        MemoryClass::Ordinary,
        &mut keys(),
        &mut || false,
    )
    .unwrap();
    oasis7_formal_protocol::incremental_record_root::RecordRootRefV1::new(schema, key.digest, root)
        .unwrap()
}
