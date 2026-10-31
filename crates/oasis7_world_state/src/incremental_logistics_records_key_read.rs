//! Closed record-local key grammar. Outer IDs are handled by the shared semantic probe.
use super::*;
use crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN;
use std::io::{self, Write};
#[derive(Debug, Clone, Copy)]
pub(super) enum ParsedCell {
    Record,
    Column(LogisticsColumnV1),
    Collection,
    Text(u64),
}
impl ParsedCell {
    pub fn profile_cell(self) -> LogisticsCellV1 {
        match self {
            Self::Record => LogisticsCellV1::Record,
            Self::Column(c) => LogisticsCellV1::Column(c),
            Self::Collection => LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds),
            Self::Text(i) => LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, i),
        }
    }
    pub fn validate(self, kind: LogisticsRecordKindV1) -> Result<(), ValueTreeError> {
        valid_cell(kind.sample(), self.profile_cell())
    }
}
struct Cursor<'a> {
    bytes: &'a [u8],
    used: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ValueTreeError> {
        let end = self
            .used
            .checked_add(n)
            .ok_or(ValueTreeError::Invalid("logistics key length"))?;
        let out = self
            .bytes
            .get(self.used..end)
            .ok_or(ValueTreeError::Invalid("logistics key framing"))?;
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
            return Err(ValueTreeError::Invalid("logistics key text token"));
        }
        let n = usize::try_from(self.number()?)
            .map_err(|_| ValueTreeError::Invalid("logistics key name"))?;
        std::str::from_utf8(self.take(n)?)
            .map_err(|_| ValueTreeError::Invalid("logistics key UTF-8"))
    }
}
fn parse(bytes: &[u8]) -> Result<ParsedCell, ValueTreeError> {
    let invalid = || ValueTreeError::Invalid("logistics relative key whitelist");
    let mut c = Cursor { bytes, used: 0 };
    if c.take(SEMANTIC_KEY_DOMAIN.len())? != SEMANTIC_KEY_DOMAIN {
        return Err(invalid());
    }
    let mut depth = 0;
    let mut tag = c.byte()?;
    while tag == 5 {
        depth += 1;
        if depth > 3 {
            return Err(invalid());
        }
        tag = c.byte()?;
    }
    if tag != 0 {
        return Err(invalid());
    }
    let cell = match depth {
        0 => ParsedCell::Record,
        2 => {
            if c.name()? != "record" {
                return Err(invalid());
            }
            let name = c.name()?;
            ParsedCell::Column(
                LogisticsColumnV1::ALL
                    .into_iter()
                    .find(|column| column.path().1 == name)
                    .ok_or_else(invalid)?,
            )
        }
        3 => {
            if c.name()? != "record" || c.name()? != "route_ids" {
                return Err(invalid());
            }
            match c.byte()? {
                0 => ParsedCell::Collection,
                2 => ParsedCell::Text(c.number()?),
                _ => return Err(invalid()),
            }
        }
        _ => return Err(invalid()),
    };
    if c.used != bytes.len() {
        return Err(invalid());
    }
    Ok(cell)
}
pub(super) struct CellKeyDecoder {
    bytes: [u8; 256],
    used: usize,
    pub bad: Option<&'static str>,
}
impl CellKeyDecoder {
    pub fn new() -> Self {
        Self {
            bytes: [0; 256],
            used: 0,
            bad: None,
        }
    }
    pub fn finish(&self) -> Result<ParsedCell, ValueTreeError> {
        if let Some(reason) = self.bad {
            return Err(ValueTreeError::Invalid(reason));
        }
        parse(&self.bytes[..self.used])
    }
}
impl Write for CellKeyDecoder {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(end) = self
            .used
            .checked_add(bytes.len())
            .filter(|end| *end <= self.bytes.len())
        else {
            self.bad = Some("logistics fixed relative key allowance");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "logistics fixed relative key allowance",
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
