use super::*;
use oasis7_formal_protocol::incremental_material::OriginalObjectReader;
use std::{cell::Cell, sync::Arc};
struct Source<'a> {
    bytes: &'a [u8],
    memory: &'a MemoryBudget,
    mode: u8,
    calls: usize,
    finished: &'a Cell<bool>,
}
impl OriginalObjectReader for Source<'_> {
    type Error = &'static str;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Recovery
    }
    fn read_original(
        &mut self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        panic!("must stream")
    }
}
impl OriginalObjectStreamReader for Source<'_> {
    fn stream_original(
        &mut self,
        _: &[u8; 32],
        max: u64,
        _: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        self.calls += 1;
        assert_eq!(max, self.bytes.len() as u64);
        if self.mode == 4 {
            return Err("missing original");
        }
        let bytes = if self.mode == 1 {
            &self.bytes[..self.bytes.len() - 1]
        } else {
            self.bytes
        };
        output.write_all(bytes).map_err(|_| "output rejected")?;
        if self.mode == 2 {
            assert!(output.write_all(&[7]).is_err()); // deliberately swallowed
            assert!(output.flush().is_err());
        }
        self.finished.set(true);
        Ok(self.bytes.len() as u64 + u64::from(self.mode == 3))
    }
}
fn work() -> RuntimeArtifactOriginalWorkV1 {
    RuntimeArtifactOriginalWorkV1 {
        objects_remaining: 1,
        bytes_remaining: 1 << 20,
        max_object_bytes: 1 << 20,
    }
}
#[test]
fn raw_original_literal_wire_and_stream_hash_are_distinct_from_business_key() {
    let original = OriginalArtifactRefV1 {
        id: [7; 32],
        byte_len: u64::MAX,
    };
    let bytes = runtime_artifact_original_bytes_v1(original);
    let mut literal = b"oasis7.iws.module-artifact-original.v1\0".to_vec();
    literal.extend([7; 32]);
    literal.extend(u64::MAX.to_be_bytes());
    assert_eq!(bytes.as_slice(), literal);
    assert_eq!(
        runtime_artifact_original_from_v1(&literal).unwrap(),
        original
    );
    for n in 0..literal.len() {
        assert!(runtime_artifact_original_from_v1(&literal[..n]).is_err());
    }
    literal.push(0);
    assert!(runtime_artifact_original_from_v1(&literal).is_err());
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let done = Cell::new(false);
    let mut source = Source {
        bytes: b"actual WASM bytes",
        memory: &memory,
        mode: 0,
        calls: 0,
        finished: &done,
    };
    let original = OriginalArtifactRefV1 {
        id: *blake3::hash(source.bytes).as_bytes(),
        byte_len: source.bytes.len() as u64,
    };
    let checked = verify_runtime_artifact_original_v1(
        original,
        &mut source,
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        &mut || false,
    )
    .unwrap();
    assert_eq!(checked.original(), original);
    assert_eq!(source.calls, 1);
    assert!(checked.uses_budget(&memory));
}
#[test]
fn truncated_swallowed_overflow_wrong_length_hash_source_and_final_cancel_do_not_mint() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    for mode in 0..6 {
        let done = Cell::new(false);
        let mut source = Source {
            bytes: b"actual original",
            memory: &memory,
            mode,
            calls: 0,
            finished: &done,
        };
        let mut original = OriginalArtifactRefV1 {
            id: *blake3::hash(source.bytes).as_bytes(),
            byte_len: source.bytes.len() as u64,
        };
        if mode == 0 {
            original.id[0] ^= 1;
        }
        let result = verify_runtime_artifact_original_v1(
            original,
            &mut source,
            &memory,
            MemoryClass::Recovery,
            &mut work(),
            &mut || mode == 5 && done.get(),
        );
        match mode {
            2 => assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Value(
                    ValueTreeError::ResourceLimited(_)
                ))
            )),
            4 => assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Source("missing original"))
            )),
            5 => assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Value(
                    ValueTreeError::Cancelled
                ))
            )),
            _ => assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Value(
                    ValueTreeError::Invalid(_)
                ))
            )),
        }
    }
    let done = Cell::new(false);
    let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let mut source = Source {
        bytes: b"x",
        memory: &foreign,
        mode: 0,
        calls: 0,
        finished: &done,
    };
    let original = OriginalArtifactRefV1 {
        id: *blake3::hash(b"x").as_bytes(),
        byte_len: 1,
    };
    assert!(
        verify_runtime_artifact_original_v1(
            original,
            &mut source,
            &memory,
            MemoryClass::Recovery,
            &mut work(),
            &mut || false
        )
        .is_err()
    );
    assert_eq!(source.calls, 0);
    source.memory = &memory;
    let mut empty = work();
    empty.objects_remaining = 0;
    assert!(
        verify_runtime_artifact_original_v1(
            original,
            &mut source,
            &memory,
            MemoryClass::Recovery,
            &mut empty,
            &mut || false
        )
        .is_err()
    );
    assert_eq!(source.calls, 0);
}

#[test]
fn borrowed_genesis_preserves_independent_string_keys_and_byte_reference_kind() {
    use crate::incremental_field_member::*;
    use std::collections::{BTreeMap, BTreeSet};
    struct Discard;
    impl ValueNodeWriter for Discard {
        fn put_node(
            &mut self,
            id: [u8; 32],
            bytes: ReservedValue<Vec<u8>>,
        ) -> Result<(), ValueTreeError> {
            assert_eq!(blake3::hash(&bytes).as_bytes(), &id);
            Ok(())
        }
    }
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(8192, MemoryClass::Ordinary).unwrap();
    let registered = BTreeSet::from(["".to_string(), " arbitrary 界 registered-only ".to_string()]);
    let originals: BTreeMap<String, Arc<[u8]>> = BTreeMap::from([
        ("".into(), Arc::from(&b""[..])),
        ("bytes-only".into(), Arc::from(&b"original"[..])),
    ]);
    let mut io = oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 1000,
        read_bytes_remaining: 1 << 20,
        write_bytes_remaining: 1 << 20,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut records = 6;
    let mut work = work();
    work.objects_remaining = 2;
    let mut persisted = 0;
    let mut metadata = 0;
    let mut set_members = 0;
    let mut byte_members = 0;
    prepare_runtime_artifact_content_genesis_v1(
        &registered,
        &originals,
        &mut MemberRecordWriteContext {
            sink: &mut Discard,
            memory: &memory,
            io: &mut io,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
        &mut records,
        &mut work,
        &mut |original, bytes| {
            assert_eq!(original.id, *blake3::hash(bytes).as_bytes());
            assert_eq!(original.byte_len, bytes.len() as u64);
            assert!(
                originals
                    .values()
                    .any(|v| v.as_ptr() == bytes.as_ptr() && v.len() == bytes.len())
            );
            persisted += 1;
            Ok(())
        },
        &mut |row| {
            let binding = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
            assert_eq!(binding.owner, FieldOwner::Runtime);
            match binding.kind {
                WorldFieldRecordKindV1::Metadata => metadata += 1,
                WorldFieldRecordKindV1::Entry => {
                    assert_eq!(binding.name, "module_artifacts");
                    set_members += 1;
                }
                WorldFieldRecordKindV1::Bytes => {
                    assert_eq!(binding.name, "module_artifact_bytes");
                    byte_members += 1;
                }
                _ => panic!("wrong artifact field kind"),
            }
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        (metadata, set_members, byte_members, persisted, records),
        (2, 2, 2, 2, 0)
    );
}
