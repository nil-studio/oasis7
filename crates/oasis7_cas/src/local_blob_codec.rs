use std::io::{Read, Write};

use sha2::{Digest, Sha256};

use crate::{HashAlgorithm, WorldError, validate_hash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobReadReceipt {
    pub decoded_bytes: usize,
    pub content_hash: String,
}

/// Decode and verify a raw or O7CBLOB1 blob from an already authorized reader.
/// Output is provisional until this function succeeds; callers must discard it
/// on error. No paths, caches, pins, or files are opened or written by this codec.
pub fn decode_local_blob_into<R: Read, W: Write>(
    algorithm: HashAlgorithm,
    expected_hash: &str,
    input: R,
    mut output: W,
    max_decoded_bytes: usize,
) -> Result<BlobReadReceipt, WorldError> {
    decode_local_blob_with(
        algorithm,
        expected_hash,
        input,
        max_decoded_bytes,
        None,
        |reader| std::io::copy(reader, &mut output).map(|_| ()),
    )
    .map(|(_, receipt)| receipt)
}

/// Consume provisional decoded bytes without an intermediate whole-blob Vec.
/// The result is returned only after EOF, length and hash verification. The
/// consumer must not publish provisional data or ignore read errors. A window
/// limit bounds zstd's history allocation; None preserves the general codec's
/// existing compatibility. This grants no path or material authority.
pub fn decode_local_blob_with<R: Read, T>(
    algorithm: HashAlgorithm,
    expected_hash: &str,
    mut input: R,
    max_decoded_bytes: usize,
    max_window_log: Option<u32>,
    consume: impl FnOnce(&mut dyn Read) -> std::io::Result<T>,
) -> Result<(T, BlobReadReceipt), WorldError> {
    validate_hash(expected_hash)?;
    let mut prefix = Vec::with_capacity(80);
    input.by_ref().take(80).read_to_end(&mut prefix)?;
    let compressed = prefix.starts_with(b"O7CBLOB1");
    let expected_len = if compressed {
        if prefix.len() != 80 || &prefix[16..80] != expected_hash.as_bytes() {
            return Err(invalid("invalid compressed blob header"));
        }
        let length = u64::from_le_bytes(prefix[8..16].try_into().expect("fixed header"));
        if length > max_decoded_bytes as u64 {
            return Err(invalid(
                "ResourceLimited: decoded blob length exceeds budget",
            ));
        }
        Some(length)
    } else {
        None
    };
    let raw = std::io::Cursor::new(prefix).chain(input);
    let reader: Box<dyn Read + '_> = if compressed {
        // Drop the consumed header; the remainder is the zstd stream.
        let mut raw = raw;
        std::io::copy(&mut raw.by_ref().take(80), &mut std::io::sink())?;
        let mut decoder = zstd::stream::read::Decoder::new(raw)?;
        if let Some(log) = max_window_log {
            decoder.window_log_max(log)?;
        }
        Box::new(decoder)
    } else {
        Box::new(raw)
    };
    let mut reader = CheckedReader {
        reader,
        blake3: blake3::Hasher::new(),
        sha256: Sha256::new(),
        count: 0,
        limit: max_decoded_bytes,
        failed: false,
    };
    let value = consume(&mut reader)?;
    if reader.failed || reader.read(&mut [0])? != 0 {
        return Err(invalid("blob consumer did not finish a valid stream"));
    }
    if expected_len.is_some_and(|length| length != reader.count as u64) {
        return Err(invalid("compressed blob decoded length mismatch"));
    }
    let actual = match algorithm {
        HashAlgorithm::Blake3 => reader.blake3.finalize().to_hex().to_string(),
        HashAlgorithm::Sha256 => hex::encode(reader.sha256.finalize()),
    };
    if actual != expected_hash {
        return Err(WorldError::BlobHashMismatch {
            expected: expected_hash.to_string(),
            actual,
        });
    }
    Ok((
        value,
        BlobReadReceipt {
            decoded_bytes: reader.count,
            content_hash: actual,
        },
    ))
}

struct CheckedReader<'a> {
    reader: Box<dyn Read + 'a>,
    blake3: blake3::Hasher,
    sha256: Sha256,
    count: usize,
    limit: usize,
    failed: bool,
}
impl Read for CheckedReader<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.failed {
            return Err(std::io::Error::other("blob stream already failed"));
        }
        let length = self.reader.read(bytes).inspect_err(|error| {
            self.failed = error.kind() != std::io::ErrorKind::Interrupted;
        })?;
        self.count = self
            .count
            .checked_add(length)
            .filter(|count| *count <= self.limit)
            .ok_or_else(|| {
                self.failed = true;
                std::io::Error::other("ResourceLimited: decoded blob exceeds budget")
            })?;
        self.blake3.update(&bytes[..length]);
        self.sha256.update(&bytes[..length]);
        Ok(length)
    }
}

