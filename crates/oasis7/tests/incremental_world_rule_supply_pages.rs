#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
    incremental_state::{
        StateDeltaV1, StateNamespaceV1 as Ns, StatePageWriter, StateRootV1, StateScanBudget,
        StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError, write_value},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    CrisisState, CrisisStatus, WorldState,
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_effects::*,
    incremental_world_rule_supply_root::*,
    world_rules::{SupplyAuthorizationV1, SupplyBatchV1, SupplyLineageV1, WorldRulesStateV1},
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        ..fixture::physical()
    }
}
#[allow(dead_code)]
#[path = "support/incremental_world_rules_fixture.rs"]
mod rules_fixture;
use rules_fixture::*;
const KEY: &str = "outer-record\0é";
const OLD: &str = "old-member\0é";
#[path = "support/incremental_world_rule_supply_fixture.rs"]
mod supply_fixture;
use supply_fixture::{authorization, batch};
fn source() -> WorldState {
    WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            supply_authorizations: [(KEY.into(), authorization())].into(),
            supply_batches: [(KEY.into(), batch())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 100000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("supply-pages".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
fn produce_all(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> Rows {
    let mut rows = produce(store, memory, source);
    let counts = prepare_supply_root_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                payload_bytes: 165,
                ..limits()
            },
            cancelled: || false,
        },
        &mut 128,
        4 << 20,
        &mut |row| {
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        counts,
        SupplyRootGenesisCountsV1 {
            authorizations: 1,
            batches: 1
        }
    );
    rows
}
#[test]
fn supply_typed_pages_transfer_and_cold_reopen_preserve_nested_records() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let original = Directory::new();
        let destination = Directory::new();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::create(&original.0.join("pages"), memory.clone(), compression)
                .unwrap();
        let roots = roots(&build(
            &store,
            &memory,
            &produce_all(&store, &memory, &source()),
        ));
        let auth = roots
            .state
            .iter()
            .find(|root| root.namespace == Ns::Authorization)
            .unwrap();
        let target =
            IncrementalPageStore::create(&destination.0.join("pages"), memory.clone(), compression)
                .unwrap();
        let copied_count = {
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut value_io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            };
            let mut context = WorldFieldReadContextV1 {
                root: auth,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut value_io,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let mut cells = 1_000_000;
            let mut outer = scan();
            let mut inner = scan();
            let mut work = SupplyPagesWorkV1 {
                outer_scan: &mut outer,
                inner_scan: &mut inner,
                cells: &mut cells,
                max_payload_bytes: 165,
                max_pages: 0,
            };
            assert!(matches!(
                mark_supply_family_pages_v1(&roots, &mut context, &mut work),
                Err(ValueTreeError::ResourceLimited("supply marked pages"))
            ));
            // Continue the same finite read/work budgets after the failed mark.
            work.max_pages = 32_768;
            let mut sink = target.value_session(physical());
            let pages = transfer_supply_family_pages_v1(&roots, &mut context, &mut work, &mut sink)
                .unwrap();
            assert_eq!(pages.qualification().record_counts(), [1, 1]);
            pages
                .qualification()
                .validate_binding(&roots.digest().unwrap())
                .unwrap();
            assert!(
                pages.page_count() > auth.leaf_count as usize,
                "actual nested state/value pages, not only outer leaves"
            );
            let mut readback_io = physical();
            pages
                .visit_pages(|id| {
                    let page = target
                        .read_page(
                            id,
                            64 << 10,
                            MemoryClass::Ordinary,
                            &mut readback_io,
                            &std::sync::atomic::AtomicBool::new(false),
                        )
                        .unwrap();
                    assert_eq!(blake3::hash(&page).as_bytes(), id);
                    Ok::<_, ()>(())
                })
                .unwrap();
            pages.page_count()
        };
        assert!(copied_count < 32_768);
        drop(target);
        drop(store);
        drop(admission);
        drop(memory);
        // Discard the original source/pool. Only destination and original roots
        // remain; use a new small pool with dedicated Recovery read admission.
        let cold_memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let cold = IncrementalPageStore::open(
            &destination.0.join("pages"),
            cold_memory.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let state = cold
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = cold.value_session(physical());
        let mut value_io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: auth,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &cold_memory,
                class: MemoryClass::Recovery,
                io: &mut value_io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family = qualify_supply_family_v1(
            &roots,
            &mut context,
            &mut scan(),
            &mut scan(),
            &mut 1_000_000,
            165,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [1, 1]);
        assert!(family.rules_present());
    }
}

use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader, incremental_value::ValueNodeWriter,
};
use std::cell::Cell;

