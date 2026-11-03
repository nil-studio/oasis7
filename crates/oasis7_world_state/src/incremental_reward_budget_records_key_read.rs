//! Closed relative key parser with constant workspace for arbitrary member IDs.
use super::*;
use crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN;
use std::io::{self, Write};
#[derive(Clone, Copy, Debug)]
pub(super) enum Parsed {
    Record,
    Column(RewardColumnV1),
    Collection(RewardCollectionV1),
    Member(RewardCollectionV1),
}
impl Parsed {
    pub fn accepts(self, kind: RewardRecordKindV1) -> bool {
        match self {
            Self::Record => true,
            Self::Column(c) => c.accepts(kind),
            Self::Collection(_) | Self::Member(_) => kind == RewardRecordKindV1::Pool,
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::Record | Self::Collection(_) => MemberPayloadCodecV1::Unit,
            Self::Column(c) => c.codec(),
            Self::Member(_) => MemberPayloadCodecV1::U64,
        }
    }
}
enum Header {
    Fixed(Parsed),
    Member(RewardCollectionV1, u64),
}
enum ParseError {
    More,
    Bad,
}
struct Cursor<'a> {
    bytes: &'a [u8],
    used: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ParseError> {
        let end = self.used.checked_add(n).ok_or(ParseError::Bad)?;
        let out = self.bytes.get(self.used..end).ok_or(ParseError::More)?;
        self.used = end;
        Ok(out)
    }
    fn byte(&mut self) -> Result<u8, ParseError> {
        Ok(self.take(1)?[0])
    }
    fn number(&mut self) -> Result<u64, ParseError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn name(&mut self) -> Result<&'a str, ParseError> {
        if self.byte()? != 1 {
            return Err(ParseError::Bad);
        }
        let n = usize::try_from(self.number()?).map_err(|_| ParseError::Bad)?;
        if n > 64 {
            return Err(ParseError::Bad);
        }
        std::str::from_utf8(self.take(n)?).map_err(|_| ParseError::Bad)
    }
}
fn header(bytes: &[u8]) -> Result<Header, ParseError> {
    let mut c = Cursor { bytes, used: 0 };
    if c.take(SEMANTIC_KEY_DOMAIN.len())? != SEMANTIC_KEY_DOMAIN {
        return Err(ParseError::Bad);
    }
    let out = match c.byte()? {
        0 => Header::Fixed(Parsed::Record),
        5 => match c.byte()? {
            0 => {
                let name = c.name()?;
                Header::Fixed(Parsed::Column(
                    RewardColumnV1::ALL
                        .into_iter()
                        .find(|v| v.name() == name)
                        .ok_or(ParseError::Bad)?,
                ))
            }
            5 => {
                if c.byte()? != 0 {
                    return Err(ParseError::Bad);
                }
                let name = c.name()?;
                let collection = RewardCollectionV1::ALL
                    .into_iter()
                    .find(|v| v.name() == name)
                    .ok_or(ParseError::Bad)?;
                match c.byte()? {
                    0 => Header::Fixed(Parsed::Collection(collection)),
                    1 => Header::Member(collection, c.number()?),
                    _ => return Err(ParseError::Bad),
                }
            }
            _ => return Err(ParseError::Bad),
        },
        _ => return Err(ParseError::Bad),
    };
    if c.used != bytes.len() {
        return Err(ParseError::Bad);
    }
    Ok(out)
}
pub(super) struct CellProbe {
    prefix: [u8; 160],
    used: usize,
    parsed: Option<Parsed>,
    remaining: u64,
    tail: [u8; 4],
    tail_len: usize,
    bad: bool,
}
impl CellProbe {
    pub fn new() -> Self {
        Self {
            prefix: [0; 160],
            used: 0,
            parsed: None,
            remaining: 0,
            tail: [0; 4],
            tail_len: 0,
            bad: false,
        }
    }
    fn fail(&mut self) -> io::Error {
        self.bad = true;
        io::Error::new(io::ErrorKind::InvalidData, "reward relative key")
    }
    pub fn is_invalid(&self) -> bool {
        self.bad
    }
    pub fn finish(&self) -> Result<Parsed, ValueTreeError> {
        if self.bad || self.remaining != 0 || self.tail_len != 0 {
            return Err(ValueTreeError::Invalid("reward relative key framing/UTF8"));
        }
        self.parsed
            .ok_or(ValueTreeError::Invalid("reward relative key incomplete"))
    }
    fn utf8(&mut self, b: u8) -> io::Result<()> {
        if self.tail_len == 4 {
            return Err(self.fail());
        }
        self.tail[self.tail_len] = b;
        self.tail_len += 1;
        match std::str::from_utf8(&self.tail[..self.tail_len]) {
            Ok(_) => self.tail_len = 0,
            Err(e) if e.error_len().is_none() => {}
            Err(_) => return Err(self.fail()),
        }
        Ok(())
    }
}
impl Write for CellProbe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bad {
            return Err(self.fail());
        }
        for b in bytes {
            if self.parsed.is_some() {
                if self.remaining == 0 {
                    return Err(self.fail());
                }
                self.utf8(*b)?;
                self.remaining -= 1;
                continue;
            }
            if self.used == self.prefix.len() {
                return Err(self.fail());
            }
            self.prefix[self.used] = *b;
            self.used += 1;
            match header(&self.prefix[..self.used]) {
                Ok(Header::Fixed(p)) => self.parsed = Some(p),
                Ok(Header::Member(c, n)) => {
                    self.parsed = Some(Parsed::Member(c));
                    self.remaining = n;
                }
                Err(ParseError::More) => {}
                Err(ParseError::Bad) => return Err(self.fail()),
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
