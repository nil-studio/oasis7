//! Closed relative-key parsing with fixed prefix storage and streaming UTF-8.
use super::*;
use crate::{incremental_semantic_key::SEMANTIC_KEY_DOMAIN, incremental_utf8_output::Utf8Output};
use std::io::{self, Write};
#[derive(Debug, Clone, Copy)]
pub(super) enum ParsedCell {
    Record,
    Column(FactoryColumnV1),
    Collection(FactoryCollectionV1),
    Text(FactoryCollectionV1, u64),
    Stack(FactoryCollectionV1, u64, FactoryStackColumnV1),
    Balance(FactoryCollectionV1),
}
impl ParsedCell {
    pub fn profile_cell(self) -> FactoryCellV1<'static> {
        match self {
            Self::Record => FactoryCellV1::Record,
            Self::Column(c) => FactoryCellV1::Column(c),
            Self::Collection(c) => FactoryCellV1::Collection(c),
            Self::Text(c, i) => FactoryCellV1::TextItem(c, i),
            Self::Stack(c, i, p) => FactoryCellV1::StackItem(c, i, p),
            Self::Balance(c) => FactoryCellV1::Balance(c, ""),
        }
    }
    // The empty text above selects ONLY the I64 value profile. Identity is the
    // fully parsed original member preimage authenticated by the actual key.
    pub fn validate(self, kind: FactoryRecordKindV1) -> Result<(), ValueTreeError> {
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
            .ok_or(ParseError::Bad("factory key framing"))?;
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
            return Err(ParseError::Bad("factory static key token"));
        }
        let n = self.integer()?;
        if n > 128 {
            return Err(ParseError::Bad("factory static key length"));
        }
        std::str::from_utf8(self.take(n as usize)?)
            .map_err(|_| ParseError::Bad("factory static key UTF-8"))
    }
}
enum Head {
    Exact(ParsedCell, usize),
    Map(ParsedCell, usize, u64),
}
fn head(bytes: &[u8]) -> Result<Head, ParseError> {
    let common = bytes.len().min(SEMANTIC_KEY_DOMAIN.len());
    if bytes[..common] != SEMANTIC_KEY_DOMAIN[..common] {
        return Err(ParseError::Bad("factory key domain"));
    }
    let mut c = Cursor { bytes, offset: 0 };
    c.take(SEMANTIC_KEY_DOMAIN.len())?;
    let mut depth = 0;
    let mut tag = c.byte()?;
    while tag == 5 {
        depth += 1;
        if depth > 4 {
            return Err(ParseError::Bad("factory key depth"));
        }
        tag = c.byte()?;
    }
    if tag != 0 {
        return Err(ParseError::Bad("factory relative key root"));
    }
    if depth == 0 {
        return Ok(Head::Exact(ParsedCell::Record, c.offset));
    }
    if !matches!(depth, 2..=4) {
        return Err(ParseError::Bad("factory relative key path"));
    }
    let section = c.name()?;
    let field = c.name()?;
    if depth == 2 {
        let column = FactoryColumnV1::ALL
            .into_iter()
            .find(|column| column.path() == (section, field))
            .ok_or(ParseError::Bad("factory column whitelist"))?;
        return Ok(Head::Exact(ParsedCell::Column(column), c.offset));
    }
    let collection = FactoryCollectionV1::ALL
        .into_iter()
        .find(|collection| collection.path() == (section, field))
        .ok_or(ParseError::Bad("factory collection whitelist"))?;
    if depth == 4 {
        if c.byte()? != 2 {
            return Err(ParseError::Bad("factory stack index token"));
        }
        let index = c.integer()?;
        let part = match c.name()? {
            "kind" => FactoryStackColumnV1::Kind,
            "amount" => FactoryStackColumnV1::Amount,
            _ => return Err(ParseError::Bad("factory stack column")),
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
        _ => Err(ParseError::Bad("factory member key token")),
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
            text: Utf8Output::new(output, "factory member key UTF-8"),
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
            return Err(ValueTreeError::Invalid("factory key truncated"));
        }
        self.cell
            .ok_or(ValueTreeError::Invalid("factory key empty"))
    }
}
impl<W: Write> Write for CellKeyDecoder<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.done {
            return Err(self.fail("factory key trailing bytes"));
        }
        if let Some(remaining) = self.remaining {
            let take = (remaining as u128).min(bytes.len() as u128) as usize;
            let n = self.text.write(&bytes[..take])?;
            self.remaining = Some(remaining - n as u64);
            if n as u64 == remaining {
                if !self.text.is_complete() {
                    return Err(self.fail("factory member key UTF-8"));
                }
                self.done = true;
            }
            return Ok(n);
        }
        // Consume only framing before touching the fallible text sink. A
        // retried Interrupted write can then never replay an accepted prefix.
        if self.used == self.prefix.len() {
            return Err(self.fail("factory key prefix bound"));
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryOuterKeyInfoV1 {
    Text { utf8_bytes: u64 },
    Unsigned(u64),
}
pub(super) struct OuterKeyDecoder<'a, W> {
    header: [u8; SEMANTIC_KEY_DOMAIN.len() + 9],
    used: usize,
    text: Utf8Output<'a, W>,
    kind: FactoryRecordKindV1,
    remaining: u64,
    info: Option<FactoryOuterKeyInfoV1>,
    done: bool,
    pub bad: Option<&'static str>,
}
impl<'a, W: Write> OuterKeyDecoder<'a, W> {
    pub fn new(kind: FactoryRecordKindV1, output: &'a mut W) -> Self {
        Self {
            header: [0; SEMANTIC_KEY_DOMAIN.len() + 9],
            used: 0,
            text: Utf8Output::new(output, "factory outer key UTF-8"),
            kind,
            remaining: 0,
            info: None,
            done: false,
            bad: None,
        }
    }
    fn fail(&mut self, reason: &'static str) -> io::Error {
        self.bad = Some(reason);
        io::Error::new(io::ErrorKind::InvalidData, reason)
    }
    pub fn finish(&self) -> Result<FactoryOuterKeyInfoV1, ValueTreeError> {
        if let Some(reason) = self.bad {
            return Err(ValueTreeError::Invalid(reason));
        }
        if !self.done || !self.text.is_complete() {
            return Err(ValueTreeError::Invalid("factory outer key truncated"));
        }
        self.info
            .ok_or(ValueTreeError::Invalid("factory outer key empty"))
    }
}
impl<W: Write> Write for OuterKeyDecoder<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.done {
            return Err(self.fail("factory outer key trailing bytes"));
        }
        if self.used < self.header.len() {
            let take = bytes.len().min(self.header.len() - self.used);
            self.header[self.used..self.used + take].copy_from_slice(&bytes[..take]);
            self.used += take;
            if self.used == self.header.len() {
                let p = SEMANTIC_KEY_DOMAIN.len();
                if &self.header[..p] != SEMANTIC_KEY_DOMAIN {
                    return Err(self.fail("factory outer key domain"));
                }
                let value = u64::from_be_bytes(self.header[p + 1..].try_into().unwrap());
                match (self.kind, self.header[p]) {
                    (FactoryRecordKindV1::Factory, 1) => {
                        self.remaining = value;
                        self.info = Some(FactoryOuterKeyInfoV1::Text { utf8_bytes: value });
                        self.done = value == 0;
                    }
                    (FactoryRecordKindV1::Build, 2) => {
                        self.info = Some(FactoryOuterKeyInfoV1::Unsigned(value));
                        self.done = true;
                    }
                    _ => return Err(self.fail("factory outer key type")),
                }
            }
            return Ok(take);
        }
        let take = (self.remaining as u128).min(bytes.len() as u128) as usize;
        let written = self.text.write(&bytes[..take])?;
        self.remaining -= written as u64;
        if self.remaining == 0 {
            if !self.text.is_complete() {
                return Err(self.fail("factory outer key UTF-8"));
            }
            self.done = true;
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.text.flush()
    }
}
