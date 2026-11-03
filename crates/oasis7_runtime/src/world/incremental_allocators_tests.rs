use super::*;
use crate::world::incremental_body::tests::{Pages, fixture, io, limits};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionParent, TransitionVerifier},
    incremental_state::{StateDeltaV1, StateWriteV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};

type Stored = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
#[derive(Clone, Default)]
struct Values {
    pages: Rc<RefCell<Stored>>,
    fail: Rc<Cell<bool>>,
    puts: Rc<Cell<usize>>,
}
impl ValueNodeWriter for Values {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.fail.get() {
            return Err(ValueTreeError::Invalid("allocator injected put"));
        }
        self.pages.borrow_mut().insert(id, Arc::new(bytes));
        self.puts.set(self.puts.get() + 1);
        Ok(())
    }
}
impl ValueNodeReader for Values {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let value = self
            .pages
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("allocator test missing value"))?;
        if value.len() > max || !value.uses_budget(memory) || class != MemoryClass::Ordinary {
            return Err(ValueTreeError::Invalid("allocator test pool/bound/class"));
        }
        Ok(value)
    }
}
fn seeded(
    memory: &MemoryBudget,
    initial: RuntimeSequenceV1,
) -> (World, ExecutionParent, Pages, Values) {
    let (mut world, mut parent, mut pages) = fixture(memory);
    world.next_event_id = initial.next;
    world.next_event_id_era = initial.era;
    world.next_action_id = initial.next;
    world.next_action_id_era = initial.era;
    world.next_intent_id = initial.next;
    world.next_intent_id_era = initial.era;
    world.next_proposal_id = initial.next;
    world.next_proposal_id_era = initial.era;
    let mut values = Values::default();
    let mut writes = Vec::new();
    world
        .write_iws_allocator_genesis(
            &mut BodyMutationContext {
                sink: &mut values,
                memory,
                io: &mut io(),
                cancelled: || false,
            },
            |key, id| {
                writes.push(StateWriteV1 {
                    key: key.to_vec(),
                    before: None,
                    after: Some(id),
                });
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(writes.len(), 4);
    let delta = StateDeltaV1::build(
        parent.roots.state[1].clone(),
        writes,
        &pages,
        4 * 1024 * 1024,
    )
    .unwrap();
    parent.roots.state[1] = delta.after;
    for page in delta.new_nodes {
        pages.0.insert(page.digest, page.bytes);
    }
    (world, parent, pages, values)
}
#[test]
fn closed_allocator_bytes_preserve_event_and_reject_cross_family_values() {
    let value = RuntimeSequenceV1 {
        next: 0x0102030405060708,
        era: u64::MAX,
    };
    for allocator in RuntimeAllocatorV1::ALL {
        let (bytes, length) = allocator.encode(value);
        let (key, key_len) = allocator.key();
        if allocator == RuntimeAllocatorV1::Event {
            assert_eq!(&key[..key_len], b"oasis7.iws.event-sequence-key.v1\0");
            let mut expected = b"oasis7.iws.event-sequence.v1\0".to_vec();
            expected.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
            expected.extend_from_slice(&[255; 8]);
            assert_eq!(&bytes[..length], expected);
        } else {
            let tag = match allocator {
                RuntimeAllocatorV1::Action => 1,
                RuntimeAllocatorV1::Intent => 2,
                RuntimeAllocatorV1::Proposal => 3,
                _ => unreachable!(),
            };
            let mut expected_key = b"oasis7.iws.runtime-sequence-key.v1\0".to_vec();
            expected_key.push(tag);
            assert_eq!(&key[..key_len], expected_key);
            let mut expected = b"oasis7.iws.runtime-sequence.v1\0".to_vec();
            expected.push(tag);
            expected.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
            expected.extend_from_slice(&[255; 8]);
            assert_eq!(&bytes[..length], expected);
        }
        assert_eq!(allocator.decode(&bytes[..length]).unwrap(), value);
        assert!(allocator.decode(&bytes[..length - 1]).is_err());
        for other in RuntimeAllocatorV1::ALL {
            if other != allocator {
                assert!(other.decode(&bytes[..length]).is_err());
            }
        }
    }
}
#[test]
fn rolling_allocator_overlay_uses_original_values_and_independent_rollover_oracle() {
    for (initial, expected_id, expected_after) in [
        (
            RuntimeSequenceV1 { next: 0, era: 7 },
            1,
            RuntimeSequenceV1 { next: 2, era: 7 },
        ),
        (
            RuntimeSequenceV1 { next: 42, era: 8 },
            42,
            RuntimeSequenceV1 { next: 43, era: 8 },
        ),
        (
            RuntimeSequenceV1 {
                next: u64::MAX,
                era: 9,
            },
            u64::MAX,
            RuntimeSequenceV1 { next: 1, era: 10 },
        ),
        (
            RuntimeSequenceV1 {
                next: u64::MAX,
                era: u64::MAX,
            },
            u64::MAX,
            RuntimeSequenceV1 {
                next: 1,
                era: u64::MAX,
            },
        ),
    ] {
        let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
        let (world, parent, pages, mut values) = seeded(&memory, initial);
        let source = values.clone();
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [71; 32], &pages, &memory, limits())
                .unwrap();
        let mut budget = io();
        let mut context = BodyMutationContext {
            sink: &mut values,
            memory: &memory,
            io: &mut budget,
            cancelled: || false,
        };
        for allocator in RuntimeAllocatorV1::ALL {
            assert_eq!(
                World::read_iws_allocator(allocator, &mut txn, &source, &mut context).unwrap(),
                initial
            );
            assert_eq!(
                World::contribute_iws_allocate(allocator, initial, &mut txn, &source, &mut context)
                    .unwrap(),
                (expected_id, expected_after)
            );
            assert_eq!(
                World::read_iws_allocator(allocator, &mut txn, &source, &mut context).unwrap(),
                expected_after
            );
        }
        // A second reservation uses the updated overlay, with an independent literal oracle.
        let second_id = expected_after.next;
        let second = RuntimeSequenceV1 {
            next: expected_after.next + 1,
            era: expected_after.era,
        };
        assert_eq!(
            World::contribute_iws_allocate(
                RuntimeAllocatorV1::Action,
                expected_after,
                &mut txn,
                &source,
                &mut context
            )
            .unwrap(),
            (second_id, second)
        );
        let guard = txn.guard();
        let prepared = txn.seal(&guard, &parent.fence().unwrap(), 0).unwrap();
        assert_eq!(prepared.record.writes.len(), 4);
        TransitionVerifier {
            parent: &parent,
            operation: [71; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 * 1024 * 1024,
        }
        .verify(&prepared.record)
        .unwrap();
        assert_eq!(world.next_action_id, initial.next);
        assert_eq!(world.next_action_id_era, initial.era);
    }
}
#[test]
fn allocator_errors_poison_prior_typed_allocation() {
    for mode in 0..4 {
        let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
        let foreign = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
        let initial = RuntimeSequenceV1 { next: 5, era: 2 };
        let (_, parent, pages, mut values) = seeded(&memory, initial);
        let source = values.clone();
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [72; 32], &pages, &memory, limits())
                .unwrap();
        World::contribute_iws_allocate(
            RuntimeAllocatorV1::Action,
            initial,
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
        let expected = if mode == 0 {
            RuntimeSequenceV1 { next: 6, era: 2 }
        } else {
            initial
        };
        values.fail.set(mode == 2);
        let result = World::contribute_iws_allocate(
            RuntimeAllocatorV1::Intent,
            expected,
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: if mode == 1 { &foreign } else { &memory },
                io: &mut io(),
                cancelled: || mode == 3,
            },
        );
        assert!(result.is_err(), "failure mode {mode}");
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &parent.fence().unwrap(), 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn final_allocator_cancellation_poison_keeps_no_successful_prefix() {
    for allocator in RuntimeAllocatorV1::ALL {
        let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
        let initial = RuntimeSequenceV1 { next: 11, era: 3 };
        let (_, parent, pages, mut values) = seeded(&memory, initial);
        let source = values.clone();
        let puts = values.puts.clone();
        let prefix = if allocator == RuntimeAllocatorV1::Action {
            RuntimeAllocatorV1::Intent
        } else {
            RuntimeAllocatorV1::Action
        };
        // Calibrate the standalone value writer, not the contributor under test.
        let (bytes, length) = allocator.encode(RuntimeSequenceV1 { next: 12, era: 3 });
        let calibration_start = puts.get();
        let mut seen = calibration_start;
        let mut after_put = 0;
        write_field_bytes(
            &bytes[..length],
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || {
                    if puts.get() != seen {
                        seen = puts.get();
                        after_put = 0;
                    }
                    after_put += 1;
                    false
                },
            },
        )
        .unwrap();
        let writer_checks = after_put;
        let writer_puts = puts.get() - calibration_start;
        assert!(writer_puts > 0);
        assert!(writer_checks > 0);
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [73; 32], &pages, &memory, limits())
                .unwrap();
        World::contribute_iws_allocate(
            prefix,
            initial,
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
        let puts_before = puts.get();
        seen = puts_before;
        after_put = 0;
        let result = World::contribute_iws_allocate(
            allocator,
            initial,
            &mut txn,
            &source,
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || {
                    if puts.get() != seen {
                        seen = puts.get();
                        after_put = 0;
                    }
                    after_put += 1;
                    puts.get() == puts_before + writer_puts && after_put == writer_checks + 1
                },
            },
        );
        assert_eq!(
            puts.get(),
            puts_before + writer_puts,
            "must reach the final real descriptor put"
        );
        assert_eq!(after_put, writer_checks + 1);
        assert!(matches!(
            result,
            Err(BodyMutationError::Value(ValueTreeError::Cancelled))
        ));
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &parent.fence().unwrap(), 0),
            Err(ExecutionError::Aborted)
        ));
    }
}
