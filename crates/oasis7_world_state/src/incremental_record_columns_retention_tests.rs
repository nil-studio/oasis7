//! Actual simultaneously-live typed outputs retain spent admission only.
use super::*;
use std::{cell::Cell, rc::Rc};
fn seed_empty(memory: &MemoryBudget) -> (Pages, RootSetV1, FieldDescriptor) {
    let mut pages = Pages::new(memory);
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "retention.columns",
        shape: FieldShape::Map,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    };
    let mut rows = Vec::new();
    {
        let mut value_io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut ctx = MemberRecordWriteContext {
            sink: &mut pages,
            memory,
            io: &mut value_io,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        };
        let row = write_field_metadata_v1(
            field,
            &mut PresenceRecordWriteContext {
                sink: &mut *ctx.sink,
                memory: ctx.memory,
                io: &mut *ctx.io,
                key_work: &mut *ctx.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                },
                cancelled: &mut ctx.cancelled,
            },
        )
        .unwrap();
        crate::incremental_field_write::emit(row.field_key, row.value_root, &mut ctx, &mut |row| {
            rows.push(row);
            Ok(())
        })
        .unwrap();
    }
    let mut roots = RootSetV1::empty("retained-parts".into(), [101; 32]).unwrap();
    install(memory, &mut pages, &mut roots, &rows);
    (pages, roots, field)
}
#[test]
fn many_small_typed_parts_do_not_pin_each_maximum_allowance() {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let (pages, roots, field) = seed_empty(&memory);
    let _vector = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let mut held = Vec::with_capacity(8);
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    let before = memory.used();
    for _ in 0..8 {
        let body = read_columns_v1(
            ColumnReadProfileV1 {
                field,
                schema: [9; 32],
                groups: &[],
                reference_schema: None,
            },
            ctx.root,
            &mut ctx,
            &mut scan(),
            4 << 20,
        )
        .unwrap();
        let charge = body.finish_parts().unwrap();
        assert!(charge.bytes() < 16 << 10);
        held.push(charge);
        assert_eq!(
            memory.used(),
            before + held.iter().map(|c| c.bytes()).sum::<usize>()
        );
    }
    while let Some(charge) = held.pop() {
        drop(charge);
        assert_eq!(
            memory.used(),
            before + held.iter().map(|c| c.bytes()).sum::<usize>()
        );
    }
}
struct DropWitness {
    memory: MemoryBudget,
    at_least: Rc<Cell<usize>>,
    seen: Rc<Cell<bool>>,
}
impl Drop for DropWitness {
    fn drop(&mut self) {
        assert!(
            self.memory.used() >= self.at_least.get(),
            "output must drop while its root and extra charges remain live"
        );
        self.seen.set(true);
    }
}
#[test]
fn small_typed_outputs_trim_unused_credit_and_drop_value_before_all_charges() {
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let (pages, roots, field) = seed_empty(&memory);
    let _vector = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
    let mut held = Vec::with_capacity(8);
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    let before = memory.used();
    for _ in 0..8 {
        let mut body = read_columns_v1(
            ColumnReadProfileV1 {
                field,
                schema: [9; 32],
                groups: &[],
                reference_schema: None,
            },
            ctx.root,
            &mut ctx,
            &mut scan(),
            4 << 20,
        )
        .unwrap();
        body.spend_output(
            std::mem::size_of::<DropWitness>() + std::mem::size_of::<MemoryReservation>() + 128,
        )
        .unwrap();
        let threshold = Rc::new(Cell::new(0));
        let seen = Rc::new(Cell::new(false));
        let extra = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
        let output = body
            .finish(
                DropWitness {
                    memory: memory.clone(),
                    at_least: threshold.clone(),
                    seen: seen.clone(),
                },
                vec![extra],
            )
            .unwrap();
        held.push((output, threshold, seen));
        assert!(memory.used() - before < 128 << 10);
    }
    while let Some((output, threshold, seen)) = held.pop() {
        threshold.set(memory.used());
        drop(output);
        assert!(seen.get());
    }
    assert_eq!(memory.used(), before);
}
