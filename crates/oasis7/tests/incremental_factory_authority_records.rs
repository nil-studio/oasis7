#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_factory_authority_records::*, incremental_field_member::*,
    incremental_field_read::*,
};
#[path = "support/incremental_factory_authority_carriers.rs"]
mod carriers;
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
#[path = "support/incremental_factory_authority_genesis.rs"]
mod genesis;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_factory_authority_malformed.rs"]
mod malformed;
#[path = "support/incremental_factory_authority_missing.rs"]
mod missing;
#[path = "support/incremental_factory_authority_overlay.rs"]
mod overlay;
#[path = "support/incremental_factory_authority_pages.rs"]
mod pages;
#[path = "support/incremental_factory_authority_points.rs"]
mod points;
#[path = "support/incremental_factory_authority_put.rs"]
mod put;
#[path = "support/incremental_factory_authority_scaling.rs"]
mod scaling;
fn state_error(error: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    use oasis7_distfs::incremental_page_store::PageStoreError as E;
    match error {
        E::Memory(e) => ValueTreeError::Memory(e),
        E::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        E::Cancelled => ValueTreeError::Cancelled,
        E::Io(e) => ValueTreeError::Io(e),
        E::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "factory authority physical page missing",
        )),
        E::Corrupt(reason) => ValueTreeError::Invalid(reason),
        E::UnsupportedPlatform => ValueTreeError::Invalid("factory authority platform"),
    }
}
fn physical_scope() -> PageIoBudget {
    let mut io = physical();
    // This one functional session includes four independent full child proofs
    // and every literal column query. The initial 1M-operation session reached
    // Raw/Ordinary/empty/AuthorityRevision and failed with the precise physical
    // read allowance. Keep the byte bound; this is no performance threshold.
    io.reads_remaining = 2_000_000;
    io
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 1000,
    }
}
fn source() -> WorldState {
    let mut source = WorldState::default();
    source.factory_site_authorities.insert(" outer-site\0界 ".into(),serde_json::from_value(serde_json::json!({
        "site_id":" inner-site\0界 ","location_id":"","owner_agent_id":"owner","authorized_agent_ids":["z","","z"],
        "chunk_ready":false,"active":true,"authority_revision":u64::MAX,"registered_at":17})).unwrap());
    for (outer, module) in [
        ("none", None),
        ("empty", Some("")),
        ("text", Some(" raw-module\0界 ")),
    ] {
        source.factory_construction_power_profiles.insert(outer.into(),serde_json::from_value(serde_json::json!({
            "factory_id":"different-inner-id","factory_kind":"","source_module_id":module,"electricity_amount":i64::MIN,
            "mode":"start_only_sink","authority_revision":u64::MAX,"active":false})).unwrap());
    }
    source
}
fn seed(source: &WorldState, store: &IncrementalPageStore, memory: &MemoryBudget) -> StateRootV1 {
    let mut values = store.value_session(physical_scope());
    let _row_admission = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(8);
    let count = prepare_factory_authority_genesis_v1(
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
        &mut FactoryAuthorityWorkV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 2 << 20,
        },
        &mut |row| {
            assert!(rows.len() < 8);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count, 4);
    assert_eq!(rows.len(), 8);
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let mut writes: Vec<_> = rows
        .iter()
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
            StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
            writes,
            &state,
            2 << 20,
        )
        .unwrap(),
    );
    values.put_state_pages(&delta, memory).unwrap();
    delta.after.clone()
}
fn owner(id: &str) -> [u8; 32] {
    // Independent original Text token, no producer or semantic-key encoder.
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&(id.len() as u64).to_be_bytes());
    bytes.extend_from_slice(id.as_bytes());
    *blake3::hash(&bytes).as_bytes()
}
#[test]
fn factory_authority_original_fields_cold_root_only_in_raw_and_zstd() {
    use FactoryAuthorityCellV1 as C;
    use FactoryAuthorityRecordKindV1 as K;
    use FactoryAuthorityValueV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let root = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let _source_admission = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(
                &directory.0.join("pages"),
                memory.clone(),
                compression,
            )
            .unwrap();
            seed(&source(), &store, &memory)
        };
        // Source World, writer store and allocator are gone; only the root and disk remain.
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let _caller_admission = memory.reserve(128 << 10, class).unwrap();
            let store = IncrementalPageStore::open(
                &directory.0.join("pages"),
                memory.clone(),
                compression,
                class,
            )
            .unwrap();
            let state = store.state_session(physical_scope(), class).unwrap();
            let values = store.value_session(physical_scope());
            let mut io = logical();
            let mut key_work = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut ctx = WorldFieldReadContextV1 {
                root: &root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut io,
                },
                key_work: &mut key_work,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            for (kind, id) in [
                (K::Site, " outer-site\0界 "),
                (K::Power, "none"),
                (K::Power, "empty"),
                (K::Power, "text"),
            ] {
                let key = FactoryAuthorityRecordKeyV1 { kind, id };
                let child = inspect_factory_authority_root_v1(key, &mut ctx)
                    .unwrap()
                    .unwrap();
                let token = {
                    let mut inner = WorldFieldReadContextV1 {
                        root: &child,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut *ctx.values.io,
                        },
                        key_work: &mut *ctx.key_work,
                        semantic_bytes: 1024,
                        record_bytes: 4096,
                        take_state_error: || state.take_read_error().map(state_error),
                        cancelled: || false,
                    };
                    qualify_factory_authority_body_v1(
                        kind,
                        owner(id),
                        &mut inner,
                        &mut scan(),
                        &mut 1000,
                        1024,
                    )
                    .unwrap()
                };
                let expected = if kind == K::Site {
                    vec![
                        (C::Record, Some(V::Unit), ""),
                        (
                            C::SiteId,
                            Some(V::Text { utf8_bytes: 0 }),
                            " inner-site\0界 ",
                        ),
                        (C::LocationId, Some(V::Text { utf8_bytes: 0 }), ""),
                        (C::OwnerAgentId, Some(V::Text { utf8_bytes: 0 }), "owner"),
                        (C::AuthorizedCount, Some(V::Unsigned(3)), ""),
                        (C::AuthorizedAgent(0), Some(V::Text { utf8_bytes: 0 }), "z"),
                        (C::AuthorizedAgent(1), Some(V::Text { utf8_bytes: 0 }), ""),
                        (C::AuthorizedAgent(2), Some(V::Text { utf8_bytes: 0 }), "z"),
                        (C::ChunkReady, Some(V::Bool(false)), ""),
                        (C::Active, Some(V::Bool(true)), ""),
                        (C::AuthorityRevision, Some(V::Unsigned(u64::MAX)), ""),
                        (C::RegisteredAt, Some(V::Unsigned(17)), ""),
                    ]
                } else {
                    vec![
                        (C::Record, Some(V::Unit), ""),
                        (
                            C::FactoryId,
                            Some(V::Text { utf8_bytes: 0 }),
                            "different-inner-id",
                        ),
                        (C::FactoryKind, Some(V::Text { utf8_bytes: 0 }), ""),
                        (C::SourceModulePresent, Some(V::Bool(id != "none")), ""),
                        (
                            C::SourceModuleId,
                            if id == "none" {
                                None
                            } else {
                                Some(V::Text { utf8_bytes: 0 })
                            },
                            if id == "text" {
                                " raw-module\0界 "
                            } else {
                                ""
                            },
                        ),
                        (C::ElectricityAmount, Some(V::Signed(i64::MIN)), ""),
                        (C::Mode, Some(V::Unsigned(0)), ""),
                        (C::AuthorityRevision, Some(V::Unsigned(u64::MAX)), ""),
                        (C::Active, Some(V::Bool(false)), ""),
                    ]
                };
                for (cell, value, text) in expected {
                    let mut output = Vec::new();
                    let value = value.map(|v| match v {
                        V::Text { .. } => V::Text {
                            utf8_bytes: text.len() as u64,
                        },
                        v => v,
                    });
                    let actual = read_qualified_factory_authority_cell_v1(
                        FactoryAuthorityCellRequestV1 {
                            key,
                            cell,
                            qualification: &token,
                        },
                        &mut ctx,
                        1024,
                        &mut output,
                    )
                    .unwrap_or_else(|e| panic!("{compression:?}/{class:?}/{id}/{cell:?}: {e:?}"));
                    assert_eq!(actual, value, "{compression:?} {class:?} {id} {cell:?}");
                    assert_eq!(output, text.as_bytes());
                }
                println!(
                    "factory-authority {compression:?}/{class:?}/{id}: state_remaining={:?}, value_remaining={:?}",
                    state.remaining_io().unwrap(),
                    values.remaining_io().unwrap()
                );
            }
        }
    }
}
