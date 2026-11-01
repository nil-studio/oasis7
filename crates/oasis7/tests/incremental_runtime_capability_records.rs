#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    CapabilityAuthorizationNonceRecord, CapabilityBudgetAccount, CapabilityEffectReceiptLink,
    incremental_field_member::*, incremental_field_read::*,
    incremental_runtime_capability_records::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
use oasis7_wasm_abi::CapabilitySubject;
use std::collections::BTreeMap;
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_250_000,
        ..fixture::physical()
    }
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(v) => ValueTreeError::Memory(v),
        PageStoreError::ResourceLimited(v) => ValueTreeError::ResourceLimited(v),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(v) => ValueTreeError::Io(v),
        _ => ValueTreeError::Invalid("fixed economy test state"),
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 1024,
        payload_bytes: 165,
        record_bytes: 4096,
    }
}
fn sources() -> (
    BTreeMap<String, CapabilityAuthorizationNonceRecord>,
    BTreeMap<String, CapabilityBudgetAccount>,
    BTreeMap<String, CapabilityEffectReceiptLink>,
) {
    let nonces = [
        (
            "nonce-key".into(),
            CapabilityAuthorizationNonceRecord {
                request_hash: "original request".into(),
                outcome_hash: "".into(),
                committed_receipt_id: Some("".into()),
                state: "arbitrary-original-state".into(),
            },
        ),
        (
            "none-key".into(),
            CapabilityAuthorizationNonceRecord {
                request_hash: "second".into(),
                outcome_hash: "different".into(),
                committed_receipt_id: None,
                state: "".into(),
            },
        ),
    ]
    .into();
    let budgets = [
        (
            "agent-key".into(),
            CapabilityBudgetAccount {
                subject: CapabilitySubject::Agent {
                    agent_id: "embedded-agent".into(),
                    owner_binding: "".into(),
                    generation: u64::MAX,
                },
                grant_id: "grant-not-map-key".into(),
                remaining_units: i64::MIN,
                reserved_units: 0,
                spent_units: i64::MAX,
            },
        ),
        (
            "module-key".into(),
            CapabilityBudgetAccount {
                subject: CapabilitySubject::Module {
                    module_id: "module".into(),
                    module_version: "".into(),
                    instance_id: "instance".into(),
                },
                grant_id: "".into(),
                remaining_units: -1,
                reserved_units: i64::MIN,
                spent_units: 1,
            },
        ),
        (
            "system-key".into(),
            CapabilityBudgetAccount {
                subject: CapabilitySubject::System {
                    system_id: "".into(),
                    epoch: u64::MAX,
                },
                grant_id: "system-grant".into(),
                remaining_units: 0,
                reserved_units: i64::MAX,
                spent_units: i64::MIN,
            },
        ),
    ]
    .into();
    (
        nonces,
        budgets,
        [(
            "link-key".into(),
            CapabilityEffectReceiptLink {
                authorization_receipt_id: "receipt-not-key".into(),
            },
        )]
        .into(),
    )
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget) -> RootSetV1 {
    let (nonces, budgets, effect_links) = sources();
    seed_source(
        store,
        memory,
        RuntimeCapabilityRecordsRefV1 {
            nonces: &nonces,
            budgets: &budgets,
            effect_links: &effect_links,
        },
        165,
    )
}
fn seed_source(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    source: RuntimeCapabilityRecordsRefV1<'_>,
    max_payload: u64,
) -> RootSetV1 {
    let _caller = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut writer = store.value_session(physical());
    let mut rows = Vec::with_capacity(12);
    let mut work = RuntimeCapabilityRecordsWorkV1 {
        cells_remaining: 72,
        max_pending_node_bytes: 2 << 20,
    };
    assert_eq!(
        prepare_runtime_capability_records_genesis_v1(
            &source,
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20
                },
                limits: MemberRecordLimitsV1 {
                    payload_bytes: max_payload,
                    record_bytes: 4096.max(max_payload + 4096),
                    ..limits()
                },
                cancelled: || false
            },
            &mut work,
            &mut |row| {
                assert!(rows.len() < 12);
                rows.push(row);
                Ok(())
            },
        )
        .unwrap(),
        6
    );
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(rows.len(), 12);
    let mut roots = RootSetV1::empty("capability-records".into(), [0x65; 32]).unwrap();
    for namespace in [StateNamespaceV1::Authorization, StateNamespaceV1::Effects] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes = rows
            .iter()
            .filter(|r| r.namespace == namespace)
            .map(|r| StateWriteV1 {
                key: r.mutation.key.clone(),
                before: None,
                after: r.mutation.after,
            })
            .collect::<Vec<_>>();
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
        writer.put_state_pages(&delta, memory).unwrap();
        *roots
            .state
            .iter_mut()
            .find(|r| r.namespace == namespace)
            .unwrap() = delta.after.clone();
    }
    roots
}
macro_rules! read_ctx {
    ($root:expr,$state:expr,$values:expr,$memory:expr,$class:expr,$io:expr,$keys:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
            state: $state,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: $class,
                io: $io,
            },
            key_work: $keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || $state.take_read_error().map(state_error),
            cancelled: || false,
        }
    };
}

