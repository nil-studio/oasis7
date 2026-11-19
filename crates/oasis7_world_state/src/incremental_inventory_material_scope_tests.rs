use super::*;
use crate::{
    MaterialLedgerId, WorldState,
    incremental_inventory_genesis::{
        InventoryGenesisContext, InventoryWriteSink, prepare_inventory_genesis_v1,
    },
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
fn scope_work() -> InventoryMaterialScopeWorkV1 {
    InventoryMaterialScopeWorkV1 {
        max_entries_per_map: 32,
        max_material_bytes: 256,
        pending_entries_remaining: 10000,
        comparison_bytes_remaining: 64 << 20,
    }
}
fn projection(scope: &CurrentInventoryMaterialScopeV1) -> (Vec<(&str, i64)>, Vec<(&str, i64)>) {
    (
        scope
            .legacy_materials()
            .map(|e| (e.material(), e.quantity()))
            .collect(),
        scope
            .world_materials()
            .map(|e| (e.material(), e.quantity()))
            .collect(),
    )
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> InventoryFamilyWorkV1 {
    InventoryFamilyWorkV1 {
        records_remaining: 100000,
        index_steps_remaining: 1_000_000,
        max_semantic_bytes: 4096,
    }
}
struct Rows(Vec<ReservedValue<StateWriteV1>>);
impl InventoryWriteSink for Rows {
    fn put_write(&mut self, w: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        self.0.push(w);
        Ok(())
    }
}
fn install(source: &WorldState, memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let mut rows = Rows(Vec::new());
    let mut writer = Writer(pages);
    prepare_inventory_genesis_v1(
        source,
        &mut InventoryGenesisContext {
            values: &mut writer,
            writes: &mut rows,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            records_remaining: 10000,
            cancelled: || false,
        },
    )
    .unwrap();
    let mut writes: Vec<_> = rows.0.iter().map(|r| (**r).clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let mut roots = RootSetV1::empty("inventory-family".into(), [8; 32]).unwrap();
    let ns = StateNamespaceV1::Business;
    let delta = StateDeltaV1::build(roots.state_root(ns).clone(), writes, pages, 8 << 20).unwrap();
    for n in &delta.new_nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes(),
                memory
                    .reserve(n.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(n.bytes.clone()),
            )
            .unwrap();
    }
    *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after;
    roots
}
fn context<'a>(
    memory: &'a MemoryBudget,
    pages: &'a Pages,
    roots: &'a RootSetV1,
    quota: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}

#[test]
fn real_parent_current_scope_preserves_missing_empty_zero_negative_and_reads_only_delta() {
    for mode in 0..3 {
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let pages = Pages::new(&memory);
        let mut source = WorldState::default();
        source.resources.clear();
        source.materials = [("zero".into(), 0), ("negative".into(), -3)].into();
        source.material_ledgers.clear();
        if mode > 0 {
            source
                .material_ledgers
                .insert(MaterialLedgerId::World, std::collections::BTreeMap::new());
        }
        if mode == 2 {
            source
                .material_ledgers
                .get_mut(&MaterialLedgerId::World)
                .unwrap()
                .extend([("world-zero".into(), 0), ("world-negative".into(), -9)]);
        }
        source.material_ledgers.insert(
            MaterialLedgerId::Agent("other".into()),
            [("unrelated".into(), 99)].into(),
        );
        let roots = install(&source, &memory, &pages);
        drop(source);
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut family = work();
        let mut sw = scope_work();
        let proof = qualify_inventory_family_v1(
            &roots,
            &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
            &mut scan(),
            &mut family,
        )
        .unwrap();
        let parent = build_parent_inventory_material_scope_v1(
            &proof,
            &roots,
            &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
            &mut scan(),
            &mut family,
            &mut sw,
        )
        .unwrap();
        let ep = ExecutionParent {
            roots: roots.clone(),
            commit: [1; 32],
            authority: AuthorityBinding {
                token: [2; 32],
                epoch: 3,
            },
        };
        let mut tx = ExecutionTransaction::begin(
            ep.clone(),
            [3; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 1024,
                max_writes: 128,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let live = tx.parent_fence();
        // A pristine ordinary view must need ZERO parent value IO: all unchanged
        // material names and quantities come from the authenticated admitted cache.
        let current = {
            let mut no_io = oasis7_formal_protocol::incremental_value::ValueIoBudget {
                nodes_remaining: 0,
                read_bytes_remaining: 0,
                write_bytes_remaining: 0,
            };
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            };
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut no_io,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 2 << 20,
                take_state_error: || None,
                cancelled: || false,
            };
            let guard = ctx.transaction.guard();
            open_current_inventory_material_scope_v1(
                &parent,
                &roots,
                &pages,
                &mut ctx,
                &mut family,
                &mut sw,
                &guard,
                &live,
            )
            .unwrap()
        };
        assert_eq!(current.world_ledger_present(), mode > 0);
        assert_eq!(projection(&current).0, vec![("negative", -3), ("zero", 0)]);
        assert_eq!(
            projection(&current).1,
            if mode == 2 {
                vec![("world-negative", -9), ("world-zero", 0)]
            } else {
                vec![]
            }
        );
        {
            let mut other = ExecutionTransaction::begin(
                ep.clone(),
                [4; 32],
                &pages,
                &memory,
                TransactionLimits {
                    max_reads: 1024,
                    max_writes: 128,
                    max_facts: 8,
                    max_pending_node_bytes: 8 << 20,
                },
            )
            .unwrap();
            let mut quota = io();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 4096,
            };
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut other,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut quota,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 2 << 20,
                take_state_error: || None,
                cancelled: || false,
            };
            let guard = ctx.transaction.guard();
            assert!(current.validate_current(&mut ctx, &guard, &live).is_err());
            let guard = ctx.transaction.guard();
            assert!(other.seal(&guard, &live, 1).is_err());
        }
        let cancelled = std::cell::Cell::new(false);
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || cancelled.get(),
        };
        let mut writer = Writer(&pages);
        let mut wi = io();
        let mut wk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut write = crate::incremental_field_member::MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut wi,
            key_work: &mut wk,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 8,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        };
        let guard = ctx.transaction.guard();
        ctx.change(
            crate::incremental_inventory_change::InventoryChangeRefV1::Material {
                key: "negative",
                expected: Some(-3),
                after: None,
            },
            &mut write,
            &guard,
            &live,
        )
        .unwrap();
        let guard = ctx.transaction.guard();
        ctx.change(
            crate::incremental_inventory_change::InventoryChangeRefV1::Material {
                key: "new-zero",
                expected: None,
                after: Some(0),
            },
            &mut write,
            &guard,
            &live,
        )
        .unwrap();
        let guard = ctx.transaction.guard();
        let changed = open_current_inventory_material_scope_v1(
            &parent,
            &roots,
            &pages,
            &mut ctx,
            &mut family,
            &mut sw,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(projection(&changed).0, vec![("new-zero", 0), ("zero", 0)]);
        assert_eq!(projection(&changed).1, projection(&current).1);
        let guard = ctx.transaction.guard();
        changed.validate_current(&mut ctx, &guard, &live).unwrap();
        if mode == 0 {
            cancelled.set(true);
            let guard = ctx.transaction.guard();
            assert!(
                open_current_inventory_material_scope_v1(
                    &parent,
                    &roots,
                    &pages,
                    &mut ctx,
                    &mut family,
                    &mut sw,
                    &guard,
                    &live
                )
                .is_err()
            );
        } else if mode == 1 {
            let field = descriptor("materials", FieldShape::Map).unwrap();
            let key = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None).unwrap();
            ctx.transaction
                .write(StateNamespaceV1::Business, key.as_bytes(), None)
                .unwrap();
            let guard = ctx.transaction.guard();
            assert!(
                open_current_inventory_material_scope_v1(
                    &parent,
                    &roots,
                    &pages,
                    &mut ctx,
                    &mut family,
                    &mut sw,
                    &guard,
                    &live
                )
                .is_err()
            );
        } else {
            let field = descriptor("material_ledgers", FieldShape::NestedMap).unwrap();
            let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
                &MaterialLedgerId::World,
                &memory,
                MemoryClass::Ordinary,
                &mut SemanticKeyBudget {
                    bytes_remaining: 4096,
                },
                || false,
            )
            .unwrap();
            let key =
                WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Group, Some(semantic.digest))
                    .unwrap();
            ctx.transaction
                .write(StateNamespaceV1::Business, key.as_bytes(), None)
                .unwrap();
            let guard = ctx.transaction.guard();
            assert!(
                open_current_inventory_material_scope_v1(
                    &parent,
                    &roots,
                    &pages,
                    &mut ctx,
                    &mut family,
                    &mut sw,
                    &guard,
                    &live
                )
                .is_err()
            );
        }
        let guard = ctx.transaction.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
    }
}
