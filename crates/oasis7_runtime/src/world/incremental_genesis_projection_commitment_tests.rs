//! Full original GEN with the existing finite work, real stored original bytes,
//! and a WorldService oracle supplied solely for equality assertions.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1},
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::mem::size_of;

struct Retained<'a> {
    memory: &'a MemoryBudget,
    originals: &'a BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    reads: usize,
}
impl crate::world::PaidProjectionRetainedOriginalReaderV1 for Retained<'_> {
    type RetainedError = BodyMutationError;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_retained_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, BodyMutationError> {
        common
            .charge(1, 32)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.memory.same_pool(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "projection fixture retained source pool",
            ));
        }
        self.reads += 1;
        let bytes = self.originals.get(id).ok_or(BodyMutationError::Invalid(
            "projection fixture actual retained original absent",
        ))?;
        if bytes.len() > max || !bytes.uses_budget(self.memory) {
            return Err(BodyMutationError::Invalid(
                "projection fixture retained original admission",
            ));
        }
        common
            .charge(1, bytes.len() as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        assert_eq!(*blake3::hash(bytes).as_bytes(), *id);
        Ok(Arc::clone(bytes))
    }
}
fn original_common_error(error: BodyMutationError, expected: &str) {
    let BodyMutationError::Original(error) = error else {
        panic!("original Common error lost: {error:?}")
    };
    assert!(
        matches!(error.downcast_ref::<FormalError>(), Some(FormalError::Invalid(note)) if *note == expected)
    );
}

pub(in crate::world) fn assert_world_root(world: &World, expected: &str) {
    // Exact existing projected_genesis_complete_journal_case fixture limits.
    let memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let baseline = memory.used();
    let meter = FormalBoundedWorkV1::new(256_000_000, 4 << 30).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let mut pages = Pages::default();
    let mut originals = BTreeMap::new();
    let mut emitted = 0;
    let mut io = ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let projected = world
        .prepare_iws_genesis_with_projection_and_artifact_work_v1(
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 8 << 20,
                    record_bytes: 9 << 20,
                },
                cancelled: || false,
            },
            (&mut work(), &mut |reference, bytes, common| {
                common
                    .charge(1, bytes.len() as u64)
                    .map_err(|e| ValueTreeError::Io(std::io::Error::other(e)))?;
                assert_eq!(reference.id, *blake3::hash(bytes).as_bytes());
                assert_eq!(reference.byte_len, bytes.len() as u64);
                let fee =
                    memory.reserve(size_of::<Vec<u8>>() + bytes.len(), MemoryClass::Ordinary)?;
                originals.insert(reference.id, Arc::new(fee.retain(bytes.to_vec())));
                Ok(())
            }),
            &mut |_, _| Err(ValueTreeError::Invalid("unexpected pending action")),
            &mut |_, _, _| Err(BodyMutationError::Invalid("unexpected mailbox")),
            &mut |row| {
                assert!(row.uses_budget(&memory));
                emitted += 1;
                Ok(())
            },
            &mut common,
        )
        .unwrap();
    assert!(emitted > 0);
    assert!(!pages.0.is_empty());
    let mut reader = Retained {
        memory: &memory,
        originals: &originals,
        reads: 0,
    };
    // Exact existing module_execution_fixture_genesis::canonical limits.
    let mut canonical = CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 32 << 20,
        max_depth: 64,
        max_map_keys: 1024,
        max_key_bytes: 4096,
    };
    let before = (meter.remaining_steps(), meter.remaining_read_bytes());
    let result = projected
        .prepare_formal_projection_commitment_bounded_v1(&mut reader, &mut common, &mut canonical)
        .unwrap();
    assert_eq!(result.root_bounded(&mut common).unwrap(), expected);
    assert!(meter.remaining_steps() < before.0);
    assert!(meter.remaining_read_bytes() < before.1);
    assert!(canonical.visits_remaining < 1_000_000);
    assert!(canonical.bytes_remaining < 32 << 20);

    // Equal fresh numeric limits never replace the source's actual Core.
    let other_work =
        FormalBoundedWorkV1::new(meter.remaining_steps(), meter.remaining_read_bytes()).unwrap();
    let mut other_stop = || false;
    let mut other = FormalBoundedContextV1::new(&memory, &other_work, &mut other_stop);
    assert!(matches!(
        result.validate_context(&mut other),
        Err(BodyMutationError::Invalid(
            "GEN projection commitment context"
        ))
    ));
    assert!(
        projected
            .prepare_formal_projection_commitment_bounded_v1(
                &mut reader,
                &mut other,
                &mut canonical
            )
            .is_err()
    );

    let other_memory = MemoryBudget::new(512 << 20, 16 << 20).unwrap();
    let mut other_stop = || false;
    let mut other = FormalBoundedContextV1::new(&other_memory, &meter, &mut other_stop);
    assert!(matches!(
        result.validate_context(&mut other),
        Err(BodyMutationError::Invalid(
            "GEN projection commitment context"
        ))
    ));

    let before_cancel = (
        meter.remaining_steps(),
        meter.remaining_read_bytes(),
        canonical.visits_remaining,
        canonical.bytes_remaining,
        reader.reads,
    );
    let mut cancelled = || true;
    let mut cancelled_common = FormalBoundedContextV1::new(&memory, &meter, &mut cancelled);
    let error = match projected.prepare_formal_projection_commitment_bounded_v1(
        &mut reader,
        &mut cancelled_common,
        &mut canonical,
    ) {
        Err(error) => error,
        Ok(_) => panic!("cancelled GEN projection succeeded"),
    };
    original_common_error(error, "bounded archive read cancelled");
    assert_eq!(
        (
            meter.remaining_steps(),
            meter.remaining_read_bytes(),
            canonical.visits_remaining,
            canonical.bytes_remaining,
            reader.reads
        ),
        before_cancel
    );
    assert_eq!(result.root_bounded(&mut common).unwrap(), expected);
    drop(result);
    drop(projected);
    drop(reader);
    drop(originals);
    drop(pages);
    assert_eq!(memory.used(), baseline);
}
