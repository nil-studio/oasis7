//! Bounded validation of the original JSON-CBOR bytes, without constructing a
//! JSON value. Canonical bytes alone confer no semantic or accepted-history
//! authority; the record-specific digest/schema layer must also succeed.
use crate::incremental_utf8_output::Utf8Output;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::io::{Read, Write};
#[path = "incremental_cognition_history_cbor_schema.rs"]
mod schema;
use crate::incremental_cognition_history::CognitionHistoryFieldV1;
pub use schema::CognitionOriginalSemanticV1;
#[path = "incremental_cognition_history_cbor_record_schema.rs"]
mod record_schema;
pub use record_schema::{CognitionHistoryRecordSchemaV1, CognitionHistoryRecordScopeV1};
#[path = "incremental_cognition_history_cbor_projection.rs"]
mod projection;
pub use projection::{CognitionJournalLifecycleProjectionV1, CognitionOriginalTextProjectionV1};
const _: () = assert!(std::mem::size_of::<CognitionHistoryRecordSchemaV1>() <= 1024);

pub struct CognitionHistoryCborWorkV1 {
    pub bytes_remaining: u64,
    pub nodes_remaining: u64,
    pub source_reads_remaining: u64,
    pub key_bytes_remaining: u64,
    pub max_depth: usize,
    pub max_key_bytes: usize,
}

