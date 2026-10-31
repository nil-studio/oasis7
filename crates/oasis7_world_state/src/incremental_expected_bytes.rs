//! Borrowed byte equality during an authenticated stream; no ownership policy.
use std::io::{self, Write};
pub(crate) struct Compare<'a> {
    remaining: &'a [u8],
    equal: bool,
}
impl<'a> Compare<'a> {
    pub(crate) fn new(expected: &'a [u8]) -> Self {
        Self {
            remaining: expected,
            equal: true,
        }
    }
    pub(crate) fn is_equal(&self) -> bool {
        self.equal && self.remaining.is_empty()
    }
}
impl Write for Compare<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.equal &= self.remaining.get(..bytes.len()) == Some(bytes);
        self.remaining = &self.remaining[bytes.len().min(self.remaining.len())..];
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
