#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::*,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    MaterialLedgerId, MaterialTransitPriority, WorldState, incremental_field_member::*,
    incremental_field_overlay::*, incremental_field_read::*, incremental_industry_jobs::*,
};
use serde_json::json;
#[path = "support/incremental_industry_fixture.rs"]
mod fixture;
use fixture::{Directory, physical};
fn physical_scope() -> PageIoBudget {
    let mut io = physical();
    io.reads_remaining = 2_000_000;
    io
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
fn source() -> WorldState {
    let mut world = WorldState::default();
    let mut recipe:oasis7_world_state::RecipeJobState=serde_json::from_value(json!({"job_id":17,"requester_agent_id":" agent/界 ","factory_id":"factory","recipe_id":"recipe","accepted_batches":u32::MAX,"consume":[{"kind":"ore","amount":i64::MIN}],"produce":[],"byproducts":[],"power_required":i64::MIN,"duration_ticks":u32::MAX,"consume_ledger":"world","output_ledger":"world","bottleneck_tags":["dup","dup"],"logistics_route_ids":[],"logistics_path_ids":[],"ready_at":u64::MAX})).unwrap();
    recipe.output_ledger = MaterialLedgerId::Factory(" original/厂 ".into());
    let mut transit:oasis7_world_state::MaterialTransitJobState=serde_json::from_value(json!({"job_id":19,"requester_agent_id":"transit","from_ledger":"world","to_ledger":"world","kind":"ore","amount":i64::MAX,"distance_km":i64::MIN,"loss_bps":0,"route_id":"原route","path_id":"path","route_ids":["route","route"],"tariff_electricity_total":-1,"reroute_count":u32::MAX,"ready_at":u64::MAX})).unwrap();
    transit.priority = MaterialTransitPriority::Urgent;
    let completion=serde_json::from_value(json!({"job_id":21,"requester_agent_id":"receipt","factory_id":"different factory","recipe_id":"recipe","accepted_batches":u32::MAX,"produce":[{"kind":"product","amount":i64::MAX}],"byproducts":[],"output_ledger":"world","bottleneck_tags":[],"logistics_route_ids":[],"logistics_path_ids":[]})).unwrap();
    let failure=serde_json::from_value(json!({"action_id":23,"requester_agent_id":"failure","factory_id":"factory","recipe_id":"recipe","blocker_kind":"产品","blocker_detail":" original ","disposition_kind":"lost","consumed_inputs":[{"kind":"ore","amount":i64::MIN}],"lost_inputs":[],"consumed_power":i64::MIN,"lost_power":i64::MAX,"next_action":"recheck","next_recheck":u64::MAX})).unwrap();
    let settlement=serde_json::from_value(json!({"job_id":25,"path_id":"路径","route_ids":["dup","dup"],"tariff_electricity_total":i64::MIN,"owner_payouts":{"":i64::MIN,"原owner":i64::MAX},"governance_tax_electricity":i64::MAX})).unwrap();
    world.pending_recipe_jobs.insert(u64::MAX, recipe);
    world.pending_material_transits.insert(2, transit);
    world.recipe_completion_receipts.insert(3, completion);
    world
        .factory_production_failure_dispositions
        .insert(4, failure);
    world.logistics_settlement_receipts.insert(5, settlement);
    world
}
fn work() -> IndustryWorkBudgetV1 {
    IndustryWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 100_000,
        },
    }
}
struct EmptyState<'a>(&'a MemoryBudget);
impl MerkleNodeReader for EmptyState<'_> {
    type Node = Vec<u8>;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.0)
    }
    fn read_node(&self, _: &str, _: usize) -> Result<Option<Self::Node>, String> {
        Ok(None)
    }
}
struct StagedPages<'a> {
    fallback: Option<oasis7_distfs::incremental_page_store::ValuePageSession<'a>>,
    memory: &'a MemoryBudget,
    staged: std::collections::BTreeMap<[u8; 32], Vec<u8>>,
    order: Vec<[u8; 32]>,
}
impl<'a> StagedPages<'a> {
    fn new(memory: &'a MemoryBudget) -> Self {
        Self {
            fallback: None,
            memory,
            staged: std::collections::BTreeMap::new(),
            order: Vec::new(),
        }
    }
    fn with_store(store: &'a IncrementalPageStore, memory: &'a MemoryBudget) -> Self {
        Self {
            fallback: Some(store.value_session(physical_scope())),
            memory,
            staged: std::collections::BTreeMap::new(),
            order: Vec::new(),
        }
    }
    fn into_reachable<'r>(
        mut self,
        state_roots: impl IntoIterator<Item = &'r StateRootV1>,
        value_roots: impl IntoIterator<Item = [u8; 32]>,
    ) -> Vec<([u8; 32], Vec<u8>)> {
        let mut live = std::collections::BTreeSet::new();
        let mut queue = Vec::new();
        let mut enqueue = |id: [u8; 32],
                           staged: &std::collections::BTreeMap<[u8; 32], Vec<u8>>,
                           queue: &mut Vec<[u8; 32]>| {
            if staged.contains_key(&id) && live.insert(id) {
                queue.push(id);
            }
        };
        for root in state_roots {
            if root.leaf_count > 0 {
                let id = *blake3::Hash::from_hex(&root.node_root).unwrap().as_bytes();
                enqueue(id, &self.staged, &mut queue);
            }
        }
        for id in value_roots {
            enqueue(id, &self.staged, &mut queue);
        }
        while let Some(id) = queue.pop() {
            let bytes = self.staged[&id].clone();
            if bytes.starts_with(b"oasis7.iws.state-page.v1\0") {
                let links = StateNodeWriteV1 {
                    digest: blake3::Hash::from_bytes(id).to_hex().to_string(),
                    bytes,
                }
                .verified_links()
                .unwrap();
                match links {
                    StatePageLinksV1::Branch { children, .. } => {
                        for child in children.into_iter().flatten() {
                            let child_id = *blake3::Hash::from_hex(&child).unwrap().as_bytes();
                            enqueue(child_id, &self.staged, &mut queue);
                        }
                    }
                    StatePageLinksV1::Value(entry) => {
                        enqueue(entry.value, &self.staged, &mut queue);
                    }
                }
            } else {
                match verified_value_links(&id, &bytes).unwrap() {
                    ValuePageLinksV1::Root(node) => enqueue(node.id, &self.staged, &mut queue),
                    ValuePageLinksV1::Index(children) => {
                        for child in children {
                            enqueue(child.id, &self.staged, &mut queue);
                        }
                    }
                    ValuePageLinksV1::Data { logical_len } => {
                        let data = &bytes[bytes.len() - logical_len as usize..];
                        if data.starts_with(FIELD_MEMBER_DOMAIN)
                            && data.len() >= FIELD_MEMBER_HEADER_BYTES + RECORD_ROOT_BYTES
                            && data[FIELD_MEMBER_DOMAIN.len() + 2]
                                == MemberPayloadCodecV1::RecordRoot as u8
                        {
                            let record_bytes = &data[data.len() - RECORD_ROOT_BYTES..];
                            RecordRootRefV1::from_canonical_bytes(record_bytes).unwrap();
                            let child = StateRootV1::from_canonical_bytes(
                                &record_bytes[RECORD_ROOT_BYTES - STATE_ROOT_BYTES..],
                            )
                            .unwrap();
                            if child.leaf_count > 0 {
                                let child_id =
                                    *blake3::Hash::from_hex(&child.node_root).unwrap().as_bytes();
                                enqueue(child_id, &self.staged, &mut queue);
                            }
                        }
                    }
                }
            }
        }
        let mut pages = Vec::with_capacity(live.len());
        for id in self.order {
            if live.contains(&id) {
                let mut bytes = self.staged.remove(&id).unwrap();
                bytes.shrink_to_fit();
                pages.push((id, bytes));
            }
        }
        pages
    }
    fn take_all(&mut self) -> Vec<([u8; 32], Vec<u8>)> {
        let order = std::mem::take(&mut self.order);
        let mut pages = Vec::with_capacity(order.len());
        for id in order {
            let mut bytes = self.staged.remove(&id).unwrap();
            bytes.shrink_to_fit();
            pages.push((id, bytes));
        }
        pages
    }
}
impl ValueNodeWriter for StagedPages<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if !bytes.uses_budget(self.memory)
            || bytes.capacity() > bytes.reserved_bytes()
            || bytes.is_empty()
            || bytes.len() > VALUE_MAX_NODE_BYTES
            || blake3::hash(&bytes).as_bytes() != &id
        {
            return Err(ValueTreeError::Invalid("staged page accounting/hash/bound"));
        }
        if let Some(existing) = self.staged.get(&id) {
            if existing.as_slice() != bytes.as_slice() {
                return Err(ValueTreeError::Invalid("staged page collision"));
            }
            return Ok(());
        }
        let mut copy = Vec::with_capacity(bytes.len());
        copy.extend_from_slice(&bytes);
        self.staged.insert(id, copy);
        self.order.push(id);
        Ok(())
    }
}
impl StatePageWriter for StagedPages<'_> {}
impl ValueNodeReader for StagedPages<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if !self.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("physical session memory pool"));
        }
        if let Some(bytes) = self.staged.get(id) {
            if bytes.len() > max_bytes {
                return Err(ValueTreeError::ResourceLimited("requested page bound"));
            }
            let charge = memory.reserve(bytes.len(), class)?;
            let mut copy = Vec::with_capacity(bytes.len());
            copy.extend_from_slice(bytes);
            return Ok(std::sync::Arc::new(charge.retain(copy)));
        }
        match &self.fallback {
            Some(fallback) => fallback.read_node(id, max_bytes, memory, class),
            None => Err(ValueTreeError::Io(std::io::ErrorKind::NotFound.into())),
        }
    }
}
struct SeedSnapshot {
    roots: RootSetV1,
    pages: Vec<([u8; 32], Vec<u8>)>,
}
fn prepare_seed_with_budget(memory: &MemoryBudget, source: &WorldState) -> SeedSnapshot {
    let mut values = StagedPages::new(memory);
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(32);
    let mut remaining = 100_000;
    let count = prepare_industry_jobs_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut remaining,
        &mut work(),
        &mut |row| {
            assert!(rows.len() < 32);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let expected = source.pending_recipe_jobs.len()
        + source.pending_material_transits.len()
        + source.recipe_completion_receipts.len()
        + source.factory_production_failure_dispositions.len()
        + source.logistics_settlement_receipts.len();
    assert_eq!(count, expected as u64);
    assert_eq!(rows.len(), 10 + expected);
    let mut roots = RootSetV1::empty("industry-source".into(), [0x41; 32]).unwrap();
    let empty_state = EmptyState(memory);
    for ns in [StateNamespaceV1::Scheduler, StateNamespaceV1::Authorization] {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == ns)
            .map(|r| StateWriteV1 {
                key: r.mutation.key.clone(),
                before: None,
                after: r.mutation.after,
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(
                StateRootV1::empty(ns).unwrap(),
                writes,
                &empty_state,
                8 << 20,
            )
            .unwrap(),
        );
        values.put_state_pages(&delta, memory).unwrap();
        let index = roots.state.iter().position(|r| r.namespace == ns).unwrap();
        roots.state[index] = delta.after.clone();
    }
    let pages = values.into_reachable(&roots.state, []);
    SeedSnapshot { roots, pages }
}
fn prepare_seed(source: &WorldState) -> SeedSnapshot {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    prepare_seed_with_budget(&memory, source)
}
fn flush_pages(store: &IncrementalPageStore, memory: &MemoryBudget, pages: &[([u8; 32], Vec<u8>)]) {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let mut io = physical_scope();
    let mut never = || false;
    let mut writer = store
        .buffered_page_writer(&mut io, &cancel, &mut never)
        .unwrap();
    for (id, bytes) in pages {
        let mut copy = Vec::with_capacity(bytes.len());
        copy.extend_from_slice(bytes);
        let charge = memory.reserve(copy.len(), MemoryClass::Ordinary).unwrap();
        writer.put_node(*id, charge.retain(copy)).unwrap();
    }
    writer.finish().unwrap();
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> RootSetV1 {
    let snapshot = prepare_seed_with_budget(memory, source);
    flush_pages(store, memory, &snapshot.pages);
    snapshot.roots
}
fn seed_default(store: &IncrementalPageStore, memory: &MemoryBudget) -> RootSetV1 {
    static DEFAULT_SEED: std::sync::OnceLock<SeedSnapshot> = std::sync::OnceLock::new();
    let snapshot = DEFAULT_SEED.get_or_init(|| prepare_seed(&source()));
    flush_pages(store, memory, &snapshot.pages);
    snapshot.roots.clone()
}
#[test]
fn industry_genesis_original_columns_continue_in_small_cold_pools() {
    use IndustryColumnV1 as C;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _source = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            match compression {
                PageCompression::Raw => seed(&store, &memory, &source()),
                PageCompression::Zstd => seed_default(&store, &memory),
            }
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _caller = memory.reserve(16 << 10, class).unwrap();
            let state = store.state_session(physical_scope(), class).unwrap();
            let values = store.value_session(physical_scope());
            let qualification = qualify_industry_family_v1(
                &roots,
                &mut IndustryFamilyReadContextV1 {
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    max_payload_bytes: 4096,
                    outer_scan: &mut StateScanBudget {
                        nodes_remaining: 1_000_000,
                        read_bytes_remaining: 256 << 20,
                        leaves_remaining: 10_000,
                    },
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("industry physical qualification"))
                    },
                    cancelled: || false,
                },
                &mut work(),
            )
            .unwrap();
            assert_eq!(qualification.record_counts(), [1, 1, 1, 1, 1]);
            qualification
                .validate_binding(&roots.digest().unwrap())
                .unwrap();
            oracle::assert_complete_original_rows(&store, &memory, &roots, class);
            for (kind, id, column, expected) in [
                (
                    IndustryRecordKindV1::Recipe,
                    u64::MAX,
                    C::RecordReadyAt,
                    IndustryValueV1::Unsigned(u64::MAX),
                ),
                (
                    IndustryRecordKindV1::Transit,
                    2,
                    C::RecordPriority,
                    IndustryValueV1::Unsigned(1),
                ),
                (
                    IndustryRecordKindV1::Completion,
                    3,
                    C::RecordJobId,
                    IndustryValueV1::Unsigned(21),
                ),
                (
                    IndustryRecordKindV1::Failure,
                    4,
                    C::RecordNextRecheck,
                    IndustryValueV1::Unsigned(u64::MAX),
                ),
                (
                    IndustryRecordKindV1::Settlement,
                    5,
                    C::RecordGovernanceTaxElectricity,
                    IndustryValueV1::Signed(i64::MAX),
                ),
            ] {
                let key = IndustryRecordKeyV1 { kind, id };
                let root = roots
                    .state
                    .iter()
                    .find(|r| r.namespace == key.namespace())
                    .unwrap();
                let mut out = [0; 64];
                let actual = read_industry_cell_v1(
                    key,
                    IndustryCellV1::Column(column),
                    &mut WorldFieldReadContextV1 {
                        root,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096,
                        },
                        semantic_bytes: 4096,
                        record_bytes: 8192,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|_| ValueTreeError::Invalid("industry physical read"))
                        },
                        cancelled: || false,
                    },
                    64,
                    &mut &mut out[..],
                )
                .unwrap();
                assert_eq!(actual, IndustryCellReadV1::Value(expected));
            }
        }
    }
}

