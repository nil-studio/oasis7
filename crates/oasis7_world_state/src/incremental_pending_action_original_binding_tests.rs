use super::*;
#[test]
fn swallowed_overflow_and_cancel_sink_errors_remain_terminal() {
    let mut cancel = || false;
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        max: 2,
        stop: None,
        cancelled: &mut cancel,
    };
    assert!(sink.write_all(b"too long").is_err());
    assert!(matches!(
        sink.failure(),
        Some(ValueTreeError::ResourceLimited(_))
    ));
    assert!(sink.write(b"a").is_err());
    assert!(sink.flush().is_err());
    assert_eq!(sink.bytes, 0);
    let mut cancel = || true;
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        max: 100,
        stop: None,
        cancelled: &mut cancel,
    };
    assert!(sink.write(b"x").is_err());
    assert!(matches!(sink.failure(), Some(ValueTreeError::Cancelled)));
    assert!(sink.write(b"").is_err());
}
#[test]
fn hash_sink_short_writes_account_exact_bytes() {
    let source = vec![0xa5; 32769];
    let mut cancel = || false;
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        max: source.len() as u64,
        stop: None,
        cancelled: &mut cancel,
    };
    sink.write_all(&source).unwrap();
    assert_eq!(sink.bytes, source.len() as u64);
    assert_eq!(sink.hash.finalize(), blake3::hash(&source));
    assert!(sink.failure().is_none());
}

pub(crate) struct Objects {
    pub(crate) memory: MemoryBudget,
    pub(crate) class: MemoryClass,
    pub(crate) bytes: Vec<u8>,
    pub(crate) swallow: bool,
}
impl oasis7_formal_protocol::incremental_material::OriginalObjectReader for Objects {
    type Error = std::io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    fn read_original(
        &mut self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        panic!("binding must stream originals")
    }
}
impl OriginalObjectStreamReader for Objects {
    fn stream_original(
        &mut self,
        _: &[u8; 32],
        _: u64,
        _: &MemoryBudget,
        out: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        let result = out.write_all(&self.bytes);
        if !self.swallow {
            result?;
        }
        Ok(self.bytes.len() as u64)
    }
}
pub(crate) fn work() -> PendingActionOriginalWorkV1 {
    PendingActionOriginalWorkV1 {
        objects_remaining: 10,
        original_bytes_remaining: 1 << 20,
        max_original_bytes: 1 << 20,
        canonical: CanonicalCborWorkV1 {
            visits_remaining: 100000,
            bytes_remaining: 1 << 20,
            max_depth: 128,
            max_map_keys: 1024,
            max_key_bytes: 4096,
        },
    }
}
#[test]
fn actual_original_binding_success_wrong_bytes_class_limits_and_swallowed_error() {
    let (memory, record, bytes) =
        crate::incremental_runtime_pending_action_record::tests::recovered_fixture();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut objects = Objects {
        memory: memory.clone(),
        class: MemoryClass::Ordinary,
        bytes: bytes.clone(),
        swallow: false,
    };
    let checked =
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut work(), || false)
            .unwrap();
    assert_eq!(checked.original(), record.value().original().unwrap());
    objects.bytes[0] ^= 1;
    assert!(
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut work(), || false)
            .is_err()
    );
    objects.bytes = bytes.clone();
    objects.class = MemoryClass::Recovery;
    assert!(
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut work(), || false)
            .is_err()
    );
    objects.class = MemoryClass::Ordinary;
    let mut limited = work();
    limited.objects_remaining = 0;
    assert!(
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut limited, || false)
            .is_err()
    );
    let mut limited = work();
    limited.original_bytes_remaining = 0;
    assert!(
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut limited, || false)
            .is_err()
    );
    assert!(
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut work(), || true)
            .is_err()
    );
    objects.bytes.extend_from_slice(b"overflow");
    objects.swallow = true;
    assert!(
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut work(), || false)
            .is_err()
    );
    objects.bytes.clear();
    assert!(
        verify_pending_action_original_v1(&record, &mut objects, &memory, &mut work(), || false)
            .is_err()
    );
}
