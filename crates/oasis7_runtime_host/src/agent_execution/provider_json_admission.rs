//! Bounded JSON decoding for the native provider context lane.
//!
//! The input/body bound is not a heap bound: a short array of tiny JSON values
//! can expand into a much larger `Vec<Value>` or map. Scan the already-bounded
//! input before invoking Serde, then retain a conservative charge for the
//! decoded context for as long as its caller uses it.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};
use std::mem::size_of;
use std::sync::Arc;

use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation, ReservedValue,
};
use oasis7_wasm_abi::AgentCommandResponse;
use serde::de::DeserializeOwned;

use super::continuous_agent_harness::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
};
use super::decision_provider::{
    ActionCatalogEntry, MemoryWriteIntent, ProviderInteractionTarget, ProviderModuleCommand,
    ProviderNavigationNode, ProviderNearbyEntity, ProviderRecentEvent, ProviderTranscriptEntry,
};
use super::{Action, AgentQuery, DecisionResponse, ProviderDecision};

const SCAN_SCRATCH_BYTES: usize = 8 * 1024;
const PARSER_SCRATCH_BYTES: usize = 16 * 1024;
const MAP_ENTRY_BYTES: usize = 128;
const CONTAINER_BYTES: usize = 64;
const MAX_JSON_DEPTH: usize = 128;
const CANCEL_POLL_BYTES: usize = 64 * 1024;
const RESPONSE_CONTEXT_COPY_BOUND: usize = 4;
const RESPONSE_TRACE_OVERHEAD_BYTES: usize = 512 * 1024;

/// Process-local accounting retained while a decoded native response context
/// is copied through the behavior and Runtime outcome sidechannels. It is not
/// serialized with provider data or material references.
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

// This sidecar is accounting metadata, not part of response identity or value
// equality. Cloning the lease shares its reservation; it does not pay for
// additional DTO copies, which must remain within the response profile's
// explicit copy bound.
impl PartialEq for ProviderResponseContextLease {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Debug)]
pub enum ProviderJsonAdmissionError {
    Memory(MemoryLimitExceeded),
    Cancelled,
    InputLimit,
    NestingLimit,
    SizeOverflow,
    Decode(serde_json::Error),
    Io(io::Error),
}

impl fmt::Display for ProviderJsonAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory(error) => write!(formatter, "provider JSON memory admission: {error}"),
            Self::Cancelled => formatter.write_str("provider JSON decoding cancelled"),
            Self::InputLimit => formatter.write_str("provider JSON input exceeds its bound"),
            Self::NestingLimit => formatter.write_str("provider JSON nesting exceeds its bound"),
            Self::SizeOverflow => formatter.write_str("provider JSON heap estimate overflow"),
            Self::Decode(error) => write!(formatter, "provider JSON decoding failed: {error}"),
            Self::Io(error) => write!(formatter, "provider JSON input read failed: {error}"),
        }
    }
}

impl std::error::Error for ProviderJsonAdmissionError {}

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
            // These are actual response variants, not sequence elements. Keep
            // their inline storage in the per-copy estimate without
            // multiplying a response enum's size by an unrelated JSON array
            // length. Their nested Vec/String payloads are counted by the
            // structural scan above.
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

#[derive(Default)]
struct JsonShape {
    array_items: usize,
    object_members: usize,
    containers: usize,
    string_bytes: usize,
    largest_string_bytes: usize,
}

struct JsonShapeScanner {
    shape: JsonShape,
    stack: [u8; MAX_JSON_DEPTH],
    depth: usize,
    in_string: bool,
    escaped: bool,
    in_primitive: bool,
    string_bytes_since_poll: usize,
    current_string_bytes: usize,
}

impl JsonShapeScanner {
    fn new() -> Self {
        Self {
            shape: JsonShape::default(),
            stack: [0; MAX_JSON_DEPTH],
            depth: 0,
            in_string: false,
            escaped: false,
            in_primitive: false,
            string_bytes_since_poll: 0,
            current_string_bytes: 0,
        }
    }

