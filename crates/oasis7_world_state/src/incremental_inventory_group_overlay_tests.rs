use super::*;
use crate::{
    WorldState,
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
fn world_group_owner_real_gen_missing_existing_corrupt_cancel_work_and_orphan() {
    let mut original_group = None;
    for case in 0..6 {
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let pages = Pages::new(&memory);
        let mut source = WorldState::default();
        source.resources.clear();
        source.materials = [("legacy-zero".into(), 0)].into();
        source.material_ledgers.clear();
        if case == 0 || case == 5 {
            source
                .material_ledgers
                .insert(MaterialLedgerId::World, std::collections::BTreeMap::new());
        }
        if case == 5 {
            source
                .material_ledgers
                .get_mut(&MaterialLedgerId::World)
                .unwrap()
                .insert("orphan".into(), -7);
        }
        let roots = install(&source, &memory, &pages);
        drop(source);
        let mut quota = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut family = work();
        let mut scope = scope_work();
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
            &mut scope,
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
            ep,
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
        let cancelled = std::cell::Cell::new(false);
        let mut oi = io();
        let mut ok = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut oi,
            },
            key_work: &mut ok,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || cancelled.get(),
        };
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
        let group =
            WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Group, Some(semantic.digest))
                .unwrap();
        if case == 0 {
            original_group = ctx
                .transaction
                .read(StateNamespaceV1::Business, group.as_bytes())
                .unwrap();
            assert!(original_group.is_some());
        }
        let corrupt_metadata = if case == 2 {
            let metadata =
                WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None).unwrap();
            Some(
                ctx.transaction
                    .read(StateNamespaceV1::Business, metadata.as_bytes())
                    .unwrap()
                    .unwrap(),
            )
        } else {
            None
        };
        let guard = ctx.transaction.guard();
        let current = open_current_inventory_material_scope_v1(
            &parent,
            &roots,
            &pages,
            &mut ctx,
            &mut family,
            &mut scope,
            &guard,
            &live,
        )
        .unwrap();
        if let Some(root) = corrupt_metadata {
            pages.remove(&root);
        }
        if case == 3 {
            cancelled.set(true);
        }
        if case == 5 {
            ctx.transaction
                .write(StateNamespaceV1::Business, group.as_bytes(), None)
                .unwrap();
        }
        let mut writer = Writer(&pages);
        let mut wi = io();
        let mut wk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut write = MemberRecordWriteContext {
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
        let mut work = InventoryWorldGroupWorkV1 {
            steps_remaining: if case == 4 { 0 } else { 100 },
        };
        let guard = ctx.transaction.guard();
        let result = ensure_world_ledger_group_overlay_v1(
            &current, &mut ctx, &mut write, &mut work, &guard, &live,
        );
        if case <= 1 {
            assert_eq!(result.unwrap(), case == 1);
            let actual = ctx
                .transaction
                .read(StateNamespaceV1::Business, group.as_bytes())
                .unwrap();
            assert_eq!(
                actual, original_group,
                "explicit group owner must match original GEN group canonical bytes"
            );
            let guard = ctx.transaction.guard();
            let refreshed = open_current_inventory_material_scope_v1(
                &parent,
                &roots,
                &pages,
                &mut ctx,
                &mut family,
                &mut scope,
                &guard,
                &live,
            )
            .unwrap();
            assert!(refreshed.world_ledger_present());
            assert_eq!(refreshed.world_materials().len(), 0);
            assert_eq!(refreshed.legacy_materials().next().unwrap().quantity(), 0);
            let guard = ctx.transaction.guard();
            assert!(
                !ensure_world_ledger_group_overlay_v1(
                    &refreshed, &mut ctx, &mut write, &mut work, &guard, &live
                )
                .unwrap()
            );
            // Real empty group leaves all scalar/member observations absent; no dummy
            // member was emitted as a side effect of ensuring the group.
            assert_eq!(refreshed.world_materials().len(), 0);
        } else {
            assert!(result.is_err());
            let guard = ctx.transaction.guard();
            assert!(tx.seal(&guard, &live, 1).is_err());
        }
    }
}
