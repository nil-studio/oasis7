use super::*;
use crate::incremental_cognition_history::CognitionOriginalIdentityStatusV1;
use serde_json::{Value, json};
use std::io::{self, Cursor};
fn work() -> CognitionHistoryCborWorkV1 {
    CognitionHistoryCborWorkV1 {
        bytes_remaining: 64 << 20,
        nodes_remaining: 100_000,
        source_reads_remaining: 100_000,
        key_bytes_remaining: 1 << 20,
        max_depth: 32,
        max_key_bytes: 4096,
    }
}
fn digest(domain: &str, value: &Value) -> String {
    crate::cognition_recovery::cognition_digest_v1(domain, value)
}
fn refresh(field: CognitionHistoryFieldV1, value: &mut Value) {
    let mut unsigned = value.clone();
    unsigned.as_object_mut().unwrap().remove("event_digest");
    if field == CognitionHistoryFieldV1::JournalEvents {
        let mut payload = unsigned.clone();
        for key in [
            "journal_seq",
            "parent_event_digest",
            "payload_digest",
            "event_digest",
        ] {
            payload.as_object_mut().unwrap().remove(key);
        }
        let id = digest("oasis7.cognition.event-payload.v1", &payload);
        value["payload_digest"] = id.clone().into();
        unsigned["payload_digest"] = id.into();
    }
    value["event_digest"] = digest(field.domain(), &unsigned).into();
}
fn journal() -> Value {
    let mut value = json!({"schema_version":"cognition-journal-event.v1","journal_seq":1,"parent_event_digest":"","event_kind":"IndependentEvent","kind":"IndependentEvent","world_id":"world","branch_id":"branch","finality_epoch":u64::MAX,"finality_block_hash":null,"finality_status":"pending","reorg_epoch":0,"logical_tick":u64::MAX,"retry_seq":0,"transport_attempt":0,"status":"done","request_digest":null,"response_digest":null,"envelope_digest":null,"causal_refs":["opaque causal reference","界"],"zzunknown":[true,{"nested":null}]});
    refresh(CognitionHistoryFieldV1::JournalEvents, &mut value);
    value
}
fn economy() -> Value {
    let mut value = json!({"schema_version":"cognition-economy-event.v1","journal_seq":1,"parent_event_digest":"","event_kind":"reserve","lease_id":"lease","operation_key":"operation","idempotency_key":"idempotency","resource":"cognition_units","reserved_amount":u64::MAX,"consumed_amount":0,"released_amount":0,"refunded_amount":0,"net_amount":0,"parent_receipt_id":null,"reason":null,"status":"reserved","receipt_id":"","zzunknown":[true,{"nested":null}]});
    value["operation_key"] = digest(
        "oasis7.cognition.economy.operation.v1",
        &json!(["lease", "reserve"]),
    )
    .into();
    refresh(CognitionHistoryFieldV1::EconomyJournal, &mut value);
    value
}
fn provision() -> Value {
    let mut request = json!({"schema_version":"cognition-provisioning.v1","provision_id":"provision","account_id":"account","owner_binding":"account","owner_generation":1,"world_id":"world","branch_id":"branch","reorg_epoch":0,"resource":"cognition_units","resource_version":"cognition_units.v1","policy_revision":"fixed_unit_experimental","allowance":u64::MAX,"authority_context":"actual source context"});
    request["authority_digest"] = digest(
        "oasis7.cognition.economy.provisioning-authority.v1",
        &json!([
            "account",
            "account",
            "world",
            1,
            "branch",
            "cognition_units",
            0,
            "cognition_units.v1",
            "fixed_unit_experimental",
            "actual source context",
            "provision"
        ]),
    )
    .into();
    request["provisioning_digest"] = digest(
        "oasis7.cognition.economy.provisioning.v1",
        &json!([
            "provision",
            "account",
            "account",
            1,
            "world",
            "branch",
            0,
            "cognition_units",
            "cognition_units.v1",
            u64::MAX,
            "fixed_unit_experimental",
            request["authority_digest"]
        ]),
    )
    .into();
    let receipt = digest(
        "oasis7.cognition.economy.provisioning-receipt-id.v1",
        &json!(["provision", request["provisioning_digest"]]),
    );
    // Unknown members stay in the original event digest; bounded request tuple
    // identities intentionally use only their original declared inputs.
    request["zzunknown"] = json!([true,{"nested":null}]);
    let mut value = json!({"schema_version":"cognition-provisioning-event.v1","journal_seq":1,"parent_event_digest":"","event_kind":"provision","request":request,"receipt_id":receipt,"zzunknown":[null,true,{"nested":"kept"}]});
    refresh(CognitionHistoryFieldV1::EconomyProvisionJournal, &mut value);
    value
}
fn bytes(value: &Value) -> Vec<u8> {
    oasis7_wasm_abi::encode_canonical_cbor(value).unwrap()
}
fn old(
    field: CognitionHistoryFieldV1,
    value: &Value,
) -> Result<ReservedValue<CognitionOriginalSemanticV1>, ValueTreeError> {
    let bytes = bytes(value);
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
fn strict(
    field: CognitionHistoryFieldV1,
    value: &Value,
) -> Result<ReservedValue<CognitionHistoryRecordSchemaV1>, ValueTreeError> {
    let bytes = bytes(value);
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    verify_cognition_history_original_record_schema_v1(
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
#[test]
fn original_lifecycle_key_projection_distinguishes_missing_null_and_empty() {
    use CognitionOriginalTextProjectionV1::*;
    for variant in 0..3 {
        let mut value = journal();
        value["agent_session_id"] = json!("session");
        value["agent_turn_id"] = json!("turn");
        value["decision_request_id"] = json!("decision");
        value["request_digest"] = json!("original request text");
        value["transport_attempt"] = json!(u64::MAX);
        match variant {
            0 => {
                value.as_object_mut().unwrap().remove("agent_id");
            }
            1 => value["agent_id"] = Value::Null,
            _ => value["agent_id"] = json!(""),
        }
        refresh(CognitionHistoryFieldV1::JournalEvents, &mut value);
        let proof = strict(CognitionHistoryFieldV1::JournalEvents, &value).unwrap();
        let projection = proof.journal_lifecycle().unwrap();
        assert!(!projection.is_complete());
        assert_eq!(projection.transport_attempt(), u64::MAX);
        let expected = match variant {
            0 => Missing,
            1 => Null,
            _ => Text {
                bytes: 0,
                utf8_digest: *blake3::hash(b"").as_bytes(),
            },
        };
        assert_eq!(projection.keys()[0], expected);
        for (index, text) in ["session", "turn", "decision", "original request text"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                projection.keys()[index + 1],
                Text {
                    bytes: text.len() as u64,
                    utf8_digest: *blake3::hash(text.as_bytes()).as_bytes()
                }
            );
        }
    }
}

#[test]
fn lifecycle_optional_original_id_has_no_new_text_cap_or_normalization() {
    use CognitionOriginalTextProjectionV1::*;
    let inputs = MemoryBudget::new(64 << 20, 0).unwrap();
    let _input = inputs.reserve(48 << 20, MemoryClass::Ordinary).unwrap();
    let raw = " \0界".repeat(2 << 18);
    let expected = Text {
        bytes: raw.len() as u64,
        utf8_digest: *blake3::hash(raw.as_bytes()).as_bytes(),
    };
    let mut value = journal();
    value["agent_id"] = Value::String(raw);
    value["agent_session_id"] = json!(" session ");
    value["agent_turn_id"] = json!("turn");
    value["decision_request_id"] = json!("decision");
    value["request_digest"] = json!("not normalized into a new business id");
    refresh(CognitionHistoryFieldV1::JournalEvents, &mut value);
    let original = bytes(&value);
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let hold = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let proof = verify_cognition_history_original_record_schema_v1(
        CognitionOriginalCborRequestV1 {
            field: CognitionHistoryFieldV1::JournalEvents,
            id: *blake3::hash(&original).as_bytes(),
            bytes: original.len() as u64,
        },
        &mut Cursor::new(&original),
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || false,
    )
    .unwrap();
    let projection = proof.journal_lifecycle().unwrap();
    assert!(projection.is_complete());
    assert_eq!(projection.keys()[0], expected);
    assert_eq!(
        projection.keys()[1],
        Text {
            bytes: 9,
            utf8_digest: *blake3::hash(b" session ").as_bytes()
        }
    );
    assert_eq!(memory.used(), (4 << 20) + 1024);
    drop(proof);
    assert_eq!(memory.used(), 4 << 20);
    drop(hold);
}
#[test]
fn local_record_fields_match_literal_originals_without_claiming_cross_record_relations() {
    for (field, value, scope, kind) in [
        (
            CognitionHistoryFieldV1::JournalEvents,
            journal(),
            CognitionHistoryRecordScopeV1::LocalJournalFields,
            "IndependentEvent",
        ),
        (
            CognitionHistoryFieldV1::EconomyJournal,
            economy(),
            CognitionHistoryRecordScopeV1::LocalEconomyFields,
            "reserve",
        ),
        (
            CognitionHistoryFieldV1::EconomyProvisionJournal,
            provision(),
            CognitionHistoryRecordScopeV1::LocalProvisionFields,
            "provision",
        ),
    ] {
        let proof = strict(field, &value).unwrap();
        assert_eq!(proof.scope(), scope);
        assert_eq!(proof.event_kind(), Some(kind));
        assert_eq!(proof.semantic().sequence(), Some(1));
        assert_eq!(proof.semantic().parent(), Some(None));
        assert_eq!(proof.semantic().class(), MemoryClass::Recovery);
        assert_eq!(
            proof.semantic().id(),
            blake3::hash(&bytes(&value)).as_bytes()
        );
        assert!(matches!(
            proof.identity_status(),
            CognitionOriginalIdentityStatusV1::DigestMatched(_)
        ));
        assert_eq!(
            proof.causal_ref_count(),
            if field == CognitionHistoryFieldV1::JournalEvents {
                2
            } else {
                0
            }
        );
    }
}

#[test]
fn provision_later_sequence_requires_a_nonempty_canonical_parent_locally() {
    let field = CognitionHistoryFieldV1::EconomyProvisionJournal;
    let mut first = provision();
    first.as_object_mut().unwrap().remove("zzunknown");
    first["request"]
        .as_object_mut()
        .unwrap()
        .remove("zzunknown");
    refresh(field, &mut first);
    assert_eq!(
        strict(field, &first).unwrap().semantic().parent(),
        Some(None)
    );
    let mut later = first;
    later["journal_seq"] = 2.into();
    refresh(field, &mut later);
    assert!(matches!(
        old(field, &later).unwrap().identity_status(),
        CognitionOriginalIdentityStatusV1::DigestMatched(_)
    ));
    assert!(
        matches!(strict(field, &later), Err(ValueTreeError::Invalid(_))),
        "sequence 2 with empty parent must fail the original single-record condition"
    );
    later["parent_event_digest"] = format!("blake3:{}", "a".repeat(64)).into();
    refresh(field, &mut later);
    assert!(strict(field, &later).is_ok());
    // The original *single-record* validator imposes no empty-parent rule on
    // sequence 1. Prefix continuity remains a separate owner's obligation.
    later["journal_seq"] = 1.into();
    refresh(field, &mut later);
    assert!(strict(field, &later).is_ok());
}
#[test]
fn digest_only_success_does_not_grant_strict_local_fields() {
    for (field, source, changes) in [
        (
            CognitionHistoryFieldV1::JournalEvents,
            journal(),
            vec![
                ("schema_version", json!("future")),
                ("kind", json!("mismatch")),
                ("world_id", Value::Null),
                ("branch_id", json!(" ")),
                ("status", json!("x".repeat(257))),
                ("reorg_epoch", json!(-1)),
                ("request_digest", json!(false)),
                ("finality_status", json!("verified")),
                ("causal_refs", json!([" "])),
                ("causal_refs", json!([0])),
                ("causal_refs", json!(["x".repeat(257)])),
            ],
        ),
        (
            CognitionHistoryFieldV1::EconomyJournal,
            economy(),
            vec![
                ("schema_version", json!("future")),
                ("status", json!("future")),
                ("status", json!("settled")),
                ("operation_key", json!("wrong operation identity")),
                ("lease_id", json!("bad\nidentity")),
                ("consumed_amount", json!(-1)),
                ("consumed_amount", json!(1)),
                ("parent_receipt_id", json!("")),
                ("released_amount", Value::Null),
                ("reason", json!(["wrong type"])),
                ("event_kind", json!("unsupported")),
            ],
        ),
        (
            CognitionHistoryFieldV1::EconomyProvisionJournal,
            provision(),
            vec![
                ("schema_version", json!("future")),
                ("event_kind", json!("not provision")),
                ("request", Value::Null),
                ("receipt_id", json!(format!("blake3:{}", "0".repeat(64)))),
            ],
        ),
    ] {
        for (key, after) in changes {
            let mut bad = source.clone();
            bad[key] = after;
            refresh(field, &mut bad);
            assert!(
                matches!(
                    old(field, &bad).unwrap().identity_status(),
                    CognitionOriginalIdentityStatusV1::DigestMatched(_)
                ),
                "old projection {key}"
            );
            assert!(
                matches!(strict(field, &bad), Err(ValueTreeError::Invalid(_))),
                "strict accepted {field:?}/{key}"
            );
        }
        let mut bad = source;
        bad.as_object_mut().unwrap().remove("schema_version");
        refresh(field, &mut bad);
        old(field, &bad).unwrap();
        assert!(matches!(
            strict(field, &bad),
            Err(ValueTreeError::Invalid(_))
        ));
    }
    for (key, after) in [
        ("owner_binding", json!("different")),
        ("owner_generation", json!(0)),
        ("allowance", json!(0)),
        ("policy_revision", json!("future")),
        (
            "authority_digest",
            json!(format!("blake3:{}", "1".repeat(64))),
        ),
        (
            "provisioning_digest",
            json!(format!("blake3:{}", "2".repeat(64))),
        ),
    ] {
        let mut bad = provision();
        bad["request"][key] = after;
        refresh(CognitionHistoryFieldV1::EconomyProvisionJournal, &mut bad);
        old(CognitionHistoryFieldV1::EconomyProvisionJournal, &bad).unwrap();
        assert!(
            matches!(
                strict(CognitionHistoryFieldV1::EconomyProvisionJournal, &bad),
                Err(ValueTreeError::Invalid(_))
            ),
            "request {key}"
        );
    }
}
#[test]
fn response_scope_retains_optional_originals_and_never_claims_nested_artifact_business_validity() {
    let artifact =
        json!({"context_discriminator":"unknown model","unbounded_model_payload":[null,true,true]});
    let mut value = json!({"response_digest":digest("oasis7.cognition.response.v1",&artifact),"response_artifact":artifact,"envelope_digest":"","extra":[true,null]});
    let proof = strict(CognitionHistoryFieldV1::Responses, &value).unwrap();
    assert_eq!(
        proof.scope(),
        CognitionHistoryRecordScopeV1::ResponseWrapperFields
    );
    assert_eq!(proof.semantic().journal_head(), None);
    assert!(matches!(
        proof.identity_status(),
        CognitionOriginalIdentityStatusV1::DigestMatched(_)
    ));
    value["response_artifact"] = Value::Null;
    assert_eq!(
        strict(CognitionHistoryFieldV1::Responses, &value)
            .unwrap()
            .identity_status(),
        CognitionOriginalIdentityStatusV1::ArtifactUnavailable
    );
    value.as_object_mut().unwrap().remove("response_artifact");
    assert_eq!(
        strict(CognitionHistoryFieldV1::Responses, &value)
            .unwrap()
            .identity_status(),
        CognitionOriginalIdentityStatusV1::ArtifactUnavailable
    );
    value["response_digest"] = "".into();
    assert_eq!(
        strict(CognitionHistoryFieldV1::Responses, &value)
            .unwrap()
            .identity_status(),
        CognitionOriginalIdentityStatusV1::MissingIdentity
    );
    value["envelope_digest"] = Value::Null;
    old(CognitionHistoryFieldV1::Responses, &value).unwrap();
    assert!(matches!(
        strict(CognitionHistoryFieldV1::Responses, &value),
        Err(ValueTreeError::Invalid(_))
    ));
}
#[test]
fn strict_read_reuses_one_continuous_source_and_preserves_resource_failures() {
    let bytes = bytes(&journal());
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let request = || CognitionOriginalCborRequestV1 {
        field: CognitionHistoryFieldV1::JournalEvents,
        id: *blake3::hash(&bytes).as_bytes(),
        bytes: bytes.len() as u64,
    };
    struct Source<'a> {
        input: Cursor<&'a [u8]>,
        reads: u64,
        done: bool,
    }
    impl Read for Source<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            let n = self.input.read(&mut output[..1])?;
            self.done = n == 0;
            Ok(n)
        }
    }
    let mut input = Source {
        input: Cursor::new(&bytes),
        reads: 0,
        done: false,
    };
    let mut allowance = work();
    allowance.bytes_remaining = bytes.len() as u64;
    let proof = verify_cognition_history_original_record_schema_v1(
        request(),
        &mut input,
        &memory,
        MemoryClass::Recovery,
        &mut allowance,
        || false,
    )
    .unwrap();
    assert!(input.done);
    assert_eq!(input.reads, bytes.len() as u64 + 1);
    assert_eq!(allowance.bytes_remaining, 0);
    drop(proof);
    assert_eq!(memory.used(), 0);
    for mode in 0..3 {
        let mut allowance = work();
        match mode {
            0 => allowance.nodes_remaining = 0,
            1 => allowance.source_reads_remaining = 0,
            _ => allowance.key_bytes_remaining = 0,
        };
        let mut input = Source {
            input: Cursor::new(&bytes),
            reads: 0,
            done: false,
        };
        assert!(matches!(
            verify_cognition_history_original_record_schema_v1(
                request(),
                &mut input,
                &memory,
                MemoryClass::Recovery,
                &mut allowance,
                || false
            ),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        if mode == 1 {
            assert_eq!(input.reads, 0);
        }
        assert_eq!(
            allowance.bytes_remaining,
            work().bytes_remaining - input.input.position()
        );
        assert_eq!(memory.used(), 0);
    }
    let mut calls = 0;
    let mut input = Cursor::new(&bytes);
    verify_cognition_history_original_record_schema_v1(
        request(),
        &mut input,
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || {
            calls += 1;
            false
        },
    )
    .unwrap();
    let mut at = 0;
    assert!(matches!(
        verify_cognition_history_original_record_schema_v1(
            request(),
            &mut Cursor::new(&bytes),
            &memory,
            MemoryClass::Recovery,
            &mut work(),
            || {
                at += 1;
                at == calls
            }
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(memory.used(), 0);
    let small = MemoryBudget::new(512, 0).unwrap();
    let mut input = Source {
        input: Cursor::new(&bytes),
        reads: 0,
        done: false,
    };
    assert!(matches!(
        verify_cognition_history_original_record_schema_v1(
            request(),
            &mut input,
            &small,
            MemoryClass::Ordinary,
            &mut work(),
            || false
        ),
        Err(ValueTreeError::Memory(_))
    ));
    assert_eq!(input.reads, 0);
}
#[test]
fn strict_response_large_artifact_uses_fixed_recovery_memory_without_a_json_model() {
    const LENGTH: u64 = 32 << 20;
    let domain = oasis7_wasm_abi::encode_canonical_cbor(&"oasis7.cognition.response.v1").unwrap();
    let string_header = [0x7a, 2, 0, 0, 0];
    let block = [b'x'; 8192];
    let mut artifact = blake3::Hasher::new();
    artifact.update(&[0x82]);
    artifact.update(&domain);
    artifact.update(&string_header);
    for _ in 0..LENGTH / 8192 {
        artifact.update(&block);
    }
    let declared = format!("blake3:{}", artifact.finalize());
    let mut prefix = vec![0xa3];
    for text in [
        "envelope_digest",
        "",
        "response_digest",
        declared.as_str(),
        "response_artifact",
    ] {
        prefix.extend(oasis7_wasm_abi::encode_canonical_cbor(&text).unwrap());
    }
    prefix.extend(string_header);
    let mut flat = blake3::Hasher::new();
    flat.update(&prefix);
    for _ in 0..LENGTH / 8192 {
        flat.update(&block);
    }
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let hold = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut input = Cursor::new(prefix.clone()).chain(io::repeat(b'x').take(LENGTH));
    let proof = verify_cognition_history_original_record_schema_v1(
        CognitionOriginalCborRequestV1 {
            field: CognitionHistoryFieldV1::Responses,
            id: *flat.finalize().as_bytes(),
            bytes: LENGTH + prefix.len() as u64,
        },
        &mut input,
        &memory,
        MemoryClass::Recovery,
        &mut work(),
        || false,
    )
    .unwrap();
    assert_eq!(
        proof.scope(),
        CognitionHistoryRecordScopeV1::ResponseWrapperFields
    );
    assert!(matches!(
        proof.identity_status(),
        CognitionOriginalIdentityStatusV1::DigestMatched(_)
    ));
    assert_eq!(memory.used(), hold.bytes() + proof.reserved_bytes());
    drop(proof);
    drop(hold);
    assert_eq!(memory.used(), 0);
}

#[test]
fn causal_entries_reuse_fixed_metadata_and_provision_hashes_spend_the_same_node_allowance() {
    let mut peak_baseline = None;
    let mut node_baseline = None;
    for count in [0, 4096] {
        let mut value = journal();
        value["causal_refs"] = json!(vec!["causal"; count]);
        refresh(CognitionHistoryFieldV1::JournalEvents, &mut value);
        let bytes = bytes(&value);
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let hold = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        struct Measured<'a> {
            input: Cursor<&'a [u8]>,
            memory: &'a MemoryBudget,
            peak: usize,
        }
        impl Read for Measured<'_> {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                self.peak = self.peak.max(self.memory.used());
                self.input.read(out)
            }
        }
        let mut source = Measured {
            input: Cursor::new(&bytes),
            memory: &memory,
            peak: 0,
        };
        let mut allowance = work();
        let proof = verify_cognition_history_original_record_schema_v1(
            CognitionOriginalCborRequestV1 {
                field: CognitionHistoryFieldV1::JournalEvents,
                id: *blake3::hash(&bytes).as_bytes(),
                bytes: bytes.len() as u64,
            },
            &mut source,
            &memory,
            MemoryClass::Recovery,
            &mut allowance,
            || false,
        )
        .unwrap();
        assert_eq!(proof.causal_ref_count(), count as u64);
        if let Some(peak) = peak_baseline {
            assert_eq!(source.peak, peak);
        } else {
            peak_baseline = Some(source.peak);
        }
        if let Some(nodes) = node_baseline {
            assert_eq!(nodes - allowance.nodes_remaining, count as u64);
        } else {
            node_baseline = Some(allowance.nodes_remaining);
        }
        assert_eq!(memory.used(), hold.bytes() + proof.reserved_bytes());
    }
    let bytes = bytes(&provision());
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let request = || CognitionOriginalCborRequestV1 {
        field: CognitionHistoryFieldV1::EconomyProvisionJournal,
        id: *blake3::hash(&bytes).as_bytes(),
        bytes: bytes.len() as u64,
    };
    let mut allowance = work();
    let proof = verify_cognition_history_original_record_schema_v1(
        request(),
        &mut Cursor::new(&bytes),
        &memory,
        MemoryClass::Recovery,
        &mut allowance,
        || false,
    )
    .unwrap();
    drop(proof);
    let used = work().nodes_remaining - allowance.nodes_remaining;
    let mut short = work();
    short.nodes_remaining = used - 1;
    let mut input = Cursor::new(&bytes);
    assert!(matches!(
        verify_cognition_history_original_record_schema_v1(
            request(),
            &mut input,
            &memory,
            MemoryClass::Recovery,
            &mut short,
            || false
        ),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    assert_eq!(input.position(), bytes.len() as u64);
    assert_eq!(
        short.bytes_remaining,
        work().bytes_remaining - bytes.len() as u64
    );
    assert_eq!(memory.used(), 0);
    let done = std::cell::Cell::new(false);
    struct Eof<'a> {
        input: Cursor<&'a [u8]>,
        done: &'a std::cell::Cell<bool>,
    }
    impl Read for Eof<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = self.input.read(out)?;
            if n == 0 {
                self.done.set(true);
            }
            Ok(n)
        }
    }
    assert!(matches!(
        verify_cognition_history_original_record_schema_v1(
            request(),
            &mut Eof {
                input: Cursor::new(&bytes),
                done: &done
            },
            &memory,
            MemoryClass::Recovery,
            &mut work(),
            || done.get()
        ),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(done.get());
    assert_eq!(memory.used(), 0);
}
