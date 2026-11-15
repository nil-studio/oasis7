//! Allocation-free guard for the EXISTING closed physical metadata schemas.
//! Valid recipe/generation map keys are <=32B, text scalars <=64B, maps <=16
//! fields, and the largest existing tuple inventory has 65536 entries. The
//! recipe envelope body is opaque bytes and is checked separately after its
//! checksum. Rejecting longer unknown keys before serde prevents Debug/error
//! escaping from allocating in proportion to hostile control-character input.
use super::*;
pub(super) fn check(bytes: &[u8], max_bytes: usize) -> Result<(), FormalError> {
    struct Scan<'a> {
        bytes: &'a [u8],
        at: usize,
        nodes: usize,
    }
    impl Scan<'_> {
        fn take(&mut self, n: usize) -> Result<&[u8], FormalError> {
            let end = self
                .at
                .checked_add(n)
                .filter(|end| *end <= self.bytes.len())
                .ok_or_else(|| invalid("physical metadata truncated"))?;
            let start = self.at;
            self.at = end;
            Ok(&self.bytes[start..end])
        }
        fn value(&mut self, depth: usize, key: bool) -> Result<(), FormalError> {
            if depth > 64 || self.nodes == 0 {
                return Err(invalid("physical metadata structural bound"));
            }
            self.nodes -= 1;
            let first = self.take(1)?[0];
            let major = first >> 5;
            let ai = first & 31;
            let n = match ai {
                0..=23 => u64::from(ai),
                24 => u64::from(self.take(1)?[0]),
                25 => u64::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap())),
                26 => u64::from(u32::from_be_bytes(self.take(4)?.try_into().unwrap())),
                27 => u64::from_be_bytes(self.take(8)?.try_into().unwrap()),
                _ => return Err(invalid("physical metadata indefinite/tag")),
            };
            if key && major != 3 {
                return Err(invalid("physical metadata nontext field"));
            }
            match major {
                0 | 1 | 7 => Ok(()),
                2 => {
                    let n =
                        usize::try_from(n).map_err(|_| invalid("physical metadata bytes bound"))?;
                    self.take(n)?;
                    Ok(())
                }
                3 => {
                    if n > if key { 32 } else { 64 } {
                        return Err(invalid("physical metadata text bound"));
                    }
                    let raw = self.take(n as usize)?;
                    std::str::from_utf8(raw).map_err(|_| invalid("physical metadata UTF8"))?;
                    Ok(())
                }
                4 => {
                    if n > 65536 || n > self.bytes.len().saturating_sub(self.at) as u64 {
                        return Err(invalid("physical metadata array bound"));
                    }
                    for _ in 0..n {
                        self.value(depth + 1, false)?;
                    }
                    Ok(())
                }
                5 => {
                    if n > 16 {
                        return Err(invalid("physical metadata map bound"));
                    }
                    for _ in 0..n {
                        self.value(depth + 1, true)?;
                        self.value(depth + 1, false)?;
                    }
                    Ok(())
                }
                _ => Err(invalid("physical metadata unsupported type")),
            }
        }
    }
    if bytes.len() > max_bytes {
        return Err(invalid("physical metadata input bound"));
    }
    let mut scan = Scan {
        bytes,
        at: 0,
        nodes: bytes.len(),
    };
    scan.value(0, false)?;
    if scan.at != bytes.len() {
        return Err(invalid("physical metadata trailing bytes"));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn hostile_unknown_key(length: usize) -> Vec<u8> {
    let mut bytes = vec![0xa1, 0x7a];
    bytes.extend_from_slice(&(length as u32).to_be_bytes());
    bytes.resize(bytes.len() + length, 0x1f);
    bytes.push(0xf6);
    bytes
}