#[derive(Debug)]
pub struct CognitionCanonicalOriginalV1 {
    id: [u8; 32],
    bytes: u64,
    class: MemoryClass,
}
impl CognitionCanonicalOriginalV1 {
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn class(&self) -> MemoryClass {
        self.class
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition canonical JSON-CBOR")
}
fn spend(value: &mut u64, amount: u64) -> Result<(), ValueTreeError> {
    *value = value
        .checked_sub(amount)
        .ok_or(ValueTreeError::ResourceLimited("Cognition CBOR work"))?;
    Ok(())
}
struct Parser<'a, R, C> {
    input: &'a mut R,
    memory: &'a MemoryBudget,
    class: MemoryClass,
    work: &'a mut CognitionHistoryCborWorkV1,
    cancelled: C,
    remaining: u64,
    hash: blake3::Hasher,
    semantic: Option<Box<schema::Schema>>,
    record: Option<Box<record_schema::Schema>>,
}
impl<R: Read, C: FnMut() -> bool> Parser<'_, R, C> {
    fn check(&mut self) -> Result<(), ValueTreeError> {
        if (self.cancelled)() {
            Err(ValueTreeError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn read(&mut self, output: &mut [u8]) -> Result<(), ValueTreeError> {
        if output.len() as u64 > self.remaining || output.len() as u64 > self.work.bytes_remaining {
            return Err(ValueTreeError::ResourceLimited("Cognition CBOR bytes"));
        }
        let mut used = 0;
        while used < output.len() {
            self.check()?;
            spend(&mut self.work.source_reads_remaining, 1)?;
            let end = (used + 8192).min(output.len());
            let n = match self.input.read(&mut output[used..end]) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                other => other.map_err(ValueTreeError::Io)?,
            };
            if n == 0 || n > end - used {
                return Err(invalid());
            }
            // Charge actual source consumption before any observer can fail.
            self.remaining -= n as u64;
            spend(&mut self.work.bytes_remaining, n as u64)?;
            self.hash.update(&output[used..used + n]);
            if let Some(semantic) = &mut self.semantic {
                semantic.feed(&output[used..used + n])?;
            }
            if let Some(record) = &mut self.record {
                record.feed(&output[used..used + n])?;
            }
            used += n;
            self.check()?;
        }
        Ok(())
    }
    fn byte(&mut self) -> Result<u8, ValueTreeError> {
        let mut byte = [0];
        self.read(&mut byte)?;
        Ok(byte[0])
    }
    fn argument(&mut self, additional: u8) -> Result<u64, ValueTreeError> {
        let width = match additional {
            0..=23 => return Ok(additional as u64),
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => return Err(invalid()),
        };
        let mut bytes = [0; 8];
        self.read(&mut bytes[8 - width..])?;
        let value = u64::from_be_bytes(bytes);
        let minimum = match width {
            1 => 24,
            2 => 256,
            4 => 65536,
            _ => 1u64 << 32,
        };
        if value < minimum {
            return Err(invalid());
        }
        Ok(value)
    }
    fn text(&mut self, length: u64) -> Result<(), ValueTreeError> {
        if length > self.remaining {
            return Err(invalid());
        }
        let mut buffer = [0; 8192];
        let mut sink = std::io::sink();
        let mut utf8 = Utf8Output::new(&mut sink, "Cognition CBOR UTF8");
        let mut left = length;
        while left != 0 {
            let n = left.min(buffer.len() as u64) as usize;
            self.read(&mut buffer[..n])?;
            utf8.write_all(&buffer[..n]).map_err(ValueTreeError::Io)?;
            left -= n as u64;
        }
        if !utf8.is_complete() {
            return Err(invalid());
        }
        self.check()
    }
    fn key(&mut self) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
        spend(&mut self.work.nodes_remaining, 1)?;
        let initial = self.byte()?;
        if initial >> 5 != 3 {
            return Err(invalid());
        }
        let length = self.argument(initial & 31)?;
        if length > self.work.max_key_bytes as u64 || length > self.remaining {
            return Err(ValueTreeError::ResourceLimited("Cognition CBOR key"));
        }
        spend(&mut self.work.key_bytes_remaining, length)?;
        let length = usize::try_from(length).map_err(|_| invalid())?;
        let charge = self
            .memory
            .reserve(length.checked_add(128).ok_or_else(invalid)?, self.class)?;
        let mut key = vec![0; length];
        self.read(&mut key)?;
        std::str::from_utf8(&key).map_err(|_| invalid())?;
        Ok(charge.retain(key))
    }
    fn value(&mut self, depth: usize, initial: u8) -> Result<(), ValueTreeError> {
        self.check()?;
        if depth > self.work.max_depth {
            return Err(ValueTreeError::ResourceLimited("Cognition CBOR depth"));
        }
        spend(&mut self.work.nodes_remaining, 1)?;
        if let Some(record) = &mut self.record {
            record.begin(depth, initial)?;
        }
        match initial >> 5 {
            0 => {
                self.argument(initial & 31)?;
            }
            1 => {
                if self.argument(initial & 31)? > i64::MAX as u64 {
                    return Err(invalid());
                }
            }
            3 => {
                let length = self.argument(initial & 31)?;
                if let Some(record) = &mut self.record {
                    record.text(depth, length)?;
                }
                self.text(length)?;
            }
            4 => {
                let length = self.argument(initial & 31)?;
                if let Some(record) = &mut self.record {
                    record.array(depth, length);
                }
                if length > self.work.nodes_remaining || length > self.remaining {
                    return Err(ValueTreeError::ResourceLimited("Cognition CBOR array"));
                }
                for _ in 0..length {
                    let first = self.byte()?;
                    self.value(depth + 1, first)?;
                }
            }
            5 => {
                let count = self.argument(initial & 31)?;
                if count > self.work.nodes_remaining / 2 || count > self.remaining / 2 {
                    return Err(ValueTreeError::ResourceLimited("Cognition CBOR map"));
                }
                if depth == 0
                    && let Some(semantic) = &mut self.semantic
                {
                    semantic.start(count);
                }
                let mut previous: Option<ReservedValue<Vec<u8>>> = None;
                for _ in 0..count {
                    if depth == 0
                        && let Some(semantic) = &mut self.semantic
                    {
                        semantic.stop();
                    }
                    let key = self.key()?;
                    if let Some(record) = &mut self.record {
                        record.key(depth, &key)?;
                    }
                    if let Some(old) = &previous
                        && (old.len(), old.as_slice()) >= (key.len(), key.as_slice())
                    {
                        return Err(invalid());
                    }
                    if depth == 0
                        && let Some(semantic) = &mut self.semantic
                    {
                        semantic.key(&key)?;
                    }
                    previous = Some(key);
                    let first = self.byte()?;
                    if depth == 0
                        && let Some(semantic) = &mut self.semantic
                    {
                        semantic.first(first);
                    }
                    self.value(depth + 1, first)?;
                    if depth == 0
                        && let Some(semantic) = &mut self.semantic
                    {
                        semantic.end_value()?;
                    }
                }
            }
            7 => match initial {
                0xf4..=0xf6 => {}
                0xf9..=0xfb => {
                    let width = match initial {
                        0xf9 => 2,
                        0xfa => 4,
                        _ => 8,
                    };
                    let mut bytes = [0; 9];
                    bytes[0] = initial;
                    self.read(&mut bytes[1..=width])?;
                    let value: f64 =
                        serde_cbor::from_slice(&bytes[..=width]).map_err(|_| invalid())?;
                    if !value.is_finite() {
                        return Err(invalid());
                    }
                    // At most nine bytes; the ABI remains the scalar wire authority.
                    let encoded =
                        oasis7_wasm_abi::encode_canonical_cbor(&value).map_err(|_| invalid())?;
                    if encoded != bytes[..=width] {
                        return Err(invalid());
                    }
                }
                _ => return Err(invalid()),
            },
            _ => return Err(invalid()),
        }
        if let Some(record) = &mut self.record {
            record.end(depth)?;
        }
        self.check()
    }
}

