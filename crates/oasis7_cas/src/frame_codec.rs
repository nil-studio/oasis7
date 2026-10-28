//! Shared pure v1 physical frame rules. Callers admit input, output and codec
//! scratch before invoking these helpers; this module opens no store or cache.
use std::io::Read;

pub const FRAME_BYTES: usize = 65536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDescriptor {
    pub stored: [u8; 32],
    pub original: [u8; 32],
    pub length: u32,
    pub compressed: bool,
}

/// Frozen gear v1, min 4 KiB, mask 14 bits, max 64 KiB. Physical cuts do not
/// participate in logical object identity. The result only borrows the input.
pub fn chunks(bytes: &[u8]) -> Vec<&[u8]> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut state = 0u64;
    for (index, byte) in bytes.iter().enumerate() {
        let value = (*byte as u64 + 1).wrapping_mul(0x9e3779b185ebca87);
        state = state
            .rotate_left(1)
            .wrapping_add(value ^ value.rotate_left(23));
        let size = index + 1 - start;
        if size >= 4096 && (state & 16383 == 0 || size == FRAME_BYTES) {
            result.push(&bytes[start..=index]);
            start = index + 1;
            state = 0;
        }
    }
    if start < bytes.len() {
        result.push(&bytes[start..]);
    }
    result
}

pub fn encode(bytes: &[u8], compress: bool) -> Result<(FrameDescriptor, Vec<u8>), std::io::Error> {
    if bytes.is_empty() || bytes.len() > FRAME_BYTES {
        return Err(invalid("frame length"));
    }
    let compressed = if compress {
        zstd::bulk::compress(bytes, 3)?
    } else {
        Vec::new()
    };
    let (stored, compressed) = if compress && compressed.len() < bytes.len() {
        (compressed, true)
    } else {
        (bytes.to_vec(), false)
    };
    Ok((
        FrameDescriptor {
            stored: *blake3::hash(&stored).as_bytes(),
            original: *blake3::hash(bytes).as_bytes(),
            length: bytes.len() as u32,
            compressed,
        },
        stored,
    ))
}

pub fn decode(frame: &FrameDescriptor, stored: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    if frame.length == 0
        || frame.length as usize > FRAME_BYTES
        || stored.len() > FRAME_BYTES
        || blake3::hash(stored).as_bytes() != &frame.stored
    {
        return Err(invalid("frame stored length/checksum"));
    }
    let bytes = if frame.compressed {
        let mut decoder = zstd::stream::read::Decoder::new(stored)?;
        decoder.window_log_max(16)?;
        // Reserve the exact bounded output capacity, never grow read_to_end
        // past the declared frame. A final one-byte probe detects overrun.
        let mut output = vec![0; frame.length as usize];
        decoder.read_exact(&mut output)?;
        let mut extra = [0u8; 1];
        if decoder.read(&mut extra)? != 0 {
            return Err(invalid("frame decoded overrun"));
        }
        output
    } else {
        stored.to_vec()
    };
    if bytes.len() != frame.length as usize || blake3::hash(&bytes).as_bytes() != &frame.original {
        return Err(invalid("frame decoded length/checksum"));
    }
    Ok(bytes)
}

fn invalid(reason: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_raw_and_zstd_frames_reject_overrun_truncation_and_corruption() {
        let input = vec![17; FRAME_BYTES];
        for compress in [false, true] {
            let (frame, bytes) = encode(&input, compress).unwrap();
            assert_eq!(decode(&frame, &bytes).unwrap(), input);
            let mut wrong = frame;
            wrong.length -= 1;
            assert!(decode(&wrong, &bytes).is_err());
            wrong = frame;
            wrong.original[0] ^= 1;
            assert!(decode(&wrong, &bytes).is_err());
            assert!(decode(&frame, &bytes[..bytes.len() - 1]).is_err());
        }
        assert!(encode(&[], false).is_err());
        assert!(encode(&vec![0; FRAME_BYTES + 1], true).is_err());
    }
}
