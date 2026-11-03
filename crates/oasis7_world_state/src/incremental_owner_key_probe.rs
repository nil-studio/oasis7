//! Fixed-workspace original semantic-key validation for owner scans.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum OwnerKeyKind {
    Text,
    Action,
}

/// Validates closed text/action semantic-key tokens without materializing
/// arbitrarily long original keys. The record reader separately hashes the
/// full stream and binds it to the structural field-key digest.
pub(super) struct OwnerSemanticKeyProbe {
    expected: OwnerKeyKind,
    prefix: [u8; SEMANTIC_KEY_DOMAIN.len() + 9],
    prefix_len: usize,
    expected_total: Option<u64>,
    bytes_seen: u64,
    max_bytes: u64,
    utf8_pending: [u8; 4],
    utf8_pending_len: usize,
    pub(super) invalid: bool,
}

impl OwnerSemanticKeyProbe {
    pub(super) fn new(expected: OwnerKeyKind, max_bytes: u64) -> Self {
        Self {
            expected,
            prefix: [0; SEMANTIC_KEY_DOMAIN.len() + 9],
            prefix_len: 0,
            expected_total: None,
            bytes_seen: 0,
            max_bytes,
            utf8_pending: [0; 4],
            utf8_pending_len: 0,
            invalid: false,
        }
    }

    fn fail(&mut self) -> io::Error {
        self.invalid = true;
        io::Error::other("WorldState owner semantic key")
    }

    fn parse_prefix(&mut self) -> io::Result<()> {
        let domain_len = SEMANTIC_KEY_DOMAIN.len();
        if &self.prefix[..domain_len] != SEMANTIC_KEY_DOMAIN {
            return Err(self.fail());
        }
        match (self.expected, self.prefix[domain_len]) {
            (OwnerKeyKind::Text, 1) => {
                let length = u64::from_be_bytes(
                    self.prefix[domain_len + 1..domain_len + 9]
                        .try_into()
                        .map_err(|_| self.fail())?,
                );
                self.expected_total = Some(
                    (self.prefix.len() as u64)
                        .checked_add(length)
                        .filter(|total| *total <= self.max_bytes)
                        .ok_or_else(|| self.fail())?,
                );
            }
            (OwnerKeyKind::Action, 2) => {
                if self.prefix.len() as u64 > self.max_bytes {
                    return Err(self.fail());
                }
                self.expected_total = Some(self.prefix.len() as u64);
            }
            _ => return Err(self.fail()),
        }
        Ok(())
    }

    fn validate_text(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        if self.utf8_pending_len != 0 {
            let width = match self.utf8_pending[0] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(self.fail()),
            };
            let take = (width - self.utf8_pending_len).min(bytes.len());
            self.utf8_pending[self.utf8_pending_len..self.utf8_pending_len + take]
                .copy_from_slice(&bytes[..take]);
            self.utf8_pending_len += take;
            bytes = &bytes[take..];
            if self.utf8_pending_len < width {
                return Ok(());
            }
            if std::str::from_utf8(&self.utf8_pending[..width]).is_err() {
                return Err(self.fail());
            }
            self.utf8_pending_len = 0;
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_none() => {
                let tail = &bytes[error.valid_up_to()..];
                if tail.len() > 3 {
                    return Err(self.fail());
                }
                self.utf8_pending[..tail.len()].copy_from_slice(tail);
                self.utf8_pending_len = tail.len();
                Ok(())
            }
            Err(_) => Err(self.fail()),
        }
    }

    pub(super) fn finish(&self) -> bool {
        self.expected_total == Some(self.bytes_seen)
            && self.prefix_len == self.prefix.len()
            && (matches!(self.expected, OwnerKeyKind::Action) || self.utf8_pending_len == 0)
            && !self.invalid
    }
}

impl Write for OwnerSemanticKeyProbe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.invalid {
            return Err(io::Error::other("WorldState owner semantic key"));
        }
        let Some(seen) = self.bytes_seen.checked_add(bytes.len() as u64) else {
            return Err(self.fail());
        };
        if seen > self.max_bytes {
            return Err(self.fail());
        }
        self.bytes_seen = seen;
        let mut offset = 0;
        if self.prefix_len < self.prefix.len() {
            let take = (self.prefix.len() - self.prefix_len).min(bytes.len());
            self.prefix[self.prefix_len..self.prefix_len + take].copy_from_slice(&bytes[..take]);
            self.prefix_len += take;
            offset = take;
            if self.prefix_len == self.prefix.len() {
                self.parse_prefix()?;
            } else {
                return Ok(bytes.len());
            }
        }
        let expected_total = self.expected_total.ok_or_else(|| self.fail())?;
        if self.bytes_seen > expected_total {
            return Err(self.fail());
        }
        let tail = &bytes[offset..];
        match self.expected {
            OwnerKeyKind::Text => self.validate_text(tail)?,
            OwnerKeyKind::Action if !tail.is_empty() => return Err(self.fail()),
            OwnerKeyKind::Action => {}
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.invalid {
            Err(io::Error::other("WorldState owner semantic key"))
        } else {
            Ok(())
        }
    }
}
