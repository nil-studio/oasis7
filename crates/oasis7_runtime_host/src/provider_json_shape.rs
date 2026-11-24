//! Inactive bounded pre-Serde provider JSON shape prepass extracted from the legacy host.
//! This counts allocation-relevant structure; it is not a JSON syntax validator.
//! The live decoder remains unchanged until the adapter and cutover pass acceptance.
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

use oasis7_proto::memory_budget::MemoryLimitExceeded;

const SCAN_SCRATCH_BYTES: usize = 8 * 1024;
const MAX_JSON_DEPTH: usize = 128;
const CANCEL_POLL_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub enum ProviderJsonAdmissionError {
    Memory(MemoryLimitExceeded),
    Cancelled,
    InputLimit,
    NestingLimit,
    SizeOverflow,
    Decode(serde_json::Error),
    Io(io::Error),
}

impl fmt::Display for ProviderJsonAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Memory(error) => write!(formatter, "provider JSON memory admission: {error}"),
            Self::Cancelled => formatter.write_str("provider JSON decoding cancelled"),
            Self::InputLimit => formatter.write_str("provider JSON input exceeds its bound"),
            Self::NestingLimit => formatter.write_str("provider JSON nesting exceeds its bound"),
            Self::SizeOverflow => formatter.write_str("provider JSON heap estimate overflow"),
            Self::Decode(error) => write!(formatter, "provider JSON decoding failed: {error}"),
            Self::Io(error) => write!(formatter, "provider JSON input read failed: {error}"),
        }
    }
}

impl std::error::Error for ProviderJsonAdmissionError {}

#[derive(Default)]
pub struct JsonShape {
    pub array_items: usize,
    pub object_members: usize,
    pub containers: usize,
    pub string_bytes: usize,
    pub largest_string_bytes: usize,
}

struct JsonShapeScanner {
    shape: JsonShape,
    stack: [u8; MAX_JSON_DEPTH],
    depth: usize,
    in_string: bool,
    escaped: bool,
    in_primitive: bool,
    string_bytes_since_poll: usize,
    current_string_bytes: usize,
}

impl JsonShapeScanner {
    fn new() -> Self {
        Self {
            shape: JsonShape::default(),
            stack: [0; MAX_JSON_DEPTH],
            depth: 0,
            in_string: false,
            escaped: false,
            in_primitive: false,
            string_bytes_since_poll: 0,
            current_string_bytes: 0,
        }
    }

    fn feed(
        &mut self,
        bytes: &[u8],
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), ProviderJsonAdmissionError> {
        for (offset, byte) in bytes.iter().copied().enumerate() {
            if offset % CANCEL_POLL_BYTES == 0 && cancelled() {
                return Err(ProviderJsonAdmissionError::Cancelled);
            }
            if self.in_string {
                self.shape.string_bytes = self
                    .shape
                    .string_bytes
                    .checked_add(1)
                    .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                self.current_string_bytes = self
                    .current_string_bytes
                    .checked_add(1)
                    .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                self.string_bytes_since_poll += 1;
                if self.string_bytes_since_poll >= CANCEL_POLL_BYTES {
                    self.string_bytes_since_poll = 0;
                    if cancelled() {
                        return Err(ProviderJsonAdmissionError::Cancelled);
                    }
                }
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                    self.shape.largest_string_bytes = self
                        .shape
                        .largest_string_bytes
                        .max(self.current_string_bytes);
                    self.current_string_bytes = 0;
                }
                continue;
            }

            if self.in_primitive {
                if !matches!(byte, b',' | b']' | b'}' | b' ' | b'\n' | b'\r' | b'\t') {
                    continue;
                }
                self.in_primitive = false;
            }

            match byte {
                b'"' => {
                    if self.parent_is_array() {
                        self.shape.array_items = self
                            .shape
                            .array_items
                            .checked_add(1)
                            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                    }
                    self.in_string = true;
                }
                b'{' => {
                    self.count_array_value()?;
                    self.push_container(2)?;
                }
                b'[' => {
                    self.count_array_value()?;
                    self.push_container(1)?;
                }
                b'}' | b']' => {
                    self.depth = self.depth.saturating_sub(1);
                }
                b':' => {
                    self.shape.object_members = self
                        .shape
                        .object_members
                        .checked_add(1)
                        .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
                }
                b'-' | b'0'..=b'9' | b't' | b'f' | b'n' => {
                    self.count_array_value()?;
                    self.in_primitive = true;
                }
                _ => {}
            }
        }
        if cancelled() {
            return Err(ProviderJsonAdmissionError::Cancelled);
        }
        Ok(())
    }

    fn parent_is_array(&self) -> bool {
        self.depth > 0 && self.stack[self.depth - 1] == 1
    }

    fn count_array_value(&mut self) -> Result<(), ProviderJsonAdmissionError> {
        if self.parent_is_array() {
            self.shape.array_items = self
                .shape
                .array_items
                .checked_add(1)
                .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        }
        Ok(())
    }

    fn push_container(&mut self, kind: u8) -> Result<(), ProviderJsonAdmissionError> {
        self.depth = self
            .depth
            .checked_add(1)
            .filter(|depth| *depth <= MAX_JSON_DEPTH)
            .ok_or(ProviderJsonAdmissionError::NestingLimit)?;
        self.stack[self.depth - 1] = kind;
        self.shape.containers = self
            .shape
            .containers
            .checked_add(1)
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        Ok(())
    }
}


