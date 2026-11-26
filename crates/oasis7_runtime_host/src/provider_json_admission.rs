//! Bounded JSON decoding for the native provider context lane.
//!
//! Scans bounded input via `provider_json_shape` before invoking Serde and
//! retains a conservative heap charge for the decoded context lifetime.
use std::fmt;
use std::io::{self, Read, Seek};
use std::mem::size_of;
use std::sync::Arc;

use oasis7_agent_api::{
    ActionCatalogEntry, MemoryWriteIntent, ProviderInteractionTarget, ProviderModuleCommand,
    ProviderNavigationNode, ProviderNearbyEntity, ProviderRecentEvent, ProviderTranscriptEntry,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_simulator::{Action, AgentQuery};
use oasis7_wasm_abi::AgentCommandResponse;
use serde::de::DeserializeOwned;

pub use crate::provider_json_shape::ProviderJsonAdmissionError;
use crate::provider_json_shape::{JsonShape, SCAN_SCRATCH_BYTES, scan_reader};
use oasis7_agent_api::ContinuousAgentRequestContextV1;

type ProviderDecision = oasis7_agent_api::ProviderDecision<Action, AgentQuery>;
type DecisionResponse = oasis7_agent_api::DecisionResponse<Action, AgentQuery>;
type ContinuousAgentResponseContextV1 =
    oasis7_agent_api::ContinuousAgentResponseContextV1<Action, AgentQuery>;

const PARSER_SCRATCH_BYTES: usize = 16 * 1024;
const MAP_ENTRY_BYTES: usize = 128;
const CONTAINER_BYTES: usize = 64;
const RESPONSE_CONTEXT_COPY_BOUND: usize = 4;
const RESPONSE_TRACE_OVERHEAD_BYTES: usize = 512 * 1024;

/// Process-local accounting retained while a decoded native response context
/// crosses behavior and Runtime outcome sidechannels.
#[derive(Clone)]
pub struct ProviderResponseContextLease(Arc<MemoryReservation>);

impl ProviderResponseContextLease {
    fn new(reservation: MemoryReservation) -> Self {
        Self(Arc::new(reservation))
    }

    pub fn reserved_bytes(&self) -> usize {
        self.0.bytes()
    }
}

impl fmt::Debug for ProviderResponseContextLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderResponseContextLease")
            .field("reserved_bytes", &self.reserved_bytes())
            .finish()
    }
}

impl PartialEq for ProviderResponseContextLease {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone, Copy)]
struct AllocationProfile {
    max_sequence_item_bytes: usize,
    max_inline_variant_bytes: usize,
    retained_sidecar_bytes: usize,
}

impl AllocationProfile {
    fn request() -> Self {
        Self {
            max_sequence_item_bytes: max_size(&[
                size_of::<String>(),
                size_of::<serde_json::Value>(),
                size_of::<ProviderNearbyEntity>(),
                size_of::<ProviderRecentEvent>(),
                size_of::<ProviderNavigationNode>(),
                size_of::<ProviderInteractionTarget>(),
                size_of::<ActionCatalogEntry>(),
                size_of::<oasis7_wasm_abi::ModuleCommandCatalogEntry>(),
                size_of::<oasis7_wasm_abi::CapabilityCatalogEntry>(),
            ]),
            max_inline_variant_bytes: 0,
            retained_sidecar_bytes: 0,
        }
    }

    fn response() -> Self {
        Self {
            max_sequence_item_bytes: max_size(&[
                size_of::<String>(),
                size_of::<serde_json::Value>(),
                size_of::<ProviderTranscriptEntry>(),
                size_of::<MemoryWriteIntent>(),
            ]),
            max_inline_variant_bytes: max_size(&[
                size_of::<ProviderModuleCommand>(),
                size_of::<AgentCommandResponse>(),
                size_of::<ProviderDecision>(),
                size_of::<DecisionResponse>(),
                size_of::<Action>(),
                size_of::<AgentQuery>(),
            ]),
            retained_sidecar_bytes: RESPONSE_TRACE_OVERHEAD_BYTES,
        }
    }
}

fn max_size(sizes: &[usize]) -> usize {
    sizes.iter().copied().max().unwrap_or(0)
}

