use super::*;

#[test]
fn frozen_bytes_preserve_variant_unicode_empty_ids_and_borrowed_text() {
    let mut expected = b"oasis7.iws.ledger-value.v1\0".to_vec();
    expected.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 5, b'a', b':', 0xe7, 0x94, 0xb2]);
    let mut bytes = Vec::new();
    let value = MaterialLedgerId::Agent("a:甲".into());
    assert_eq!(
        write_ledger_value_v1(&value, &mut bytes).unwrap(),
        expected.len() as u64
    );
    assert_eq!(bytes, expected);
    let LedgerValueRefV1::Agent(id) = read_ledger_value_v1(&bytes).unwrap() else {
        panic!("kind")
    };
    assert_eq!(id, "a:甲");
    assert_eq!(id.as_ptr(), bytes[LEDGER_VALUE_HEADER_BYTES..].as_ptr());
    for (value, expected_kind) in [
        (MaterialLedgerId::World, 0),
        (MaterialLedgerId::Agent(String::new()), 1),
        (MaterialLedgerId::Site(String::new()), 2),
        (MaterialLedgerId::Factory(String::new()), 3),
    ] {
        let mut bytes = Vec::new();
        assert_eq!(
            ledger_value_len_v1(&value).unwrap(),
            LEDGER_VALUE_HEADER_BYTES as u64
        );
        write_ledger_value_v1(&value, &mut bytes).unwrap();
        let mut expected = b"oasis7.iws.ledger-value.v1\0".to_vec();
        expected.push(expected_kind);
        expected.extend_from_slice(&[0; 8]);
        assert_eq!(bytes, expected);
        assert!(read_ledger_value_v1(&bytes).is_ok());
    }
}

#[test]
fn malformed_frames_and_wrong_domains_never_decode() {
    let mut valid = Vec::new();
    write_ledger_value_v1(&MaterialLedgerId::Site("s".into()), &mut valid).unwrap();
    for mode in 0..8 {
        let mut bytes = valid.clone();
        match mode {
            0 => bytes.clear(),
            1 => bytes[0] ^= 1,
            2 => bytes[LEDGER_VALUE_DOMAIN.len()] = 4,
            3 => bytes.truncate(LEDGER_VALUE_HEADER_BYTES - 1),
            4 => bytes.push(0),
            5 => bytes[LEDGER_VALUE_DOMAIN.len()] = 0,
            6 => bytes[LEDGER_VALUE_HEADER_BYTES] = 0xff,
            _ => bytes[LEDGER_VALUE_DOMAIN.len() + 1..LEDGER_VALUE_HEADER_BYTES].fill(0xff),
        }
        assert!(read_ledger_value_v1(&bytes).is_err(), "mode {mode}");
    }
    assert!(read_ledger_value_v1(b"oasis7.iws.semantic-key.v1\0\x04\0").is_err());
}

#[test]
fn source_id_is_passed_directly_and_short_writes_or_errors_are_respected() {
    let value = MaterialLedgerId::Factory("not-reformatted".into());
    let MaterialLedgerId::Factory(id) = &value else {
        unreachable!()
    };
    struct Observe {
        pointer: *const u8,
        length: usize,
        borrowed: bool,
    }
    impl Write for Observe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() == self.length {
                self.borrowed |= bytes.as_ptr() == self.pointer;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut observer = Observe {
        pointer: id.as_ptr(),
        length: id.len(),
        borrowed: false,
    };
    write_ledger_value_v1(&value, &mut observer).unwrap();
    assert!(observer.borrowed);
    struct Short {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Short {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.bytes.len() == self.limit {
                return Err(io::Error::other("test stop"));
            }
            let count = bytes.len().min(2).min(self.limit - self.bytes.len());
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut expected = Vec::new();
    write_ledger_value_v1(&value, &mut expected).unwrap();
    let mut short = Short {
        bytes: Vec::new(),
        limit: usize::MAX,
    };
    write_ledger_value_v1(&value, &mut short).unwrap();
    assert_eq!(short.bytes, expected);
    let mut stopped = Short {
        bytes: Vec::new(),
        limit: LEDGER_VALUE_HEADER_BYTES + 2,
    };
    assert!(write_ledger_value_v1(&value, &mut stopped).is_err());
    assert!(read_ledger_value_v1(&stopped.bytes).is_err());
}
