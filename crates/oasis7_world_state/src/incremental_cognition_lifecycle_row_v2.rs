//! Exact original five-string key and fixed scalar lifecycle row. The key
//! stream is retained by the standard member value; no ID-sized buffer exists.
use crate::{
    incremental_cognition_history_cbor::CognitionOriginalTextProjectionV1,
    incremental_cognition_history_lifecycle::CognitionLifecycleStateV1,
    incremental_field_member::*,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_semantic_key::SEMANTIC_KEY_DOMAIN,
    incremental_utf8_output::Utf8Output,
};
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1,
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::{self, Write};
const ROW: &[u8] = b"oasis7.cognition.lifecycle-row.v2\0";
pub const LIFECYCLE_ROW_BYTES_V2: usize = ROW.len() + 1 + 8 + 8 + 48;
pub fn lifecycle_tuple_field_v2() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Cognition,
        name: "history.lifecycle.tuple.v2",
        destination: FieldDestination::State(StateNamespaceV1::Artifacts),
        shape: FieldShape::ByteMap,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CognitionLifecycleRowV2 {
    pub state: CognitionLifecycleStateV1,
    pub attempt: u64,
    pub journal_ordinal: u64,
    pub original: FactLocationV1,
}
impl CognitionLifecycleRowV2 {
    pub fn canonical_bytes(&self) -> [u8; LIFECYCLE_ROW_BYTES_V2] {
        let mut bytes = [0; LIFECYCLE_ROW_BYTES_V2];
        let mut at = 0;
        let mut put = |part: &[u8]| {
            bytes[at..at + part.len()].copy_from_slice(part);
            at += part.len();
        };
        put(ROW);
        put(&[self.state as u8]);
        put(&self.attempt.to_be_bytes());
        put(&self.journal_ordinal.to_be_bytes());
        put(&self.original.original);
        put(&self.original.height.to_be_bytes());
        put(&self.original.ordinal.to_be_bytes());
        bytes
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ValueTreeError> {
        if bytes.len() != LIFECYCLE_ROW_BYTES_V2 || !bytes.starts_with(ROW) {
            return Err(bad());
        }
        use CognitionLifecycleStateV1::*;
        let state = match bytes[ROW.len()] {
            1 => Started,
            2 => ContextCaptured,
            3 => Dispatched,
            4 => ResponseRecorded,
            5 => Submitted,
            6 => Validated,
            7 => Linked,
            8 => Failed,
            9 => Cancelled,
            10 => Completed,
            _ => return Err(bad()),
        };
        let mut at = ROW.len() + 1;
        let mut num = || {
            let n = u64::from_be_bytes(bytes[at..at + 8].try_into().unwrap());
            at += 8;
            n
        };
        let attempt = num();
        let journal_ordinal = num();
        let original = bytes[at..at + 32].try_into().unwrap();
        at += 32;
        let height = u64::from_be_bytes(bytes[at..at + 8].try_into().unwrap());
        at += 8;
        let ordinal = u64::from_be_bytes(bytes[at..].try_into().unwrap());
        Ok(Self {
            state,
            attempt,
            journal_ordinal,
            original: FactLocationV1 {
                original,
                height,
                ordinal,
            },
        })
    }
}
pub struct CheckedCognitionLifecycleRowV2 {
    row: CognitionLifecycleRowV2,
    keys: [CognitionOriginalTextProjectionV1; 5],
}
impl CheckedCognitionLifecycleRowV2 {
    pub fn row(&self) -> CognitionLifecycleRowV2 {
        self.row
    }
    pub fn keys(&self) -> &[CognitionOriginalTextProjectionV1; 5] {
        &self.keys
    }
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition lifecycle tuple row")
}
#[derive(Default)]
struct Hash(blake3::Hasher);
impl Write for Hash {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct KeyStream<'a> {
    text: [Utf8Output<'a, Hash>; 5],
    prefix: usize,
    field: usize,
    header: [u8; 10],
    used: usize,
    remaining: u64,
    lengths: [u64; 5],
    failed: bool,
}
impl KeyStream<'_> {
    fn accept(&mut self, mut b: &[u8]) -> io::Result<()> {
        while self.prefix < SEMANTIC_KEY_DOMAIN.len() && !b.is_empty() {
            if b[0] != SEMANTIC_KEY_DOMAIN[self.prefix] {
                return Err(io::Error::other("lifecycle key domain"));
            }
            self.prefix += 1;
            b = &b[1..];
        }
        while !b.is_empty() {
            if self.field == 5 {
                return Err(io::Error::other("lifecycle key trailing"));
            }
            if self.remaining != 0 {
                let n = b
                    .len()
                    .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
                self.text[self.field].write_all(&b[..n])?;
                self.remaining -= n as u64;
                b = &b[n..];
                if self.remaining == 0 {
                    if !self.text[self.field].is_complete() {
                        return Err(io::Error::other("lifecycle UTF8 incomplete"));
                    }
                    self.field += 1;
                    self.used = 0;
                }
                continue;
            }
            let width = if self.field < 4 { 10 } else { 9 };
            let n = (width - self.used).min(b.len());
            self.header[self.used..self.used + n].copy_from_slice(&b[..n]);
            self.used += n;
            b = &b[n..];
            if self.used == width {
                let at = usize::from(self.field < 4);
                if (at == 1 && self.header[0] != 5) || self.header[at] != 1 {
                    return Err(io::Error::other("lifecycle tuple/text tags"));
                }
                self.remaining = u64::from_be_bytes(self.header[at + 1..width].try_into().unwrap());
                if self.remaining == 0 {
                    return Err(io::Error::other("sparse lifecycle key"));
                }
                self.lengths[self.field] = self.remaining;
            }
        }
        Ok(())
    }
    fn finish(self) -> Result<[u64; 5], ValueTreeError> {
        if self.failed
            || self.prefix != SEMANTIC_KEY_DOMAIN.len()
            || self.field != 5
            || self.remaining != 0
        {
            return Err(bad());
        }
        Ok(self.lengths)
    }
}
impl Write for KeyStream<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("poisoned lifecycle key"));
        }
        self.failed = true;
        self.accept(b)?;
        self.failed = false;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            Err(io::Error::other("poisoned lifecycle key"))
        } else {
            Ok(())
        }
    }
}
pub fn read_cognition_lifecycle_row_v2<R: ValueNodeReader>(
    id: &[u8; 32],
    key: &WorldFieldKeyV1,
    read: &mut ValueReadContext<'_, R>,
    limits: MemberRecordLimitsV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedCognitionLifecycleRowV2>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(|_| bad())?;
    let field = lifecycle_tuple_field_v2();
    if parsed.owner != field.owner
        || parsed.name != field.name
        || parsed.namespace != StateNamespaceV1::Artifacts
        || parsed.shape != FieldShape::ByteMap
        || parsed.kind != WorldFieldRecordKindV1::Bytes
    {
        return Err(bad());
    }
    let charge = read.memory.reserve(8192, read.class)?;
    let mut hashes: [Hash; 5] = std::array::from_fn(|_| Hash::default());
    let text = hashes
        .each_mut()
        .map(|h| Utf8Output::new(h, "lifecycle key UTF8"));
    let mut sink = KeyStream {
        text,
        prefix: 0,
        field: 0,
        header: [0; 10],
        used: 0,
        remaining: 0,
        lengths: [0; 5],
        failed: false,
    };
    let mut payload = [0u8; LIFECYCLE_ROW_BYTES_V2];
    let mut out = payload.as_mut_slice();
    let result = read_member_record_v1(id, key, read, limits, &mut sink, &mut out, &mut cancelled);
    if sink.failed {
        return Err(bad());
    }
    let info = result?;
    if info.codec != MemberPayloadCodecV1::Bytes || !out.is_empty() {
        return Err(bad());
    }
    let lengths = sink.finish()?;
    let keys = std::array::from_fn(|i| CognitionOriginalTextProjectionV1::Text {
        bytes: lengths[i],
        utf8_digest: *hashes[i].0.finalize().as_bytes(),
    });
    let row = CognitionLifecycleRowV2::from_bytes(&payload)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(CheckedCognitionLifecycleRowV2 { row, keys }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_five_strings_stream_without_normalization_or_truncation() {
        let long = "汉🙂e\u{301}".repeat(1000);
        let values = [" agent ", "session", "turn", "request", long.as_str()];
        let key = (values[0], (values[1], (values[2], (values[3], values[4]))));
        let mut wire = Vec::new();
        crate::incremental_semantic_key::write_semantic_key_v1(&key, &mut wire).unwrap();
        let mut independent = SEMANTIC_KEY_DOMAIN.to_vec();
        for (i, value) in values.into_iter().enumerate() {
            if i < 4 {
                independent.push(5);
            }
            independent.push(1);
            independent.extend_from_slice(&(value.len() as u64).to_be_bytes());
            independent.extend_from_slice(value.as_bytes());
        }
        assert_eq!(wire, independent);
        for chunk in [1, 2, 3, 4, 7, 17, 128, 4096, wire.len()] {
            let mut hashes: [Hash; 5] = std::array::from_fn(|_| Hash::default());
            let text = hashes.each_mut().map(|h| Utf8Output::new(h, "UTF8"));
            let mut sink = KeyStream {
                text,
                prefix: 0,
                field: 0,
                header: [0; 10],
                used: 0,
                remaining: 0,
                lengths: [0; 5],
                failed: false,
            };
            for part in wire.chunks(chunk) {
                sink.write_all(part).unwrap();
            }
            let lengths = sink.finish().unwrap();
            for (i, value) in values.into_iter().enumerate() {
                assert_eq!(lengths[i], value.len() as u64);
                assert_eq!(hashes[i].0.finalize(), blake3::hash(value.as_bytes()));
            }
        }
        for mutate in [0, SEMANTIC_KEY_DOMAIN.len(), wire.len() - 1, wire.len()] {
            let mut bad = wire.clone();
            if mutate == bad.len() {
                bad.push(0)
            } else {
                bad[mutate] = 0xff;
            }
            let mut hashes: [Hash; 5] = std::array::from_fn(|_| Hash::default());
            let text = hashes.each_mut().map(|h| Utf8Output::new(h, "UTF8"));
            let mut sink = KeyStream {
                text,
                prefix: 0,
                field: 0,
                header: [0; 10],
                used: 0,
                remaining: 0,
                lengths: [0; 5],
                failed: false,
            };
            assert!(sink.write_all(&bad).is_err());
            assert!(sink.write(&[]).is_err());
            assert!(sink.finish().is_err());
        }
    }
    #[test]
    fn lifecycle_row_wire_preserves_original_state_attempt_and_locator() {
        let row = CognitionLifecycleRowV2 {
            state: CognitionLifecycleStateV1::Dispatched,
            attempt: u64::MAX,
            journal_ordinal: 7,
            original: FactLocationV1 {
                original: [0x5a; 32],
                height: 9,
                ordinal: 11,
            },
        };
        let bytes = row.canonical_bytes();
        assert_eq!(bytes[ROW.len()], 3);
        assert_eq!(
            &bytes[ROW.len() + 1..ROW.len() + 9],
            &u64::MAX.to_be_bytes()
        );
        assert_eq!(CognitionLifecycleRowV2::from_bytes(&bytes).unwrap(), row);
        let mut bad = bytes;
        bad[ROW.len()] = 0;
        assert!(CognitionLifecycleRowV2::from_bytes(&bad).is_err());
        assert!(CognitionLifecycleRowV2::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    }
}
