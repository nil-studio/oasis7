//! Extract only the five original strings, without a JSON value or H buffer.
//! This is not a schema proof: the caller first authenticates canonical CBOR,
//! then the lifecycle receiver compares these exact strings to its projection.
use super::*;
use crate::incremental_cognition_history_lifecycle::COGNITION_LIFECYCLE_KEY_FIELDS_V1;
use oasis7_proto::memory_budget::MemoryBudget;
struct Keys<'a, R, C> {
    input: R,
    remaining: u64,
    work: &'a mut CognitionHistoryCborWorkV1,
    cancelled: C,
}
impl<R: Read, C: FnMut() -> bool> Keys<'_, R, C> {
    fn bytes(&mut self, out: &mut [u8]) -> Result<(), ValueTreeError> {
        let mut used = 0;
        while used < out.len() {
            if (self.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            self.work.source_reads_remaining =
                self.work.source_reads_remaining.checked_sub(1).ok_or(
                    ValueTreeError::ResourceLimited("Cognition key source reads"),
                )?;
            let end = (used + 8192).min(out.len());
            let allowed = (end - used) as u64;
            if allowed > self.remaining || allowed > self.work.bytes_remaining {
                return Err(ValueTreeError::ResourceLimited(
                    "Cognition key source bytes",
                ));
            }
            let n = match self.input.read(&mut out[used..end]) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                other => other.map_err(ValueTreeError::Io)?,
            };
            if n == 0 || n > end - used {
                return Err(invalid());
            }
            self.remaining -= n as u64;
            self.work.bytes_remaining -= n as u64;
            used += n;
        }
        Ok(())
    }
    fn discard(&mut self, mut n: u64) -> Result<(), ValueTreeError> {
        let mut scratch = [0; 8192];
        while n != 0 {
            let take = n.min(scratch.len() as u64) as usize;
            self.bytes(&mut scratch[..take])?;
            n -= take as u64;
        }
        Ok(())
    }
    fn header(&mut self) -> Result<(u8, u64), ValueTreeError> {
        self.work.nodes_remaining =
            self.work
                .nodes_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Cognition key source nodes",
                ))?;
        let mut byte = [0];
        self.bytes(&mut byte)?;
        let major = byte[0] >> 5;
        let low = byte[0] & 31;
        let width = match low {
            0..=23 => 0,
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => return Err(invalid()),
        };
        let mut buf = [0; 8];
        self.bytes(&mut buf[8 - width..])?;
        let value = if width == 0 {
            u64::from(low)
        } else {
            u64::from_be_bytes(buf)
        };
        Ok((major, value))
    }
    fn skip(&mut self, major: u8, n: u64, depth: usize) -> Result<(), ValueTreeError> {
        if depth > self.work.max_depth {
            return Err(ValueTreeError::ResourceLimited("Cognition key depth"));
        }
        match major {
            0 | 1 | 7 => Ok(()),
            2 | 3 => self.discard(n),
            4 | 5 => {
                let count = if major == 5 {
                    n.checked_mul(2).ok_or_else(invalid)?
                } else {
                    n
                };
                for _ in 0..count {
                    let (m, n) = self.header()?;
                    self.skip(m, n, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(invalid()),
        }
    }
}
pub(super) fn read_keys<R: Read, C: FnMut() -> bool>(
    input: R,
    bytes: u64,
    memory: &MemoryBudget,
    work: &mut CognitionHistoryCborWorkV1,
    cancelled: C,
) -> Result<[ReservedValue<String>; 5], ValueTreeError> {
    let empty = || -> Result<ReservedValue<String>, ValueTreeError> {
        Ok(memory
            .reserve(std::mem::size_of::<String>(), MemoryClass::Ordinary)?
            .retain(String::new()))
    };
    let mut result = [empty()?, empty()?, empty()?, empty()?, empty()?];
    let mut parser = Keys {
        input,
        remaining: bytes,
        work,
        cancelled,
    };
    let (major, count) = parser.header()?;
    if major != 5 {
        return Err(invalid());
    }
    for _ in 0..count {
        let (major, len) = parser.header()?;
        if major != 3 {
            return Err(invalid());
        }
        let mut name = [0; 64];
        let index = if len <= name.len() as u64 {
            let n = len as usize;
            parser.bytes(&mut name[..n])?;
            COGNITION_LIFECYCLE_KEY_FIELDS_V1
                .iter()
                .position(|key| key.as_bytes() == &name[..n])
        } else {
            parser.discard(len)?;
            None
        };
        let (major, len) = parser.header()?;
        if let Some(index) = index.filter(|_| major == 3) {
            let n = usize::try_from(len)
                .map_err(|_| ValueTreeError::ResourceLimited("Cognition original key length"))?;
            parser.work.key_bytes_remaining =
                parser.work.key_bytes_remaining.checked_sub(len).ok_or(
                    ValueTreeError::ResourceLimited("Cognition original key bytes"),
                )?;
            let fee = n
                .checked_add(std::mem::size_of::<String>())
                .ok_or_else(invalid)?;
            let charge = memory.reserve(fee, MemoryClass::Ordinary)?;
            let mut text = vec![0; n];
            parser.bytes(&mut text)?;
            result[index] = charge.retain(String::from_utf8(text).map_err(|_| invalid())?);
        } else {
            parser.skip(major, len, 1)?;
        }
    }
    if parser.remaining != 0 {
        return Err(invalid());
    }
    if (parser.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn work() -> CognitionHistoryCborWorkV1 {
        CognitionHistoryCborWorkV1 {
            bytes_remaining: 1 << 20,
            nodes_remaining: 10000,
            source_reads_remaining: 10000,
            key_bytes_remaining: 1 << 20,
            max_depth: 32,
            max_key_bytes: 4096,
        }
    }
    #[test]
    fn original_key_reader_preserves_exact_utf8_and_sparse_defaults() {
        let value = serde_json::json!({"agent_id":"  agent 界  ","agent_session_id":null,
            "agent_turn_id":7,"decision_request_id":"","request_digest":"digest",
            "unknown":{"agent_session_id":"nested must not escape","n":[1.5,-2,true,null]}});
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&value).unwrap();
        let memory = MemoryBudget::new(1 << 20, 1 << 19).unwrap();
        let keys = read_keys(
            bytes.as_slice(),
            bytes.len() as u64,
            &memory,
            &mut work(),
            || false,
        )
        .unwrap();
        assert_eq!(
            std::array::from_fn::<_, 5, _>(|i| keys[i].as_str()),
            ["  agent 界  ", "", "", "", "digest"]
        );
        assert!(memory.used() > 0);
        drop(keys);
        assert_eq!(memory.used(), 0);
        let mut limited = work();
        limited.key_bytes_remaining = 0;
        assert!(matches!(
            read_keys(
                bytes.as_slice(),
                bytes.len() as u64,
                &memory,
                &mut limited,
                || false
            ),
            Err(ValueTreeError::ResourceLimited(_))
        ));
        assert_eq!(memory.used(), 0);
        assert!(matches!(
            read_keys(
                bytes.as_slice(),
                bytes.len() as u64,
                &memory,
                &mut work(),
                || true
            ),
            Err(ValueTreeError::Cancelled)
        ));
        assert_eq!(memory.used(), 0);
    }
}
