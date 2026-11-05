use super::*;
use std::io::{self, Cursor};
fn work() -> CognitionHistoryCborWorkV1 {
    CognitionHistoryCborWorkV1 {
        bytes_remaining: 64 << 20,
        nodes_remaining: 10000,
        source_reads_remaining: 100000,
        key_bytes_remaining: 1 << 20,
        max_depth: 32,
        max_key_bytes: 4096,
    }
}
fn validate(bytes: &[u8]) -> Result<ReservedValue<CognitionCanonicalOriginalV1>, ValueTreeError> {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    validate_cognition_history_canonical_cbor_v1(
        *blake3::hash(bytes).as_bytes(),
        bytes.len() as u64,
        &mut Cursor::new(bytes),
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || false,
    )
}
#[test]
fn canonical_original_matches_abi_for_nested_json_and_rejects_noncanonical_wire() {
    let value = serde_json::json!({"z":null,"a":[true,false,-9223372036854775808i64,u64::MAX,1.5,-0.0],"界":{"λ":"text"}});
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
    let proof = validate(&bytes).unwrap();
    assert_eq!(proof.id(), blake3::hash(&bytes).as_bytes());
    assert_eq!(proof.bytes(), bytes.len() as u64);
    assert_eq!(proof.class(), MemoryClass::Recovery);
    for length in 0..bytes.len() {
        assert!(validate(&bytes[..length]).is_err());
    }
    for malformed in [
        vec![0xa1, 0x61, b'a', 0x18, 0],                // nonminimal integer
        vec![0xa1, 0x78, 1, b'a', 0],                   // nonminimal string length
        vec![0xa2, 0x61, b'b', 0, 0x61, b'a', 0],       // key order
        vec![0xa2, 0x61, b'a', 0, 0x61, b'a', 0],       // duplicate key
        vec![0xa1, 0, 0],                               // JSON keys are text
        vec![0xa1, 0x61, b'a', 0x41, 0],                // CBOR bytes are not JSON
        vec![0xbf, 0xff],                               // indefinite map
        vec![0xa1, 0x61, b'a', 0xf9, 0x7c, 0],          // infinity
        vec![0xa1, 0x61, b'a', 0xf9, 0x7e, 0],          // NaN
        vec![0xa1, 0x61, b'a', 0xfa, 0x3f, 0xc0, 0, 0], // wider than ABI1.5
        vec![0xa1, 0x61, b'a', 0x61, 0xff],             // bad text UTF8
        vec![0xa1, 0x61, 0xff, 0],                      // bad key UTF8
        vec![0xa0, 0],                                  // trailing original bytes
        vec![0x80],                                     // root record must be an object
    ] {
        assert!(validate(&malformed).is_err(), "{malformed:x?}");
    }
}
struct Interrupted(usize);
impl Read for Interrupted {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        self.0 += 1;
        Err(io::ErrorKind::Interrupted.into())
    }
}
#[test]
fn source_work_hash_length_memory_and_final_cancellation_are_independent_gates() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let bytes = [0xa0];
    let id = *blake3::hash(&bytes).as_bytes();
    let mut input = Interrupted(0);
    let mut allowance = work();
    allowance.source_reads_remaining = 3;
    assert!(matches!(
        validate_cognition_history_canonical_cbor_v1(
            id,
            1,
            &mut input,
            &memory,
            MemoryClass::Recovery,
            &mut allowance,
            || false
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert_eq!(input.0, 3);
    for (expected, length) in [([0; 32], 1), (id, 2)] {
        assert!(
            validate_cognition_history_canonical_cbor_v1(
                expected,
                length,
                &mut Cursor::new(bytes),
                &memory,
                MemoryClass::Recovery,
                &mut work(),
                || false
            )
            .is_err()
        );
    }
    let mut no_reads = Interrupted(0);
    let mut allowance = work();
    allowance.bytes_remaining = 0;
    assert!(
        validate_cognition_history_canonical_cbor_v1(
            id,
            1,
            &mut no_reads,
            &memory,
            MemoryClass::Recovery,
            &mut allowance,
            || false
        )
        .is_err()
    );
    assert_eq!(no_reads.0, 0);
    let done = std::cell::Cell::new(false);
    struct AtEof<'a>(&'a std::cell::Cell<bool>, Cursor<[u8; 1]>);
    impl Read for AtEof<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = self.1.read(out)?;
            if n == 0 {
                self.0.set(true);
            }
            Ok(n)
        }
    }
    assert!(matches!(
        validate_cognition_history_canonical_cbor_v1(
            id,
            1,
            &mut AtEof(&done, Cursor::new(bytes)),
            &memory,
            MemoryClass::Recovery,
            &mut work(),
            || done.get()
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(memory.used(), 0);
}
#[test]
fn long_text_streams_under_recovery_while_ordinary_capacity_is_held() {
    const LENGTH: u64 = 32 << 20;
    // Independent literal canonical map {"text": "xxxxx..."}, no whole DTO.
    let prefix = [0xa1, 0x64, b't', b'e', b'x', b't', 0x7a, 2, 0, 0, 0];
    let mut hash = blake3::Hasher::new();
    hash.update(&prefix);
    let block = [b'x'; 8192];
    for _ in 0..LENGTH / 8192 {
        hash.update(&block);
    }
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let held = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut input = Cursor::new(prefix).chain(io::repeat(b'x').take(LENGTH));
    let proof = validate_cognition_history_canonical_cbor_v1(
        *hash.finalize().as_bytes(),
        LENGTH + prefix.len() as u64,
        &mut input,
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || false,
    )
    .unwrap();
    assert_eq!(proof.bytes(), LENGTH + prefix.len() as u64);
    assert_eq!(memory.used(), held.bytes() + proof.reserved_bytes());
    drop(proof);
    drop(held);
    assert_eq!(memory.used(), 0);
}

fn semantic(
    field: CognitionHistoryFieldV1,
    value: &serde_json::Value,
) -> Result<ReservedValue<CognitionOriginalSemanticV1>, ValueTreeError> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(value).unwrap();
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    verify_cognition_history_original_semantics_v1(
        CognitionOriginalCborRequestV1 {
            field,
            id: *blake3::hash(&bytes).as_bytes(),
            bytes: bytes.len() as u64,
        },
        &mut Cursor::new(bytes),
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || false,
    )
}
fn digest(domain: &str, value: &serde_json::Value) -> String {
    crate::cognition_recovery::cognition_digest_v1(domain, value)
}
#[test]
fn event_projections_match_original_abi_with_unknown_null_absent_and_nested_fields() {
    use crate::incremental_cognition_history::CognitionOriginalIdentityStatusV1;
    for field in [
        CognitionHistoryFieldV1::JournalEvents,
        CognitionHistoryFieldV1::EconomyJournal,
        CognitionHistoryFieldV1::EconomyProvisionJournal,
    ] {
        let mut event = serde_json::json!({"journal_seq":1,"parent_event_digest":"","unknown":{"x":[null,true,1.5]},"z":null});
        if field == CognitionHistoryFieldV1::JournalEvents {
            let payload = serde_json::json!({"unknown":{"x":[null,true,1.5]},"z":null});
            event["payload_digest"] = digest("oasis7.cognition.event-payload.v1", &payload).into();
        }
        let identity = digest(field.domain(), &event);
        event["event_digest"] = identity.clone().into();
        let proof = semantic(field, &event).unwrap();
        assert!(matches!(
            proof.identity_status(),
            CognitionOriginalIdentityStatusV1::DigestMatched(_)
        ));
        assert_eq!(proof.sequence(), Some(1));
        assert_eq!(proof.parent(), Some(None));
        // The original unknown member participates in both projections. The
        // flat identity is recomputed by the fixture, so this is semantic failure.
        event["unknown"]["x"][0] = false.into();
        assert!(matches!(
            semantic(field, &event),
            Err(ValueTreeError::Invalid(
                "Cognition original semantic digest"
            ))
        ));
    }
    let mut event = serde_json::json!({"journal_seq":1,"parent_event_digest":"","payload_digest":format!("blake3:{}","0".repeat(64))});
    event["event_digest"] = digest(CognitionHistoryFieldV1::JournalEvents.domain(), &event).into();
    assert!(matches!(
        semantic(CognitionHistoryFieldV1::JournalEvents, &event),
        Err(ValueTreeError::Invalid("Cognition original payload digest"))
    ));
    let missing = semantic(
        CognitionHistoryFieldV1::EconomyJournal,
        &serde_json::json!({"unknown":null}),
    )
    .unwrap();
    assert_eq!(
        missing.identity_status(),
        CognitionOriginalIdentityStatusV1::MissingIdentity
    );
    assert_eq!(missing.sequence(), None);
}
#[test]
fn response_uses_only_original_artifact_and_does_not_upgrade_missing_material() {
    use crate::incremental_cognition_history::CognitionOriginalIdentityStatusV1;
    let artifact = serde_json::json!({"actions":[{"x":null},true,true],"unknown":"λ"});
    let mut record = serde_json::json!({"response_artifact":artifact,"response_digest":digest("oasis7.cognition.response.v1",&artifact),"journal_head":"","envelope_digest":"original envelope unchanged"});
    let proof = semantic(CognitionHistoryFieldV1::Responses, &record).unwrap();
    assert!(matches!(
        proof.identity_status(),
        CognitionOriginalIdentityStatusV1::DigestMatched(_)
    ));
    assert_eq!(proof.journal_head(), Some(None));
    record["envelope_digest"] = "different envelope, separately verified by owner".into();
    semantic(CognitionHistoryFieldV1::Responses, &record).unwrap();
    record["response_artifact"] = serde_json::Value::Null;
    assert_eq!(
        semantic(CognitionHistoryFieldV1::Responses, &record)
            .unwrap()
            .identity_status(),
        CognitionOriginalIdentityStatusV1::ArtifactUnavailable
    );
    record.as_object_mut().unwrap().remove("response_artifact");
    assert_eq!(
        semantic(CognitionHistoryFieldV1::Responses, &record)
            .unwrap()
            .identity_status(),
        CognitionOriginalIdentityStatusV1::ArtifactUnavailable
    );
    record["response_digest"] = "".into();
    assert_eq!(
        semantic(CognitionHistoryFieldV1::Responses, &record)
            .unwrap()
            .identity_status(),
        CognitionOriginalIdentityStatusV1::MissingIdentity
    );
    record["response_digest"] = "blake3:BAD".into();
    assert!(semantic(CognitionHistoryFieldV1::Responses, &record).is_err());
}

#[test]
fn rejected_semantic_scalar_still_spends_every_actual_source_byte() {
    let value = serde_json::json!({"event_digest":"x".repeat(16384)});
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
    struct Counted<'a>(Cursor<&'a [u8]>, usize);
    impl Read for Counted<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = self.0.read(out)?;
            self.1 += n;
            Ok(n)
        }
    }
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let mut input = Counted(Cursor::new(bytes.as_slice()), 0);
    let mut allowance = work();
    let before = allowance.bytes_remaining;
    let result = verify_cognition_history_original_semantics_v1(
        CognitionOriginalCborRequestV1 {
            field: CognitionHistoryFieldV1::EconomyJournal,
            id: *blake3::hash(&bytes).as_bytes(),
            bytes: bytes.len() as u64,
        },
        &mut input,
        &memory,
        MemoryClass::Recovery,
        &mut allowance,
        || false,
    );
    assert!(matches!(
        result,
        Err(ValueTreeError::ResourceLimited("Cognition semantic scalar"))
    ));
    assert!(input.1 >= 8192);
    assert_eq!(before - allowance.bytes_remaining, input.1 as u64);
    assert_eq!(memory.used(), 0);
}