    fn feed(
        &mut self,
        bytes: &[u8],
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), ProviderJsonAdmissionError> {
        for (offset, byte) in bytes.iter().copied().enumerate() {
            if offset % CANCEL_POLL_BYTES == 0 && cancelled() {
                return Err(ProviderJsonAdmissionError::Cancelled);
            }
            if self.in_string {
                self.shape.string_bytes = self
                    .shape
                    .string_bytes
                    .checked_add(1)
                    .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                self.current_string_bytes = self
                    .current_string_bytes
                    .checked_add(1)
                    .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                self.string_bytes_since_poll += 1;
                if self.string_bytes_since_poll >= CANCEL_POLL_BYTES {
                    self.string_bytes_since_poll = 0;
                    if cancelled() {
                        return Err(ProviderJsonAdmissionError::Cancelled);
                    }
                }
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                    self.shape.largest_string_bytes = self
                        .shape
                        .largest_string_bytes
                        .max(self.current_string_bytes);
                    self.current_string_bytes = 0;
                }
                continue;
            }

            if self.in_primitive {
                if !matches!(byte, b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t') {
                    continue;
                }
                self.in_primitive = false;
            }

            match byte {
                b'"' => {
                    if self.parent_is_array() {
                        self.shape.array_items = self
                            .shape
                            .array_items
                            .checked_add(1)
                            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                    }
                    self.in_string = true;
                }
                b'{' => {
                    self.count_array_value()?;
                    self.push_container(2)?;
                }
                b'[' => {
                    self.count_array_value()?;
                    self.push_container(1)?;
                }
                b'}' | b']' => {
                    self.depth = self.depth.saturating_sub(1);
                }
                b':' => {
                    self.shape.object_members = self
                        .shape
                        .object_members
                        .checked_add(1)
                        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                }
                b'-' | b'0'..=b'9' | b't' | b'f' | b'n' => {
                    self.count_array_value()?;
                    self.in_primitive = true;
                }
                _ => {}
            }
        }
        if cancelled() {
            return Err(ProviderJsonAdmissionError::Cancelled);
        }
        Ok(())
    }

    fn parent_is_array(&self) -> bool {
        self.depth > 0 && self.stack[self.depth - 1] == 1
    }

    fn count_array_value(&mut self) -> Result<(), ProviderJsonAdmissionError> {
        if self.parent_is_array() {
            self.shape.array_items = self
                .shape
                .array_items
                .checked_add(1)
                .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        }
        Ok(())
    }

    fn push_container(&mut self, kind: u8) -> Result<(), ProviderJsonAdmissionError> {
        self.depth = self
            .depth
            .checked_add(1)
            .filter(|depth| *depth <= MAX_JSON_DEPTH)
            .ok_or(ProviderJsonAdmissionError::NestingLimit)?;
        self.stack[self.depth - 1] = kind;
        self.shape.containers = self
            .shape
            .containers
            .checked_add(1)
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        Ok(())
    }
}

fn max_size(sizes: &[usize]) -> usize {
    sizes.iter().copied().max().unwrap_or(0)
}

impl JsonShape {
    fn heap_bound<T>(
        &self,
        profile: AllocationProfile,
        retained_copies: usize,
    ) -> Result<usize, ProviderJsonAdmissionError> {
        let sequence_item = profile
            .max_sequence_item_bytes
            .checked_mul(2)
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        let array_bytes = self
            .array_items
            .checked_mul(sequence_item)
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        let map_bytes = self
            .object_members
            .checked_mul(MAP_ENTRY_BYTES)
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        // Serde's streaming JSON parser keeps a reusable decoded string
        // scratch buffer alongside retained String allocations. The total
        // retained-string capacity is bounded by 2x wire bytes; the largest
        // individual token bounds the additional parser scratch.
        let string_bytes = self
            .string_bytes
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(self.largest_string_bytes))
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        let container_bytes = self
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
}

fn scan_reader<R: Read + Seek>(
    reader: &mut R,
    max_bytes: u64,
    cancelled: &impl Fn() -> bool,
) -> Result<(JsonShape, u64), ProviderJsonAdmissionError> {
    let mut scanner = JsonShapeScanner::new();
    let mut scratch = [0; SCAN_SCRATCH_BYTES];
    let mut total = 0u64;
    loop {
        if cancelled() {
            return Err(ProviderJsonAdmissionError::Cancelled);
        }
        let read = reader.read(&mut scratch).map_err(|error| {
            if cancelled() {
                ProviderJsonAdmissionError::Cancelled
            } else {
                ProviderJsonAdmissionError::Io(error)
            }
        })?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        if total > max_bytes {
            return Err(ProviderJsonAdmissionError::InputLimit);
        }
        scanner.feed(&scratch[..read], cancelled)?;
    }
    reader
        .seek(SeekFrom::Start(0))
        .map_err(ProviderJsonAdmissionError::Io)?;
    Ok((scanner.shape, total))
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
            // serde_json reads through `Read::bytes()`, which retries
            // Interrupted errors. Use a non-retryable kind and preserve the
            // cancellation fact separately so the caller still gets the
            // typed Cancelled result.
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
    let heap_bytes = shape.heap_bound::<T>(profile, retained_copies)?;
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

/// Scan, admit, then decode a bounded request context while retaining a
/// conservative heap charge with the returned DTO.
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

/// Scan, admit, then decode a bounded response context while retaining a
/// conservative heap charge with the returned DTO.
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

/// Decode a response context for the native provider pipeline. The lease
/// includes four DTO-sized bounds for the decoded response, the provider
/// behavior's inner response, its selected decision, and the retained trace
/// decision. It also covers the bounded transcript/tool trace sidecar. Further
/// ownership is carried by the same one-outcome sidechannel; any new path that
/// clones another response DTO must increase this bound or share the DTO.
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
#[path = "provider_json_admission_tests.rs"]
mod tests;
