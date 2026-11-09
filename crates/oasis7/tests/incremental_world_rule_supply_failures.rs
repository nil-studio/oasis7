#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1, RootSetV1,
        TransactionLimits,
    },
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
    incremental_field_overlay::WorldFieldOverlayContextV1,
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
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError, incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1;
use std::cell::Cell;
use supply_fixture::{authorization, batch};
struct Controlled<'a, W> {
    inner: &'a mut W,
    puts: &'a Cell<u64>,
    cancelled: &'a Cell<bool>,
    cancel: bool,
    fail: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for Controlled<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.fail && self.puts.get() == 1 {
            return Err(ValueTreeError::Invalid(
                "injected supply put after first real put",
            ));
        }
        self.inner.put_node(id, bytes)?;
        self.puts.set(self.puts.get() + 1);
        if self.cancel {
            self.cancelled.set(true)
        }
        Ok(())
    }
}
impl<W: ValueNodeWriter> StatePageWriter for Controlled<'_, W> {}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 100000,
        read_bytes_remaining: 256 << 20,
    }
}
fn roots(root: &StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("supply-failures".into(), [3; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == Ns::Authorization)
        .unwrap() = root.clone();
    roots
}
#[test]
fn qualified_member_failures_poison_a_successful_prefix_and_cancel_after_real_put() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("failures"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let source = WorldState {
        world_rules: Some(WorldRulesStateV1 {
            governance_effects: [("p".into(), "original".into())].into(),
            supply_authorizations: [(KEY.into(), authorization())].into(),
            supply_batches: [(KEY.into(), batch())].into(),
            ..WorldRulesStateV1::default()
        }),
        ..WorldState::default()
    };
    let mut rows = produce(&store, &memory, &source);
    let mut writer = store.value_session(physical());
    {
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                payload_bytes: 165,
                ..limits()
            },
            cancelled: || false,
        };
        prepare_supply_root_genesis_v1(&source, &mut write, &mut 128, 4 << 20, &mut |row| {
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            Ok(())
        })
        .unwrap();
    }
    let before = build(&store, &memory, &rows);
    let parent = ExecutionParent {
        roots: roots(&before),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut fixed_io = logical();
    let mut fixed_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut fixed = WorldFieldReadContextV1 {
        root: &before,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut fixed_io,
        },
        key_work: &mut fixed_keys,
        semantic_bytes: 1024,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let family = qualify_supply_family_v1(
        &parent.roots,
        &mut fixed,
        &mut scan(),
        &mut scan(),
        &mut 4096,
        165,
    )
    .unwrap();
    let proof = open_qualified_supply_record_v1(
        SupplyRootKeyV1::Authorization(KEY),
        &parent.roots,
        &family,
        &mut fixed,
    )
    .unwrap()
    .unwrap();
    let no_op_boundary = Cell::new(usize::MAX);
    let scalar_boundary = Cell::new(usize::MAX);
    for case in 0..16 {
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
        {
            let mut prefix = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    payload_bytes: 165,
                    ..limits()
                },
                cancelled: || false,
            };
            let guard = prefix.transaction.guard();
            assert!(
                change_world_rule_effect_overlay_v1(
                    WorldRuleEffectChangeV1 {
                        key: "p",
                        expected: WorldRuleEffectExpectedV1::Value("original"),
                        after: Some("prefix"),
                        max_current_payload_bytes: 128
                    },
                    &mut prefix,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
        }
        let ticks = Cell::new(0usize);
        let puts = Cell::new(0u64);
        let cancelled = Cell::new(false);
        let alien = MemoryBudget::new(64 << 20, 1 << 20).unwrap();
        let mut read_io = logical();
        if case == 6 {
            read_io.read_bytes_remaining = 0;
        }
        let mut keys = SemanticKeyBudget {
            bytes_remaining: if case == 7 { 0 } else { 1 << 20 },
        };
        let mut read = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: if case == 5 {
                    MemoryClass::Recovery
                } else {
                    MemoryClass::Ordinary
                },
                io: &mut read_io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || {
                ticks.set(ticks.get() + 1);
                case == 10
                    || cancelled.get()
                    || (case == 11 && ticks.get() >= no_op_boundary.get())
                    || (case == 15 && ticks.get() >= scalar_boundary.get())
            },
        };
        let mut controlled = Controlled {
            inner: &mut writer,
            puts: &puts,
            cancelled: &cancelled,
            cancel: case == 12,
            fail: case == 13,
        };
        let mut write_io = logical();
        if case == 9 {
            write_io.write_bytes_remaining = 0;
        }
        let mut bound = MemberRecordLimitsV1 {
            payload_bytes: 165,
            ..limits()
        };
        if case == 8 {
            bound.payload_bytes = 0;
        }
        let mut write = MemberRecordWriteContext {
            sink: &mut controlled,
            memory: if case == 4 { &alien } else { &memory },
            io: &mut write_io,
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: bound,
            cancelled: || false,
        };
        let guard = if case == 2 {
            stale
        } else {
            read.transaction.guard()
        };
        let mut fence = live;
        if case == 3 {
            fence.authority.epoch += 1;
        }
        let result = if case >= 14 {
            let original = source
                .world_rules
                .as_ref()
                .unwrap()
                .supply_authorizations
                .get(KEY)
                .unwrap();
            change_qualified_supply_scalars_v1(
                SupplyScalarChangeV1 {
                    owner: SupplyRootKeyV1::Authorization(KEY),
                    qualification: &proof,
                    expected: SupplyRootSourceV1::Authorization(original),
                    after: SupplyRootSourceV1::Authorization(original),
                    max_current_payload_bytes: 128,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut read,
                &mut write,
                &guard,
                &fence,
            )
        } else {
            change_qualified_supply_member_v1(
                SupplyMemberChangeV1 {
                    request: SupplyMemberRequestV1 {
                        owner: SupplyRootKeyV1::Authorization(KEY),
                        member: SupplyMemberKeyV1::Request(OLD),
                        qualification: &proof,
                    },
                    expected: Some(SupplyMemberValueV1::I64(if case == 1 {
                        i64::MAX
                    } else {
                        i64::MIN
                    })),
                    after: Some(SupplyMemberValueV1::I64(if case == 0 || case == 11 {
                        i64::MIN
                    } else {
                        i64::MAX
                    })),
                    max_pending_node_bytes: 4 << 20,
                },
                &mut read,
                &mut write,
                &guard,
                &fence,
            )
        };
        if case == 0 || case == 14 {
            assert!(result.unwrap().is_none());
            let boundary = if case == 0 {
                &no_op_boundary
            } else {
                &scalar_boundary
            };
            boundary.set(ticks.get());
            assert!(boundary.get() > 1);
            eprintln!(
                "no-op case {case} final cancellation boundary={}",
                boundary.get()
            );
            let guard = tx.guard();
            tx.seal(&guard, &live, 1).unwrap();
            continue;
        }
        assert!(result.is_err(), "case {case}");
        if matches!(case, 10..=12 | 15) {
            assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)
                        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::Cancelled
                        )))
                ),
                "case {case}: {result:?}"
            );
        }
        if case == 11 {
            assert_eq!(ticks.get(), no_op_boundary.get());
        }
        if case == 15 {
            assert_eq!(ticks.get(), scalar_boundary.get());
            assert_eq!(puts.get(), 0);
        }
        if case == 12 || case == 13 {
            assert!(puts.get() > 0, "case {case} failed before real put");
        }
        if case == 13 {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("injected supply put after first real put")
            );
        }
        let guard = tx.guard();
        assert!(
            matches!(tx.seal(&guard, &live, 1), Err(ExecutionError::Aborted)),
            "case {case}"
        );
    }
}