#[test]
fn capability_records_cold_original_values() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let canonical = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory).canonical_bytes().unwrap()
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let _caller = memory.reserve(128 << 10, MemoryClass::Recovery).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Recovery,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_capability_family_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [2, 3, 1]);
        family.validate_binding(&roots.digest().unwrap()).unwrap();
        assert!(family.validate_binding(&[0; 32]).is_err());
        use RuntimeCapabilityColumnV1 as C;
        use RuntimeCapabilityRecordKindV1 as K;
        use RuntimeCapabilityValueV1 as V;
        // Literal expectations independent of producer visitor, preserving key vs embedded identity.
        for (kind, id, column, want, text) in [
            (
                K::Nonce,
                "nonce-key",
                C::RequestHash,
                V::Text { utf8_bytes: 16 },
                "original request",
            ),
            (
                K::Nonce,
                "nonce-key",
                C::OutcomeHash,
                V::Text { utf8_bytes: 0 },
                "",
            ),
            (
                K::Nonce,
                "nonce-key",
                C::CommittedReceiptId,
                V::Text { utf8_bytes: 0 },
                "",
            ),
            (K::Nonce, "none-key", C::CommittedReceiptId, V::Unit, ""),
            (
                K::Nonce,
                "nonce-key",
                C::State,
                V::Text { utf8_bytes: 24 },
                "arbitrary-original-state",
            ),
            (
                K::Budget,
                "agent-key",
                C::SubjectKind,
                V::Text { utf8_bytes: 5 },
                "agent",
            ),
            (
                K::Budget,
                "agent-key",
                C::SubjectAgentId,
                V::Text { utf8_bytes: 14 },
                "embedded-agent",
            ),
            (
                K::Budget,
                "agent-key",
                C::SubjectOwnerBinding,
                V::Text { utf8_bytes: 0 },
                "",
            ),
            (
                K::Budget,
                "agent-key",
                C::SubjectGeneration,
                V::Unsigned(u64::MAX),
                "",
            ),
            (K::Budget, "agent-key", C::SubjectSystemId, V::Unit, ""),
            (
                K::Budget,
                "agent-key",
                C::GrantId,
                V::Text { utf8_bytes: 17 },
                "grant-not-map-key",
            ),
            (
                K::Budget,
                "agent-key",
                C::RemainingUnits,
                V::Signed(i64::MIN),
                "",
            ),
            (
                K::Budget,
                "agent-key",
                C::SpentUnits,
                V::Signed(i64::MAX),
                "",
            ),
            (
                K::Budget,
                "module-key",
                C::SubjectKind,
                V::Text { utf8_bytes: 6 },
                "module",
            ),
            (
                K::Budget,
                "module-key",
                C::SubjectModuleVersion,
                V::Text { utf8_bytes: 0 },
                "",
            ),
            (K::Budget, "module-key", C::SubjectAgentId, V::Unit, ""),
            (
                K::Budget,
                "system-key",
                C::SubjectKind,
                V::Text { utf8_bytes: 6 },
                "system",
            ),
            (
                K::Budget,
                "system-key",
                C::SubjectSystemId,
                V::Text { utf8_bytes: 0 },
                "",
            ),
            (
                K::Budget,
                "system-key",
                C::SubjectEpoch,
                V::Unsigned(u64::MAX),
                "",
            ),
            (
                K::EffectLink,
                "link-key",
                C::AuthorizationReceiptId,
                V::Text { utf8_bytes: 15 },
                "receipt-not-key",
            ),
        ] {
            ctx.root = roots
                .state
                .iter()
                .find(|r| r.namespace == kind.namespace())
                .unwrap();
            let key = RuntimeCapabilityRecordKeyV1 { kind, id };
            let record =
                open_qualified_runtime_capability_record_v1(key, &roots, &family, &mut ctx)
                    .unwrap()
                    .unwrap();
            let mut output = Vec::new();
            assert_eq!(
                read_qualified_runtime_capability_cell_v1(
                    RuntimeCapabilityCellRequestV1 {
                        key,
                        column,
                        qualification: &record
                    },
                    &roots,
                    &mut ctx,
                    165,
                    &mut output
                )
                .unwrap(),
                want
            );
            assert_eq!(output, text.as_bytes());
        }
    }
}

