//! Explicit v3 framing. Decoding public capsule bytes grants no qualification.
use super::*;
use oasis7_formal_protocol::{
    incremental_material::FactLocationV1, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

type CognitionHistoryDecodedPartsV3 = (
    ReservedValue<CognitionHistoryViewV1>,
    CognitionHistoryDerivedRefsV2,
    ReservedValue<Box<CognitionHistorySourceSummaryV3>>,
);
pub(super) type CognitionHistoryDecodedProfileV3 = (
    ReservedValue<CognitionHistoryViewV1>,
    Option<CognitionHistoryDerivedRefsV2>,
    Option<ReservedValue<Box<CognitionHistorySourceSummaryV3>>>,
);

pub const COGNITION_HISTORY_VIEW_V3_MAGIC: &[u8] = b"oasis7.cognition.history-view.v3\0";
pub const COGNITION_HISTORY_SOURCE_CAPSULE_V3_BYTES: usize = 436;
pub const COGNITION_HISTORY_VIEW_V3_BYTES: usize = COGNITION_HISTORY_VIEW_V3_MAGIC.len()
    + 8
    + COGNITION_HISTORY_VIEW_V2_BYTES
    + COGNITION_HISTORY_SOURCE_CAPSULE_V3_BYTES;
fn profile() -> [u8; 32] {
    *blake3::hash(b"oasis7.cognition.history-source-summary.v3\0").as_bytes()
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition History v3 framing")
}
pub struct DecodedCognitionHistoryViewV3 {
    pub view: ReservedValue<CognitionHistoryViewV1>,
    pub derived: CognitionHistoryDerivedRefsV2,
    pub source: ReservedValue<Box<CognitionHistorySourceSummaryV3>>,
}
pub fn cognition_history_view_v3_bytes(
    view: &CognitionHistoryViewV1,
    derived: &CognitionHistoryDerivedRefsV2,
    source: &CognitionHistorySourceSummaryV3,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<[u8; COGNITION_HISTORY_VIEW_V3_BYTES]>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    source.validate_shape(view, derived)?;
    let v2 = cognition_history_view_v2_bytes(view, derived, memory, &mut cancelled)?;
    let mut bytes = [0; COGNITION_HISTORY_VIEW_V3_BYTES];
    let mut at = 0;
    let mut put = |part: &[u8]| {
        bytes[at..at + part.len()].copy_from_slice(part);
        at += part.len();
    };
    put(COGNITION_HISTORY_VIEW_V3_MAGIC);
    put(&(COGNITION_HISTORY_VIEW_V2_BYTES as u64).to_be_bytes());
    put(v2.as_slice());
    put(&profile());
    put(&[match source.mode {
        CognitionHistorySourceModeV3::Bootstrap => 0,
        CognitionHistorySourceModeV3::Delta => 1,
    }]);
    for slot in 0..3 {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        put(&source.cache_state_digests[slot].unwrap_or([0; 32]));
        put(&[u8::from(source.last[slot].is_some())]);
        match &source.last[slot] {
            Some(last) => {
                put(&last.sequence.to_be_bytes());
                put(&last.semantic_digest);
                put(&last.location.height.to_be_bytes());
                put(&last.location.ordinal.to_be_bytes());
                put(&last.location.original);
                put(&last.original_bytes.to_be_bytes());
            }
            None => put(&[0; 96]),
        }
    }
    put(&source.lifecycle_keys.to_be_bytes());
    put(&source.lifecycle_journal_records.to_be_bytes());
    if at != bytes.len() {
        return Err(invalid());
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(bytes))
}
fn decode_parts_v3(
    bytes: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    mut cancelled: impl FnMut() -> bool,
) -> Result<CognitionHistoryDecodedPartsV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if bytes.len() != COGNITION_HISTORY_VIEW_V3_BYTES
        || !bytes.starts_with(COGNITION_HISTORY_VIEW_V3_MAGIC)
    {
        return Err(invalid());
    }
    let _charge = memory.reserve(4096, class)?;
    let source_charge =
        memory.reserve(COGNITION_HISTORY_SOURCE_SUMMARY_ADMISSION_V3_BYTES, class)?;
    let mut r = Reader {
        bytes,
        at: COGNITION_HISTORY_VIEW_V3_MAGIC.len(),
    };
    if r.number()? != COGNITION_HISTORY_VIEW_V2_BYTES as u64 {
        return Err(invalid());
    }
    let begin = r.at;
    let end = begin
        .checked_add(COGNITION_HISTORY_VIEW_V2_BYTES)
        .ok_or_else(invalid)?;
    let v2 = bytes.get(begin..end).ok_or_else(invalid)?;
    if !v2.starts_with(COGNITION_HISTORY_VIEW_V2_MAGIC) {
        return Err(invalid());
    }
    let (view, derived) = super::manifest_v2::decode_profile(v2, memory, class, &mut cancelled)?;
    let derived = derived.ok_or_else(invalid)?;
    r.at = end;
    if r.take::<32>()? != profile() {
        return Err(invalid());
    }
    let mode = match r.take::<1>()?[0] {
        0 => CognitionHistorySourceModeV3::Bootstrap,
        1 => CognitionHistorySourceModeV3::Delta,
        _ => return Err(invalid()),
    };
    let mut cache_state_digests = [None; 3];
    let mut last = [None, None, None];
    for slot in 0..3 {
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let digest = r.take::<32>()?;
        if derived.caches[slot].is_some() {
            cache_state_digests[slot] = Some(digest);
        } else if digest != [0; 32] {
            return Err(invalid());
        }
        let present = r.take::<1>()?[0];
        let sequence = r.number()?;
        let semantic_digest = r.take::<32>()?;
        let height = r.number()?;
        let ordinal = r.number()?;
        let original = r.take::<32>()?;
        let original_bytes = r.number()?;
        match present {
            1 => {
                last[slot] = Some(CognitionHeadSourceLastV3 {
                    sequence,
                    semantic_digest,
                    location: FactLocationV1 {
                        height,
                        ordinal,
                        original,
                    },
                    original_bytes,
                })
            }
            0 if sequence == 0
                && semantic_digest == [0; 32]
                && height == 0
                && ordinal == 0
                && original == [0; 32]
                && original_bytes == 0 => {}
            _ => return Err(invalid()),
        }
    }
    let source = CognitionHistorySourceSummaryV3 {
        mode,
        cache_state_digests,
        last,
        lifecycle_keys: r.number()?,
        lifecycle_journal_records: r.number()?,
    };
    if r.at != bytes.len() {
        return Err(invalid());
    }
    source.validate_shape(&view, &derived)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok((view, derived, source_charge.retain(Box::new(source))))
}
pub fn decode_cognition_history_view_v3(
    bytes: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<DecodedCognitionHistoryViewV3>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let charge = memory.reserve(
        4096usize.max(std::mem::size_of::<DecodedCognitionHistoryViewV3>()),
        class,
    )?;
    let (view, derived, source) = decode_parts_v3(bytes, memory, class, &mut cancelled)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(DecodedCognitionHistoryViewV3 {
        view,
        derived,
        source,
    }))
}
pub(super) fn decode_profile_with_source_v3(
    bytes: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    cancelled: impl FnMut() -> bool,
) -> Result<CognitionHistoryDecodedProfileV3, ValueTreeError> {
    if bytes.starts_with(COGNITION_HISTORY_VIEW_V3_MAGIC) {
        let (view, derived, source) = decode_parts_v3(bytes, memory, class, cancelled)?;
        Ok((view, Some(derived), Some(source)))
    } else {
        super::manifest_v2::decode_profile(bytes, memory, class, cancelled)
            .map(|(view, derived)| (view, derived, None))
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], ValueTreeError> {
        let end = self.at.checked_add(N).ok_or_else(invalid)?;
        let value = self
            .bytes
            .get(self.at..end)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?;
        self.at = end;
        Ok(value)
    }
    fn number(&mut self) -> Result<u64, ValueTreeError> {
        Ok(u64::from_be_bytes(self.take()?))
    }
}

#[cfg(test)]
#[path = "incremental_cognition_history_manifest_v3_tests.rs"]
mod tests;
