use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionFence, ExecutionParent, ExecutionTransaction, PreparedGuard,
    TransactionLimits,
};
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueNodeWriter};
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::cell::Cell;
struct Attempt<'a> {
    mode: usize,
    proof: &'a ReservedValue<OrganizationRecordQualificationV1>,
    family: &'a ReservedValue<OrganizationFamilyQualificationV1>,
}
fn attempt<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    input: Attempt<'_>,
    read: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let Attempt {
        mode,
        proof,
        family,
    } = input;
    let selected = scope("", Right::Preserve);
    match mode {
        14 | 15 | 21 => change_qualified_organization_set_v1(
            OrganizationSetChangeV1 {
                request: OrganizationSetRequestV1 {
                    case: CASE,
                    key: if mode == 14 {
                        OrganizationRelativeSetKeyV1::Member("")
                    } else {
                        OrganizationRelativeSetKeyV1::Scope {
                            grant: GRANT,
                            scope: &selected,
                        }
                    },
                    qualification: proof,
                },
                expected: if mode == 21 {
                    OrganizationSetReadV1::Present
                } else {
                    OrganizationSetReadV1::Absent
                },
                after: mode == 21,
                max_pending_node_bytes: 4 << 20,
            },
            read,
            write,
            guard,
            live,
        )
        .map(|_| ()),
        16 | 17 | 22 => change_qualified_organization_receipt_v1(
            WorldRuleOrganizationReceiptChangeV1::Replace {
                case: CASE,
                ordinal: if mode == 22 { 0 } else { 1 },
                expected_len: if mode == 16 { 2 } else { 3 },
                expected: if mode == 17 {
                    "repeax"
                } else if mode == 22 {
                    ""
                } else {
                    "repeat"
                },
                after: if mode == 22 { "" } else { "changed" },
                max_current_payload_bytes: 128,
            },
            proof,
            4 << 20,
            read,
            write,
            guard,
            live,
        )
        .map(|_| ()),
        18 => change_qualified_organization_receipt_v1(
            WorldRuleOrganizationReceiptChangeV1::Pop {
                case: CASE,
                expected_len: 0,
                expected: "",
                max_current_payload_bytes: 128,
            },
            proof,
            4 << 20,
            read,
            write,
            guard,
            live,
        )
        .map(|_| ()),
        19 | 20 => {
            let c = case();
            let g = grant();
            delete_qualified_organization_closed_v1(
                OrganizationClosedDeleteV1 {
                    owner: OrganizationRootKeyV1::Case(CASE),
                    record: if mode == 19 {
                        OrganizationRelativeRecordV1::Case
                    } else {
                        OrganizationRelativeRecordV1::Grant(GRANT)
                    },
                    expected: if mode == 19 {
                        WorldRuleOrganizationScalarsRefV1::Case(&c)
                    } else {
                        WorldRuleOrganizationScalarsRefV1::Grant(&g)
                    },
                    qualification: proof,
                    max_pending_node_bytes: 4 << 20,
                },
                read,
                write,
                guard,
                live,
                &mut 32,
            )
            .map(|_| ())
        }
        23 => change_qualified_organization_receipt_v1(
            WorldRuleOrganizationReceiptChangeV1::Append {
                case: CASE,
                expected_len: 3,
                after: "new",
            },
            proof,
            4 << 20,
            read,
            write,
            guard,
            live,
        )
        .map(|_| ()),
        24 => create_qualified_organization_empty_v1(
            OrganizationEmptyRootCreateV1 {
                owner: OrganizationRootKeyV1::Case("new-case"),
                source: WorldRuleOrganizationScalarsRefV1::Case(&case()),
                parent: family,
                max_pending_node_bytes: 4 << 20,
            },
            read,
            write,
            guard,
            live,
            &mut 32,
        )
        .map(|_| ()),
        25 => create_qualified_organization_child_empty_v1(
            OrganizationEmptyChildCreateV1 {
                case: CASE,
                record: OrganizationRelativeRecordV1::Grant("new-grant-key"),
                source: WorldRuleOrganizationScalarsRefV1::Grant(&grant()),
                qualification: proof,
                max_pending_node_bytes: 4 << 20,
            },
            read,
            write,
            guard,
            live,
            &mut 32,
        )
        .map(|_| ()),
        _ => change_qualified_organization_field_v1(
            OrganizationFieldChangeV1 {
                request: OrganizationFieldRequestV1 {
                    owner: OrganizationRootKeyV1::Case(CASE),
                    record: OrganizationRelativeRecordV1::Case,
                    field: OrganizationScalarFieldV1::Id,
                    qualification: proof,
                },
                expected: OrganizationFieldRefV1::Text(if mode == 0 {
                    "case-other"
                } else {
                    "case-inner"
                }),
                after: OrganizationFieldRefV1::Text(if mode == 10 {
                    "case-inner"
                } else {
                    "case-next!"
                }),
                max_current_payload_bytes: 128,
                max_pending_node_bytes: 4 << 20,
            },
            read,
            write,
            guard,
            live,
        )
        .map(|_| ()),
    }
}
struct Controlled<'a, W> {
    inner: &'a mut W,
    armed: bool,
    fail: bool,
    put: &'a Cell<bool>,
    cancelled: &'a Cell<bool>,
}
impl<W: ValueNodeWriter> ValueNodeWriter for Controlled<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.put.set(true);
        if self.armed {
            if self.fail {
                return Err(ValueTreeError::Invalid("durable put then injected failure"));
            }
            self.cancelled.set(true);
        }
        Ok(())
    }
}
impl<W: ValueNodeWriter> StatePageWriter for Controlled<'_, W> {}
fn context<'a, 'tx, 'store, M: MerkleNodeReader, R: ValueNodeReader, C: FnMut() -> bool>(
    transaction: &'a mut ExecutionTransaction<'tx, M>,
    values: &'store R,
    memory: &'store MemoryBudget,
    io: &'store mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    key_work: &'store mut SemanticKeyBudget,
    cancelled: C,
) -> WorldFieldOverlayContextV1<'a, 'tx, 'store, M, R, fn() -> Option<ValueTreeError>, C> {
    WorldFieldOverlayContextV1 {
        transaction,
        values: ValueReadContext {
            source: values,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || None,
        cancelled,
    }
}
fn cancelled(result: &Result<(), WorldFieldOverlayErrorV1>) -> bool {
    matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)
            | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                ValueTreeError::Cancelled
            )))
    )
}
pub(super) fn exercise() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("point-errors"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let before = build(&store, &memory, &produce_all(&store, &memory, &source()));
    let parent = ExecutionParent {
        roots: roots(&before),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let initial = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut fixed = WorldFieldReadContextV1 {
        root: &before,
        state: &initial,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || initial.take_read_error().map(state_error),
        cancelled: || false,
    };
    let family = qualify_organization_family_v1(
        &parent.roots,
        &mut fixed,
        &mut scan(),
        &mut scan(),
        &mut 1024,
        165,
    )
    .unwrap();
    let proof = open_qualified_organization_record_v1(
        OrganizationRootKeyV1::Case(CASE),
        &parent.roots,
        &family,
        &mut fixed,
    )
    .unwrap()
    .unwrap();
    for mode in 0..26 {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 96,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let stale = tx.guard();
        let mut writer = store.value_session(physical());
        let mut bounds = limits();
        bounds.payload_bytes = 165;
        {
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || false);
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: bounds,
                cancelled: || false,
            };
            let guard = read.transaction.guard();
            assert!(
                change_world_rule_effect_overlay_v1(
                    WorldRuleEffectChangeV1 {
                        key: "p",
                        expected: WorldRuleEffectExpectedV1::Value("original"),
                        after: Some("prefix"),
                        max_current_payload_bytes: 128
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
        }
        let ticks = Cell::new(0);
        let boundary = Cell::new(usize::MAX);
        if matches!(mode, 10 | 21 | 22) {
            // Warm the overlay, then observe the public successful final NoOp
            // boundary on this exact implementation; no private test hook.
            for _ in 0..2 {
                ticks.set(0);
                let mut io = logical();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                };
                let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || {
                    ticks.set(ticks.get() + 1);
                    false
                });
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    limits: bounds,
                    cancelled: || false,
                };
                let guard = read.transaction.guard();
                attempt(
                    Attempt {
                        mode,
                        proof: &proof,
                        family: &family,
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap();
                boundary.set(ticks.get());
            }
            assert!(boundary.get() > 1);
            ticks.set(0);
        }
        let put = Cell::new(false);
        let stopped = Cell::new(false);
        let mut controlled = Controlled {
            inner: &mut writer,
            armed: matches!(mode, 8 | 13 | 23),
            fail: mode == 8,
            put: &put,
            cancelled: &stopped,
        };
        let alien = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let mut io = logical();
        if mode == 11 {
            io.read_bytes_remaining = 0;
        }
        let mut keys = SemanticKeyBudget {
            bytes_remaining: if mode == 12 { 0 } else { 1 << 20 },
        };
        let mut read = context(&mut tx, &values, &memory, &mut io, &mut keys, || {
            ticks.set(ticks.get() + 1);
            mode == 6
                || stopped.get()
                || (matches!(mode, 10 | 21 | 22) && ticks.get() >= boundary.get())
        });
        if mode == 9 {
            read.values.class = MemoryClass::Recovery;
        }
        let mut limits = bounds;
        if mode == 4 {
            limits.payload_bytes = 0;
        }
        if mode == 5 {
            limits.semantic_bytes = 0;
        }
        let mut write_io = logical();
        if mode == 7 {
            write_io.write_bytes_remaining = 0;
        }
        let mut write = MemberRecordWriteContext {
            sink: &mut controlled,
            memory: if mode == 3 { &alien } else { &memory },
            io: &mut write_io,
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits,
            cancelled: || false,
        };
        let guard = if mode == 1 {
            stale
        } else {
            read.transaction.guard()
        };
        let mut fence = live;
        if mode == 2 {
            fence.authority.epoch += 1;
        }
        let result = attempt(
            Attempt {
                mode,
                proof: &proof,
                family: &family,
            },
            &mut read,
            &mut write,
            &guard,
            &fence,
        );
        assert!(result.is_err(), "mode{mode}");
        if matches!(mode, 6 | 10 | 13 | 21 | 22 | 23) {
            assert!(cancelled(&result), "mode{mode}: {result:?}");
        }
        if matches!(mode, 8 | 13 | 23) {
            assert!(put.get(), "mode{mode} must follow a real durable put");
        }
        if mode == 25 {
            assert!(
                !put.get(),
                "nonempty Grant scopes must reject before any write"
            );
        }
        let guard = tx.guard();
        assert!(
            tx.seal(&guard, &live, 1).is_err(),
            "mode{mode} must poison prior typed prefix"
        );
    }
}
