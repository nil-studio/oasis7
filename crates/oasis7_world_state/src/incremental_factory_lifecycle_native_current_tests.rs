use super::*;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_codec::TypedRecordWriteWorkV1;
use family::WorldNativeFieldReadWorkV1;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, TransactionLimits,
};
#[path = "incremental_world_native_family_test_support.rs"]
mod support;
use support::*;
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [41; 32],
        authority: AuthorityBinding {
            token: [42; 32],
            epoch: 0,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 256,
        max_writes: 16,
        max_facts: 4,
        max_pending_node_bytes: 8 << 20,
    }
}
fn receipt() -> crate::FactoryRecycleReceiptV1 {
    crate::FactoryRecycleReceiptV1 {
        operator_agent_id: "builder".into(),
        factory_id: "factory".into(),
        recycle_ledger: crate::MaterialLedgerId::world(),
        recovered: vec![crate::MaterialStack::new("ore", 7)],
        durability_ppm: 500000,
    }
}
fn tw() -> TypedRecordWriteWorkV1 {
    support::write_work()
}
#[test]
fn lifecycle_receipt_actual_point_cas_then_reread_and_foreign_target_abort() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let initial = receipt();
    let source = rows::<FactoryRecycleReceiptsFieldV1>(
        &memory,
        &pages,
        &[("factory".into(), initial.clone())],
    );
    let ns = FactoryRecycleReceiptsFieldV1::profile()
        .unwrap()
        .namespace()
        .unwrap();
    let roots = install(&memory, &pages, ns, &source);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(&memory, &pages, roots.state_root(ns), &mut quota, &mut keys);
    let q = family::qualify_world_native_field_v1::<FactoryRecycleReceiptsFieldV1>(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent(&roots), [43; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let observed = selected::<FactoryRecycleReceiptsFieldV1, _, _, _, _, _>(
        SelectedKey::Receipt("factory"),
        &q,
        &roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut work().typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(observed.value(), Some(&initial));
    let mut next = initial.clone();
    next.durability_ppm = 250000;
    {
        let mut writer = Writer(&pages);
        let mut write_io = io();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        };
        write_current_factory_recycle_receipt_v1(
            &observed,
            "factory",
            &next,
            &mut tx,
            &mut ctx,
            &mut write,
            &mut 8,
            &mut tw(),
            &live,
        )
        .unwrap();
    }
    let guard = tx.guard();
    let reread = selected::<FactoryRecycleReceiptsFieldV1, _, _, _, _, _>(
        SelectedKey::Receipt("factory"),
        &q,
        &roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut work().typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(reread.value(), Some(&next));
    {
        let mut writer = Writer(&pages);
        let mut write_io = io();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        };
        assert!(
            write_current_factory_recycle_receipt_v1(
                &reread,
                "foreign-target",
                &initial,
                &mut tx,
                &mut ctx,
                &mut write,
                &mut 8,
                &mut tw(),
                &live
            )
            .is_err()
        );
    }
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
#[test]
fn lifecycle_native_absence_is_real_and_stale_owner_cannot_write() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::new(&memory);
    let source = rows::<FactoryRecycleReceiptsFieldV1>(&memory, &pages, &[]);
    let ns = FactoryRecycleReceiptsFieldV1::profile()
        .unwrap()
        .namespace()
        .unwrap();
    let roots = install(&memory, &pages, ns, &source);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(&memory, &pages, roots.state_root(ns), &mut quota, &mut keys);
    let q = family::qualify_world_native_field_v1::<FactoryRecycleReceiptsFieldV1>(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent(&roots), [44; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let observed = selected::<FactoryRecycleReceiptsFieldV1, _, _, _, _, _>(
        SelectedKey::Receipt("missing"),
        &q,
        &roots,
        &mut tx,
        &mut ctx,
        &mut scan(),
        &mut work().typed,
        &guard,
        &live,
    )
    .unwrap();
    assert!(observed.value().is_none());
    tx.write(
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
        b"revision-probe",
        None,
    )
    .unwrap();
    assert!(observed.validate(&mut tx, &memory, &live).is_err());
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
