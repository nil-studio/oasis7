use super::*;
use serde::Deserialize;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

const CONTEXT_LIMIT: usize = 1024 * 1024;

struct ParseProbe;

impl<'de> serde::Deserialize<'de> for ParseProbe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        PARSER_ENTERED.store(true, Ordering::Release);
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self)
    }
}

static PARSER_ENTERED: AtomicBool = AtomicBool::new(false);

fn large_value_array_json(item_count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(item_count.saturating_mul(5).saturating_add(16));
    bytes.extend_from_slice(b"{\"value\":[");
    for index in 0..item_count {
        if index != 0 {
            bytes.push(b',');
        }
        bytes.extend_from_slice(b"null");
    }
    bytes.extend_from_slice(b"]}");
    bytes
}

fn large_response_json(item_count: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(item_count.saturating_mul(5).saturating_add(512));
    bytes.extend_from_slice(
        b"{\"base_decision_response\":{\"decision\":\"wait\",\"trace_payload\":{\"upstream_trace\":[",
    );
    for index in 0..item_count {
        if index != 0 {
            bytes.push(b',');
        }
        bytes.extend_from_slice(b"null");
    }
    bytes.extend_from_slice(
        b"]}},\"context_discriminator\":\"oasis7.continuous-agent-context\",\"context_version\":1,\"agent_session_id\":\"session\",\"agent_turn_id\":\"turn\",\"decision_request_id\":\"request\",\"retry_seq\":1,\"transport_attempt\":1,\"request_digest\":\"blake3:0000000000000000000000000000000000000000000000000000000000000000\",\"response_digest\":\"blake3:0000000000000000000000000000000000000000000000000000000000000000\"}",
    );
    bytes
}

#[test]
fn dynamic_json_heap_is_admitted_before_deserializer_runs() {
    PARSER_ENTERED.store(false, Ordering::Release);
    let input = large_value_array_json(65_536);
    assert!(input.len() < CONTEXT_LIMIT);

    let memory = MemoryBudget::new(3 * 1024 * 1024, 0).expect("small test pool");
    let raw = memory
        .reserve(input.len(), MemoryClass::Ordinary)
        .expect("bounded raw input fits");
    let legacy_context_reservation = memory
        .reserve(CONTEXT_LIMIT, MemoryClass::Ordinary)
        .expect("the previous fixed context charge fits");
    drop(legacy_context_reservation);

    let result = decode_reader::<ParseProbe, _, _>(
        &mut Cursor::new(input.as_slice()),
        input.len() as u64,
        &memory,
        MemoryClass::Ordinary,
        AllocationProfile {
            max_sequence_item_bytes: size_of::<serde_json::Value>(),
        },
        1,
        || false,
    );
    assert!(matches!(result, Err(ProviderJsonAdmissionError::Memory(_))));
    assert!(!PARSER_ENTERED.load(Ordering::Acquire));
    assert_eq!(memory.used(), raw.bytes());
}

#[test]
fn response_upstream_trace_array_is_not_charged_as_its_wire_length() {
    let input = large_response_json(65_536);
    assert!(input.len() < CONTEXT_LIMIT);

    let memory = MemoryBudget::new(3 * 1024 * 1024, 0).expect("small test pool");
    let raw = memory
        .reserve(input.len(), MemoryClass::Ordinary)
        .expect("bounded raw input fits");
    let legacy_context_reservation = memory
        .reserve(CONTEXT_LIMIT, MemoryClass::Ordinary)
        .expect("the previous fixed context charge fits");
    drop(legacy_context_reservation);

    let result = decode_response_context(
        &mut Cursor::new(input.as_slice()),
        input.len() as u64,
        &memory,
        MemoryClass::Ordinary,
        || false,
    );
    assert!(matches!(result, Err(ProviderJsonAdmissionError::Memory(_))));
    assert_eq!(memory.used(), raw.bytes());
}
