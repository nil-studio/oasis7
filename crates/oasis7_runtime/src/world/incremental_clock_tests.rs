use super::*;
use crate::world::incremental_body::{
    tests::{fixture, io, limits},
    write_field_bytes,
};
use oasis7_formal_protocol::incremental_execution::{ExecutionError, TransitionVerifier};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};

fn literal(time: u64, metadata: bool) -> (Vec<u8>, Vec<u8>) {
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    semantic.push(0);
    let mut key = b"oasis7.iws.world-field-key.v1\0".to_vec();
    key.extend_from_slice(&[1, 0, 2, 1, if metadata { 1 } else { 3 }, 0, 4]);
    key.extend_from_slice(b"time");
    key.extend_from_slice(&if metadata {
        [0; 32]
    } else {
        *blake3::hash(&semantic).as_bytes()
    });
    let mut value = if metadata {
        b"oasis7.iws.field-presence.v1\0".to_vec()
    } else {
        b"oasis7.iws.field-member.v1\0".to_vec()
    };
    value.extend_from_slice(&(key.len() as u16).to_be_bytes());
    if metadata {
        value.extend_from_slice(&0u64.to_be_bytes());
    } else {
        value.push(2);
        value.extend_from_slice(&(semantic.len() as u64).to_be_bytes());
        value.extend_from_slice(&8u64.to_be_bytes());
    }
    value.extend_from_slice(&key);
    if !metadata {
        value.extend_from_slice(&semantic);
        value.extend_from_slice(&time.to_be_bytes());
    }
    (key, value)
}
#[test]
fn clock_genesis_matches_registered_unit_u64_literal_profile() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _source = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    for time in [0, 11, u64::MAX] {
        let mut world = World::new();
        world.state.time = time;
        let mut fields = Vec::new();
        world
            .write_iws_clock_genesis(
                &mut BodyMutationContext {
                    sink: &mut Discard,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                |key, id| {
                    fields.push((key.to_vec(), id));
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(fields.len(), 2);
        for ((key, id), metadata) in fields.into_iter().zip([true, false]) {
            let (expected_key, expected_value) = literal(time, metadata);
            let expected_id = write_field_bytes(
                &expected_value,
                &mut BodyMutationContext {
                    sink: &mut Discard,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
            )
            .unwrap();
            assert_eq!(key, expected_key);
            assert_eq!(id, expected_id);
        }
    }
}
#[test]
fn runtime_body_reads_the_updated_registered_clock_overlay() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (mut world, parent, pages) = fixture(&memory);
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [91; 32], &pages, &memory, limits()).unwrap();
    world.state.time = 12;
    world
        .write_iws_clock_genesis(
            &mut BodyMutationContext {
                sink: &mut Discard,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
            |key, id| {
                txn.write(Ns::Allocator, key, Some(id))?;
                Ok(())
            },
        )
        .unwrap();
    let mut view = world.state.agents["alice"].state.body_view.clone();
    view.mass_kg = 12;
    world
        .contribute_iws_body_attributes_update(
            "alice",
            &view,
            &mut txn,
            &mut BodyMutationContext {
                sink: &mut Discard,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &parent.fence().unwrap(), 0).unwrap();
    assert_eq!(prepared.record.writes.len(), 2);
    assert_eq!(prepared.record.after.simulation_tick, 0); // WorldTime is independent.
    TransitionVerifier {
        parent: &parent,
        operation: [91; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
}
#[test]
fn missing_stale_or_old_private_clock_never_preserves_a_typed_body_prefix() {
    for mode in 0..4 {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let (mut world, parent, pages) = fixture(&memory);
        let mut txn =
            ExecutionTransaction::begin(parent.clone(), [92; 32], &pages, &memory, limits())
                .unwrap();
        let mut view = world.state.agents["alice"].state.body_view.clone();
        view.mass_kg = 12;
        world
            .contribute_iws_body_attributes_update(
                "alice",
                &view,
                &mut txn,
                &mut BodyMutationContext {
                    sink: &mut Discard,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
            )
            .unwrap();
        let (metadata, _) = literal(11, true);
        let (entry, _) = literal(11, false);
        match mode {
            0 => txn.write(Ns::Allocator, &metadata, None).unwrap(),
            1 => txn.write(Ns::Allocator, &entry, None).unwrap(),
            2 => {
                world.state.time = 12;
                world
                    .write_iws_clock_genesis(
                        &mut BodyMutationContext {
                            sink: &mut Discard,
                            memory: &memory,
                            io: &mut io(),
                            cancelled: || false,
                        },
                        |key, id| {
                            txn.write(Ns::Allocator, key, Some(id))?;
                            Ok(())
                        },
                    )
                    .unwrap();
                world.state.time = 11;
            }
            _ => {
                txn.write(Ns::Allocator, &metadata, None).unwrap();
                txn.write(Ns::Allocator, &entry, None).unwrap();
                let mut old = b"oasis7.iws.world-time-value.v1\0".to_vec();
                old.extend_from_slice(&11u64.to_be_bytes());
                let id = write_field_bytes(
                    &old,
                    &mut BodyMutationContext {
                        sink: &mut Discard,
                        memory: &memory,
                        io: &mut io(),
                        cancelled: || false,
                    },
                )
                .unwrap();
                txn.write(Ns::Allocator, b"oasis7.iws.world-time-key.v1\0", Some(id))
                    .unwrap();
            }
        }
        view.mass_kg = 13;
        let result = world.contribute_iws_body_attributes_update(
            "alice",
            &view,
            &mut txn,
            &mut BodyMutationContext {
                sink: &mut Discard,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        );
        assert!(
            matches!(
                result,
                Err(BodyMutationError::Invalid("world clock precondition"))
            ),
            "mode {mode}: {result:?}"
        );
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &parent.fence().unwrap(), 0),
            Err(ExecutionError::Aborted)
        ));
    }
}