fn heap_bound<T>(
    shape: &JsonShape,
    profile: AllocationProfile,
    retained_copies: usize,
) -> Result<usize, ProviderJsonAdmissionError> {
    let sequence_item = profile
        .max_sequence_item_bytes
        .checked_mul(2)
        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
    let array_bytes = shape
        .array_items
        .checked_mul(sequence_item)
        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
    let map_bytes = shape
        .object_members
        .checked_mul(MAP_ENTRY_BYTES)
        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
    let string_bytes = shape
        .string_bytes
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(shape.largest_string_bytes))
        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
    let container_bytes = shape
        .containers
        .checked_mul(CONTAINER_BYTES)
        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
    size_of::<T>()
        .checked_add(profile.max_inline_variant_bytes)
        .and_then(|bytes| bytes.checked_add(array_bytes))
        .and_then(|bytes| bytes.checked_add(map_bytes))
        .and_then(|bytes| bytes.checked_add(string_bytes))
        .and_then(|bytes| bytes.checked_add(container_bytes))
        .and_then(|bytes| bytes.checked_mul(retained_copies))
        .and_then(|bytes| bytes.checked_add(profile.retained_sidecar_bytes))
        .ok_or(ProviderJsonAdmissionError::SizeOverflow)
}

struct CancellationReader<'a, R, F> {
    inner: &'a mut R,
    cancelled: &'a F,
    observed_cancellation: bool,
}

impl<R: Read, F: Fn() -> bool> Read for CancellationReader<'_, R, F> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if (self.cancelled)() {
            self.observed_cancellation = true;
            return Err(io::Error::other("provider JSON decode cancelled"));
        }
        self.inner.read(output)
    }
}

fn decode_reader<T, R, F>(
    reader: &mut R,
    max_bytes: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
    profile: AllocationProfile,
    retained_copies: usize,
    cancelled: F,
) -> Result<(T, MemoryReservation), ProviderJsonAdmissionError>
where
    T: DeserializeOwned,
    R: Read + Seek,
    F: Fn() -> bool,
{
    if cancelled() {
        return Err(ProviderJsonAdmissionError::Cancelled);
    }
    let _scan_scratch = memory
        .reserve(SCAN_SCRATCH_BYTES, class)
        .map_err(ProviderJsonAdmissionError::Memory)?;
    let (shape, _) = scan_reader(reader, max_bytes, &cancelled)?;
    let heap_bytes = heap_bound::<T>(&shape, profile, retained_copies)?;
    if cancelled() {
        return Err(ProviderJsonAdmissionError::Cancelled);
    }
    let parse_bytes = heap_bytes
        .checked_add(PARSER_SCRATCH_BYTES)
        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
    let mut charge = memory
        .reserve(parse_bytes, class)
        .map_err(ProviderJsonAdmissionError::Memory)?;
    let (decoded, observed_cancellation) = {
        let mut source = CancellationReader {
            inner: reader,
            cancelled: &cancelled,
            observed_cancellation: false,
        };
        let decoded = serde_json::from_reader(&mut source);
        (decoded, source.observed_cancellation)
    };
    let decoded = match (decoded, observed_cancellation) {
        (_, true) => return Err(ProviderJsonAdmissionError::Cancelled),
        (Ok(decoded), false) => decoded,
        (Err(error), false) => {
            return Err(if cancelled() {
                ProviderJsonAdmissionError::Cancelled
            } else {
                ProviderJsonAdmissionError::Decode(error)
            });
        }
    };
    if cancelled() {
        return Err(ProviderJsonAdmissionError::Cancelled);
    }
    drop(
        charge
            .split(PARSER_SCRATCH_BYTES)
            .expect("parser scratch was included in its reservation"),
    );
    Ok((decoded, charge))
}

/// Scan, admit, then decode a bounded request context while retaining heap charge.
pub fn decode_request_context<R: Read + Seek, F: Fn() -> bool>(
    reader: &mut R,
    max_bytes: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
    cancelled: F,
) -> Result<ReservedValue<ContinuousAgentRequestContextV1>, ProviderJsonAdmissionError> {
    let (decoded, charge) = decode_reader(
        reader,
        max_bytes,
        memory,
        class,
        AllocationProfile::request(),
        1,
        cancelled,
    )?;
    Ok(charge.retain(decoded))
}

/// Scan, admit, then decode a bounded response context while retaining heap charge.
pub fn decode_response_context<R: Read + Seek, F: Fn() -> bool>(
    reader: &mut R,
    max_bytes: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
    cancelled: F,
) -> Result<ReservedValue<ContinuousAgentResponseContextV1>, ProviderJsonAdmissionError> {
    let (decoded, charge) = decode_reader(
        reader,
        max_bytes,
        memory,
        class,
        AllocationProfile::response(),
        1,
        cancelled,
    )?;
    Ok(charge.retain(decoded))
}