#[path = "support/incremental_runtime_capability_operations.rs"]
mod operations;

#[path = "support/incremental_runtime_capability_schema.rs"]
mod schema;

#[path = "support/incremental_runtime_capability_pages.rs"]
mod pages;

#[path = "support/incremental_runtime_capability_scaling.rs"]
mod scaling;

#[path = "support/incremental_runtime_capability_faults.rs"]
mod faults;

#[test]
fn capability_empty_genesis_final_emit_and_zero_budget_boundaries() {
    use std::cell::Cell;
    let directory = Directory::new();
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("genesis"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let nonces = BTreeMap::new();
    let budgets = BTreeMap::new();
    let links = BTreeMap::new();
    for mode in 0..4 {
        let stopped = Cell::new(false);
        let emitted = Cell::new(0);
        let mut writer = store.value_session(physical());
        let mut work = RuntimeCapabilityRecordsWorkV1 {
            cells_remaining: if mode == 3 { 0 } else { 6 },
            max_pending_node_bytes: 2 << 20,
        };
        let result = prepare_runtime_capability_records_genesis_v1(
            &RuntimeCapabilityRecordsRefV1 {
                nonces: &nonces,
                budgets: &budgets,
                effect_links: &links,
            },
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 8192,
                },
                limits: limits(),
                cancelled: || stopped.get(),
            },
            &mut work,
            &mut |_| {
                emitted.set(emitted.get() + 1);
                if emitted.get() == 6 {
                    if mode == 1 {
                        stopped.set(true);
                    }
                    if mode == 2 {
                        return Err(ValueTreeError::Io(std::io::Error::other(
                            "last actual provisional emit",
                        )));
                    }
                }
                Ok(())
            },
        );
        match mode {
            0 => {
                assert_eq!(result.unwrap(), 0);
                assert_eq!(emitted.get(), 6);
                assert_eq!(work.cells_remaining, 0);
            }
            1 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Cancelled))
                ));
                assert_eq!(emitted.get(), 6);
            }
            2 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Io(_)))
                ));
                assert_eq!(emitted.get(), 6);
            }
            3 => {
                assert!(matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
                ));
                assert_eq!(emitted.get(), 0);
            }
            _ => unreachable!(),
        }
    }
}
