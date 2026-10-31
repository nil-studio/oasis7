#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_material_records::*,
};
use serde_json::json;
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
const PROFILE: &str = " outer-profile\0界 ";
fn physical_scope() -> PageIoBudget {
    let mut io = physical();
    // All three bootstrap qualifications plus thirty independent column
    // queries share this one finite session. Measured Raw work exhausted
    // 1M operations with over 210MiB of the unchanged byte budget left.
    io.reads_remaining = 1_250_000;
    io
}
fn source() -> WorldState {
    let mut source = WorldState::default();
    source.material_profiles.insert(PROFILE.into(),serde_json::from_value(json!({"kind":" raw-kind\0界 ","tier":255,"category":"","stack_limit":i64::MIN,"transport_loss_class":"high","decay_bps_per_tick":i64::MAX,"default_priority":"urgent"})).unwrap());
    for (outer, inner, route) in [(0, u64::MAX, None), (u64::MAX, 17, Some(""))] {
        // Construct the original typed enum directly: the storage contract
        // preserves it even when a string-wire parser rejects an empty ID.
        source.direct_material_transfer_receipts.insert(
            outer,
            oasis7_runtime::MaterialTransferReceiptV1 {
                transfer_id: inner,
                requester_agent_id: " original requester\0界 ".into(),
                from_ledger: oasis7_runtime::MaterialLedgerId::Agent(String::new()),
                to_ledger: oasis7_runtime::MaterialLedgerId::Factory(" original factory ".into()),
                kind: String::new(),
                amount: i64::MIN,
                distance_km: i64::MAX,
                priority: oasis7_runtime::MaterialTransitPriority::Urgent,
                route_id: route.map(str::to_owned),
            },
        );
    }
    source
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 1000,
    }
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> RootSetV1 {
    let mut values = store.value_session(physical_scope());
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(16);
    let mut records = 1000;
    let count = prepare_material_records_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1024,
                record_bytes: 4096,
            },
            cancelled: || false,
        },
        &mut records,
        &mut MaterialWorkBudgetV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 2 << 20,
            scan: scan(),
        },
        &mut |row| {
            assert!(rows.len() < 16);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count, 3);
    assert_eq!(
        rows.len(),
        7,
        "two metadata+format pairs and three exact outer edges"
    );
    let mut roots = RootSetV1::empty("material-fixture".into(), [0x65; 32]).unwrap();
    for namespace in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|row| row.namespace == namespace)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                writes,
                &state,
                2 << 20,
            )
            .unwrap(),
        );
        values.put_state_pages(&delta, memory).unwrap();
        let index = roots
            .state
            .iter()
            .position(|root| root.namespace == namespace)
            .unwrap();
        roots.state[index] = delta.after.clone();
    }
    roots
}
fn expected(
    key: MaterialRecordKeyV1<'_>,
) -> Vec<(MaterialCellV1, Option<MaterialValueV1>, &'static str)> {
    use MaterialCellV1 as C;
    use MaterialValueV1 as V;
    match key {
        MaterialRecordKeyV1::Profile(_) => vec![
            (C::Record, Some(V::Unit), ""),
            (
                C::Kind,
                Some(V::Text {
                    utf8_bytes: " raw-kind\0界 ".len() as u64,
                }),
                " raw-kind\0界 ",
            ),
            (C::Tier, Some(V::Unsigned(255)), ""),
            (C::Category, Some(V::Text { utf8_bytes: 0 }), ""),
            (C::StackLimit, Some(V::Signed(i64::MIN)), ""),
            (C::TransportLossClass, Some(V::Unsigned(2)), ""),
            (C::DecayBpsPerTick, Some(V::Signed(i64::MAX)), ""),
            (C::DefaultPriority, Some(V::Unsigned(0)), ""),
        ],
        MaterialRecordKeyV1::Transfer(id) => {
            let mut rows = vec![
                (C::Record, Some(V::Unit), ""),
                (
                    C::TransferId,
                    Some(V::Unsigned(if id == 0 { u64::MAX } else { 17 })),
                    "",
                ),
                (
                    C::RequesterAgentId,
                    Some(V::Text {
                        utf8_bytes: " original requester\0界 ".len() as u64,
                    }),
                    " original requester\0界 ",
                ),
                (C::Kind, Some(V::Text { utf8_bytes: 0 }), ""),
                (C::Amount, Some(V::Signed(i64::MIN)), ""),
                (C::DistanceKm, Some(V::Signed(i64::MAX)), ""),
                (C::Priority, Some(V::Unsigned(0)), ""),
                (C::RoutePresent, Some(V::Bool(id != 0)), ""),
                (
                    C::RouteId,
                    if id == 0 {
                        None
                    } else {
                        Some(V::Text { utf8_bytes: 0 })
                    },
                    "",
                ),
            ];
            // Ledger kinds are asserted separately from their original ID output.
            rows.push((C::FromLedger, None, ""));
            rows.push((C::ToLedger, None, " original factory "));
            rows
        }
    }
}
#[test]
fn material_genesis_root_only_qualification_and_all_column_reads_preserve_originals() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let roots = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let _source = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let source = source();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source)
        };
        // The source World, original page-store handle and original pool are gone.
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let _caller = memory.reserve(128 << 10, class).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state = store.state_session(physical_scope(), class).unwrap();
            let values = store.value_session(physical_scope());
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut scan = scan();
            let mut cells = 1000;
            for key in [
                MaterialRecordKeyV1::Profile(PROFILE),
                MaterialRecordKeyV1::Transfer(0),
                MaterialRecordKeyV1::Transfer(u64::MAX),
            ] {
                let top = roots
                    .state
                    .iter()
                    .find(|root| root.namespace == key.kind().namespace())
                    .unwrap();
                let child = {
                    let mut ctx = WorldFieldReadContextV1 {
                        root: top,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut io,
                        },
                        key_work: &mut keys,
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
                        },
                        cancelled: || false,
                    };
                    inspect_material_record_root_v1(key, &mut ctx)
                        .unwrap()
                        .unwrap()
                };
                // Independent literal original-key preimage; no producer key
                // helper or private hasher is reused by this cold-read oracle.
                let mut preimage = b"oasis7.iws.semantic-key.v1\0".to_vec();
                match key {
                    MaterialRecordKeyV1::Profile(id) => {
                        preimage.push(1);
                        preimage.extend_from_slice(&(id.len() as u64).to_be_bytes());
                        preimage.extend_from_slice(id.as_bytes());
                    }
                    MaterialRecordKeyV1::Transfer(id) => {
                        preimage.push(2);
                        preimage.extend_from_slice(&id.to_be_bytes());
                    }
                }
                let digest = *blake3::hash(&preimage).as_bytes();
                let qualification = {
                    let mut ctx = WorldFieldReadContextV1 {
                        root: &child,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut io,
                        },
                        key_work: &mut keys,
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
                        },
                        cancelled: || false,
                    };
                    qualify_material_body_v1(
                        key.kind(),
                        digest,
                        &mut ctx,
                        &mut scan,
                        &mut cells,
                        1024,
                    )
                    .unwrap()
                };
                eprintln!(
                    "material qualified {compression:?}/{class:?}/{key:?}: state={:?} values={:?}",
                    state.remaining_io(),
                    values.remaining_io()
                );
                assert!(
                    qualification
                        .validate_binding(key.kind(), &[0; 32], &child)
                        .is_err()
                );
                for (cell, want, text) in expected(key) {
                    let mut output = Vec::with_capacity(128);
                    let mut ctx = WorldFieldReadContextV1 {
                        root: top,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut io,
                        },
                        key_work: &mut keys,
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
                        },
                        cancelled: || false,
                    };
                    let got = read_qualified_material_cell_v1(
                        MaterialCellRequestV1 {
                            key,
                            cell,
                            qualification: &qualification,
                        },
                        &mut ctx,
                        1024,
                        &mut output,
                    )
                    .unwrap_or_else(|error| panic!("material query {compression:?}/{class:?}/{key:?}/{cell:?}: {error}; state={:?} values={:?}",state.remaining_io(),values.remaining_io()));
                    if matches!(cell, MaterialCellV1::FromLedger | MaterialCellV1::ToLedger) {
                        let Some(MaterialValueV1::Ledger(info)) = got else {
                            panic!("original ledger codec")
                        };
                        assert_eq!(info.id_bytes, text.len() as u64);
                        assert_eq!(
                            info.kind,
                            if cell == MaterialCellV1::FromLedger {
                                oasis7_runtime::incremental_ledger_value::LedgerValueKindV1::Agent
                            } else {
                                oasis7_runtime::incremental_ledger_value::LedgerValueKindV1::Factory
                            }
                        );
                    } else {
                        assert_eq!(got, want, "{cell:?}");
                    }
                    assert_eq!(output, text.as_bytes());
                }
            }
        }
    }
}

#[test]
fn material_family_cold_roots_qualify_both_namespaces() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("family-pages");
        let roots = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let source = source();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source)
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state = store.state_session(physical_scope(), class).unwrap();
            let values = store.value_session(physical_scope());
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut outer = scan();
            let mut inner = scan();
            let mut cells = 1000;
            let mut ctx = WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || {
                    state
                        .take_read_error()
                        .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
                },
                cancelled: || false,
            };
            let proof = qualify_material_family_v1(
                &roots,
                &mut ctx,
                &mut outer,
                &mut inner,
                &mut cells,
                1 << 20,
            )
            .unwrap();
            assert_eq!(proof.record_counts(), [1, 2]);
            proof.validate_binding(&roots.digest().unwrap()).unwrap();
            assert!(proof.validate_binding(&[0; 32]).is_err());
        }
    }
}
