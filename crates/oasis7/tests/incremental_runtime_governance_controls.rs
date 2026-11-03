#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::*;
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    World, incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_controls::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
use std::{io::Cursor, sync::atomic::AtomicBool};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(r) => ValueTreeError::ResourceLimited(r),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        _ => ValueTreeError::Invalid("control state store"),
    }
}
struct Dual<'a, A, B>(&'a mut A, &'a mut B, &'a MemoryBudget);
impl<A: ValueNodeWriter, B: ValueNodeWriter> ValueNodeWriter for Dual<'_, A, B> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        // The second bounded page copy is charged before allocating it.
        // Tests use the source producer pool for both buffered destinations.
        let copy = self
            .2
            .reserve(bytes.len(), MemoryClass::Ordinary)?
            .retain(bytes.to_vec());
        self.0.put_node(id, bytes)?;
        self.1.put_node(id, copy)
    }
}
#[test]
fn actual_world_controls_finish_raw_zstd_mirrors_and_reopen_with_recovery_only_allowance() {
    for source_compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let paths = [
            directory.0.join("source"),
            directory.0.join("raw"),
            directory.0.join("zstd"),
        ];
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let mut expected_authority = [0u8; 128];
        let authority_len;
        let roots = {
            // The original World is caller-owned functional input, not a C<T
            // measurement. GEN borrows it; no snapshot/World clone is made.
            let world = World::new();
            authority_len = world.tick_consensus_authority_source().len();
            assert!(authority_len <= expected_authority.len());
            expected_authority[..authority_len]
                .copy_from_slice(world.tick_consensus_authority_source().as_bytes());
            assert_eq!(world.governance_emergency_brake_until_tick(), None);
            let source =
                IncrementalPageStore::create(&paths[0], memory.clone(), source_compression)
                    .unwrap();
            let roots = {
                let _batch = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
                let mut rows = Vec::with_capacity(6);
                let mut values = source.value_session(physical());
                world
                    .prepare_incremental_runtime_controls_genesis_v1(
                        &mut MemberRecordWriteContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut logical(),
                            key_work: &mut SemanticKeyBudget {
                                bytes_remaining: 1 << 20,
                            },
                            limits: MemberRecordLimitsV1 {
                                semantic_bytes: 1024,
                                payload_bytes: 4096,
                                record_bytes: 8192,
                            },
                            cancelled: || false,
                        },
                        &mut 6,
                        &mut |row| {
                            assert!(rows.len() < 6);
                            rows.push(row);
                            Ok(())
                        },
                    )
                    .unwrap();
                assert_eq!(rows.len(), 6);
                let mut roots =
                    RootSetV1::empty("actual-runtime-controls".into(), [37; 32]).unwrap();
                for ns in [StateNamespaceV1::Allocator, StateNamespaceV1::Authorization] {
                    let mut writes: Vec<StateWriteV1> = rows
                        .iter()
                        .filter(|r| r.namespace == ns)
                        .map(|r| r.mutation.clone())
                        .collect();
                    writes.sort_by(|a, b| a.key.cmp(&b.key));
                    let workspace = memory
                        .reserve((writes.len() + 1) * (512 << 10), MemoryClass::Ordinary)
                        .unwrap();
                    let state = source
                        .state_session(physical(), MemoryClass::Ordinary)
                        .unwrap();
                    let delta = workspace.retain(
                        StateDeltaV1::build(
                            StateRootV1::empty(ns).unwrap(),
                            writes,
                            &state,
                            8 << 20,
                        )
                        .unwrap(),
                    );
                    source
                        .value_session(physical())
                        .put_state_pages(&delta, &memory)
                        .unwrap();
                    *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() =
                        delta.after.clone();
                }
                roots
            };
            let targets = [
                IncrementalPageStore::create(&paths[1], memory.clone(), PageCompression::Raw)
                    .unwrap(),
                IncrementalPageStore::create(&paths[2], memory.clone(), PageCompression::Zstd)
                    .unwrap(),
            ];
            let state = source
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = source.value_session(physical());
            let mut reads = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut ctx = WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let stopped = AtomicBool::new(false);
            let mut a_io = physical();
            let mut b_io = physical();
            let mut never_a = || false;
            let mut never_b = || false;
            let mut a = targets[0]
                .buffered_page_writer(&mut a_io, &stopped, &mut never_a)
                .unwrap();
            let mut b = targets[1]
                .buffered_page_writer(&mut b_io, &stopped, &mut never_b)
                .unwrap();
            let marked = transfer_runtime_controls_pages_v1(
                &roots,
                &mut ctx,
                &mut RuntimeControlsPagesWorkV1 {
                    outer_scan: &mut scan(),
                    max_text_bytes: 4096,
                    max_pages: 6 * (257 + 2),
                },
                &mut Dual(&mut a, &mut b, &memory),
            )
            .unwrap();
            a.finish().unwrap();
            b.finish().unwrap();
            let first = targets[0].value_session(physical());
            let second = targets[1].value_session(physical());
            marked
                .visit_pages::<ValueTreeError>(|id| {
                    for reader in [&first, &second] {
                        let page = reader.read_node(id, 8192, &memory, MemoryClass::Ordinary)?;
                        assert_eq!(blake3::hash(&page).as_bytes(), id);
                    }
                    Ok(())
                })
                .unwrap();
            // The second operation arms cancellation only after traversal and
            // after a real newly published final recipe. No provisional marker
            // escapes when buffered finish fails, even if all bytes reached disk.
            let cancelled_target = IncrementalPageStore::create(
                &directory.0.join("finish-cancel"),
                memory.clone(),
                PageCompression::Zstd,
            )
            .unwrap();
            let phase = std::cell::Cell::new(false);
            let before_finish = std::cell::Cell::new(usize::MAX);
            let recipe_count = || {
                std::fs::read_dir(cancelled_target.root().join("recipes"))
                    .unwrap()
                    .count()
            };
            let mut cancel_after_publish = || phase.get() && recipe_count() > before_finish.get();
            let mut final_io = physical();
            let mut writer = cancelled_target
                .buffered_page_writer(&mut final_io, &stopped, &mut cancel_after_publish)
                .unwrap();
            let completion = (|| {
                let provisional = transfer_runtime_controls_pages_v1(
                    &roots,
                    &mut ctx,
                    &mut RuntimeControlsPagesWorkV1 {
                        outer_scan: &mut scan(),
                        max_text_bytes: 4096,
                        max_pages: 6 * (257 + 2),
                    },
                    &mut writer,
                )?;
                assert!(writer.pending_reserved_bytes() > 0);
                before_finish.set(recipe_count());
                phase.set(true);
                writer.finish().map_err(state_error)?;
                Ok::<_, ValueTreeError>(provisional)
            })();
            assert!(matches!(completion, Err(ValueTreeError::Cancelled)));
            assert!(recipe_count() > before_finish.get());
            roots
        };
        assert_eq!(memory.used(), 0);
        drop(memory);
        // World, producer sources, target writers and original pool are gone.
        // Each target independently rebuilds the complete typed qualification.
        for (path, compression) in [
            (&paths[1], PageCompression::Raw),
            (&paths[2], PageCompression::Zstd),
        ] {
            let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let store =
                IncrementalPageStore::open(path, cold.clone(), compression, MemoryClass::Recovery)
                    .unwrap();
            let _ordinary = cold
                .reserve((4 << 20) - cold.used(), MemoryClass::Ordinary)
                .unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Recovery)
                .unwrap();
            let values = store.value_session(physical());
            let mut reads = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut ctx = WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &cold,
                    class: MemoryClass::Recovery,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            qualify_runtime_controls_v1(&roots, &mut ctx, &mut scan(), 4096)
                .unwrap()
                .validate_binding(&roots)
                .unwrap();
            ctx.root = roots.state_root(StateNamespaceV1::Authorization);
            let mut output = [0; 128];
            let value = read_runtime_control_v1(
                RuntimeControlFieldV1::TickConsensusAuthoritySource,
                &mut ctx,
                128,
                &mut Cursor::new(output.as_mut_slice()),
            )
            .unwrap();
            assert_eq!(
                value,
                RuntimeControlReadV1::Text {
                    utf8_bytes: authority_len as u64
                }
            );
            assert_eq!(
                &output[..authority_len],
                &expected_authority[..authority_len]
            );
            assert_eq!(
                read_runtime_control_v1(
                    RuntimeControlFieldV1::GovernanceEmergencyBrakeUntilTick,
                    &mut ctx,
                    128,
                    &mut std::io::sink()
                )
                .unwrap(),
                RuntimeControlReadV1::Unit
            );
            ctx.root = roots.state_root(StateNamespaceV1::Allocator);
            assert_eq!(
                read_runtime_control_v1(
                    RuntimeControlFieldV1::NextGovernanceIdentityPenaltyId,
                    &mut ctx,
                    128,
                    &mut std::io::sink()
                )
                .unwrap(),
                RuntimeControlReadV1::Unsigned(1)
            );
        }
    }
}
