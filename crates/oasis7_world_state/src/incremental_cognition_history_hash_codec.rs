//! Fixed-width persistence of the existing original-hash frontier. Decoding
//! restores a resource carrier only; accepted/current binding is separate.
use super::*;
use std::io::{Read, Write};
const MAGIC: &[u8] = b"oasis7.cognition.head-cache.v1\0";
pub const COGNITION_HEAD_CACHE_BYTES_V1: usize =
    MAGIC.len() + 8 + CHUNK + ALIGNMENTS * (1 + 2 + 8 + 2 + CHUNK + 1 + SPANS * 41);
fn spend(work: &mut CognitionHeadHashWorkV1, n: usize) -> Result<(), ValueTreeError> {
    work.input_bytes_remaining = work
        .input_bytes_remaining
        .checked_sub(n as u64)
        .ok_or(ValueTreeError::ResourceLimited("head cache codec bytes"))?;
    Ok(())
}
impl CognitionHeadHashStateV1 {
    pub fn write_canonical_cache_v1(
        &self,
        output: &mut impl Write,
        work: &mut CognitionHeadHashWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        let mut put = |bytes: &[u8]| -> Result<(), ValueTreeError> {
            check(&mut cancelled)?;
            spend(work, bytes.len())?;
            output.write_all(bytes).map_err(ValueTreeError::Io)?;
            check(&mut cancelled)
        };
        put(MAGIC)?;
        put(&self.data.body_bytes.to_be_bytes())?;
        put(&self.data.first)?;
        for row in &self.data.rows {
            row.validate(self.data.body_bytes)?;
            put(&[row.kind as u8])?;
            put(&row.prefix_len.to_be_bytes())?;
            put(&row.complete.to_be_bytes())?;
            put(&row.tail_len.to_be_bytes())?;
            put(&row.tail[..usize::from(row.tail_len)])?;
            put(&[0; CHUNK][..CHUNK - usize::from(row.tail_len)])?;
            put(&[row.used])?;
            for i in 0..SPANS {
                let span = if i < usize::from(row.used) {
                    row.spans[i]
                } else {
                    Span::EMPTY
                };
                put(&span.start.to_be_bytes())?;
                put(&[span.height])?;
                put(&span.cv)?;
            }
        }
        Ok(())
    }
}
pub fn read_cognition_head_cache_v1(
    input: &mut impl Read,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut CognitionHeadHashWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CognitionHeadHashStateV1>, ValueTreeError> {
    check(&mut cancelled)?;
    let charge = memory.reserve(CognitionHeadHashBuilderV1::admission_bytes(), class)?;
    let mut data = Box::new(Data::EMPTY);
    alignment::initialize(&mut data)?;
    let mut get = |bytes: &mut [u8]| -> Result<(), ValueTreeError> {
        check(&mut cancelled)?;
        spend(work, bytes.len())?;
        input.read_exact(bytes).map_err(ValueTreeError::Io)?;
        check(&mut cancelled)
    };
    let mut magic = [0u8; MAGIC.len()];
    get(&mut magic)?;
    if magic != MAGIC {
        return Err(invalid());
    }
    let mut number = [0; 8];
    get(&mut number)?;
    data.body_bytes = u64::from_be_bytes(number);
    get(&mut data.first)?;
    if data.body_bytes < CHUNK as u64
        && data.first[data.body_bytes as usize..]
            .iter()
            .any(|b| *b != 0)
    {
        return Err(invalid());
    }
    for row in &mut data.rows {
        let expected_kind = row.kind;
        let expected_prefix = row.prefix_len;
        let mut byte = [0; 1];
        get(&mut byte)?;
        if byte[0] != expected_kind as u8 {
            return Err(invalid());
        }
        let mut two = [0; 2];
        get(&mut two)?;
        if u16::from_be_bytes(two) != expected_prefix {
            return Err(invalid());
        }
        get(&mut number)?;
        row.complete = u64::from_be_bytes(number);
        get(&mut two)?;
        row.tail_len = u16::from_be_bytes(two);
        if usize::from(row.tail_len) > CHUNK {
            return Err(invalid());
        }
        get(&mut row.tail)?;
        if row.tail[usize::from(row.tail_len)..]
            .iter()
            .any(|b| *b != 0)
        {
            return Err(invalid());
        }
        get(&mut byte)?;
        row.used = byte[0];
        if usize::from(row.used) > SPANS {
            return Err(invalid());
        }
        for i in 0..SPANS {
            get(&mut number)?;
            let start = u64::from_be_bytes(number);
            get(&mut byte)?;
            let height = byte[0];
            let mut cv = [0; 32];
            get(&mut cv)?;
            if i >= usize::from(row.used) && (start != 0 || height != 0 || cv != [0; 32]) {
                return Err(invalid());
            }
            row.spans[i] = Span { start, height, cv };
        }
        row.validate(data.body_bytes)?;
    }
    check(&mut cancelled)?;
    let mut extra = [0];
    spend(work, 1)?;
    if input.read(&mut extra).map_err(ValueTreeError::Io)? != 0 {
        return Err(invalid());
    }
    check(&mut cancelled)?;
    Ok(charge.retain(CognitionHeadHashStateV1 { data, class }))
}
