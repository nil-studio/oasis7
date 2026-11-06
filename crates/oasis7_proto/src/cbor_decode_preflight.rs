//! Allocation-free definite-CBOR structural accounting. This is not a typed
//! decoder or a heap bound: closed schema owners must supply proven layouts.
#[derive(Debug, Clone, Copy, Default)]
pub struct CborShapeV1 {
    pub values: u64,
    pub string_bytes: u64,
    pub byte_strings: u64,
    pub sequence_entries: u64,
    pub map_entries: u64,
    pub max_depth: usize,
}
pub struct CborPreflightWorkV1 {
    pub bytes_remaining: u64,
    pub values_remaining: u64,
    pub max_depth: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CborPreflightErrorV1 {
    Invalid,
    ResourceLimited,
    Cancelled,
}
/// Rejects indefinite values/tags before a serde scratch buffer can grow. Every
/// declared length must fit the actual remaining input before conversion/use.
pub fn preflight_definite_cbor_v1(
    bytes: &[u8],
    work: &mut CborPreflightWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<CborShapeV1, CborPreflightErrorV1> {
    use CborPreflightErrorV1 as Error;
    struct Scan<'a, 'b, C> {
        bytes: &'a [u8],
        at: usize,
        work: &'b mut CborPreflightWorkV1,
        cancelled: &'b mut C,
        shape: CborShapeV1,
    }
    impl<C: FnMut() -> bool> Scan<'_, '_, C> {
        fn take(&mut self, n: usize) -> Result<&[u8], Error> {
            if (self.cancelled)() {
                return Err(Error::Cancelled);
            }
            let end = self
                .at
                .checked_add(n)
                .filter(|n| *n <= self.bytes.len())
                .ok_or(Error::Invalid)?;
            self.work.bytes_remaining = self
                .work
                .bytes_remaining
                .checked_sub(n as u64)
                .ok_or(Error::ResourceLimited)?;
            let start = self.at;
            self.at = end;
            Ok(&self.bytes[start..end])
        }
        fn value(&mut self, depth: usize) -> Result<(), Error> {
            if depth > self.work.max_depth.min(64) {
                return Err(Error::ResourceLimited);
            }
            self.shape.max_depth = self.shape.max_depth.max(depth);
            self.work.values_remaining = self
                .work
                .values_remaining
                .checked_sub(1)
                .ok_or(Error::ResourceLimited)?;
            self.shape.values = self
                .shape
                .values
                .checked_add(1)
                .ok_or(Error::ResourceLimited)?;
            let initial = self.take(1)?[0];
            let major = initial >> 5;
            let ai = initial & 31;
            let n =
                match ai {
                    0..=23 => ai as u64,
                    24 => self.take(1)?[0] as u64,
                    25 => u16::from_be_bytes(self.take(2)?.try_into().map_err(|_| Error::Invalid)?)
                        as u64,
                    26 => u32::from_be_bytes(self.take(4)?.try_into().map_err(|_| Error::Invalid)?)
                        as u64,
                    27 => u64::from_be_bytes(self.take(8)?.try_into().map_err(|_| Error::Invalid)?),
                    _ => return Err(Error::Invalid),
                };
            match major {
                0 | 1 => Ok(()),
                2 | 3 => {
                    if major == 2 {
                        self.shape.byte_strings = self
                            .shape
                            .byte_strings
                            .checked_add(1)
                            .ok_or(Error::ResourceLimited)?;
                    }
                    let count = usize::try_from(n).map_err(|_| Error::ResourceLimited)?;
                    self.shape.string_bytes = self
                        .shape
                        .string_bytes
                        .checked_add(n)
                        .ok_or(Error::ResourceLimited)?;
                    let data = self.take(count)?;
                    if major == 3 {
                        std::str::from_utf8(data).map_err(|_| Error::Invalid)?;
                    }
                    Ok(())
                }
                4 | 5 => {
                    let items = if major == 5 {
                        self.shape.map_entries = self
                            .shape
                            .map_entries
                            .checked_add(n)
                            .ok_or(Error::ResourceLimited)?;
                        n.checked_mul(2).ok_or(Error::ResourceLimited)?
                    } else {
                        self.shape.sequence_entries = self
                            .shape
                            .sequence_entries
                            .checked_add(n)
                            .ok_or(Error::ResourceLimited)?;
                        n
                    };
                    if items > self.work.values_remaining
                        || items > (self.bytes.len() - self.at) as u64
                    {
                        return Err(Error::ResourceLimited);
                    }
                    for _ in 0..items {
                        self.value(depth + 1)?;
                    }
                    Ok(())
                }
                7 if matches!(ai,20..=22|25..=27) => Ok(()),
                _ => Err(Error::Invalid),
            }
        }
    }
    let mut scan = Scan {
        bytes,
        at: 0,
        work,
        cancelled: &mut cancelled,
        shape: CborShapeV1::default(),
    };
    scan.value(0)?;
    if scan.at != bytes.len() {
        return Err(Error::Invalid);
    }
    if (scan.cancelled)() {
        return Err(Error::Cancelled);
    }
    Ok(scan.shape)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn work() -> CborPreflightWorkV1 {
        CborPreflightWorkV1 {
            bytes_remaining: 1 << 20,
            values_remaining: 1000,
            max_depth: 32,
        }
    }
    #[test]
    fn definite_shape_rejects_huge_hints_indefinite_tail_and_cancel() {
        for bad in [
            &[0x9b, 255, 255, 255, 255, 255, 255, 255, 255][..],
            &[0x7f, 0xff],
            &[0x01, 0x02],
            &[0x63, b'a'],
        ] {
            assert!(preflight_definite_cbor_v1(bad, &mut work(), || false).is_err());
        }
        assert!(matches!(
            preflight_definite_cbor_v1(&[0], &mut work(), || true),
            Err(CborPreflightErrorV1::Cancelled)
        ));
    }
    #[test]
    fn actual_container_and_utf8_storage_are_counted_without_values() {
        let shape =
            preflight_definite_cbor_v1(b"\xa1\x61x\x82\x01\x62ab", &mut work(), || false).unwrap();
        assert_eq!(shape.values, 5);
        assert_eq!(shape.map_entries, 1);
        assert_eq!(shape.sequence_entries, 2);
        assert_eq!(shape.string_bytes, 3);
    }
}