fn parent(roots: RootSetV1) -> ExecutionParent {
    ExecutionParent {
        commit: [0x34; 32],
        roots,
        authority: AuthorityBinding {
            token: [0x56; 32],
            epoch: 7,
        },
    }
}
fn tx_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 128,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
fn qualify(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
) -> ReservedValue<IndustryFamilyQualificationV1> {
    let state = store.state_session(physical_scope(), class).unwrap();
    let values = store.value_session(physical_scope());
    qualify_industry_family_v1(
        roots,
        &mut IndustryFamilyReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 8192,
            max_payload_bytes: 4096,
            outer_scan: &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 256 << 20,
                leaves_remaining: 10_000,
            },
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("industry fixture qualification"))
            },
            cancelled: || false,
        },
        &mut work(),
    )
    .unwrap()
}
#[test]
fn industry_terminal_delta_receiver_and_buffered_transfer_continue_from_roots_only() {
    use IndustryColumnV1 as C;
    use IndustryRecordKindV1 as K;
    use IndustryValueRefV1 as V;
    let mut prepared_overlay = None;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let target_dir = Directory::new();
        let target_path = target_dir.0.join("pages");
        let after_roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store =
                IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), compression)
                    .unwrap();
            let roots = seed_default(&store, &memory);
            let qualification = qualify(&store, &memory, &roots, MemoryClass::Ordinary);
            let parent = parent(roots);
            let live = parent.fence().unwrap();
            let (record, staged) = prepared_overlay.get_or_insert_with(|| {
                let source = source();
                let mut created_recipe = source.pending_recipe_jobs[&u64::MAX].clone();
                created_recipe.industrial_root_ref = Some(
                    serde_json::from_value(json!({
                        "world_binding":" raw/界 ","domain":"d","schema":"s","id":"embedded-id",
                        "revision":u64::MAX,"digest":"Original-Digest"
                    }))
                    .unwrap(),
                );
                created_recipe.committed_recipe_origin = Some(serde_json::from_value(json!({
                    "submission":{"verified_player_id":" player ","public_key":"RawKey",
                    "auth_nonce":u64::MAX,"hosted_registration_nonce":" raw nonce ",
                    "requester_agent_id":"submit-agent","factory_id":"submit-factory","recipe_id":"submit-recipe"},
                    "consensus_action_id":u64::MAX,"consensus_submitter_player_id":"submitter",
                    "action_payload_hash":"OriginalHash","committed_height":u64::MAX,"action_root":"OriginalRoot"
                })).unwrap());
                let mut created_completion = source.recipe_completion_receipts[&3].clone();
                created_completion.committed_recipe_origin =
                    created_recipe.committed_recipe_origin.clone();
                let state = store
                    .state_session(physical_scope(), MemoryClass::Ordinary)
                    .unwrap();
                let values = store.value_session(physical_scope());
                let mut writer = StagedPages::with_store(&store, &memory);
                let mut transaction = ExecutionTransaction::begin(
                    parent.clone(),
                    [0x78; 32],
                    &state,
                    &memory,
                    tx_limits(),
                )
                .unwrap();
                {
                    let mut read = WorldFieldOverlayContextV1 {
                        transaction: &mut transaction,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 16 << 20,
                        },
                        semantic_bytes: 4096,
                        record_bytes: 8192,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|_| ValueTreeError::Invalid("industry fixture state"))
                        },
                        cancelled: || false,
                    };
                    let mut write = MemberRecordWriteContext {
                        sink: &mut writer,
                        memory: &memory,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 16 << 20,
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 4096,
                            payload_bytes: 4096,
                            record_bytes: 8192,
                        },
                        cancelled: || false,
                    };
                    let mut work = work();
                    let guard = read.transaction.guard();
                    assert!(
                        change_industry_cell_overlay_v1(
                            IndustryCellChangeV1 {
                                key: IndustryRecordKeyV1 {
                                    kind: K::Failure,
                                    id: 4
                                },
                                cell: IndustryCellV1::Column(C::RecordLostPower),
                                expected: Some(V::Signed(i64::MAX)),
                                after: Some(V::Signed(17)),
                                max_current_payload_bytes: 8
                            },
                            &mut read,
                            &mut write,
                            &mut work,
                            &guard,
                            &live
                        )
                        .unwrap()
                    );
                    for (kind, id) in [(K::Recipe, u64::MAX), (K::Transit, 2)] {
                        let key = IndustryRecordKeyV1 { kind, id };
                        let expected =
                            read_industry_record_root_overlay_v1(key, &mut read).unwrap();
                        assert!(expected.is_some());
                        let guard = read.transaction.guard();
                        assert!(
                            delete_industry_record_overlay_v1(
                                IndustryRecordDeleteV1 {
                                    key,
                                    expected_root: expected,
                                    qualification: &qualification
                                },
                                &mut read,
                                &mut write,
                                &mut work,
                                &guard,
                                &live
                            )
                            .unwrap()
                        );
                    }
                    for (kind, id, after) in [
                        (K::Recipe, 60, IndustryRecordRefV1::Recipe(&created_recipe)),
                        (
                            K::Transit,
                            20,
                            IndustryRecordRefV1::Transit(&source.pending_material_transits[&2]),
                        ),
                        (
                            K::Completion,
                            30,
                            IndustryRecordRefV1::Completion(&created_completion),
                        ),
                        (
                            K::Settlement,
                            40,
                            IndustryRecordRefV1::Settlement(
                                &source.logistics_settlement_receipts[&5],
                            ),
                        ),
                    ] {
                        let guard = read.transaction.guard();
                        create_industry_record_overlay_v1(
                            IndustryRecordCreateV1 {
                                key: IndustryRecordKeyV1 { kind, id },
                                after,
                                qualification: &qualification,
                            },
                            &mut read,
                            &mut write,
                            &mut work,
                            &guard,
                            &live,
                        )
                        .unwrap();
                    }
                }
                let guard = transaction.guard();
                let sealed = transaction.seal(&guard, &live, 1).unwrap();
                assert_eq!(sealed.record.writes.len(), 7);
                let staged = writer.into_reachable(
                    [],
                    sealed.record.writes.iter().filter_map(|w| w.mutation.after),
                );
                (sealed.record.clone(), staged)
            });
            flush_pages(&store, &memory, staged);
            let receiver = store.value_session(physical_scope());
            let verified = verify_industry_transition_v1(
                &parent,
                [0x78; 32],
                record,
                &qualification,
                &mut IndustryTransitionReadContextV1 {
                    values: ValueReadContext {
                        source: &receiver,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    max_payload_bytes: 4096,
                    cancelled: || false,
                },
                &mut work(),
            )
            .unwrap();
            assert_eq!(verified.qualification().record_counts(), [1, 1, 2, 1, 2]);
            let after = verified.transition().record.after.clone();
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let mut io = physical_scope();
            let mut never = || false;
            let mut persisted = store
                .buffered_page_writer(&mut io, &cancel, &mut never)
                .unwrap();
            for node in &verified.transition().new_nodes {
                let id = *blake3::hash(&node.bytes).as_bytes();
                assert_eq!(node.digest, blake3::hash(&node.bytes).to_hex().to_string());
                let charge = memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                persisted
                    .put_node(id, charge.retain(node.bytes.clone()))
                    .unwrap();
            }
            persisted.finish().unwrap();
            let target =
                IncrementalPageStore::create(&target_path, memory.clone(), compression).unwrap();
            let cancelled_dir = Directory::new();
            let cancelled_store = IncrementalPageStore::create(
                &cancelled_dir.0.join("pages"),
                memory.clone(),
                compression,
            )
            .unwrap();
            let final_phase = std::cell::Cell::new(false);
            let recipe_baseline = std::cell::Cell::new(0);
            let mut after_final_publish = || {
                final_phase.get()
                    && std::fs::read_dir(cancelled_store.root().join("recipes"))
                        .unwrap()
                        .count()
                        > recipe_baseline.get()
            };
            let mut cancelled_io = physical_scope();
            let mut cancelled_sink = cancelled_store
                .buffered_page_writer(&mut cancelled_io, &cancel, &mut after_final_publish)
                .unwrap();
            let mut target_io = physical_scope();
            let mut never = || false;
            let mut sink = target
                .buffered_page_writer(&mut target_io, &cancel, &mut never)
                .unwrap();
            let transfer_state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let transfer_values = store.value_session(physical_scope());
            let transferred = transfer_industry_family_pages_v1(
                &after,
                &mut IndustryFamilyReadContextV1 {
                    state: &transfer_state,
                    values: ValueReadContext {
                        source: &transfer_values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    max_payload_bytes: 4096,
                    outer_scan: &mut StateScanBudget {
                        nodes_remaining: 1_000_000,
                        read_bytes_remaining: 256 << 20,
                        leaves_remaining: 10_000,
                    },
                    take_state_error: || {
                        transfer_state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("industry transfer source"))
                    },
                    cancelled: || false,
                },
                &mut work(),
                // Seven complete records (including two newly created pending jobs).
                40_000,
                &mut flush::MirroredPages {
                    accepted: &mut sink,
                    cancelled: &mut cancelled_sink,
                    memory: &memory,
                },
            )
            .unwrap();
            assert!(sink.pending_reserved_bytes() > 0);
            assert!(cancelled_sink.pending_reserved_bytes() > 0);
            recipe_baseline.set(
                std::fs::read_dir(cancelled_store.root().join("recipes"))
                    .unwrap()
                    .count(),
            );
            final_phase.set(true);
            let rejected_acceptance = cancelled_sink
                .finish()
                .map(|()| transferred.qualification());
            assert!(matches!(
                rejected_acceptance,
                Err(oasis7_distfs::incremental_page_store::PageStoreError::Cancelled)
            ));
            assert!(
                std::fs::read_dir(cancelled_store.root().join("recipes"))
                    .unwrap()
                    .count()
                    > recipe_baseline.get()
            );
            sink.finish().unwrap();
            assert_eq!(transferred.qualification().record_counts(), [1, 1, 2, 1, 2]);
            assert!(transferred.page_count() > 0);
            eprintln!(
                "industry complete transfer codec={compression:?} pages={}",
                transferred.page_count()
            );
            after
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::open(
            &target_path,
            memory.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let qualification = qualify(&store, &memory, &after_roots, MemoryClass::Recovery);
        assert_eq!(qualification.record_counts(), [1, 1, 2, 1, 2]);
    }
}

