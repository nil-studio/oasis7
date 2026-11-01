//! Bounded grammar of record-local keys; never buffers original outer IDs.
use super::*;
use crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN;
use std::io::{self, Write};
pub(super) struct CellKeyProbe {
    bytes: [u8; 128],
    used: usize,
    pub bad: bool,
}
impl CellKeyProbe {
    pub fn new() -> Self {
        Self {
            bytes: [0; 128],
            used: 0,
            bad: false,
        }
    }
    pub fn finish(
        &self,
        kind: FactoryAuthorityRecordKindV1,
    ) -> Result<FactoryAuthorityCellV1, ValueTreeError> {
        if self.bad {
            return Err(read::invalid());
        }
        let mut c = Cursor {
            bytes: &self.bytes[..self.used],
            used: 0,
        };
        if c.take(SEMANTIC_KEY_DOMAIN.len())? != SEMANTIC_KEY_DOMAIN {
            return Err(read::invalid());
        }
        let cell = match c.byte()? {
            0 => FactoryAuthorityCellV1::Record,
            5 => {
                let tag = c.byte()?;
                let indexed = tag == 5;
                if indexed {
                    if c.byte()? != 0 {
                        return Err(read::invalid());
                    }
                } else if tag != 0 {
                    return Err(read::invalid());
                }
                let name = c.name()?;
                if indexed {
                    if name != "authorized_agent_ids" || c.byte()? != 2 {
                        return Err(read::invalid());
                    }
                    FactoryAuthorityCellV1::AuthorizedAgent(c.number()?)
                } else {
                    FactoryAuthorityCellV1::FIXED
                        .into_iter()
                        .find(|cell| *cell != FactoryAuthorityCellV1::Record && cell.name() == name)
                        .ok_or_else(read::invalid)?
                }
            }
            _ => return Err(read::invalid()),
        };
        if c.used != self.used || !cell.accepts(kind) {
            return Err(read::invalid());
        }
        Ok(cell)
    }
}
struct Cursor<'a> {
    bytes: &'a [u8],
    used: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ValueTreeError> {
        let end = self.used.checked_add(n).ok_or_else(read::invalid)?;
        let out = self.bytes.get(self.used..end).ok_or_else(read::invalid)?;
        self.used = end;
        Ok(out)
    }
    fn byte(&mut self) -> Result<u8, ValueTreeError> {
        Ok(self.take(1)?[0])
    }
    fn number(&mut self) -> Result<u64, ValueTreeError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn name(&mut self) -> Result<&'a str, ValueTreeError> {
        if self.byte()? != 1 {
            return Err(read::invalid());
        }
        let n = usize::try_from(self.number()?).map_err(|_| read::invalid())?;
        std::str::from_utf8(self.take(n)?).map_err(|_| read::invalid())
    }
}
impl Write for CellKeyProbe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(end) = self
            .used
            .checked_add(bytes.len())
            .filter(|n| *n <= self.bytes.len())
        else {
            self.bad = true;
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "factory authority key bound",
            ));
        };
        self.bytes[self.used..end].copy_from_slice(bytes);
        self.used = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn text(name: &str) -> Vec<u8> {
        let mut out = vec![1];
        out.extend_from_slice(&(name.len() as u64).to_be_bytes());
        out.extend_from_slice(name.as_bytes());
        out
    }
    fn fixed(name: &str) -> Vec<u8> {
        let mut out = b"oasis7.iws.semantic-key.v1\0".to_vec();
        out.extend_from_slice(&[5, 0]);
        out.extend(text(name));
        out
    }
    fn indexed(name: &str, tag: u8) -> Vec<u8> {
        let mut out = b"oasis7.iws.semantic-key.v1\0".to_vec();
        out.extend_from_slice(&[5, 5, 0]);
        out.extend(text(name));
        out.push(tag);
        out.extend_from_slice(&u64::MAX.to_be_bytes());
        out
    }
    #[test]
    fn exact_relative_key_grammar_has_independent_literal_frames() {
        let mut record = b"oasis7.iws.semantic-key.v1\0".to_vec();
        record.push(0);
        for (bytes, kind, expected) in [
            (
                record,
                FactoryAuthorityRecordKindV1::Site,
                FactoryAuthorityCellV1::Record,
            ),
            (
                fixed("active"),
                FactoryAuthorityRecordKindV1::Power,
                FactoryAuthorityCellV1::Active,
            ),
            (
                fixed("source_module_id"),
                FactoryAuthorityRecordKindV1::Power,
                FactoryAuthorityCellV1::SourceModuleId,
            ),
            (
                indexed("authorized_agent_ids", 2),
                FactoryAuthorityRecordKindV1::Site,
                FactoryAuthorityCellV1::AuthorizedAgent(u64::MAX),
            ),
        ] {
            let mut probe = CellKeyProbe::new();
            for chunk in bytes.chunks(1) {
                probe.write_all(chunk).unwrap();
            }
            assert_eq!(probe.finish(kind).unwrap(), expected);
        }
        let mut trailing = fixed("active");
        trailing.push(0);
        for bytes in [
            fixed("unknown"),
            fixed("record"),
            fixed("source_module_id"),
            indexed("authorized_agent_ids", 3),
            indexed("unknown", 2),
            trailing,
        ] {
            let mut probe = CellKeyProbe::new();
            probe.write_all(&bytes).unwrap();
            assert!(probe.finish(FactoryAuthorityRecordKindV1::Site).is_err());
        }
        let mut probe = CellKeyProbe::new();
        assert!(probe.write_all(&[0; 129]).is_err());
        assert!(probe.finish(FactoryAuthorityRecordKindV1::Site).is_err());
    }
}
