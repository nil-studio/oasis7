//! Push producer over the same value pages, batch barriers and original locator
//! as Read imports. An incomplete producer never publishes a locator.
use super::*;
use oasis7_formal_protocol::incremental_value::ValueStreamWriter;

#[derive(Debug)]
pub enum ObjectStreamWriteError<E> {
    Producer(E),
    Store(PageStoreError),
}
impl<E> From<PageStoreError> for ObjectStreamWriteError<E> {
    fn from(error: PageStoreError) -> Self {
        Self::Store(error)
    }
}
struct Output<'a, W> {
    writer: &'a mut W,
    hash: blake3::Hasher,
    length: u64,
    failed: bool,
}
impl<W: Write> Write for Output<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let n = match self.writer.write(bytes) {
            Ok(n) => n,
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        if n > bytes.len() {
            self.failed = true;
            return Err(std::io::Error::other("original writer overreported"));
        }
        self.hash.update(&bytes[..n]);
        self.length = self
            .length
            .checked_add(n as u64)
            .ok_or(std::io::ErrorKind::InvalidData)?;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush().inspect_err(|_| {
            self.failed = true;
        })
    }
}
impl IncrementalObjectStore<'_> {
    /// Consume a borrowed push producer, finish value+batch writers, publish the
    /// expected flat-ID locator and verify its winning original before success.
    /// The producer and store share the supplied continuous IO allowances. No
    /// full object/file buffer is introduced. Producer errors retain their type.
    pub fn write_object_from_stream<E>(
        &self,
        length: u64,
        expected: [u8; 32],
        io: &mut ObjectIo<'_>,
        cancelled: &mut dyn FnMut() -> bool,
        produce: impl FnOnce(&mut dyn Write) -> std::result::Result<(), E>,
    ) -> std::result::Result<[u8; 32], ObjectStreamWriteError<E>> {
        let _workspace = self
            .pages
            .memory
            .reserve(WORKSPACE, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        self.validate(io)?;
        check_cancelled(io.cancelled)?;
        let mut pages = self
            .pages
            .buffered_page_writer(io.physical, io.cancelled, cancelled)?;
        let mut writer =
            ValueStreamWriter::new(&mut pages, &self.pages.memory, io.logical, length, || {
                io.cancelled.load(Ordering::Acquire)
            })
            .map_err(from_value)?;
        let (produced, id, actual, failed) = {
            let mut output = Output {
                writer: &mut writer,
                hash: blake3::Hasher::new(),
                length: 0,
                failed: false,
            };
            let result = produce(&mut output);
            (
                result,
                *output.hash.finalize().as_bytes(),
                output.length,
                output.failed,
            )
        };
        if let Err(error) = produced {
            // Failed Write keeps the underlying typed cause in finish. A
            // producer-only error drops its provisional writer without flush.
            if failed && let Err(error) = writer.finish() {
                return Err(ObjectStreamWriteError::Store(from_value(error)));
            }
            return Err(ObjectStreamWriteError::Producer(error));
        }
        let root = writer.finish().map_err(from_value)?;
        if actual != length || root.0.logical_len != length || id != expected {
            return Err(ObjectStreamWriteError::Store(PageStoreError::Corrupt(
                "original stream identity/length",
            )));
        }
        pages.flush()?;
        drop(pages);
        if cancelled() {
            return Err(PageStoreError::Cancelled.into());
        }
        let id = self.publish_original(id, length, root.digest().map_err(from_value)?, io)?;
        if cancelled() {
            return Err(PageStoreError::Cancelled.into());
        }
        Ok(id)
    }
}