#[path = "support/incremental_industry_oracle.rs"]
mod oracle;

#[path = "support/incremental_industry_boundaries.rs"]
mod boundaries;
#[test]
fn industry_noops_cancel_after_public_resolver_and_poison_real_prefix() {
    boundaries::noops_cancel_after_public_resolver_and_poison_real_prefix();
}
#[test]
fn industry_state_and_value_carriers_require_caller_pool_and_capacity() {
    boundaries::state_and_value_carriers_require_the_actual_caller_pool_and_capacity();
}

#[path = "support/incremental_industry_malformed.rs"]
mod malformed;
#[test]
fn industry_malformed_raw_and_changed_closures_are_rejected() {
    malformed::malformed_raw_and_changed_closures_are_rejected();
}

#[path = "support/incremental_industry_scaling.rs"]
mod scaling;
#[test]
fn industry_fixed_scalar_receiver_cost_excludes_unchanged_text_and_vectors() {
    scaling::fixed_scalar_receiver_cost_excludes_unchanged_text_and_vectors();
}

#[path = "support/incremental_industry_missing.rs"]
mod missing;
#[test]
fn industry_generic_complete_top_roots_do_not_prove_missing_typed_child() {
    missing::generic_complete_top_roots_do_not_prove_missing_typed_child();
}

#[path = "support/incremental_industry_put_boundaries.rs"]
mod put_boundaries;
#[test]
fn industry_final_real_root_put_cancel_or_failure_never_seals_prefix() {
    put_boundaries::final_real_root_put_cancel_or_failure_never_seals_the_successful_prefix();
}

#[path = "support/incremental_industry_flush.rs"]
mod flush;
