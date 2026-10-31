#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_agent_api::world_action::{
    OrganizationEstateRightV1 as Right, OrganizationEstateScopeV1 as Scope,
};
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
    WorldState,
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_effects::*,
    incremental_world_rule_organization::root::*,
    world_rules::{
        OrganizationCaseV1, OrganizationDispositionPhaseV1 as Disposition,
        OrganizationEstateDepositV1, OrganizationGrantV1, OrganizationProtectionPhaseV1 as Phase,
        OrganizationRecoveryRequestV1, WorldRulesStateV1,
    },
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
// The rich owner fixture has55 original leaves and at most19 fixed query paths.
// Choose finite fixture operations up front; retain the existing byte caps.
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
const CASE: &str = " outer-case\0\u{e9} ";
const ESTATE: &str = "estate-key\0\u{e9}";
const GRANT: &str = "grant\0outer";
const REQUEST: &str = "request\0outer";
fn scope(object_id: &str, right: Right) -> Scope {
    Scope {
        object_id: object_id.into(),
        right,
    }
}
fn grant() -> OrganizationGrantV1 {
    OrganizationGrantV1 {
        id: "grant-inner".into(),
        delegate_id: "delegate".into(),
        scopes: [
            scope("", Right::Preserve),
            scope("return\0\u{e9}", Right::ReturnTrust),
            scope("residual", Right::DistributeResidual),
            scope("finalize", Right::FinalizeCase),
            scope("advance", Right::AdvanceCase),
        ]
        .into(),
        revision: 0,
        expires_at: u64::MAX,
        source_proposal: String::new(),
        replaces: Some(String::new()),
    }
}
fn recovery() -> OrganizationRecoveryRequestV1 {
    OrganizationRecoveryRequestV1 {
        requester_id: "requester-inner".into(),
        appeal: true,
        approved: Some(false),
        resolved_by: Some(String::new()),
    }
}
fn case() -> OrganizationCaseV1 {
    OrganizationCaseV1 {
        id: "case-inner".into(),
        organization_id: "org\0\u{e9}".into(),
        founder_id: String::new(),
        members: [String::new(), "member\0\u{e9}".into()].into(),
        policy_version: u32::MAX,
        revision: u64::MAX,
        phase: Phase::Protected,
        disposition: Disposition::Obligations,
        notified_at: None,
        protected_at: Some(0),
        grants: [(GRANT.into(), grant())].into(),
        requests: [(REQUEST.into(), recovery())].into(),
        stage_receipts: vec![String::new(), "repeat".into(), "repeat".into()],
    }
}
fn estate() -> OrganizationEstateDepositV1 {
    OrganizationEstateDepositV1 {
        id: "estate-inner".into(),
        organization_id: "different-org".into(),
        contributor_id: "contributor".into(),
        material: String::new(),
        amount: i64::MIN,
        held_in_trust: true,
        disposed: false,
        effect_receipt: Some(String::new()),
    }
}
fn source() -> WorldState {
    WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            organization_cases: [(CASE.into(), case())].into(),
            organization_estate: [(ESTATE.into(), estate())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        leaves_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("organization-relative-oracle".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
fn produce_all(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> Rows {
    let mut rows = produce(store, memory, source);
    let mut remaining = 128;
    let mut bounds = limits();
    bounds.payload_bytes = 165;
    let counts = prepare_qualified_organization_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: bounds,
            cancelled: || false,
        },
        &mut remaining,
        4 << 20,
        &mut |row| {
            assert_eq!(row.namespace, Ns::Authorization);
            assert_eq!(row.mutation.before, None);
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        (counts.cases, counts.estate, counts.grants, counts.recovery),
        (1, 1, 1, 1)
    );
    assert_eq!(remaining, 74, "52 relative cells and two outer references");
    assert_eq!(rows.len(), 5);
    rows
}

#[test]
fn organization_typed_pages_transfer_and_cold_reopen_preserve_nested_records() {
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
            let mut work = OrganizationPagesWorkV1 {
                outer_scan: &mut outer,
                inner_scan: &mut inner,
                cells: &mut cells,
                max_payload_bytes: 165,
                max_pages: 0,
            };
            assert!(matches!(
                mark_organization_family_pages_v1(&roots, &mut context, &mut work),
                Err(ValueTreeError::ResourceLimited("organization marked pages"))
            ));
            // Continue the same finite read/work budgets after the failed mark.
            work.max_pages = 32_768;
            let cancelled_directory = Directory::new();
            let cancelled_store = IncrementalPageStore::create(
                &cancelled_directory.0.join("pages"),
                memory.clone(),
                compression,
            )
            .unwrap();
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let final_phase = Cell::new(false);
            let baseline = Cell::new(0);
            let mut after_final_publish = || {
                final_phase.get()
                    && std::fs::read_dir(cancelled_store.root().join("recipes"))
                        .unwrap()
                        .count()
                        > baseline.get()
            };
            let mut cancelled_io = physical();
            let mut cancelled_sink = cancelled_store
                .buffered_page_writer(&mut cancelled_io, &cancel, &mut after_final_publish)
                .unwrap();
            let mut target_io = physical();
            let mut never = || false;
            let mut sink = target
                .buffered_page_writer(&mut target_io, &cancel, &mut never)
                .unwrap();
            let pages = transfer_organization_family_pages_v1(
                &roots,
                &mut context,
                &mut work,
                &mut flush::MirroredPages {
                    accepted: &mut sink,
                    cancelled: &mut cancelled_sink,
                    memory: &memory,
                },
            )
            .unwrap();
            assert!(sink.pending_reserved_bytes() > 0);
            assert!(cancelled_sink.pending_reserved_bytes() > 0);
            baseline.set(
                std::fs::read_dir(cancelled_store.root().join("recipes"))
                    .unwrap()
                    .count(),
            );
            final_phase.set(true);
            let rejected_acceptance = cancelled_sink.finish().map(|()| pages.qualification());
            assert!(matches!(
                rejected_acceptance,
                Err(PageStoreError::Cancelled)
            ));
            assert!(
                std::fs::read_dir(cancelled_store.root().join("recipes"))
                    .unwrap()
                    .count()
                    > baseline.get()
            );
            // Qualification and every cold read below are used only after the
            // positive sink's real final flush has completed successfully.
            sink.finish().unwrap();
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
        let family = qualify_organization_family_v1(
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
                "injected organization sink failure",
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
fn organization_page_failures_never_return_a_completed_family() {
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
    let mut work = OrganizationPagesWorkV1 {
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
                transfer_organization_family_pages_v1(&roots, &mut context, &mut work, &mut sink);
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
        inspect_organization_root_v1(OrganizationRootKeyV1::Case(CASE), &mut context)
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
        mark_organization_family_pages_v1(&roots, &mut context, &mut work),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(
        missing.reached.get(),
        "the actual typed child edge was traversed"
    );
}

#[path = "support/incremental_organization_flush.rs"]
mod flush;
