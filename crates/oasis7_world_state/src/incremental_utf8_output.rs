//! Shared provisional UTF-8 validation; only accepted output bytes advance.
use crate::incremental_field_member::{
    MemberReadProfileV1, WorldFieldKeyV1, read_member_record_profile_v1,
};
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueTreeError};
use std::io::{self, Write};
pub(crate) struct Utf8Output<'a, W> {
    output: &'a mut W,
    error: &'static str,
    pending: [u8; 4],
    pending_len: usize,
    invalid: bool,
}
impl<W> Utf8Output<'_, W> {
    pub(crate) fn new<'a>(output: &'a mut W, error: &'static str) -> Utf8Output<'a, W> {
        Utf8Output {
            output,
            error,
            pending: [0; 4],
            pending_len: 0,
            invalid: false,
        }
    }
    pub(crate) fn is_invalid(&self) -> bool {
        self.invalid
    }
    pub(crate) fn is_complete(&self) -> bool {
        !self.invalid && self.pending_len == 0
    }
    fn validate(&mut self, mut bytes: &[u8]) -> Result<(), ()> {
        if self.pending_len != 0 {
            let width = match self.pending[0] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(()),
            };
            let take = (width - self.pending_len).min(bytes.len());
            self.pending[self.pending_len..self.pending_len + take].copy_from_slice(&bytes[..take]);
            self.pending_len += take;
            bytes = &bytes[take..];
            if self.pending_len < width {
                return Ok(());
            }
            std::str::from_utf8(&self.pending[..width]).map_err(|_| ())?;
            self.pending_len = 0;
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_none() => {
                let tail = &bytes[error.valid_up_to()..];
                if tail.len() > 3 {
                    return Err(());
                }
                self.pending[..tail.len()].copy_from_slice(tail);
                self.pending_len = tail.len();
                Ok(())
            }
            Err(_) => Err(()),
        }
    }
}
impl<W: Write> Write for Utf8Output<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.invalid {
            return Err(io::Error::other(self.error));
        }
        // Return each short write/Interrupted to ValueReadContext so its
        // cancellation checks run before any further output attempt.
        let written = self.output.write(bytes)?;
        if written > bytes.len() {
            return Ok(written); // The context rejects invalid byte counts.
        }
        if self.validate(&bytes[..written]).is_err() {
            self.invalid = true;
            return Err(io::Error::other(self.error));
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.invalid {
            return Err(io::Error::other(self.error));
        }
        self.output.flush()
    }
}

pub(crate) fn read_text_value<R: ValueNodeReader>(
    root: &[u8; 32],
    key: &WorldFieldKeyV1,
    context: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    profile: MemberReadProfileV1,
    output: &mut impl Write,
    error: &'static str,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<u64, ValueTreeError> {
    let mut checked = Utf8Output::new(output, error);
    let result = read_member_record_profile_v1(
        root,
        key,
        context,
        profile,
        &mut std::io::sink(),
        &mut checked,
        &mut *cancelled,
    );
    if checked.is_invalid() {
        return Err(ValueTreeError::Invalid(error));
    }
    let info = result?;
    if !checked.is_complete() {
        return Err(ValueTreeError::Invalid(error));
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(info.payload_bytes)
}
