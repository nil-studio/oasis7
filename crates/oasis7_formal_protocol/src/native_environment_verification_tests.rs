use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::Cell, sync::Arc};

struct Charged(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Charged {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct Fixture {
    memory: MemoryBudget,
    descriptor: NativeExecutionEnvironmentV1,
    data: Arc<ReservedValue<Vec<u8>>>,
    metadata: Arc<ReservedValue<Vec<u8>>>,
    data_id: String,
    metadata_id: String,
    _descriptor_admission: MemoryReservation,
}
impl Fixture {
    fn new() -> Self {
        let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
        let admission = memory.reserve(16 * 1024, MemoryClass::Ordinary).unwrap();
        let data = Arc::new(
            memory
                .reserve(MAX_NATIVE_CHUNK_BYTES + 128, MemoryClass::Ordinary)
                .unwrap()
                .retain(vec![7; MAX_NATIVE_CHUNK_BYTES]),
        );
        let metadata = Arc::new(
            memory
                .reserve(129, MemoryClass::Ordinary)
                .unwrap()
                .retain(vec![9]),
        );
        let data_id = blake3::hash(&data).to_hex().to_string();
        let metadata_id = blake3::hash(&metadata).to_hex().to_string();
        let mut whole = blake3::Hasher::new();
        for _ in 0..9 {
            whole.update(&data);
        }
        let descriptor = NativeExecutionEnvironmentV1 {
            schema_version: 1,
            target: "native-cursor-fixture".into(),
            os: "test".into(),
            arch: "test".into(),
            components: vec![
                NativeEnvironmentComponentV1 {
                    role: NativeEnvironmentRole::Executable,
                    name: "exe".into(),
                    byte_len: 9 * MAX_NATIVE_CHUNK_BYTES as u64,
                    content_digest: whole.finalize().to_hex().to_string(),
                    chunks: (0..9)
                        .map(|_| NativeEnvironmentChunkV1 {
                            digest: data_id.clone(),
                            byte_len: MAX_NATIVE_CHUNK_BYTES as u32,
                        })
                        .collect(),
                },
                NativeEnvironmentComponentV1 {
                    role: NativeEnvironmentRole::BuildMetadata,
                    name: "build".into(),
                    byte_len: 1,
                    content_digest: metadata_id.clone(),
                    chunks: vec![NativeEnvironmentChunkV1 {
                        digest: metadata_id.clone(),
                        byte_len: 1,
                    }],
                },
            ],
        };
        Self {
            memory,
            descriptor,
            data,
            metadata,
            data_id,
            metadata_id,
            _descriptor_admission: admission,
        }
    }
    fn read(&self, id: &str) -> Result<Charged, &'static str> {
        if id == self.data_id {
            Ok(Charged(self.data.clone()))
        } else if id == self.metadata_id {
            Ok(Charged(self.metadata.clone()))
        } else {
            Err("missing fixture chunk")
        }
    }
}

#[test]
fn charged_carriers_keep_eight_occurrence_steps_and_no_reads_after_completion() {
    let fixture = Fixture::new();
    let before = fixture.memory.used();
    let mut cursor = NativeVerificationCursor::new(fixture.descriptor.clone()).unwrap();
    let calls = Cell::new(0);
    for expected in [8, 10, 10] {
        cursor
            .advance_with_visit(
                |id| {
                    calls.set(calls.get() + 1);
                    fixture.read(id)
                },
                |id, _| {
                    let source = if id == fixture.data_id {
                        &fixture.data
                    } else {
                        &fixture.metadata
                    };
                    assert_eq!(Arc::strong_count(source), 2);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(cursor.verified_chunks(), expected);
        assert_eq!(calls.get(), expected);
        assert_eq!(cursor.complete(), expected == 10);
        assert_eq!(fixture.memory.used(), before);
        assert_eq!(Arc::strong_count(&fixture.data), 1);
        assert_eq!(Arc::strong_count(&fixture.metadata), 1);
    }
}

#[test]
fn source_visit_and_caught_unwind_preserve_the_committed_prefix_for_retry() {
    let fixture = Fixture::new();
    let before = fixture.memory.used();
    for mode in 0..3 {
        let mut cursor = NativeVerificationCursor::new(fixture.descriptor.clone()).unwrap();
        let reads = Cell::new(0);
        let visits = Cell::new(0);
        let mut read = |id: &str| {
            reads.set(reads.get() + 1);
            if reads.get() == 3 {
                if mode == 0 {
                    return Err("interrupted");
                }
                if mode == 2 {
                    panic!("source unwound after two verified chunks");
                }
            }
            fixture.read(id)
        };
        if mode == 2 {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cursor.advance(&mut read)
                }))
                .is_err()
            );
        } else {
            let result = cursor.advance_with_visit(&mut read, |_, _| {
                visits.set(visits.get() + 1);
                if mode == 1 && visits.get() == 3 {
                    Err("interrupted")
                } else {
                    Ok(())
                }
            });
            assert!(matches!(
                result,
                Err(NativeVerifyError::Source("interrupted"))
            ));
        }
        assert_eq!(cursor.verified_chunks(), 2);
        assert!(!cursor.complete());
        cursor.advance(|id| fixture.read(id)).unwrap();
        assert_eq!(cursor.verified_chunks(), 10);
        assert!(cursor.complete());
        assert_eq!(fixture.memory.used(), before);
        assert_eq!(Arc::strong_count(&fixture.data), 1);
    }
}

#[test]
fn failed_chunk_or_component_integrity_never_advances_that_chunk() {
    let fixture = Fixture::new();
    let mut cursor = NativeVerificationCursor::new(fixture.descriptor.clone()).unwrap();
    assert!(matches!(
        cursor.advance(|_| Ok::<_, ()>(Charged(fixture.metadata.clone()))),
        Err(NativeVerifyError::ChunkIntegrity)
    ));
    assert_eq!(cursor.verified_chunks(), 0);
    let mut wrong = fixture.descriptor.clone();
    wrong.components[0].content_digest = "00".repeat(32);
    let mut cursor = NativeVerificationCursor::new(wrong).unwrap();
    cursor.advance(|id| fixture.read(id)).unwrap();
    assert_eq!(cursor.verified_chunks(), 8);
    for _ in 0..2 {
        assert!(matches!(
            cursor.advance(|id| fixture.read(id)),
            Err(NativeVerifyError::ComponentIntegrity)
        ));
        assert_eq!(cursor.verified_chunks(), 8);
        assert!(!cursor.complete());
    }
}
