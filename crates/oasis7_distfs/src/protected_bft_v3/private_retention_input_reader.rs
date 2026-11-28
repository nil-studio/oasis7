//! Original material only; every value remains provisional until both reads
//! verify the same hash/length and the second read matches canonical bytes.
use super::*;
use oasis7_cas::{HashAlgorithm, decode_local_blob_with};
use serde_cbor::de::{EitherLifetime, IoRead, Read as CborRead};
use std::io::{self, Read, Seek, Write};

fn invalid_stream<E>(_: E) -> BftSignerErrorV3 {
    invalid("original admission stream validation failed")
}

/// IoRead otherwise grows its scratch before invoking a typed Visitor. Bound
/// the whole token, including all indefinite-string segments, before that
/// allocation. Delegate parsing and lookahead to the existing pinned codec.
struct BoundedRead<R: Read> {
    inner: IoRead<R>,
    token: usize,
}
impl<R: Read> BoundedRead<R> {
    fn new(reader: R) -> Self {
        Self {
            inner: IoRead::new(reader),
            token: 0,
        }
    }
}
impl<'de, R: Read> CborRead<'de> for BoundedRead<R> {
    fn next(&mut self) -> serde_cbor::Result<Option<u8>> {
        self.inner.next()
    }
    fn peek(&mut self) -> serde_cbor::Result<Option<u8>> {
        self.inner.peek()
    }
    fn clear_buffer(&mut self) {
        self.token = 0;
        self.inner.clear_buffer();
    }
    fn read_to_buffer(&mut self, n: usize) -> serde_cbor::Result<()> {
        self.token = self
            .token
            .checked_add(n)
            .filter(|n| *n <= BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1)
            .ok_or_else(|| {
                <serde_cbor::Error as serde::de::Error>::custom("original token bound")
            })?;
        self.inner.read_to_buffer(n)
    }
    fn take_buffer<'a>(&'a mut self) -> EitherLifetime<'a, 'de> {
        self.inner.take_buffer()
    }
    fn read_into(&mut self, bytes: &mut [u8]) -> serde_cbor::Result<()> {
        self.inner.read_into(bytes)
    }
    fn discard(&mut self) {
        self.inner.discard();
    }
    fn offset(&self) -> u64 {
        self.inner.offset()
    }
}

struct CompareReader<'a>(&'a mut dyn Read);
impl Write for CompareReader<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buffer = [0; 16 * 1024];
        for chunk in bytes.chunks(buffer.len()) {
            self.0.read_exact(&mut buffer[..chunk.len()])?;
            if &buffer[..chunk.len()] != chunk {
                return Err(io::Error::other("noncanonical original admission input"));
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn read_verified<R: Read + Seek, T>(
    expected: &BftPrivateRetentionAdmissionInputRefV1,
    reader: &mut R,
    consume: impl FnOnce(&mut dyn Read) -> io::Result<T>,
) -> Result<T, BftSignerErrorV3> {
    reader.rewind().map_err(invalid_stream)?;
    let (value, receipt) = decode_local_blob_with(
        HashAlgorithm::Blake3,
        &expected.object.digest,
        reader.take(expected.object.bytes * 2 + 4096),
        expected.object.bytes as usize,
        // Existing CAS level-3 encodings fit this history bound. Window
        // memory is <=8MiB; token scratch is bounded separately above.
        Some(23),
        consume,
    )
    .map_err(invalid_stream)?;
    if receipt.decoded_bytes as u64 != expected.object.bytes {
        return Err(invalid("original admission stream length mismatch"));
    }
    Ok(value)
}

impl BftPrivateRetentionAdmissionInputV1 {
    /// Read an independently authorized original from a seekable private
    /// staging file or a freshly opened CAS descriptor. No full raw Vec is
    /// retained beside the owned input. This neither publishes nor grants read
    /// permission. Both passes start at zero and require the exact original.
    pub fn from_canonical_reader<R: Read + Seek>(
        expected: &BftPrivateRetentionAdmissionInputRefV1,
        mut reader: R,
    ) -> Result<Self, BftSignerErrorV3> {
        expected.validate()?;
        let input = read_verified(expected, &mut reader, |stream| {
            let mut decoder = serde_cbor::Deserializer::new(BoundedRead::new(stream));
            let input = Self::deserialize(&mut decoder).map_err(io::Error::other)?;
            decoder.end().map_err(io::Error::other)?;
            Ok(input)
        })?;
        expected.validate_input(&input)?;
        // The first decoder, token scratch and zstd history have ended. Reuse
        // the same descriptor; an altered/truncated second read cannot release
        // the provisional input even if the first hash was valid.
        read_verified(expected, &mut reader, |stream| {
            serde_cbor::to_writer(CompareReader(stream), &input).map_err(io::Error::other)
        })?;
        Ok(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_stream_tokens_reject_oversize_before_payload_reads() {
        use std::cell::Cell;
        struct Count<'a>(io::Cursor<Vec<u8>>, &'a Cell<usize>);
        impl Read for Count<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.1.set(self.1.get() + 1);
                self.0.read(bytes)
            }
        }
        let maximum = BFT_PRIVATE_RETENTION_INPUT_MAX_DEPENDENCY_OBJECT_BYTES_V1;
        for length in [maximum as u64 + 1, u64::MAX] {
            let mut header = vec![0x5b];
            header.extend_from_slice(&length.to_be_bytes());
            let reads = Cell::new(0);
            let source = Count(io::Cursor::new(header), &reads);
            let mut decoder = serde_cbor::Deserializer::new(BoundedRead::new(source));
            assert!(serde::de::IgnoredAny::deserialize(&mut decoder).is_err());
            assert_eq!(reads.get(), 2, "no attempted payload read/allocation");
        }
        let mut segmented = vec![0x5f, 0x5a];
        segmented.extend_from_slice(&(maximum as u32).to_be_bytes());
        segmented.resize(segmented.len() + maximum, 42);
        segmented.extend_from_slice(&[0x41, 43, 0xff]);
        let mut source = io::Cursor::new(segmented);
        let mut decoder = serde_cbor::Deserializer::new(BoundedRead::new(&mut source));
        assert!(serde::de::IgnoredAny::deserialize(&mut decoder).is_err());
        drop(decoder);
        assert_eq!(source.position(), maximum as u64 + 7);
    }
}
