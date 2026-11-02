//! Public ordinary operations after a real typed prefix; no physical claim.
use super::{body_tests::*, *};
use crate::{
    incremental_field_member::*, incremental_field_overlay::*, incremental_record_tree as tree,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionError, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};

#[derive(Clone, Copy, Debug)]
enum Mode {
    Getter,
    NoOp,
    Family,
    Body,
    Json,
    Put,
    Read,
    AfterPut,
    Foreign,
    Quota,
}
struct FaultPages<'a> {
    pages: Pages,
    armed: &'a Cell<bool>,
    mode: Mode,
    puts: &'a Cell<u64>,
}
impl ValueNodeReader for FaultPages<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if self.armed.get() && matches!(self.mode, Mode::Read) {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "injected source failure",
            )));
        }
        self.pages.read_node(id, max, memory, class)
    }
}
impl ValueNodeWriter for FaultPages<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.armed.get() {
            self.puts.set(self.puts.get() + 1);
            if matches!(self.mode, Mode::Put) {
                return Err(ValueTreeError::Io(std::io::Error::other(
                    "injected actual put",
                )));
            }
        }
        self.pages.put_node(id, value)
    }
}
impl StatePageWriter for FaultPages<'_> {}

fn run(mode: Mode, channel: bool, stop: Option<u64>) -> u64 {
    let memory = MemoryBudget::new(192 << 20, 4 << 20).unwrap();
    let _index = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages {
        nodes: Rc::new(RefCell::new(BTreeMap::new())),
    };
    let (roots, _) = seed(&memory, &mut pages);
    let (family, body) = family_in_pool(&roots, &pages, &memory, MemoryClass::Ordinary).unwrap();
    let foreign_memory = MemoryBudget::new(32 << 20, 0).unwrap();
    let foreign = if matches!(mode, Mode::Foreign) {
        Some(family_in_pool(&roots, &pages, &foreign_memory, MemoryClass::Ordinary).unwrap())
    } else {
        None
    };
    let (recovery, recovery_body) =
        family_in_pool(&roots, &pages, &memory, MemoryClass::Recovery).unwrap();
    let json_address = ArtifactConfigAddressV1 {
        section: "subscriptions",
        ordinal: Some(0),
        inner_ordinal: None,
        member: None,
        column: "filters",
    };
    let recovery_json = json_in_pool(
        &recovery_body,
        json_address,
        &pages,
        &memory,
        MemoryClass::Recovery,
    );
    let armed = Cell::new(false);
    let puts = Cell::new(0);
    let polls = Cell::new(0);
    let source = FaultPages {
        pages: pages.clone(),
        armed: &armed,
        mode,
        puts: &puts,
    };
    let mut sink = FaultPages {
        pages,
        armed: &armed,
        mode,
        puts: &puts,
    };
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut no_cancel = || false;
    let state_cancel = RefCell::new(&mut no_cancel);
    let error = RefCell::new(None);
    let state = tree::Source {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &state_cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let parent = ExecutionParent {
        roots,
        commit: [31; 32],
        authority: AuthorityBinding {
            token: [32; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [33; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 64,
            max_facts: 16,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut read_io = io();
    let mut write_io = io();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let callback = || {
        if !armed.get() {
            return false;
        }
        polls.set(polls.get() + 1);
        stop.is_some_and(|n| polls.get() >= n) || matches!(mode, Mode::AfterPut) && puts.get() > 0
    };
    let mut context = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || error.borrow_mut().take(),
        cancelled: || !channel && callback(),
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || channel && callback(),
    };
    // A successful public typed operation, not an arbitrary raw Txn write.
    let guard = context.transaction.guard();
    assert!(
        change_qualified_artifact_config_record_v1(
            ArtifactConfigRecordChangeV1 {
                field: ArtifactConfigFieldV1::RegistryActive,
                key: ArtifactConfigRecordKeyV1::Member("typed-prefix"),
                family: &family,
                expected: None,
                after: Some(ArtifactConfigBodyRefV1::Active("active original"))
            },
            &mut context,
            &mut write,
            &mut write_work(),
            &guard,
            &live
        )
        .unwrap()
        .changed
    );
    armed.set(true);
    if matches!(mode, Mode::Quota) {
        write.io.nodes_remaining = 0;
    }
    let guard = context.transaction.guard();
    let address = ArtifactConfigAddressV1 {
        section: "record",
        ordinal: None,
        inner_ordinal: None,
        member: None,
        column: "registered_at",
    };
    let request = ArtifactConfigCellRequestV1 {
        field: ArtifactConfigFieldV1::RegistryRegisteredAt,
        key: ArtifactConfigRecordKeyV1::Member("raw external key"),
        address,
        family: if matches!(mode, Mode::Family) {
            &recovery
        } else if let Some((family, _)) = &foreign {
            family
        } else {
            &family
        },
        qualification: if matches!(mode, Mode::Body) {
            &recovery_body
        } else {
            &body
        },
        max_payload_bytes: 1 << 20,
    };
    let result = if matches!(mode, Mode::Getter) {
        read_qualified_artifact_config_overlay_v1(
            request,
            &mut context,
            &guard,
            &live,
            &mut std::io::sink(),
        )
        .map(|_| ())
    } else if matches!(mode, Mode::Json) {
        let mut request = request;
        request.address = json_address;
        change_qualified_artifact_config_json_v1(
            ArtifactConfigJsonChangeV1 {
                request,
                json: &recovery_json,
                path: &[],
                edit: crate::incremental_json_records::JsonSubtreeEditV1::Replace {
                    expected: recovery_json.root(),
                    after: &serde_json::Value::Bool(true),
                },
            },
            &mut context,
            &mut write,
            &mut write_work().json,
            &mut json_work(),
            &guard,
            &live,
        )
        .map(|_| ())
    } else {
        change_qualified_artifact_config_cell_v1(
            ArtifactConfigCellChangeV1 {
                request,
                expected: ArtifactConfigScalarRefV1::Unsigned(u64::MAX),
                after: ArtifactConfigScalarRefV1::Unsigned(
                    if matches!(mode, Mode::Put | Mode::AfterPut | Mode::Quota) {
                        0
                    } else {
                        u64::MAX
                    },
                ),
                max_pending_node_bytes: 1 << 20,
            },
            &mut context,
            &mut write,
            &guard,
            &live,
        )
        .map(|_| ())
    };
    let failed = stop.is_some() || !matches!(mode, Mode::Getter | Mode::NoOp);
    if failed {
        let error = result.unwrap_err();
        if matches!(mode, Mode::Family | Mode::Body | Mode::Json | Mode::Foreign) {
            assert!(
                matches!(
                    error,
                    WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_))
                ),
                "{mode:?}: {error:?}"
            );
            assert_eq!(puts.get(), 0);
        }
        if matches!(mode, Mode::Put | Mode::Read) {
            assert!(
                error.to_string().contains(if matches!(mode, Mode::Put) {
                    "injected actual put"
                } else {
                    "injected source failure"
                }),
                "{mode:?}: {error:?}"
            );
        }
        if matches!(mode, Mode::Quota) {
            assert!(
                matches!(
                    error,
                    WorldFieldOverlayErrorV1::Value(ValueTreeError::ResourceLimited(_))
                        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::ResourceLimited(_)
                        ))
                ),
                "{mode:?}: {error:?}"
            );
            assert_eq!(puts.get(), 0);
        }
        if stop.is_some() || matches!(mode, Mode::AfterPut) {
            assert!(
                matches!(
                    error,
                    WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)
                        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::Cancelled
                        ))
                ),
                "{mode:?}: {error:?}"
            );
        }
        if matches!(mode, Mode::Put | Mode::AfterPut) {
            assert!(puts.get() > 0);
        }
        let guard = tx.guard();
        assert!(
            matches!(tx.seal(&guard, &live, 1), Err(ExecutionError::Aborted)),
            "{mode:?}"
        );
    } else {
        result.unwrap();
        let guard = tx.guard();
        assert_eq!(tx.seal(&guard, &live, 1).unwrap().record.writes.len(), 1);
    }
    polls.get()
}
fn json_work() -> crate::incremental_json_records::JsonChangeWorkV1 {
    crate::incremental_json_records::JsonChangeWorkV1 {
        paths_remaining: 1000,
        max_pending_pairs: 100,
        json: crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 10000,
            json_nodes_remaining: 100,
            max_payload_bytes: 1 << 20,
            max_depth: 16,
        },
    }
}
#[test]
fn typed_prefix_class_faults_and_independent_getter_noop_completion() {
    let getter = run(Mode::Getter, false, None);
    let noop = run(Mode::NoOp, false, None);
    eprintln!("artifact getter polls={getter} noop={noop}");
    assert_eq!(
        noop,
        getter + 1,
        "NoOp expected comparison must retain its final completion check"
    );
    run(Mode::NoOp, false, Some(getter + 1));
    run(Mode::NoOp, true, Some(getter + 1));
    for mode in [
        Mode::Family,
        Mode::Body,
        Mode::Json,
        Mode::Put,
        Mode::Read,
        Mode::Foreign,
        Mode::Quota,
    ] {
        run(mode, false, None);
    }
    run(Mode::AfterPut, false, None);
    run(Mode::AfterPut, true, None);
}
