//! Bounded streaming parser for Runtime Artifact semantic keys.
use crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN;
use std::io::{self, Write};

const LABEL_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyValue {
    Simple,
    Unit,
    Text,
    Unsigned(u64),
    Resource(u8),
    Ledger(u8),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ParsedKey {
    pub label: [u8; LABEL_BYTES],
    pub label_len: usize,
    pub value: KeyValue,
    pub right_text: [u8; 16],
    pub right_text_len: usize,
    pub right_text_truncated: bool,
}

impl ParsedKey {
    pub fn label(&self) -> Result<&str, &'static str> {
        std::str::from_utf8(&self.label[..self.label_len])
            .map_err(|_| "runtime artifact key label UTF-8")
    }

    pub fn right_text(&self) -> Option<&str> {
        if self.value != KeyValue::Text || self.right_text_truncated {
            return None;
        }
        std::str::from_utf8(&self.right_text[..self.right_text_len]).ok()
    }
}

#[derive(Clone, Copy)]
enum State {
    Domain,
    RootTag,
    LeftTag,
    LeftLength,
    LeftText,
    RightTag,
    RightLength,
    RightText,
    RightUnsigned,
    RightResource,
    RightLedger,
    RightLedgerTextTag,
    RightLedgerLength,
    RightLedgerText,
    Done,
}

/// Parses only the bounded field path and token type. Long dynamic text IDs
/// are UTF-8 checked incrementally and discarded rather than retained.
pub(super) struct KeyDecoder {
    state: State,
    domain_used: usize,
    header: [u8; 8],
    header_used: usize,
    remaining: u64,
    label: [u8; LABEL_BYTES],
    label_len: usize,
    pair: bool,
    value: KeyValue,
    right_text: [u8; 16],
    right_text_len: usize,
    right_text_truncated: bool,
    right_text_expected: u64,
    number: [u8; 8],
    number_used: usize,
    utf8_pending: [u8; 4],
    utf8_pending_len: usize,
    bad: Option<&'static str>,
}

impl KeyDecoder {
    pub fn new() -> Self {
        Self {
            state: State::Domain,
            domain_used: 0,
            header: [0; 8],
            header_used: 0,
            remaining: 0,
            label: [0; LABEL_BYTES],
            label_len: 0,
            pair: false,
            value: KeyValue::Simple,
            right_text: [0; 16],
            right_text_len: 0,
            right_text_truncated: false,
            right_text_expected: 0,
            number: [0; 8],
            number_used: 0,
            utf8_pending: [0; 4],
            utf8_pending_len: 0,
            bad: None,
        }
    }

