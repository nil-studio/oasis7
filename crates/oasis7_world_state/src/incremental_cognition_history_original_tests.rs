use super::*;
use crate::cognition_recovery::cognition_digest_v1;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::{CanonicalCborWorkV1, encode_canonical_cbor};
use serde_json::json;
use std::{
    cell::Cell,
    io::{self, Write},
};
#[test]
fn history_view_wire_is_fixed_and_does_not_turn_index_references_into_qualification() {
    use oasis7_formal_protocol::incremental_state::{StateNamespaceV1, StateRootV1};
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let absent = || CognitionHistoryIndexRefV1 {
        present: false,
        records: 0,
        root: StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
    };
    let head = || CognitionHistoryHeadV1 {
        sequence: 0,
        digest: None,
    };
    let mut view = CognitionHistoryViewV1 {
        roots: [0x35; 32],
        enabled: false,
        revision: u64::MAX,
        previous_view: Some(
            oasis7_formal_protocol::incremental_material::FactLocationV1 {
                original: [0x44; 32],
                height: u64::MAX,
                ordinal: 17,
            },
        ),
        heads: [head(), head(), head()],
        indexes: [absent(), absent(), absent(), absent()],
    };
    let bytes = view.canonical_bytes(&memory, || false).unwrap();
    let start = COGNITION_HISTORY_VIEW_MAGIC.len();
    assert_eq!(&bytes[..start], b"oasis7.cognition.history-view.v1\0");
    assert_eq!(&bytes[start..start + 32], &[0x35; 32]);
    assert_eq!(bytes[start + 32], 0);
    assert_eq!(&bytes[start + 33..start + 41], &u64::MAX.to_be_bytes());
    let decoded = CognitionHistoryViewV1::from_canonical_bytes(
        bytes.as_slice(),
        &memory,
        MemoryClass::Recovery,
        || false,
    )
    .unwrap();
    assert_eq!(&*decoded, &view);
    // A noncanonical absent digest must not have a second encoding.
    let mut corrupt = *bytes;
    let first_head_digest = start + 32 + 1 + 8 + 1 + 48 + 8 + 1;
    corrupt[first_head_digest] = 1;
    assert!(matches!(
        CognitionHistoryViewV1::from_canonical_bytes(
            &corrupt,
            &memory,
            MemoryClass::Recovery,
            || false
        ),
        Err(ValueTreeError::Invalid(_))
    ));
    // These deliberately unavailable roots are only metadata, never a proof.
    for index in &mut view.indexes[..2] {
        index.present = true;
        index.root = StateRootV1 {
            namespace: StateNamespaceV1::Artifacts,
            node_root: "11".repeat(32),
            leaf_count: 2,
        };
    }
    view.enabled = true;
    view.heads[0] = CognitionHistoryHeadV1 {
        sequence: u64::MAX,
        digest: Some([0x55; 32]),
    };
    let bytes = view.canonical_bytes(&memory, || false).unwrap();
    assert_eq!(
        &*CognitionHistoryViewV1::from_canonical_bytes(
            bytes.as_slice(),
            &memory,
            MemoryClass::Recovery,
            || false
        )
        .unwrap(),
        &view
    );
    view.indexes[0].records = 1;
    assert!(matches!(
        view.canonical_bytes(&memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    view.indexes[0].records = u64::MAX;
    assert!(matches!(
        view.canonical_bytes(&memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
}
struct Collect {
    bytes: Vec<u8>,
}
impl CognitionHistoryOriginalSinkV1 for Collect {
    fn store<C: FnMut() -> bool>(
        &mut self,
        producer: &mut CognitionHistoryOriginalProducerV1<'_, '_, '_, C>,
    ) -> Result<(), ValueTreeError> {
        producer.write_to(&mut self.bytes)
    }
}
fn work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 256 << 20,
        max_depth: 32,
        max_map_keys: 128,
        max_key_bytes: 4096,
    }
}
fn event(field: CognitionHistoryFieldV1) -> Value {
    let mut value = json!({"schema_version":"original-schema","journal_seq":12,"parent_event_digest":"原父ID","extra":null,"unknown_detail":{"unicode":"原文😀","u":u64::MAX}});
    let id = cognition_digest_v1(field.domain(), &value);
    value["event_digest"] = json!(id);
    value
}
#[test]
fn borrowed_history_originals_preserve_wire_and_distinguish_semantic_identity() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let _small_output = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    for field in CognitionHistoryFieldV1::ALL {
        let value = if field == CognitionHistoryFieldV1::Responses {
            let artifact = json!({"text":"原 response","number":u64::MAX});
            json!({"response_digest":cognition_digest_v1("oasis7.cognition.response.v1",&artifact),"response_artifact":artifact,"envelope_digest":"原 envelope","journal_head":"原头"})
        } else {
            event(field)
        };
        let reference = CognitionHistoryRecordRefV1 {
            field,
            ordinal: 17,
            value: &value,
        };
        let expected = encode_canonical_cbor(&value).unwrap();
        let mut context = CognitionHistoryWriteContextV1::new(&memory, work(), 4, || false);
        let mut sink = Collect {
            bytes: Vec::with_capacity(4096),
        };
        let stored =
            store_cognition_history_original_v1(reference, &mut context, &mut sink).unwrap();
        assert_eq!(sink.bytes, expected);
        assert_eq!(stored.id(), blake3::hash(&expected).as_bytes());
        assert_eq!(stored.field(), field);
        assert_eq!(stored.ordinal(), 17);
        assert_eq!(stored.fact().kind, field.fact_kind());
        assert_eq!(stored.encoded_bytes(), expected.len() as u64);
        assert!(!context.is_poisoned());
        let CognitionOriginalIdentityStatusV1::DigestMatched(semantic) = stored.identity_status()
        else {
            panic!("missing original digest")
        };
        assert_ne!(stored.id(), &semantic);
        assert_eq!(
            format!("blake3:{}", blake3::Hash::from(semantic)),
            reference.declared_identity().unwrap().unwrap()
        );
        assert_eq!(context.records_remaining, 3);
    }
}
#[test]
fn disabled_history_does_not_invent_a_projection_or_rewrite_nullable_records() {
    for input in [None, Some(&Value::Null)] {
        let source = CognitionHistorySourceRefV1::from_original(input).unwrap();
        assert!(!source.enabled());
        for field in CognitionHistoryFieldV1::ALL {
            let collection = CognitionHistoryCollectionRefV1::new(field, source).unwrap();
            assert!(!collection.present());
            assert!(collection.is_empty());
            assert!(collection.record(0).unwrap().is_none());
        }
    }
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let value = json!({"response_digest":"blake3:0000000000000000000000000000000000000000000000000000000000000000","response_artifact":null,"envelope_digest":"原","journal_head":""});
    let record = CognitionHistoryRecordRefV1 {
        field: CognitionHistoryFieldV1::Responses,
        ordinal: 0,
        value: &value,
    };
    let mut context = CognitionHistoryWriteContextV1::new(&memory, work(), 1, || false);
    let mut sink = Collect { bytes: Vec::new() };
    let result = store_cognition_history_original_v1(record, &mut context, &mut sink).unwrap();
    assert_eq!(
        result.identity_status(),
        CognitionOriginalIdentityStatusV1::ArtifactUnavailable
    );
    assert_eq!(sink.bytes, encode_canonical_cbor(&value).unwrap());
}
struct Skip;
impl CognitionHistoryOriginalSinkV1 for Skip {
    fn store<C: FnMut() -> bool>(
        &mut self,
        _: &mut CognitionHistoryOriginalProducerV1<'_, '_, '_, C>,
    ) -> Result<(), ValueTreeError> {
        Ok(())
    }
}
struct Fail;
impl Write for Fail {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl CognitionHistoryOriginalSinkV1 for Fail {
    fn store<C: FnMut() -> bool>(
        &mut self,
        p: &mut CognitionHistoryOriginalProducerV1<'_, '_, '_, C>,
    ) -> Result<(), ValueTreeError> {
        p.write_to(self)
    }
}
struct CancelAfter<'a>(&'a Cell<bool>);
impl CognitionHistoryOriginalSinkV1 for CancelAfter<'_> {
    fn store<C: FnMut() -> bool>(
        &mut self,
        p: &mut CognitionHistoryOriginalProducerV1<'_, '_, '_, C>,
    ) -> Result<(), ValueTreeError> {
        p.write_to(&mut io::sink())?;
        self.0.set(true);
        Ok(())
    }
}
#[test]
fn original_stream_failures_cannot_return_completion_or_reuse_allowances() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let value = event(CognitionHistoryFieldV1::JournalEvents);
    let record = CognitionHistoryRecordRefV1 {
        field: CognitionHistoryFieldV1::JournalEvents,
        ordinal: 0,
        value: &value,
    };
    let mut context = CognitionHistoryWriteContextV1::new(&memory, work(), 2, || false);
    assert!(matches!(
        store_cognition_history_original_v1(record, &mut context, &mut Skip),
        Err(ValueTreeError::Invalid(
            "Cognition History sink skipped producer"
        ))
    ));
    assert!(context.is_poisoned());
    assert_eq!(context.records_remaining, 1);
    assert!(matches!(
        store_cognition_history_original_v1(record, &mut context, &mut Skip),
        Err(ValueTreeError::Invalid(
            "Cognition History context poisoned"
        ))
    ));
    assert_eq!(context.records_remaining, 1);
    let mut context = CognitionHistoryWriteContextV1::new(&memory, work(), 1, || false);
    assert!(
        matches!(store_cognition_history_original_v1(record,&mut context,&mut Fail),Err(ValueTreeError::Io(ref e)) if e.kind()==io::ErrorKind::BrokenPipe)
    );
    assert!(context.is_poisoned());
    let cancelled = Cell::new(false);
    let mut context = CognitionHistoryWriteContextV1::new(&memory, work(), 1, || cancelled.get());
    assert!(matches!(
        store_cognition_history_original_v1(record, &mut context, &mut CancelAfter(&cancelled)),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(context.is_poisoned());
    let mut exhausted = work();
    exhausted.bytes_remaining = 1;
    let mut context = CognitionHistoryWriteContextV1::new(&memory, exhausted, 1, || false);
    assert!(matches!(
        store_cognition_history_original_v1(record, &mut context, &mut Skip),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert!(context.is_poisoned());
}

struct IgnoreFailure;
impl CognitionHistoryOriginalSinkV1 for IgnoreFailure {
    fn store<C: FnMut() -> bool>(
        &mut self,
        p: &mut CognitionHistoryOriginalProducerV1<'_, '_, '_, C>,
    ) -> Result<(), ValueTreeError> {
        assert!(p.write_to(&mut Fail).is_err());
        // A sink may catch its own callback error; the producer must remember
        // it and reject retry/completion even if the sink reports success.
        let _ = p.write_to(&mut io::sink());
        Ok(())
    }
}
#[test]
fn original_producer_cannot_hide_a_failed_stream_by_retrying() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let value = event(CognitionHistoryFieldV1::JournalEvents);
    let record = CognitionHistoryRecordRefV1 {
        field: CognitionHistoryFieldV1::JournalEvents,
        ordinal: 0,
        value: &value,
    };
    let mut context = CognitionHistoryWriteContextV1::new(&memory, work(), 1, || false);
    let result = store_cognition_history_original_v1(record, &mut context, &mut IgnoreFailure);
    assert!(
        result.is_err(),
        "failed stream was retried into a completion: {result:?}"
    );
    assert!(context.is_poisoned());
}
