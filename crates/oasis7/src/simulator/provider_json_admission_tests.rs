use super::*;
use std::io::Cursor;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

static CANCEL_PARSE_ENTERED: AtomicBool = AtomicBool::new(false);

struct ParseCancellationProbe;

impl<'de> serde::Deserialize<'de> for ParseCancellationProbe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        CANCEL_PARSE_ENTERED.store(true, Ordering::Release);
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self)
    }
}

struct ParseCancellationReader<'a> {
    bytes: &'a [u8],
    position: usize,
    rewound: Arc<AtomicBool>,
}

impl Read for ParseCancellationReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.bytes.len().saturating_sub(self.position);
        let count = remaining.min(output.len());
        output[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

impl Seek for ParseCancellationReader<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(offset) => self.position as i128 + i128::from(offset),
            SeekFrom::End(offset) => self.bytes.len() as i128 + i128::from(offset),
        };
        let next = usize::try_from(next)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "negative seek"))?;
        if next > self.bytes.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek beyond end",
            ));
        }
        self.position = next;
        if matches!(position, SeekFrom::Start(0)) {
            self.rewound.store(true, Ordering::Release);
        }
        Ok(self.position as u64)
    }
}

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
            max_inline_variant_bytes: 0,
            retained_sidecar_bytes: 0,
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

#[test]
fn cancellation_during_second_pass_json_decode_is_not_retried_as_interrupted_io() {
    CANCEL_PARSE_ENTERED.store(false, Ordering::Release);
    let input = b"null";
    let rewound = Arc::new(AtomicBool::new(false));
    let checks_after_rewind = AtomicUsize::new(0);
    let mut reader = ParseCancellationReader {
        bytes: input,
        position: 0,
        rewound: Arc::clone(&rewound),
    };
    let memory = MemoryBudget::new(1024 * 1024, 0).expect("decoder pool");

    let result = decode_reader::<ParseCancellationProbe, _, _>(
        &mut reader,
        input.len() as u64,
        &memory,
        MemoryClass::Ordinary,
        AllocationProfile {
            max_sequence_item_bytes: 0,
            max_inline_variant_bytes: 0,
            retained_sidecar_bytes: 0,
        },
        1,
        || {
            if !rewound.load(Ordering::Acquire) {
                return false;
            }
            // The first post-scan check is the admission boundary. Cancel on
            // the first read of the second pass, then let a retry proceed so
            // an InterruptedReader regression fails quickly instead of
            // spinning forever.
            checks_after_rewind.fetch_add(1, Ordering::AcqRel) == 1
        },
    );

    assert!(matches!(result, Err(ProviderJsonAdmissionError::Cancelled)));
    assert!(CANCEL_PARSE_ENTERED.load(Ordering::Acquire));
    assert_eq!(memory.used(), 0);
}

#[test]
fn response_profile_bounds_actual_nested_variants_and_trace_sidecar() {
    let profile = AllocationProfile::response();
    assert!(
        profile.max_sequence_item_bytes >= size_of::<MemoryWriteIntent>()
            && profile.max_sequence_item_bytes >= size_of::<ProviderTranscriptEntry>()
            && profile.max_sequence_item_bytes >= size_of::<serde_json::Value>()
    );
    assert!(
        profile.max_inline_variant_bytes >= size_of::<ProviderModuleCommand>()
            && profile.max_inline_variant_bytes >= size_of::<AgentCommandResponse>()
    );
    assert_eq!(RESPONSE_CONTEXT_COPY_BOUND, 4);
    assert_eq!(
        profile.retained_sidecar_bytes,
        RESPONSE_TRACE_OVERHEAD_BYTES
    );
}

#[test]
fn live_scanner_preserves_original_shape_heap_bound_and_rewind() {
    let input = serde_json::to_vec(&serde_json::json!({
        "nested": [null, 7, {"escaped": "quoted\\\"".repeat(20_000)}],
        "tail": [true, false, "text"]
    }))
    .expect("multi-chunk JSON fixture");
    let mut original = Cursor::new(input.as_slice());
    let mut live = Cursor::new(input.as_slice());
    let max = input.len() as u64;
    let (old, old_bytes) = legacy_scan_reader(&mut original, max, &|| false).unwrap();
    let (new, new_bytes) = scan_reader(&mut live, max, &|| false).unwrap();

    assert_eq!(old_bytes, max);
    assert_eq!(new_bytes, old_bytes);
    assert_eq!(live.position(), original.position());
    assert_eq!(live.position(), 0);
    assert_eq!(new.array_items, old.array_items);
    assert_eq!(new.object_members, old.object_members);
    assert_eq!(new.containers, old.containers);
    assert_eq!(new.string_bytes, old.string_bytes);
    assert_eq!(new.largest_string_bytes, old.largest_string_bytes);
    for (profile, copies) in [
        (AllocationProfile::request(), 1),
        (AllocationProfile::response(), 4),
    ] {
        assert_eq!(
            new.heap_bound::<serde_json::Value>(profile, copies)
                .unwrap(),
            old.heap_bound::<serde_json::Value>(profile, copies)
                .unwrap()
        );
    }
}

#[test]
fn live_scanner_preserves_original_rejection_boundaries() {
    let nested = "[".repeat(MAX_JSON_DEPTH + 1) + &"]".repeat(MAX_JSON_DEPTH + 1);
    for (input, limit, cancelled) in [
        (b"null".as_slice(), 3, false),
        (nested.as_bytes(), nested.len() as u64, false),
        (b"null".as_slice(), 4, true),
    ] {
        let old = legacy_scan_reader(&mut Cursor::new(input), limit, &|| cancelled)
            .err()
            .expect("original scanner rejects");
        let new = scan_reader(&mut Cursor::new(input), limit, &|| cancelled)
            .err()
            .expect("live scanner rejects");
        assert_eq!(std::mem::discriminant(&old), std::mem::discriminant(&new));
        assert_eq!(old.to_string(), new.to_string());
    }
}