/// Decode a response context for the native provider pipeline with copy-bounded lease.
pub fn decode_response_context_with_lease<R: Read + Seek, F: Fn() -> bool>(
    reader: &mut R,
    max_bytes: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
    cancelled: F,
) -> Result<
    (
        ContinuousAgentResponseContextV1,
        ProviderResponseContextLease,
    ),
    ProviderJsonAdmissionError,
> {
    let (decoded, charge) = decode_reader(
        reader,
        max_bytes,
        memory,
        class,
        AllocationProfile::response(),
        RESPONSE_CONTEXT_COPY_BOUND,
        cancelled,
    )?;
    Ok((decoded, ProviderResponseContextLease::new(charge)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, SeekFrom};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    const CONTEXT_LIMIT: usize = 1024 * 1024;
    static PARSER_ENTERED: AtomicBool = AtomicBool::new(false);
    static CANCEL_PARSE_ENTERED: AtomicBool = AtomicBool::new(false);

    struct ParseProbe;
    impl<'de> serde::Deserialize<'de> for ParseProbe {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            PARSER_ENTERED.store(true, Ordering::Release);
            serde::de::IgnoredAny::deserialize(deserializer)?;
            Ok(Self)
        }
    }

    struct ParseCancellationProbe;
    impl<'de> serde::Deserialize<'de> for ParseCancellationProbe {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = self
                .bytes
                .len()
                .saturating_sub(self.position)
                .min(output.len());
            output[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
            self.position += count;
            Ok(count)
        }
    }

    impl Seek for ParseCancellationReader<'_> {
        fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
            let next = match position {
                SeekFrom::Start(pos) => i128::from(pos),
                SeekFrom::Current(offset) => self.position as i128 + i128::from(offset),
                SeekFrom::End(offset) => self.bytes.len() as i128 + i128::from(offset),
            };
            let next = usize::try_from(next)
                .ok()
                .filter(|&n| n <= self.bytes.len())
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"))?;
            self.position = next;
            if matches!(position, SeekFrom::Start(0)) {
                self.rewound.store(true, Ordering::Release);
            }
            Ok(self.position as u64)
        }
    }

    #[test]
    fn dynamic_json_heap_is_admitted_before_deserializer_runs() {
        PARSER_ENTERED.store(false, Ordering::Release);
        let mut input = b"{\"value\":[".to_vec();
        for index in 0..65_536 {
            if index != 0 {
                input.push(b',');
            }
            input.extend_from_slice(b"null");
        }
        input.extend_from_slice(b"]}");
        assert!(input.len() < CONTEXT_LIMIT);

        let memory = MemoryBudget::new(3 * 1024 * 1024, 0).expect("small test pool");
        let raw = memory.reserve(input.len(), MemoryClass::Ordinary).unwrap();
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
        let mut input =
            b"{\"base_decision_response\":{\"decision\":\"wait\",\"trace_payload\":{\"upstream_trace\":["
                .to_vec();
        for index in 0..65_536 {
            if index != 0 {
                input.push(b',');
            }
            input.extend_from_slice(b"null");
        }
        input.extend_from_slice(b"]}}}");
        assert!(input.len() < CONTEXT_LIMIT);

        let memory = MemoryBudget::new(3 * 1024 * 1024, 0).expect("small test pool");
        let raw = memory.reserve(input.len(), MemoryClass::Ordinary).unwrap();
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
        let rewound = Arc::new(AtomicBool::new(false));
        let checks_after_rewind = AtomicUsize::new(0);
        let mut reader = ParseCancellationReader {
            bytes: b"null",
            position: 0,
            rewound: Arc::clone(&rewound),
        };
        let memory = MemoryBudget::new(1024 * 1024, 0).unwrap();
        let result = decode_reader::<ParseCancellationProbe, _, _>(
            &mut reader,
            4,
            &memory,
            MemoryClass::Ordinary,
            AllocationProfile {
                max_sequence_item_bytes: 0,
                max_inline_variant_bytes: 0,
                retained_sidecar_bytes: 0,
            },
            1,
            || {
                rewound.load(Ordering::Acquire)
                    && checks_after_rewind.fetch_add(1, Ordering::AcqRel) == 1
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
}
