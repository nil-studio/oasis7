//! Closed relative-key parsing with fixed prefix storage and streaming UTF-8.
use super::*;
use crate::{incremental_semantic_key::SEMANTIC_KEY_DOMAIN, incremental_utf8_output::Utf8Output};
use std::io::{self, Write};
#[derive(Debug, Clone, Copy)]
pub(super) enum ParsedCell {
    Record,
    Column(IndustryColumnV1),
    Collection(IndustryCollectionV1),
    Text(IndustryCollectionV1, u64),
    Stack(IndustryCollectionV1, u64, IndustryStackColumnV1),
    Balance(IndustryCollectionV1),
}
impl ParsedCell {
    pub fn profile_cell(self) -> IndustryCellV1<'static> {
        match self {
            Self::Record => IndustryCellV1::Record,
            Self::Column(c) => IndustryCellV1::Column(c),
            Self::Collection(c) => IndustryCellV1::Collection(c),
            Self::Text(c, i) => IndustryCellV1::TextItem(c, i),
            Self::Stack(c, i, p) => IndustryCellV1::StackItem(c, i, p),
            Self::Balance(c) => IndustryCellV1::Balance(c, ""),
        }
    }
    // The empty text above selects ONLY the I64 value profile. Identity is the
    // fully parsed original member preimage authenticated by the actual key.
    pub fn validate(self, kind: IndustryRecordKindV1) -> Result<(), ValueTreeError> {
        valid_cell(kind.sample(), self.profile_cell())
    }
}
enum ParseError {
    More,
    Bad(&'static str),
}
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        let end = self
            .offset
            .checked_add(n)
            .ok_or(ParseError::Bad("industry key framing"))?;
        let bytes = self.bytes.get(self.offset..end).ok_or(ParseError::More)?;
        self.offset = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }
    fn integer(&mut self) -> Result<u64, ParseError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn name(&mut self) -> Result<&'a str, ParseError> {
        if self.byte()? != 1 {
            return Err(ParseError::Bad("industry static key token"));
        }
        let n = self.integer()?;
        if n > 128 {
            return Err(ParseError::Bad("industry static key length"));
        }
        std::str::from_utf8(self.take(n as usize)?)
            .map_err(|_| ParseError::Bad("industry static key UTF-8"))
    }
}
enum Head {
    Exact(ParsedCell, usize),
    Map(ParsedCell, usize, u64),
}
fn head(bytes: &[u8]) -> Result<Head, ParseError> {
    let common = bytes.len().min(SEMANTIC_KEY_DOMAIN.len());
    if bytes[..common] != SEMANTIC_KEY_DOMAIN[..common] {
        return Err(ParseError::Bad("industry key domain"));
    }
    let mut c = Cursor { bytes, offset: 0 };
    c.take(SEMANTIC_KEY_DOMAIN.len())?;
    let mut depth = 0;
    let mut tag = c.byte()?;
    while tag == 5 {
        depth += 1;
        if depth > 4 {
            return Err(ParseError::Bad("industry key depth"));
        }
        tag = c.byte()?;
    }
    if tag != 0 {
        return Err(ParseError::Bad("industry relative key root"));
    }
    if depth == 0 {
        return Ok(Head::Exact(ParsedCell::Record, c.offset));
    }
    if !matches!(depth, 2..=4) {
        return Err(ParseError::Bad("industry relative key path"));
    }
    let section = c.name()?;
    let field = c.name()?;
    if depth == 2 {
        let column = IndustryColumnV1::ALL
            .into_iter()
            .find(|column| (column.path().0.name(), column.path().1) == (section, field))
            .ok_or(ParseError::Bad("industry column whitelist"))?;
        return Ok(Head::Exact(ParsedCell::Column(column), c.offset));
    }
    let collection = IndustryCollectionV1::ALL
        .into_iter()
        .find(|collection| collection.path() == (section, field))
        .ok_or(ParseError::Bad("industry collection whitelist"))?;
    if depth == 4 {
        if c.byte()? != 2 {
            return Err(ParseError::Bad("industry stack index token"));
        }
        let index = c.integer()?;
        let part = match c.name()? {
            "kind" => IndustryStackColumnV1::Kind,
            "amount" => IndustryStackColumnV1::Amount,
            _ => return Err(ParseError::Bad("industry stack column")),
        };
        return Ok(Head::Exact(
            ParsedCell::Stack(collection, index, part),
            c.offset,
        ));
    }
    match c.byte()? {
        0 => Ok(Head::Exact(ParsedCell::Collection(collection), c.offset)),
        2 => {
            let index = c.integer()?;
            Ok(Head::Exact(ParsedCell::Text(collection, index), c.offset))
        }
        1 => {
            let length = c.integer()?;
            Ok(Head::Map(ParsedCell::Balance(collection), c.offset, length))
        }
        _ => Err(ParseError::Bad("industry member key token")),
    }
}
pub(super) struct CellKeyDecoder<'a, W> {
    prefix: [u8; 512],
    used: usize,
    remaining: Option<u64>,
    cell: Option<ParsedCell>,
    text: Utf8Output<'a, W>,
    done: bool,
    pub bad: Option<&'static str>,
}
impl<'a, W: Write> CellKeyDecoder<'a, W> {
    pub fn new(output: &'a mut W) -> Self {
        Self {
            prefix: [0; 512],
            used: 0,
            remaining: None,
            cell: None,
            text: Utf8Output::new(output, "industry member key UTF-8"),
            done: false,
            bad: None,
        }
    }
    fn fail(&mut self, reason: &'static str) -> io::Error {
        self.bad = Some(reason);
        io::Error::new(io::ErrorKind::InvalidData, reason)
    }
    pub fn finish(&self) -> Result<ParsedCell, ValueTreeError> {
        if let Some(reason) = self.bad {
            return Err(ValueTreeError::Invalid(reason));
        }
        if !self.done || !self.text.is_complete() {
            return Err(ValueTreeError::Invalid("industry key truncated"));
        }
        self.cell
            .ok_or(ValueTreeError::Invalid("industry key empty"))
    }
}
impl<W: Write> Write for CellKeyDecoder<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.done {
            return Err(self.fail("industry key trailing bytes"));
        }
        if let Some(remaining) = self.remaining {
            let take = (remaining as u128).min(bytes.len() as u128) as usize;
            let n = self.text.write(&bytes[..take])?;
            self.remaining = Some(remaining - n as u64);
            if n as u64 == remaining {
                if !self.text.is_complete() {
                    return Err(self.fail("industry member key UTF-8"));
                }
                self.done = true;
            }
            return Ok(n);
        }
        // Consume only framing before touching the fallible text sink. A
        // retried Interrupted write can then never replay an accepted prefix.
        if self.used == self.prefix.len() {
            return Err(self.fail("industry key prefix bound"));
        }
        self.prefix[self.used] = bytes[0];
        self.used += 1;
        match head(&self.prefix[..self.used]) {
            Err(ParseError::More) => {}
            Err(ParseError::Bad(reason)) => return Err(self.fail(reason)),
            Ok(Head::Exact(cell, length)) => {
                debug_assert_eq!(self.used, length);
                self.cell = Some(cell);
                self.done = true;
            }
            Ok(Head::Map(cell, start, length)) => {
                debug_assert_eq!(self.used, start);
                self.cell = Some(cell);
                self.remaining = Some(length);
                self.done = length == 0;
            }
        }
        Ok(1)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) struct OuterKeyDecoder {
    bytes: [u8; SEMANTIC_KEY_DOMAIN.len() + 9],
    used: usize,
    pub bad: Option<&'static str>,
}
impl OuterKeyDecoder {
    pub fn new<W: Write>(_kind: IndustryRecordKindV1, _output: &mut W) -> Self {
        Self {
            bytes: [0; SEMANTIC_KEY_DOMAIN.len() + 9],
            used: 0,
            bad: None,
        }
    }
    pub fn finish(&self) -> Result<u64, ValueTreeError> {
        if let Some(reason) = self.bad {
            return Err(ValueTreeError::Invalid(reason));
        }
        let n = SEMANTIC_KEY_DOMAIN.len();
        if self.used != self.bytes.len()
            || &self.bytes[..n] != SEMANTIC_KEY_DOMAIN
            || self.bytes[n] != 2
        {
            return Err(ValueTreeError::Invalid("industry outer u64 key"));
        }
        Ok(u64::from_be_bytes(self.bytes[n + 1..].try_into().unwrap()))
    }
}
impl Write for OuterKeyDecoder {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.bytes.len() - self.used {
            self.bad = Some("industry outer key trailing bytes");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                self.bad.unwrap(),
            ));
        }
        self.bytes[self.used..self.used + bytes.len()].copy_from_slice(bytes);
        self.used += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