fn invalid(reason: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provisional_consumer_result_requires_eof_hash_and_unignored_io() {
        use std::sync::{Arc, atomic::AtomicUsize, atomic::Ordering};
        struct Provisional(Arc<AtomicUsize>);
        impl Drop for Provisional {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let bytes = vec![42; 100];
        let hash = crate::blake3_hex(&bytes);
        let drops = Arc::new(AtomicUsize::new(0));
        for (maximum, expected, consume_all) in [
            (100, hash.clone(), false),
            (99, hash.clone(), true),
            (100, "00".repeat(32), true),
        ] {
            let before = drops.load(Ordering::SeqCst);
            let result = decode_local_blob_with(
                HashAlgorithm::Blake3,
                &expected,
                bytes.as_slice(),
                maximum,
                Some(23),
                |reader| {
                    if consume_all {
                        // Deliberately swallow an over-budget read error.
                        let _ = std::io::copy(reader, &mut std::io::sink());
                    }
                    Ok(Provisional(drops.clone()))
                },
            );
            assert!(result.is_err());
            assert_eq!(drops.load(Ordering::SeqCst), before + 1);
        }
        struct LateError(std::io::Cursor<Vec<u8>>);
        impl Read for LateError {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                match self.0.read(out)? {
                    0 => Err(std::io::Error::other("late source error")),
                    n => Ok(n),
                }
            }
        }
        assert!(
            decode_local_blob_with(
                HashAlgorithm::Blake3,
                &hash,
                LateError(std::io::Cursor::new(bytes)),
                100,
                Some(23),
                |reader| {
                    let _ = std::io::copy(reader, &mut std::io::sink());
                    Ok(Provisional(drops.clone()))
                },
            )
            .is_err()
        );
        assert_eq!(drops.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn bounded_consumer_preserves_legacy_level3_and_enforces_window_limit() {
        let hash = "0f227458a96cb2b18610d9624d5750390613bf9e84107e46ff48902b5b31be88";
        let mut encoded = b"O7CBLOB1".to_vec();
        encoded.extend_from_slice(&65536u64.to_le_bytes());
        encoded.extend_from_slice(hash.as_bytes());
        encoded.extend_from_slice(include_bytes!(
            "../tests/fixtures/zstd-0.13.3-level3-stream.zst"
        ));
        for (log, accepted) in [(23, true), (10, false)] {
            let result = decode_local_blob_with(
                HashAlgorithm::Blake3,
                hash,
                encoded.as_slice(),
                65536,
                Some(log),
                |reader| std::io::copy(reader, &mut std::io::sink()),
            );
            assert_eq!(result.is_ok(), accepted);
            if let Ok((count, receipt)) = result {
                assert_eq!(count, 65536);
                assert_eq!(receipt.decoded_bytes, 65536);
            }
        }
    }

    #[test]
    fn verifies_raw_and_compressed_and_enforces_exact_budget() {
        let bytes = vec![b'a'; 4096];
        let hash = crate::blake3_hex(&bytes);
        let mut encoded = b"O7CBLOB1".to_vec();
        encoded.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        encoded.extend_from_slice(hash.as_bytes());
        encoded.extend(zstd::stream::encode_all(bytes.as_slice(), 3).unwrap());
        for input in [bytes.clone(), encoded] {
            let mut output = Vec::new();
            assert_eq!(
                decode_local_blob_into(
                    HashAlgorithm::Blake3,
                    &hash,
                    input.as_slice(),
                    &mut output,
                    bytes.len()
                )
                .unwrap()
                .decoded_bytes,
                bytes.len()
            );
            assert_eq!(output, bytes);
            assert!(
                decode_local_blob_into(
                    HashAlgorithm::Blake3,
                    &hash,
                    input.as_slice(),
                    std::io::sink(),
                    bytes.len() - 1
                )
                .is_err()
            );
        }
    }
    #[test]
    fn rejects_truncation_and_wrong_hash() {
        let hash = crate::blake3_hex(b"hello");
        assert!(
            decode_local_blob_into(
                HashAlgorithm::Blake3,
                &hash,
                &b"hell"[..],
                std::io::sink(),
                10
            )
            .is_err()
        );
        assert!(
            decode_local_blob_into(
                HashAlgorithm::Blake3,
                &hash,
                &b"O7CBLOB1"[..],
                std::io::sink(),
                10
            )
            .is_err()
        );
    }
}
