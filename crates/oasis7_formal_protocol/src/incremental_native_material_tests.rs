use super::*;
use crate::native_environment::{
    NativeEnvironmentChunkV1, NativeEnvironmentComponentV1, NativeEnvironmentRole,
    NativeExecutionEnvironmentV1,
};
use fixture::incremental::setup_with_native;
fn native() -> (NativeExecutionEnvironmentV1, Vec<Vec<u8>>) {
    let raw = vec![
        vec![3; 1024 * 1024],
        vec![7; 17],
        b"exact build metadata".to_vec(),
    ];
    let mut executable = blake3::Hasher::new();
    executable.update(&raw[0]);
    executable.update(&raw[1]);
    let chunk = |bytes: &[u8]| NativeEnvironmentChunkV1 {
        digest: blake3::hash(bytes).to_hex().to_string(),
        byte_len: bytes.len() as u32,
    };
    (
        NativeExecutionEnvironmentV1 {
            schema_version: 1,
            target: "test-native".into(),
            os: "test".into(),
            arch: "test".into(),
            components: vec![
                NativeEnvironmentComponentV1 {
                    role: NativeEnvironmentRole::Executable,
                    name: "runtime".into(),
                    byte_len: (raw[0].len() + raw[1].len()) as u64,
                    content_digest: executable.finalize().to_hex().to_string(),
                    chunks: vec![chunk(&raw[0]), chunk(&raw[1])],
                },
                NativeEnvironmentComponentV1 {
                    role: NativeEnvironmentRole::BuildMetadata,
                    name: "build".into(),
                    byte_len: raw[2].len() as u64,
                    content_digest: blake3::hash(&raw[2]).to_hex().to_string(),
                    chunks: vec![chunk(&raw[2])],
                },
            ],
        },
        raw,
    )
}
fn prepared(
    descriptor: Vec<u8>,
    raw: Vec<Vec<u8>>,
) -> (Setup, Objects, ReservedValue<CheckedCommitCore>) {
    let id = *blake3::hash(&descriptor).as_bytes();
    let s = setup_with_native(MemoryBudget::new(64 << 20, 4 << 20).unwrap(), id);
    let (mut objects, pages) = sources(&s, &s.history);
    for bytes in raw.into_iter().chain(std::iter::once(descriptor)) {
        let charge = s
            .memory
            .reserve(bytes.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        objects.values.insert(
            *blake3::hash(&bytes).as_bytes(),
            Arc::new(charge.retain(bytes)),
        );
    }
    let candidate = candidate(&s, s.commit.record().clone());
    let core = verify_commit_core(
        &request(&s, &candidate),
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    (s, objects, core)
}
#[test]
fn original_native_descriptor_and_every_component_are_checked_and_bound_to_core() {
    let (native, raw) = native();
    let bytes = native.encode().unwrap();
    let length = bytes.len() as u64 + raw.iter().map(|b| b.len() as u64).sum::<u64>();
    let (s, mut objects, core) = prepared(bytes, raw);
    let mut quota = budget();
    quota.objects_remaining = 4;
    quota.original_bytes_remaining = length;
    let checked =
        verify_native_environment(&core, &mut objects, &s.memory, &mut quota, || false).unwrap();
    assert_eq!(checked.body_id(), s.history.body.digest().unwrap());
    assert_eq!(checked.environment_id(), s.config.execution_environment);
    assert_eq!(
        hex::encode(checked.executable_id()),
        native.components[0].content_digest
    );
    assert_eq!(checked.chunk_occurrences(), 3);
    assert_eq!(
        checked.component_bytes(),
        native.components.iter().map(|c| c.byte_len).sum::<u64>()
    );
    assert_eq!(
        (quota.objects_remaining, quota.original_bytes_remaining),
        (0, 0)
    );
    assert_eq!(
        core.execution_artifacts().native,
        s.config.execution_environment
    );
}
#[test]
fn missing_substituted_and_individually_valid_but_wrong_whole_component_fail() {
    for mode in 0..4 {
        let (mut native, raw) = native();
        if mode == 2 {
            native.components[0].content_digest = fixture::h(99);
        }
        let (s, mut objects, core) = prepared(native.encode().unwrap(), raw);
        let id = if mode == 3 {
            s.config.execution_environment
        } else {
            hex::decode(&native.components[0].chunks[0].digest)
                .unwrap()
                .try_into()
                .unwrap()
        };
        if mode == 0 {
            objects.values.remove(&id);
        }
        if mode == 1 || mode == 3 {
            let charge = s.memory.reserve(64, MemoryClass::Ordinary).unwrap();
            objects
                .values
                .insert(id, Arc::new(charge.retain(vec![9; 32])));
        }
        let before = s.memory.used();
        let mut quota = budget();
        quota.original_bytes_remaining = 16 << 20;
        let error = verify_native_environment(&core, &mut objects, &s.memory, &mut quota, || false)
            .unwrap_err();
        if mode == 2 {
            assert!(matches!(
                error,
                CoreError::Artifact("native component content hash")
            ));
        }
        assert_eq!(s.memory.used(), before);
    }
}
#[test]
fn admission_cancel_pool_and_one_less_object_or_bytes_never_return_marker() {
    for mode in 0..6 {
        let (native, raw) = native();
        let bytes = native.encode().unwrap();
        let length = bytes.len() as u64 + raw.iter().map(|b| b.len() as u64).sum::<u64>();
        let (s, mut objects, core) = prepared(bytes, raw);
        let before_reads = objects.reads;
        let mut quota = budget();
        quota.objects_remaining = 4;
        quota.original_bytes_remaining = length;
        if mode == 0 {
            quota.objects_remaining = 3;
        }
        if mode == 1 {
            quota.original_bytes_remaining -= 1;
        }
        if mode == 3 {
            objects.memory = MemoryBudget::new(64 << 20, 0).unwrap();
        }
        let held = if mode == 4 {
            Some(s.memory.reserve(48 << 20, MemoryClass::Ordinary).unwrap())
        } else {
            None
        };
        let before = s.memory.used();
        let mut calls = 0;
        assert!(
            verify_native_environment(&core, &mut objects, &s.memory, &mut quota, || {
                calls += 1;
                mode == 2 || (mode == 5 && calls > 2)
            })
            .is_err()
        );
        assert_eq!(s.memory.used(), before);
        if mode == 2 || mode == 3 || mode == 4 {
            assert_eq!(objects.reads, before_reads);
        }
        drop(held);
    }
}
#[test]
fn hostile_descriptor_declaration_is_rejected_under_metadata_admission() {
    let mut sequence = vec![0xa1, 0x6a];
    sequence.extend_from_slice(b"components");
    sequence.push(0x9a);
    sequence.extend_from_slice(&257u32.to_be_bytes());
    let mut long_key = vec![0xa1, 0x7a];
    long_key.extend_from_slice(&(256u32 * 1024).to_be_bytes());
    long_key.resize(long_key.len() + 256 * 1024, b'x');
    long_key.push(0xf6);
    for bytes in [sequence, long_key] {
        let (s, mut objects, core) = prepared(bytes, vec![]);
        let before = s.memory.used();
        let error =
            verify_native_environment(&core, &mut objects, &s.memory, &mut budget(), || false)
                .unwrap_err();
        assert!(matches!(
            error,
            CoreError::Artifact("native descriptor format/canonical bytes")
        ));
        assert!(
            error.to_string().len() < 128,
            "untrusted parse text must not outlive its admission"
        );
        assert_eq!(s.memory.used(), before);
    }
}

#[test]
fn caught_chunk_source_unwind_keeps_allowance_spent_and_releases_phase() {
    struct Panics<'a> {
        source: &'a mut Objects,
        reads: usize,
    }
    impl OriginalObjectReader for Panics<'_> {
        type Error = std::io::Error;
        fn allocation_budget(&self) -> &MemoryBudget {
            &self.source.memory
        }
        fn read_original(
            &mut self,
            id: &[u8; 32],
            max: usize,
            memory: &MemoryBudget,
        ) -> std::io::Result<Arc<ReservedValue<Vec<u8>>>> {
            self.reads += 1;
            let bytes = self.source.read_original(id, max, memory)?;
            if self.reads == 2 {
                panic!("native chunk source unwind");
            }
            Ok(bytes)
        }
    }
    let (native, raw) = native();
    let descriptor = native.encode().unwrap();
    let length = descriptor.len() as u64;
    let (s, mut objects, core) = prepared(descriptor, raw);
    let mut reader = Panics {
        source: &mut objects,
        reads: 0,
    };
    let mut quota = budget();
    quota.original_bytes_remaining = 16 << 20;
    let before = s.memory.used();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            verify_native_environment(&core, &mut reader, &s.memory, &mut quota, || false)
        }))
        .is_err()
    );
    assert_eq!(quota.objects_remaining, 30);
    assert_eq!(
        quota.original_bytes_remaining,
        (16 << 20) - length - (1 << 20)
    );
    assert_eq!(s.memory.used(), before);
}
