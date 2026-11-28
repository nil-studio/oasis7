//! Only stable private descriptors and immutable slices reach the shared reader.
use super::*;
use serde_cbor::de::{EitherLifetime, IoRead, Read as CborRead};
use std::io::{self, BufReader, Read, Seek, Write};

const MAX_TOKEN: usize = 4 * 1024 * 1024 + 128;
struct BoundedRead<R: Read> {
    inner: IoRead<R>,
    token: usize,
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
        // IoRead allocates before calling the typed Visitor. Include every
        // segment of an indefinite token in this pre-allocation bound.
        self.token = self
            .token
            .checked_add(n)
            .filter(|n| *n <= MAX_TOKEN)
            .ok_or_else(|| {
                <serde_cbor::Error as de::Error>::custom("private package token capacity")
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

struct Compare<R> {
    reader: R,
    hash: blake3::Hasher,
    count: usize,
}
impl<R: Read> Write for Compare<R> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut buffer = [0; 16 * 1024];
        for chunk in bytes.chunks(buffer.len()) {
            self.reader.read_exact(&mut buffer[..chunk.len()])?;
            if &buffer[..chunk.len()] != chunk {
                return Err(io::Error::other("noncanonical private package"));
            }
            self.hash.update(chunk);
            self.count += chunk.len();
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn stream_error<E>(_: E) -> String {
    "private package stream validation".into()
}

pub(super) fn slice(bytes: &[u8]) -> Result<NodeBftPrivateCandidatePackageV3, String> {
    decode(io::Cursor::new(bytes), bytes.len(), &package_digest(bytes))
}
pub(in super::super) fn staged(
    stage: super::super::staging::PrivateStage,
    manifest: &wire::Manifest,
) -> Result<NodeBftPrivateCandidatePackageV3, String> {
    let length = usize::try_from(manifest.total_bytes).map_err(stream_error)?;
    decode(stage, length, &manifest.package_digest)
}
// The concrete entries above own the source or borrow immutable bytes. Do not
// export an arbitrary mutable Read+Seek as a transport allocation guarantee.
fn decode<R: Read + Seek>(
    mut reader: R,
    length: usize,
    expected: &str,
) -> Result<NodeBftPrivateCandidatePackageV3, String> {
    if length > MAX_PACKAGE {
        return Err("private package capacity".into());
    }
    wire::hash(expected)?;
    reader.rewind().map_err(stream_error)?;
    wire::scan_reader(BufReader::new(&mut reader), length, true)?;
    reader.rewind().map_err(stream_error)?;
    let input = {
        let stream = BufReader::new((&mut reader).take(length as u64));
        let mut decoder = serde_cbor::Deserializer::new(BoundedRead {
            inner: IoRead::new(stream),
            token: 0,
        });
        let input = In::deserialize(&mut decoder).map_err(stream_error)?;
        decoder.end().map_err(stream_error)?;
        input
    };
    if input.domain != DOMAIN || input.schema_version != 1 {
        return Err("private package domain/schema".into());
    }
    // Typed input remains provisional. Its decoder and token scratch have
    // ended before the same descriptor is compared against canonical bytes.
    reader.rewind().map_err(stream_error)?;
    {
        let mut hash = blake3::Hasher::new();
        hash.update(&(DOMAIN.len() as u64).to_be_bytes());
        hash.update(DOMAIN.as_bytes());
        hash.update(&(length as u64).to_be_bytes());
        let mut compare = Compare {
            reader: BufReader::new(&mut reader),
            hash,
            count: 0,
        };
        serde_cbor::to_writer(&mut compare, &input).map_err(stream_error)?;
        if compare.count != length
            || compare.reader.read(&mut [0]).map_err(stream_error)? != 0
            || compare.hash.finalize().to_hex().as_str() != expected
        {
            return Err("private package original digest/length mismatch".into());
        }
    }
    finish(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_capacity_precedes_payload_allocation_and_accumulates_segments() {
        for length in [MAX_TOKEN as u64 + 1, u64::MAX] {
            let mut bytes = vec![0x5b];
            bytes.extend(length.to_be_bytes());
            let mut source = io::Cursor::new(bytes);
            let mut decoder = serde_cbor::Deserializer::new(BoundedRead {
                inner: IoRead::new(&mut source),
                token: 0,
            });
            let error = de::IgnoredAny::deserialize(&mut decoder).unwrap_err();
            assert!(error.to_string().contains("private package token capacity"));
            drop(decoder);
            assert_eq!(source.position(), 9);
        }
        let mut bytes = vec![0x5f, 0x5a];
        bytes.extend((MAX_TOKEN as u32).to_be_bytes());
        bytes.resize(bytes.len() + MAX_TOKEN, 42);
        bytes.extend([0x41, 42, 0xff]);
        let mut source = io::Cursor::new(bytes);
        let mut decoder = serde_cbor::Deserializer::new(BoundedRead {
            inner: IoRead::new(&mut source),
            token: 0,
        });
        assert!(
            de::IgnoredAny::deserialize(&mut decoder)
                .unwrap_err()
                .to_string()
                .contains("private package token capacity")
        );
        drop(decoder);
        assert_eq!(source.position(), MAX_TOKEN as u64 + 7);
    }
    // Artificial mutable sources test rejection, not an arbitrary-reader memory guarantee.
    struct Changing {
        bytes: io::Cursor<Vec<u8>>,
        pass: u8,
        alter_at: u8,
        fail: bool,
    }
    impl Read for Changing {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if self.fail && self.pass == self.alter_at {
                return Err(io::Error::other("injected read failure"));
            }
            let count = bytes.len().min(7);
            self.bytes.read(&mut bytes[..count])
        }
    }
    impl Seek for Changing {
        fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
            if position == io::SeekFrom::Start(0) {
                self.pass += 1;
                if self.pass == self.alter_at {
                    self.bytes.get_mut()[0] = 0xa4;
                }
            }
            self.bytes.seek(position)
        }
    }
    #[test]
    fn short_reads_work_but_changed_or_failed_later_passes_release_no_package() {
        let bytes = encode_package(&wire::tests::candidate_fixture(&[])).unwrap();
        for (alter_at, fail) in [(0, false), (2, false), (3, false), (3, true)] {
            let source = Changing {
                bytes: io::Cursor::new(bytes.clone()),
                pass: 0,
                alter_at,
                fail,
            };
            assert_eq!(
                decode(source, bytes.len(), &package_digest(&bytes)).is_ok(),
                alter_at == 0
            );
        }
    }
    #[test]
    fn near_capacity_malformed_public_is_rejected_before_owned_binding() {
        let mut input: In =
            serde_cbor::from_slice(&encode_package(&wire::tests::candidate_fixture(&[])).unwrap())
                .unwrap();
        let validators = &mut input
            .public_value
            .execution_commitment
            .transition
            .as_mut()
            .unwrap()
            .new_validators;
        validators.resize(2048, validators[0].clone());
        for validator in validators {
            validator.validator_id = "x".repeat(32768);
        }
        let bytes = encode(&input).unwrap();
        assert!(bytes.len() > 64 * 1024 * 1024 && bytes.len() < MAX_PACKAGE);
        let (stage, manifest) = wire::tests::stage(&bytes);
        drop(bytes);
        drop(input);
        let error = staged(stage, &manifest).unwrap_err();
        assert!(error.contains("transition validator count"));
    }
}
