//! Fixed-workspace decoding of identity + three amounts; no account policy.
use crate::{incremental_field_member::*, incremental_utf8_output::Utf8Output};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueReadContext, ValueTreeError,
};
use std::io::{self, Write};
struct Output<'a, W> {
    identity: Utf8Output<'a, W>,
    header: [u8; 8],
    header_bytes: usize,
    identity_bytes: u64,
    remaining: u64,
    max_identity: u64,
    amounts: [u8; 24],
    amount_bytes: usize,
    error: Option<ValueTreeError>,
}
impl<W: Write> Write for Output<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::Error::other("balance payload decoder"));
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.header_bytes < 8 {
            let count = bytes.len().min(8 - self.header_bytes);
            self.header[self.header_bytes..self.header_bytes + count]
                .copy_from_slice(&bytes[..count]);
            self.header_bytes += count;
            if self.header_bytes == 8 {
                self.identity_bytes = u64::from_be_bytes(self.header);
                self.remaining = self.identity_bytes;
                if self.identity_bytes > self.max_identity {
                    self.error = Some(ValueTreeError::ResourceLimited("balance identity bytes"));
                }
            }
            // Return only this accepted prefix; never report an error after
            // consuming a header or retry an underlying output in this call.
            return Ok(count);
        }
        if self.remaining != 0 {
            let count = bytes
                .len()
                .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
            let accepted = self.identity.write(&bytes[..count])?;
            self.remaining -= accepted as u64;
            return Ok(accepted);
        }
        if self.amount_bytes < 24 {
            let count = bytes.len().min(24 - self.amount_bytes);
            self.amounts[self.amount_bytes..self.amount_bytes + count]
                .copy_from_slice(&bytes[..count]);
            self.amount_bytes += count;
            return Ok(count);
        }
        self.error = Some(ValueTreeError::Invalid("balance payload suffix"));
        Err(io::Error::other("balance payload suffix"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn read_identity_amounts<R: ValueNodeReader>(
    root: &[u8; 32],
    key: &WorldFieldKeyV1,
    context: &mut ValueReadContext<'_, R>,
    profile: MemberReadProfileV1,
    output: &mut impl Write,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(u64, [u64; 3]), ValueTreeError> {
    let max_identity = profile
        .limits
        .payload_bytes
        .checked_sub(32)
        .ok_or(ValueTreeError::ResourceLimited("balance identity limit"))?;
    let mut decoded = Output {
        identity: Utf8Output::new(output, "balance identity UTF-8"),
        header: [0; 8],
        header_bytes: 0,
        identity_bytes: 0,
        remaining: 0,
        max_identity,
        amounts: [0; 24],
        amount_bytes: 0,
        error: None,
    };
    let result = read_member_record_profile_v1(
        root,
        key,
        context,
        profile,
        &mut io::sink(),
        &mut decoded,
        &mut *cancelled,
    );
    if let Some(error) = decoded.error.take() {
        return Err(error);
    }
    if decoded.identity.is_invalid() {
        return Err(ValueTreeError::Invalid("balance identity UTF-8"));
    }
    let info = result?;
    if !decoded.identity.is_complete() {
        return Err(ValueTreeError::Invalid("balance identity UTF-8"));
    }
    if decoded.header_bytes != 8
        || decoded.remaining != 0
        || decoded.amount_bytes != 24
        || decoded.identity_bytes.checked_add(32) != Some(info.payload_bytes)
    {
        return Err(ValueTreeError::Invalid("balance payload framing"));
    }
    let amounts = std::array::from_fn(|n| {
        u64::from_be_bytes(decoded.amounts[n * 8..n * 8 + 8].try_into().unwrap())
    });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok((decoded.identity_bytes, amounts))
}
