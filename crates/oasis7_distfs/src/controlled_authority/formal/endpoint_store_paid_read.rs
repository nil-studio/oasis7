//! Continuing metered in-memory input for the original serde codecs.
use super::super::FormalArchiveReadWorkV1;
use super::FormalError;

pub(super) struct PaidRead<'a> {
    bytes: &'a [u8],
    at: usize,
    work: &'a FormalArchiveReadWorkV1,
    cancelled: &'a mut dyn FnMut() -> bool,
    first: Option<FormalError>,
}
impl std::io::Read for PaidRead<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.first.is_some() {
            return Err(std::io::ErrorKind::Other.into());
        }
        let take = out.len().min(8192).min(self.bytes.len() - self.at);
        if let Err(error) = self.work.consume_read_bytes(take as u64, self.cancelled) {
            self.first = Some(error);
            return Err(std::io::ErrorKind::Other.into());
        }
        out[..take].copy_from_slice(&self.bytes[self.at..self.at + take]);
        self.at += take;
        Ok(take)
    }
}

impl<'a> PaidRead<'a> {
    pub(super) fn new(
        bytes: &'a [u8],
        work: &'a FormalArchiveReadWorkV1,
        cancelled: &'a mut dyn FnMut() -> bool,
    ) -> Self {
        Self {
            bytes,
            at: 0,
            work,
            cancelled,
            first: None,
        }
    }
    pub(super) fn take_first_error(&mut self) -> Option<FormalError> {
        self.first.take()
    }
    pub(super) fn check_cancelled(&mut self) -> Result<(), FormalError> {
        self.work.consume_steps(1, self.cancelled)
    }
}
