use super::*;
#[test]
fn large_ledger_identity_is_borrowed_and_hashed_with_fixed_extra_workspace() {
    let length = (65 << 20) + 7;
    let memory = MemoryBudget::new(66 << 20, 4096).unwrap();
    let charge = memory.reserve(length + 256, MemoryClass::Ordinary).unwrap();
    let source = charge.retain(MaterialLedgerId::agent("x".repeat(length)));
    let MaterialLedgerId::Agent(text) = &*source else {
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
                self.borrowed = bytes.as_ptr() == self.pointer;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut observe = Observe {
        pointer: text.as_ptr(),
        length,
        borrowed: false,
    };
    write_semantic_key_v1(&*source, &mut observe).unwrap();
    assert!(
        observe.borrowed,
        "ledger ID must not pass through serde into/clone"
    );
    let before = memory.used();
    let logical_length = (SEMANTIC_KEY_DOMAIN.len() + 3 + 8 + length) as u64;
    let mut budget = SemanticKeyBudget {
        bytes_remaining: logical_length,
    };
    let hash = hash_semantic_key_v1(
        &*source,
        &memory,
        MemoryClass::Ordinary,
        &mut budget,
        || false,
    )
    .unwrap();
    let mut expected = blake3::Hasher::new();
    expected.update(SEMANTIC_KEY_DOMAIN);
    expected.update(&[4, 1, 1]);
    expected.update(&(length as u64).to_be_bytes());
    expected.update(text.as_bytes());
    assert_eq!(hash.digest, *expected.finalize().as_bytes());
    assert_eq!(hash.encoded_bytes, logical_length);
    assert_eq!(memory.used(), before);
    assert_eq!(budget.bytes_remaining, 0);
}
struct TerminalSource<'a> {
    cancelled: &'a AtomicBool,
    unwind: bool,
}
impl serde::Serialize for TerminalSource<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }
}
impl sealed::Key for TerminalSource<'_> {
    fn encode<W: Write>(&self, output: &mut W, _: usize) -> io::Result<()> {
        output.write_all(&[0])?;
        if self.unwind {
            panic!("source unwind after final bytes");
        }
        self.cancelled.store(true, Ordering::Release);
        Ok(())
    }
}
#[test]
fn final_source_cancellation_and_unwind_never_return_a_digest_or_restore_work() {
    let memory = MemoryBudget::new(8192, 4096).unwrap();
    let flag = AtomicBool::new(false);
    let length = SEMANTIC_KEY_DOMAIN.len() as u64 + 1;
    let mut budget = SemanticKeyBudget {
        bytes_remaining: length,
    };
    assert!(matches!(
        hash_semantic_key_v1(
            &TerminalSource {
                cancelled: &flag,
                unwind: false
            },
            &memory,
            MemoryClass::Ordinary,
            &mut budget,
            || flag.load(Ordering::Acquire)
        ),
        Err(SemanticKeyError::Cancelled)
    ));
    assert_eq!(budget.bytes_remaining, 0);
    assert_eq!(memory.used(), 0);
    flag.store(false, Ordering::Release);
    let mut budget = SemanticKeyBudget {
        bytes_remaining: length,
    };
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        hash_semantic_key_v1(
            &TerminalSource {
                cancelled: &flag,
                unwind: true,
            },
            &memory,
            MemoryClass::Ordinary,
            &mut budget,
            || false,
        )
    }));
    assert!(caught.is_err());
    assert_eq!(budget.bytes_remaining, 0);
    assert_eq!(memory.used(), 0);
}