pub fn scan_reader<R: Read + Seek>(
    reader: &mut R,
    max_bytes: u64,
    cancelled: &impl Fn() -> bool,
) -> Result<(JsonShape, u64), ProviderJsonAdmissionError> {
    let mut scanner = JsonShapeScanner::new();
    let mut scratch = [0; SCAN_SCRATCH_BYTES];
    let mut total = 0u64;
    loop {
        if cancelled() {
            return Err(ProviderJsonAdmissionError::Cancelled);
        }
        let read = reader.read(&mut scratch).map_err(|error| {
            if cancelled() {
                ProviderJsonAdmissionError::Cancelled
            } else {
                ProviderJsonAdmissionError::Io(error)
            }
        })?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(ProviderJsonAdmissionError::SizeOverflow)?;
        if total > max_bytes {
            return Err(ProviderJsonAdmissionError::InputLimit);
        }
        scanner.feed(&scratch[..read], cancelled)?;
    }
    reader
        .seek(SeekFrom::Start(0))
        .map_err(ProviderJsonAdmissionError::Io)?;
    Ok((scanner.shape, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn shape_scan_counts_nested_allocations_and_rewinds() {
        let bytes = br#"{"items":[1,"x",{"v":true}],"more":[]}"#;
        let mut reader = Cursor::new(bytes);
        let (shape, count) = scan_reader(&mut reader, bytes.len() as u64, &|| false).unwrap();
        assert_eq!(count, bytes.len() as u64);
        assert_eq!(shape.array_items, 3);
        assert_eq!(shape.object_members, 3);
        assert_eq!(shape.containers, 4);
        assert!(shape.string_bytes >= shape.largest_string_bytes);
        assert!(shape.largest_string_bytes > 0);
        assert_eq!(reader.position(), 0);
    }

    #[test]
    fn shape_scan_rejects_over_bound_input() {
        let mut reader = Cursor::new(b"[1,2,3]");
        assert!(matches!(
            scan_reader(&mut reader, 3, &|| false),
            Err(ProviderJsonAdmissionError::InputLimit)
        ));
    }

    #[test]
    fn shape_scan_rejects_excessive_depth() {
        let bytes = "[".repeat(MAX_JSON_DEPTH + 1);
        let mut reader = Cursor::new(bytes.as_bytes());
        assert!(matches!(
            scan_reader(&mut reader, bytes.len() as u64, &|| false),
            Err(ProviderJsonAdmissionError::NestingLimit)
        ));
    }

    #[test]
    fn shape_scan_observes_cancellation() {
        let mut reader = Cursor::new(b"[]");
        assert!(matches!(
            scan_reader(&mut reader, 2, &|| true),
            Err(ProviderJsonAdmissionError::Cancelled)
        ));
    }
}
