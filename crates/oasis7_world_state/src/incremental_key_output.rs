//! One output attempt per call; the enclosing reader owns cancellation/retry.
use std::io::{self, Write};

pub(crate) struct KeyOutput<'a, W> {
    pub(crate) output: &'a mut W,
    pub(crate) hash: blake3::Hasher,
}
impl<W: Write> Write for KeyOutput<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.output.write(bytes)?;
        if written <= bytes.len() {
            self.hash.update(&bytes[..written]);
        }
        // ValueReadContext rejects an invalid byte count before retrying.
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