pub fn validate_cognition_history_canonical_cbor_v1<R: Read, C: FnMut() -> bool>(
    expected: [u8; 32],
    length: u64,
    input: &mut R,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut CognitionHistoryCborWorkV1,
    cancelled: C,
) -> Result<ReservedValue<CognitionCanonicalOriginalV1>, ValueTreeError> {
    parse(
        CanonicalRequest {
            expected,
            length,
            memory,
            class,
        },
        input,
        work,
        cancelled,
        None,
        None,
    )
    .map(|(canonical, _, _)| canonical)
}
struct CanonicalRequest<'a> {
    expected: [u8; 32],
    length: u64,
    memory: &'a MemoryBudget,
    class: MemoryClass,
}
type Parsed = (
    ReservedValue<CognitionCanonicalOriginalV1>,
    Option<CognitionOriginalSemanticV1>,
    Option<record_schema::Fields>,
);
fn parse<R: Read, C: FnMut() -> bool>(
    request: CanonicalRequest<'_>,
    input: &mut R,
    work: &mut CognitionHistoryCborWorkV1,
    cancelled: C,
    semantic: Option<Box<schema::Schema>>,
    record: Option<Box<record_schema::Schema>>,
) -> Result<Parsed, ValueTreeError> {
    let CanonicalRequest {
        expected,
        length,
        memory,
        class,
    } = request;
    if work.max_depth > 128 || length > work.bytes_remaining {
        return Err(ValueTreeError::ResourceLimited("Cognition CBOR admission"));
    }
    let retained = memory.reserve(512, class)?;
    // Account for bounded recursion, UTF8 chunk storage and small ABI scalar work.
    let _scratch = memory.reserve(16 * 1024 + work.max_depth * 512, class)?;
    let mut parser = Parser {
        input,
        memory,
        class,
        work,
        cancelled,
        remaining: length,
        hash: blake3::Hasher::new(),
        semantic,
        record,
    };
    parser.check()?;
    let first = parser.byte()?;
    if first >> 5 != 5 {
        return Err(invalid());
    }
    parser.value(0, first)?;
    if parser.remaining != 0 || parser.hash.finalize().as_bytes() != &expected {
        return Err(invalid());
    }
    parser.check()?;
    spend(&mut parser.work.source_reads_remaining, 1)?;
    let mut extra = [0];
    loop {
        parser.check()?;
        match parser.input.read(&mut extra) {
            Ok(0) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                spend(&mut parser.work.source_reads_remaining, 1)?
            }
            Err(error) => return Err(ValueTreeError::Io(error)),
            Ok(n) => {
                spend(&mut parser.work.bytes_remaining, n as u64)?;
                return Err(invalid());
            }
        }
    }
    if parser.record.is_some() {
        parser.check()?;
    }
    let semantic = parser
        .semantic
        .as_ref()
        .map(|value| value.finish(expected, length, class))
        .transpose()?;
    let record = parser
        .record
        .as_ref()
        .map(|record| record.finish(semantic.as_ref().ok_or_else(invalid)?, parser.work))
        .transpose()?;
    parser.check()?;
    Ok((
        retained.retain(CognitionCanonicalOriginalV1 {
            id: expected,
            bytes: length,
            class,
        }),
        semantic,
        record,
    ))
}

pub struct CognitionOriginalCborRequestV1 {
    pub field: CognitionHistoryFieldV1,
    pub id: [u8; 32],
    pub bytes: u64,
}
/// Verifies the original per-record digest projections, not lifecycle or an
/// accepted History occurrence. Missing identity/artifact remain explicit.
pub fn verify_cognition_history_original_semantics_v1<R: Read, C: FnMut() -> bool>(
    request: CognitionOriginalCborRequestV1,
    input: &mut R,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut CognitionHistoryCborWorkV1,
    cancelled: C,
) -> Result<ReservedValue<CognitionOriginalSemanticV1>, ValueTreeError> {
    let retained = memory.reserve(512, class)?;
    let _semantic = memory.reserve(std::mem::size_of::<schema::Schema>() + 32768, class)?;
    let semantic = Box::new(schema::Schema::new(request.field)?);
    let (_, semantic, _) = parse(
        CanonicalRequest {
            expected: request.id,
            length: request.bytes,
            memory,
            class,
        },
        input,
        work,
        cancelled,
        Some(semantic),
        None,
    )?;
    Ok(retained.retain(semantic.ok_or_else(invalid)?))
}

/// Strict *local fields* in one original. This is deliberately separate from
/// the digest-only API; no cross-record/lifecycle or accepted-history grant.
pub fn verify_cognition_history_original_record_schema_v1<R: Read, C: FnMut() -> bool>(
    request: CognitionOriginalCborRequestV1,
    input: &mut R,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut CognitionHistoryCborWorkV1,
    cancelled: C,
) -> Result<ReservedValue<CognitionHistoryRecordSchemaV1>, ValueTreeError> {
    let retained = memory.reserve(1024, class)?;
    let _semantic = memory.reserve(std::mem::size_of::<schema::Schema>() + 32768, class)?;
    let _record = memory.reserve(std::mem::size_of::<record_schema::Schema>() + 32768, class)?;
    let semantic = Box::new(schema::Schema::new(request.field)?);
    let record = Box::new(record_schema::Schema::new(request.field));
    let (_, semantic, record) = parse(
        CanonicalRequest {
            expected: request.id,
            length: request.bytes,
            memory,
            class,
        },
        input,
        work,
        cancelled,
        Some(semantic),
        Some(record),
    )?;
    Ok(retained.retain(record_schema::qualified(
        semantic.ok_or_else(invalid)?,
        record.ok_or_else(invalid)?,
    )))
}

#[cfg(test)]
#[path = "incremental_cognition_history_cbor_record_schema_tests.rs"]
mod record_schema_tests;
#[cfg(test)]
#[path = "incremental_cognition_history_cbor_tests.rs"]
mod tests;
