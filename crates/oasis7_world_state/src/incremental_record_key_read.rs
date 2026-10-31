//! Streaming original Text/U64 semantic-key framing, without retaining the ID.
//! Hash/field/owner binding is separately enforced by the member-record reader.
use crate::{incremental_semantic_key::SEMANTIC_KEY_DOMAIN, incremental_utf8_output::Utf8Output};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use std::io::{self, Write};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginalKeyKind {
    Text,
    Unsigned,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginalKeyInfo {
    Text { utf8_bytes: u64 },
    Unsigned(u64),
}
pub(crate) struct OriginalSemanticKeyProbe<'a, W> {
    kind: OriginalKeyKind,
    prefix: [u8; SEMANTIC_KEY_DOMAIN.len() + 9],
    used: usize,
    remaining: Option<u64>,
    text: Utf8Output<'a, W>,
    bad: Option<&'static str>,
}
impl<'a, W: Write> OriginalSemanticKeyProbe<'a, W> {
    pub(crate) fn new(kind: OriginalKeyKind, output: &'a mut W) -> Self {
        Self {
            kind,
            prefix: [0; SEMANTIC_KEY_DOMAIN.len() + 9],
            used: 0,
            remaining: None,
            text: Utf8Output::new(output, "original semantic key UTF-8"),
            bad: None,
        }
    }
    fn fail(&mut self, reason: &'static str) -> io::Error {
        self.bad = Some(reason);
        io::Error::new(io::ErrorKind::InvalidData, reason)
    }
    pub(crate) fn error(&self) -> Option<&'static str> {
        self.bad.or_else(|| {
            self.text
                .is_invalid()
                .then_some("original semantic key UTF-8")
        })
    }
    fn head(&mut self) -> io::Result<()> {
        let n = SEMANTIC_KEY_DOMAIN.len();
        if &self.prefix[..n] != SEMANTIC_KEY_DOMAIN {
            return Err(self.fail("original semantic key domain"));
        }
        let tag = match self.kind {
            OriginalKeyKind::Text => 1,
            OriginalKeyKind::Unsigned => 2,
        };
        if self.prefix[n] != tag {
            return Err(self.fail("original semantic key token"));
        }
        self.remaining = Some(match self.kind {
            OriginalKeyKind::Text => u64::from_be_bytes(self.prefix[n + 1..].try_into().unwrap()),
            OriginalKeyKind::Unsigned => 0,
        });
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<OriginalKeyInfo, ValueTreeError> {
        if let Some(reason) = self.error() {
            return Err(ValueTreeError::Invalid(reason));
        }
        if self.used != self.prefix.len() || self.remaining != Some(0) {
            return Err(ValueTreeError::Invalid("original semantic key framing"));
        }
        if !self.text.is_complete() {
            return Err(ValueTreeError::Invalid("original semantic key UTF-8"));
        }
        let value = u64::from_be_bytes(
            self.prefix[SEMANTIC_KEY_DOMAIN.len() + 1..]
                .try_into()
                .unwrap(),
        );
        Ok(match self.kind {
            OriginalKeyKind::Text => OriginalKeyInfo::Text { utf8_bytes: value },
            OriginalKeyKind::Unsigned => OriginalKeyInfo::Unsigned(value),
        })
    }
}
impl<W: Write> Write for OriginalSemanticKeyProbe<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if let Some(reason) = self.error() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, reason));
        }
        if self.used < self.prefix.len() {
            let take = bytes.len().min(self.prefix.len() - self.used);
            self.prefix[self.used..self.used + take].copy_from_slice(&bytes[..take]);
            self.used += take;
            if self.used == self.prefix.len() {
                self.head()?;
            }
            // Never combine accepted framing with a fallible output write:
            // Interrupted must retry only the original ID bytes, not the head.
            return Ok(take);
        }
        let remaining = self.remaining.unwrap();
        if remaining == 0 {
            return Err(self.fail("original semantic key trailing bytes"));
        }
        let take = (remaining as u128).min(bytes.len() as u128) as usize;
        let written = self.text.write(&bytes[..take])?;
        self.remaining = Some(remaining - written as u64);
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.text.flush()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn encoded(tag: u8, value: u64, tail: &[u8]) -> Vec<u8> {
        let mut bytes = SEMANTIC_KEY_DOMAIN.to_vec();
        bytes.push(tag);
        bytes.extend_from_slice(&value.to_be_bytes());
        bytes.extend_from_slice(tail);
        bytes
    }
    #[test]
    fn original_tokens_and_fragmented_utf8_are_exact() {
        for text in ["", " raw\0界😀 "] {
            let bytes = encoded(1, text.len() as u64, text.as_bytes());
            let mut output = Vec::new();
            let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut output);
            for fragment in bytes.chunks(1) {
                probe.write_all(fragment).unwrap();
            }
            assert_eq!(
                probe.finish().unwrap(),
                OriginalKeyInfo::Text {
                    utf8_bytes: text.len() as u64
                }
            );
            assert_eq!(output, text.as_bytes());
        }
        let mut sink = io::sink();
        let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Unsigned, &mut sink);
        probe.write_all(&encoded(2, u64::MAX, &[])).unwrap();
        assert_eq!(probe.finish().unwrap(), OriginalKeyInfo::Unsigned(u64::MAX));
    }
    #[test]
    fn wrong_type_truncated_utf8_and_trailing_bytes_never_finish() {
        for bytes in [
            encoded(2, 0, &[]),
            encoded(1, 2, b"a"),
            encoded(1, 2, &[0xe7, 0x95]),
            encoded(1, 0, b"extra"),
        ] {
            let mut sink = io::sink();
            let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut sink);
            let written = probe.write_all(&bytes);
            assert!(written.is_err() || probe.finish().is_err());
        }
    }
    struct Interrupted {
        first: bool,
        bytes: Vec<u8>,
    }
    impl Write for Interrupted {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.first {
                self.first = false;
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn interrupted_payload_does_not_replay_accepted_header() {
        let mut out = Interrupted {
            first: true,
            bytes: Vec::new(),
        };
        let mut probe = OriginalSemanticKeyProbe::new(OriginalKeyKind::Text, &mut out);
        probe.write_all(&encoded(1, 3, b"key")).unwrap();
        assert_eq!(
            probe.finish().unwrap(),
            OriginalKeyInfo::Text { utf8_bytes: 3 }
        );
        assert_eq!(out.bytes, b"key");
    }
}
