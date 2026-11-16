//! Compare the original complete wrapper images, excluding only journal_head.
//! The serializer borrows the original parsed Values; no shadow response DTO.
use super::*;
use serde::ser::{Serialize, SerializeMap, Serializer};
use std::io::{self, Write};
struct WithoutHead<'a>(&'a serde_json::Map<String, serde_json::Value>);
impl Serialize for WithoutHead<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        for (k, v) in self.0 {
            if k != "journal_head" {
                m.serialize_entry(k, v)?
            }
        }
        m.end()
    }
}
struct ImageWriter<'a> {
    output: Option<&'a mut Vec<u8>>,
    expected: &'a [u8],
    at: usize,
    capacity: usize,
    equal: bool,
    work: &'a mut BusinessDiffProcessingWorkV1,
    stop: &'a mut dyn FnMut() -> bool,
    error: Option<ValueTreeError>,
}
impl Write for ImageWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result =
            (|| {
                if (self.stop)() {
                    return Err(ValueTreeError::Cancelled);
                }
                let n = u64::try_from(bytes.len()).map_err(|_| bad())?;
                self.work.steps_remaining = self
                    .work
                    .steps_remaining
                    .checked_sub(n.checked_add(1).ok_or_else(bad)?)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "Responses comparison steps",
                    ))?;
                self.work.bytes_remaining = self.work.bytes_remaining.checked_sub(n).ok_or(
                    ValueTreeError::ResourceLimited("Responses comparison bytes"),
                )?;
                let end = self.at.checked_add(bytes.len()).ok_or_else(bad)?;
                if let Some(out) = &mut self.output {
                    if end > self.capacity {
                        return Err(bad());
                    }
                    out.extend_from_slice(bytes)
                } else {
                    self.equal &= self.expected.get(self.at..end) == Some(bytes)
                }
                self.at = end;
                Ok(bytes.len())
            })();
        match result {
            Ok(n) => Ok(n),
            Err(e) => {
                self.error = Some(e);
                Err(io::Error::other("Responses image work"))
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn compare_response_change(
    before: &serde_json::Value,
    after: &serde_json::Value,
    source_bytes: usize,
    memory: &MemoryBudget,
    work: &mut BusinessDiffProcessingWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    let before = before.as_object().ok_or_else(bad)?;
    let after = after.as_object().ok_or_else(bad)?;
    if !after.contains_key("journal_head") {
        return Err(bad());
    }
    // JSON escaping is <=6 bytes per input UTF8 byte; CBOR scalar images,
    // punctuation and finite numeric images fit this conservative paid bound.
    let capacity = source_bytes
        .checked_mul(12)
        .and_then(|n| n.checked_add(512))
        .ok_or_else(bad)?;
    work.steps_remaining =
        work.steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "Responses comparison allocation work",
            ))?;
    let _fee = memory.reserve(
        capacity
            .checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(bad)?,
        MemoryClass::Ordinary,
    )?;
    let mut image = Vec::new();
    image
        .try_reserve_exact(capacity)
        .map_err(|_| ValueTreeError::ResourceLimited("Responses comparison allocation"))?;
    {
        let mut writer = ImageWriter {
            output: Some(&mut image),
            expected: &[],
            at: 0,
            capacity,
            equal: true,
            work,
            stop,
            error: None,
        };
        let encoded = serde_json::to_writer(&mut writer, &WithoutHead(before));
        if let Some(e) = writer.error {
            return Err(e);
        }
        if encoded.is_err() {
            return Err(bad());
        }
    }
    let mut writer = ImageWriter {
        output: None,
        expected: &image,
        at: 0,
        capacity: 0,
        equal: true,
        work,
        stop,
        error: None,
    };
    let encoded = serde_json::to_writer(&mut writer, &WithoutHead(after));
    if let Some(e) = writer.error {
        return Err(e);
    }
    if encoded.is_err() || !writer.equal || writer.at != image.len() {
        return Err(ValueTreeError::Invalid(
            "Responses replacement changed immutable wrapper",
        ));
    }
    Ok(())
}
#[cfg(test)]
#[path = "incremental_cognition_history_current_responses_compare_tests.rs"]
mod tests;