    pub fn finish(&self) -> Result<ParsedKey, &'static str> {
        if let Some(reason) = self.bad {
            return Err(reason);
        }
        if !matches!(self.state, State::Done)
            || self.domain_used != SEMANTIC_KEY_DOMAIN.len()
            || self.utf8_pending_len != 0
        {
            return Err("runtime artifact semantic key framing");
        }
        if !self.pair && !matches!(self.value, KeyValue::Simple | KeyValue::Unit) {
            return Err("runtime artifact semantic key root");
        }
        Ok(ParsedKey {
            label: self.label,
            label_len: self.label_len,
            value: self.value,
            right_text: self.right_text,
            right_text_len: self.right_text_len,
            right_text_truncated: self.right_text_truncated,
        })
    }

    fn fail(&mut self, reason: &'static str) -> io::Error {
        self.bad = Some(reason);
        io::Error::new(io::ErrorKind::InvalidData, reason)
    }

    fn text(&mut self, bytes: &[u8], keep_label: bool) -> io::Result<usize> {
        let take = (self.remaining as u128).min(bytes.len() as u128) as usize;
        let text = &bytes[..take];
        if keep_label {
            let end = self
                .label_len
                .checked_add(take)
                .filter(|end| *end <= self.label.len())
                .ok_or_else(|| self.fail("runtime artifact key label bound"))?;
            self.label[self.label_len..end].copy_from_slice(text);
            self.label_len = end;
        } else if matches!(self.state, State::RightText) {
            let take_label = (self.right_text.len() - self.right_text_len).min(take);
            let end = self.right_text_len + take_label;
            self.right_text[self.right_text_len..end].copy_from_slice(&text[..take_label]);
            self.right_text_len = end;
            if self.right_text_expected > self.right_text.len() as u64 {
                self.right_text_truncated = true;
            }
        }
        self.validate_utf8(text)
            .map_err(|_| self.fail("runtime artifact dynamic key UTF-8"))?;
        self.remaining -= take as u64;
        if self.remaining == 0 {
            if self.utf8_pending_len != 0 {
                return Err(self.fail("runtime artifact dynamic key UTF-8"));
            }
            self.state = match self.state {
                State::LeftText if self.pair => State::RightTag,
                State::LeftText => State::Done,
                State::RightText | State::RightLedgerText => State::Done,
                _ => return Err(self.fail("runtime artifact key parser state")),
            };
        }
        Ok(take)
    }

    fn validate_utf8(&mut self, mut bytes: &[u8]) -> Result<(), ()> {
        if self.utf8_pending_len != 0 {
            let width = match self.utf8_pending[0] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(()),
            };
            let take = (width - self.utf8_pending_len).min(bytes.len());
            self.utf8_pending[self.utf8_pending_len..self.utf8_pending_len + take]
                .copy_from_slice(&bytes[..take]);
            self.utf8_pending_len += take;
            bytes = &bytes[take..];
            if self.utf8_pending_len < width {
                return Ok(());
            }
            std::str::from_utf8(&self.utf8_pending[..width]).map_err(|_| ())?;
            self.utf8_pending_len = 0;
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_none() => {
                let tail = &bytes[error.valid_up_to()..];
                if tail.len() > 3 {
                    return Err(());
                }
                self.utf8_pending[..tail.len()].copy_from_slice(tail);
                self.utf8_pending_len = tail.len();
                Ok(())
            }
            Err(_) => Err(()),
        }
    }

    fn length_byte(&mut self, byte: u8, next: State) -> Result<(), io::Error> {
        self.header[self.header_used] = byte;
        self.header_used += 1;
        if self.header_used == self.header.len() {
            self.remaining = u64::from_be_bytes(self.header);
            self.header_used = 0;
            self.state = next;
            if self.remaining == 0 {
                self.state = match next {
                    State::LeftText if self.pair => State::RightTag,
                    State::LeftText => State::Done,
                    State::RightText | State::RightLedgerText => State::Done,
                    _ => return Err(self.fail("runtime artifact key parser state")),
                };
            }
        }
        Ok(())
    }
}