struct FaultSink<'a, W> {
    sink: &'a mut W,
    written: &'a Cell<usize>,
    fail: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for FaultSink<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.fail && self.written.get() == 3 {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "injected supply sink failure",
            )));
        }
        self.sink.put_node(id, bytes)?;
        self.written.set(self.written.get() + 1);
        Ok(())
    }
}
struct MissingChild<'a, M> {
    source: &'a M,
    digest: &'a str,
    reached: Cell<bool>,
}
impl<M: MerkleNodeReader> MerkleNodeReader for MissingChild<'_, M> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if digest == self.digest {
            self.reached.set(true);
            return Ok(None);
        }
        self.source.read_node(digest, max)
    }
}

#[test]
fn supply_page_failures_never_return_a_completed_family() {
    let original = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let _source_admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &original.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = roots(&build(
        &store,
        &memory,
        &produce_all(&store, &memory, &source()),
    ));
    let auth = roots
        .state
        .iter()
        .find(|root| root.namespace == Ns::Authorization)
        .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut value_io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut outer = scan();
    let mut inner = scan();
    let mut cells = 1_000_000;
    let mut work = SupplyPagesWorkV1 {
        outer_scan: &mut outer,
        inner_scan: &mut inner,
        cells: &mut cells,
        max_payload_bytes: 165,
        max_pages: 32_768,
    };
    let written = Cell::new(0);
    let cancel = Cell::new(false);
    let child = {
        let mut context = WorldFieldReadContextV1 {
            root: auth,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || cancel.get() && written.get() >= 3,
        };
        // Two distinct operations share the caller's finite work/read budgets.
        // A fresh destination ensures the observed puts really publish pages.
        for fail in [true, false] {
            written.set(0);
            cancel.set(!fail);
            let directory = Directory::new();
            let target = IncrementalPageStore::create(
                &directory.0.join("pages"),
                memory.clone(),
                PageCompression::Raw,
            )
            .unwrap();
            let mut target_values = target.value_session(physical());
            let mut sink = FaultSink {
                sink: &mut target_values,
                written: &written,
                fail,
            };
            let result =
                transfer_supply_family_pages_v1(&roots, &mut context, &mut work, &mut sink);
            if fail {
                assert!(matches!(result, Err(ValueTreeError::Io(_))));
            } else {
                assert!(matches!(result, Err(ValueTreeError::Cancelled)));
            }
            assert_eq!(
                written.get(),
                3,
                "three actual successful puts precede failure/cancellation"
            );
            assert!(
                std::fs::read_dir(directory.0.join("pages/recipes"))
                    .unwrap()
                    .next()
                    .is_some(),
                "a physical prefix exists, yet no completed collection escaped"
            );
        }
        cancel.set(false);
        inspect_supply_root_v1(SupplyRootKeyV1::Authorization(KEY), &mut context)
            .unwrap()
            .expect("actual authenticated child reference")
    };
    let missing = MissingChild {
        source: &state,
        digest: &child.node_root,
        reached: Cell::new(false),
    };
    let mut context = WorldFieldReadContextV1 {
        root: auth,
        state: &missing,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    assert!(matches!(
        mark_supply_family_pages_v1(&roots, &mut context, &mut work),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(
        missing.reached.get(),
        "the actual typed child edge was traversed"
    );
}
