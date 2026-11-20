use super::ModuleArtifactIdentity;
use std::io::{self, Write};

fn original_payload([wasm, source, manifest, signer]: [&str; 4]) -> Vec<u8> {
    format!("modsig:ed25519:v1|{wasm}|{source}|{manifest}|{signer}").into_bytes()
}

fn write_payload(fields: [&str; 4], output: &mut dyn Write) -> io::Result<()> {
    ModuleArtifactIdentity::write_signing_payload_v1(
        fields[0], fields[1], fields[2], fields[3], output,
    )
}

#[test]
fn streamed_payload_preserves_original_bytes_and_vec_interface() {
    for fields in [
        ["", "", "", ""],
        ["wasm", "source", "build", "node"],
        ["哈希|原件\0", "", "build\n\"", "节点🙂"],
    ] {
        let expected = original_payload(fields);
        let mut actual = Vec::new();
        write_payload(fields, &mut actual).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            ModuleArtifactIdentity::signing_payload_v1(fields[0], fields[1], fields[2], fields[3],),
            expected
        );
    }
}

#[derive(Default)]
struct BoundedWrites {
    bytes: Vec<u8>,
    largest_write: usize,
}

impl Write for BoundedWrites {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        assert!(bytes.len() <= 8192);
        self.largest_write = self.largest_write.max(bytes.len());
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn every_field_streams_across_utf8_and_chunk_boundaries() {
    for field in 0..4 {
        for prefix_len in [8191, 8192, 8193, 16384] {
            let mut fields = ["wasm", "source", "build", "node"].map(str::to_owned);
            fields[field] = "a".repeat(prefix_len);
            // With 8191 prefix bytes, the first chunk ends inside this character.
            fields[field].push_str("界🙂");
            let fields = fields.each_ref().map(String::as_str);
            let mut sink = BoundedWrites::default();
            write_payload(fields, &mut sink).unwrap();
            assert_eq!(sink.bytes, original_payload(fields));
            assert_eq!(sink.largest_write, 8192);
        }
    }
}

#[test]
fn streamed_payload_retries_interrupted_and_short_writes() {
    struct ShortWrites {
        bytes: Vec<u8>,
        interrupt_next: bool,
        interruptions: usize,
    }
    impl Write for ShortWrites {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.interrupt_next {
                self.interrupt_next = false;
                self.interruptions += 1;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.interrupt_next = true;
            let count = bytes.len().min(3);
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let fields = ["wasm", "原件", "build", "节点🙂"];
    let mut sink = ShortWrites {
        bytes: Vec::new(),
        interrupt_next: true,
        interruptions: 0,
    };
    write_payload(fields, &mut sink).unwrap();
    assert_eq!(sink.bytes, original_payload(fields));
    assert!(sink.interruptions > 1);
}

#[test]
fn streamed_payload_preserves_first_io_error_and_stops_at_each_prefix() {
    struct Failing {
        bytes: Vec<u8>,
        remaining: usize,
        failures: usize,
    }
    impl Write for Failing {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                self.failures += 1;
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "sink refused",
                ));
            }
            let count = bytes.len().min(self.remaining);
            self.bytes.extend_from_slice(&bytes[..count]);
            self.remaining -= count;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let fields = ["hash", "原件", "build", "node"];
    let expected = original_payload(fields);
    for offset in 0..expected.len() {
        let mut sink = Failing {
            bytes: Vec::new(),
            remaining: offset,
            failures: 0,
        };
        let error = write_payload(fields, &mut sink).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "sink refused");
        assert_eq!(sink.bytes, expected[..offset]);
        assert_eq!(sink.failures, 1);
    }
}

#[test]
fn streamed_payload_rejects_zero_progress() {
    struct ZeroWriter(usize);
    impl Write for ZeroWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            self.0 += 1;
            Ok(0)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut sink = ZeroWriter(0);
    let error = write_payload(["hash", "source", "build", "node"], &mut sink).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WriteZero);
    assert_eq!(sink.0, 1);
}