#[test]
fn trailing_probe_charges_consumed_byte_and_preserves_exact_eof_budget() {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let id = *blake3::hash(&[0xa0]).as_bytes();
    let mut allowance = work();
    let before = allowance.bytes_remaining;
    let mut input = Cursor::new([0xa0, 0]);
    assert!(
        validate_cognition_history_canonical_cbor_v1(
            id,
            1,
            &mut input,
            &memory,
            MemoryClass::Recovery,
            &mut allowance,
            || false
        )
        .is_err()
    );
    assert_eq!(input.position(), 2);
    assert_eq!(before - allowance.bytes_remaining, 2);
    let mut exact = work();
    exact.bytes_remaining = 1;
    validate_cognition_history_canonical_cbor_v1(
        id,
        1,
        &mut Cursor::new([0xa0]),
        &memory,
        MemoryClass::Recovery,
        &mut exact,
        || false,
    )
    .unwrap();
    assert_eq!(exact.bytes_remaining, 0);
    exact = work();
    exact.bytes_remaining = 1;
    assert!(matches!(
        validate_cognition_history_canonical_cbor_v1(
            id,
            1,
            &mut Cursor::new([0xa0, 0]),
            &memory,
            MemoryClass::Recovery,
            &mut exact,
            || false
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert_eq!(memory.used(), 0);
}

#[test]
fn large_response_artifact_digest_streams_with_ordinary_capacity_exhausted() {
    use crate::incremental_cognition_history::CognitionOriginalIdentityStatusV1;
    const LENGTH: u64 = 32 << 20;
    let text_header = [0x7a, 2, 0, 0, 0];
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let fixture = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let block = [b'x'; 8192];
    // Independent original (domain,artifact) wire, not a materialized Value.
    let mut artifact = blake3::Hasher::new();
    artifact.update(&[0x82]);
    artifact
        .update(&oasis7_wasm_abi::encode_canonical_cbor(&"oasis7.cognition.response.v1").unwrap());
    artifact.update(&text_header);
    for _ in 0..LENGTH / 8192 {
        artifact.update(&block);
    }
    let expected = *artifact.finalize().as_bytes();
    let digest = format!("blake3:{}", artifact.finalize());
    let mut prefix = vec![0xa2, 0x6f];
    prefix.extend_from_slice(b"response_digest");
    prefix.extend_from_slice(&[0x78, 71]);
    prefix.extend_from_slice(digest.as_bytes());
    prefix.push(0x71);
    prefix.extend_from_slice(b"response_artifact");
    prefix.extend_from_slice(&text_header);
    let mut original = blake3::Hasher::new();
    original.update(&prefix);
    for _ in 0..LENGTH / 8192 {
        original.update(&block);
    }
    let held = memory
        .reserve((4 << 20) - fixture.bytes(), MemoryClass::Ordinary)
        .unwrap();
    assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
    let length = LENGTH + prefix.len() as u64;
    let mut input = Cursor::new(prefix).chain(io::repeat(b'x').take(LENGTH));
    let proof = verify_cognition_history_original_semantics_v1(
        CognitionOriginalCborRequestV1 {
            field: CognitionHistoryFieldV1::Responses,
            id: *original.finalize().as_bytes(),
            bytes: length,
        },
        &mut input,
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || false,
    )
    .unwrap();
    assert_eq!(
        proof.identity_status(),
        CognitionOriginalIdentityStatusV1::DigestMatched(expected)
    );
    assert_eq!(proof.bytes(), length);
    assert_eq!(
        memory.used(),
        fixture.bytes() + held.bytes() + proof.reserved_bytes()
    );
    drop(proof);
    drop(input);
    drop(held);
    drop(fixture);
    assert_eq!(memory.used(), 0);
}