impl Write for KeyDecoder {
    fn write(&mut self, mut bytes: &[u8]) -> io::Result<usize> {
        if let Some(reason) = self.bad {
            return Err(io::Error::new(io::ErrorKind::InvalidData, reason));
        }
        let original = bytes.len();
        while !bytes.is_empty() {
            match self.state {
                State::Domain => {
                    let take = (SEMANTIC_KEY_DOMAIN.len() - self.domain_used).min(bytes.len());
                    if bytes[..take]
                        != SEMANTIC_KEY_DOMAIN[self.domain_used..self.domain_used + take]
                    {
                        return Err(self.fail("runtime artifact semantic key domain"));
                    }
                    self.domain_used += take;
                    bytes = &bytes[take..];
                    if self.domain_used == SEMANTIC_KEY_DOMAIN.len() {
                        self.state = State::RootTag;
                    }
                }
                State::RootTag => {
                    let tag = bytes[0];
                    bytes = &bytes[1..];
                    self.state = match tag {
                        0 => {
                            self.value = KeyValue::Unit;
                            State::Done
                        }
                        1 => State::LeftLength,
                        5 => {
                            self.pair = true;
                            State::LeftTag
                        }
                        _ => return Err(self.fail("runtime artifact semantic key root")),
                    };
                }
                State::LeftTag => {
                    let tag = bytes[0];
                    bytes = &bytes[1..];
                    if tag != 1 {
                        return Err(self.fail("runtime artifact semantic key left token"));
                    }
                    self.state = State::LeftLength;
                }
                State::LeftLength => {
                    let byte = bytes[0];
                    bytes = &bytes[1..];
                    self.length_byte(byte, State::LeftText)?;
                }
                State::LeftText => {
                    let consumed = self.text(bytes, true)?;
                    bytes = &bytes[consumed..];
                }
                State::RightTag => {
                    let tag = bytes[0];
                    bytes = &bytes[1..];
                    match tag {
                        0 => {
                            self.value = KeyValue::Unit;
                            self.state = State::Done;
                        }
                        1 => {
                            self.value = KeyValue::Text;
                            self.state = State::RightLength;
                        }
                        2 => {
                            self.value = KeyValue::Unsigned(0);
                            self.number_used = 0;
                            self.state = State::RightUnsigned;
                        }
                        3 => self.state = State::RightResource,
                        4 => self.state = State::RightLedger,
                        _ => return Err(self.fail("runtime artifact semantic key pair token")),
                    }
                }
                State::RightLength => {
                    let byte = bytes[0];
                    bytes = &bytes[1..];
                    self.length_byte(byte, State::RightText)?;
                    if matches!(self.state, State::RightText) {
                        self.right_text_expected = self.remaining;
                        self.right_text_len = 0;
                        self.right_text_truncated = self.remaining > self.right_text.len() as u64;
                    }
                }
                State::RightText => {
                    let consumed = self.text(bytes, false)?;
                    bytes = &bytes[consumed..];
                }
                State::RightUnsigned => {
                    let take = (8 - self.number_used).min(bytes.len());
                    self.number[self.number_used..self.number_used + take]
                        .copy_from_slice(&bytes[..take]);
                    self.number_used += take;
                    bytes = &bytes[take..];
                    if self.number_used == 8 {
                        self.value = KeyValue::Unsigned(u64::from_be_bytes(self.number));
                        self.state = State::Done;
                    }
                }
                State::RightResource => {
                    let tag = bytes[0];
                    bytes = &bytes[1..];
                    if !matches!(tag, 1 | 2) {
                        return Err(self.fail("runtime artifact resource key tag"));
                    }
                    self.value = KeyValue::Resource(tag);
                    self.state = State::Done;
                }
                State::RightLedger => {
                    let tag = bytes[0];
                    bytes = &bytes[1..];
                    if tag > 3 {
                        return Err(self.fail("runtime artifact ledger key tag"));
                    }
                    self.value = KeyValue::Ledger(tag);
                    self.state = if tag == 0 {
                        State::Done
                    } else {
                        State::RightLedgerTextTag
                    };
                }
                State::RightLedgerTextTag => {
                    let tag = bytes[0];
                    bytes = &bytes[1..];
                    if tag != 1 {
                        return Err(self.fail("runtime artifact ledger text token"));
                    }
                    self.state = State::RightLedgerLength;
                }
                State::RightLedgerLength => {
                    let byte = bytes[0];
                    bytes = &bytes[1..];
                    self.length_byte(byte, State::RightLedgerText)?;
                }
                State::RightLedgerText => {
                    let consumed = self.text(bytes, false)?;
                    bytes = &bytes[consumed..];
                }
                State::Done => {
                    return Err(self.fail("runtime artifact semantic key trailing bytes"));
                }
            }
        }
        Ok(original)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_semantic_key::write_semantic_key_v1;

    fn decode<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        key: &K,
    ) -> Result<ParsedKey, &'static str> {
        let mut encoded = Vec::new();
        write_semantic_key_v1(key, &mut encoded).unwrap();
        let mut decoder = KeyDecoder::new();
        decoder.write_all(&encoded).unwrap();
        decoder.finish()
    }

    #[test]
    fn accepts_unit_outer_entry_key_and_closed_text_pair_profiles() {
        let unit = decode(&()).unwrap();
        assert_eq!(unit.value, KeyValue::Unit);

        let field = decode("schema_version").unwrap();
        assert_eq!(field.value, KeyValue::Simple);
        assert_eq!(field.label().unwrap(), "schema_version");

        let child = decode(&(String::from("material_ledgers"), String::from("ore"))).unwrap();
        assert_eq!(child.label().unwrap(), "material_ledgers");
        assert_eq!(child.right_text(), Some("ore"));
    }
}
