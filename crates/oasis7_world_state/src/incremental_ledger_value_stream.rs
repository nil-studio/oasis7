use super::*;
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueReadContext, ValueTreeError,
};

struct Utf8Output<'a, W> {
    output: &'a mut W,
    pending: [u8; 4],
    pending_len: usize,
    invalid: bool,
}
impl<W> Utf8Output<'_, W> {
    fn validate(&mut self, mut bytes: &[u8]) -> Result<(), ()> {
        if self.pending_len != 0 {
            let width = match self.pending[0] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => return Err(()),
            };
            let take = (width - self.pending_len).min(bytes.len());
            self.pending[self.pending_len..self.pending_len + take].copy_from_slice(&bytes[..take]);
            self.pending_len += take;
            bytes = &bytes[take..];
            if self.pending_len < width {
                return Ok(());
            }
            std::str::from_utf8(&self.pending[..width]).map_err(|_| ())?;
            self.pending_len = 0;
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_none() => {
                let tail = &bytes[error.valid_up_to()..];
                if tail.len() > 3 {
                    return Err(());
                }
                self.pending[..tail.len()].copy_from_slice(tail);
                self.pending_len = tail.len();
                Ok(())
            }
            Err(_) => Err(()),
        }
    }
}
impl<W: Write> Write for Utf8Output<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.invalid {
            return Err(io::Error::other("ledger UTF-8"));
        }
        // Return each short write/Interrupted to ValueReadContext so its
        // cancellation checks run before any further output attempt.
        let written = self.output.write(bytes)?;
        if written > bytes.len() {
            return Ok(written); // The context rejects invalid byte counts.
        }
        if self.validate(&bytes[..written]).is_err() {
            self.invalid = true;
            return Err(io::Error::other("ledger UTF-8"));
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.invalid {
            return Err(io::Error::other("ledger UTF-8"));
        }
        self.output.flush()
    }
}

/// Authenticate the value descriptor and stream only the original ID bytes.
/// Output remains provisional until success. `context` supplies the applicable
/// pool/class and finite node/read/output allowances; the ID is never owned.
/// This checks value content, not field registration, authority or World state.
pub fn read_ledger_value_tree_v1<R: ValueNodeReader>(
    expected_root: &[u8; 32],
    context: &mut ValueReadContext<'_, R>,
    max_id_bytes: u64,
    output: &mut impl Write,
    mut cancelled: impl FnMut() -> bool,
) -> Result<LedgerValueInfoV1, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _metadata = context.memory.reserve(512, context.class)?;
    let root = context.load_root(expected_root)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if root.0.logical_len < LEDGER_VALUE_HEADER_BYTES as u64 {
        return Err(ValueTreeError::Invalid("ledger value header"));
    }
    let mut header = [0; LEDGER_VALUE_HEADER_BYTES];
    context.read_range(
        &root,
        0,
        LEDGER_VALUE_HEADER_BYTES as u64,
        &mut &mut header[..],
        &mut cancelled,
    )?;
    let info = header_v1(&header, root.0.logical_len).map_err(ValueTreeError::Invalid)?;
    if info.id_bytes > max_id_bytes {
        return Err(ValueTreeError::ResourceLimited("ledger ID length"));
    }
    let mut checked = Utf8Output {
        output,
        pending: [0; 4],
        pending_len: 0,
        invalid: false,
    };
    if info.id_bytes != 0 {
        let result = context.read_range(
            &root,
            LEDGER_VALUE_HEADER_BYTES as u64,
            info.id_bytes,
            &mut checked,
            &mut cancelled,
        );
        if checked.invalid {
            return Err(ValueTreeError::Invalid("ledger UTF-8"));
        }
        result?;
    }
    if checked.pending_len != 0 {
        return Err(ValueTreeError::Invalid("ledger UTF-8"));
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_is_independent_of_output_write_boundaries() {
        struct ShortOutput(Vec<u8>, usize);
        impl Write for ShortOutput {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let written = bytes.len().min(self.1);
                self.0.extend_from_slice(&bytes[..written]);
                Ok(written)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let source = "x甲é😀z".as_bytes();
        for size in 1..=source.len() {
            for limit in 1..=source.len() {
                let mut output = ShortOutput(Vec::new(), limit);
                let mut checked = Utf8Output {
                    output: &mut output,
                    pending: [0; 4],
                    pending_len: 0,
                    invalid: false,
                };
                for bytes in source.chunks(size) {
                    checked.write_all(bytes).unwrap();
                }
                assert_eq!(checked.pending_len, 0);
                assert_eq!(output.0, source);
            }
        }
    }
    #[test]
    fn invalid_and_truncated_utf8_is_not_completed_by_a_later_write() {
        for source in [
            &b"\xe7x"[..],
            &b"\xc0\x80"[..],
            &b"\xed\xa0\x80"[..],
            &b"\xf4\x90\x80\x80"[..],
            &b"\xf0\x9f"[..],
        ] {
            let mut output = Vec::new();
            let mut checked = Utf8Output {
                output: &mut output,
                pending: [0; 4],
                pending_len: 0,
                invalid: false,
            };
            for byte in source {
                if checked.write_all(&[*byte]).is_err() {
                    break;
                }
            }
            assert!(checked.invalid || checked.pending_len != 0);
            if checked.invalid {
                assert!(checked.write_all(b"later").is_err());
            }
        }
    }
    #[test]
    fn cancellation_stops_after_the_first_interrupted_or_short_output_attempt() {
        use oasis7_formal_protocol::incremental_value::{
            ValueIoBudget, ValueNodeWriter, write_value,
        };
        use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
        use std::{cell::Cell, sync::Arc};

        #[derive(Default)]
        struct Pages(Vec<([u8; 32], ReservedValue<Vec<u8>>)>);
        impl ValueNodeWriter for Pages {
            fn put_node(
                &mut self,
                id: [u8; 32],
                bytes: ReservedValue<Vec<u8>>,
            ) -> Result<(), ValueTreeError> {
                self.0.push((id, bytes));
                Ok(())
            }
        }
        impl ValueNodeReader for Pages {
            fn read_node(
                &self,
                id: &[u8; 32],
                max_bytes: usize,
                memory: &MemoryBudget,
                class: MemoryClass,
            ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
                let bytes = &self.0.iter().find(|(key, _)| key == id).unwrap().1;
                assert!(bytes.len() <= max_bytes);
                Ok(Arc::new(
                    memory.reserve(bytes.len(), class)?.retain(bytes.to_vec()),
                ))
            }
        }
        struct CancelOutput<'a> {
            cancelled: &'a Cell<bool>,
            interrupted: bool,
            calls: usize,
        }
        impl Write for CancelOutput<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.calls += 1;
                // A second call returns success so the broken implementation
                // fails deterministically instead of hanging in write_all.
                if self.calls == 1 {
                    self.cancelled.set(true);
                    if self.interrupted {
                        return Err(io::ErrorKind::Interrupted.into());
                    }
                    return Ok(1.min(bytes.len()));
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
        let quota = || ValueIoBudget {
            nodes_remaining: 100,
            read_bytes_remaining: 1 << 20,
            write_bytes_remaining: 1 << 20,
        };
        let mut encoded = Vec::new();
        write_ledger_value_v1(&MaterialLedgerId::Agent("甲é😀".into()), &mut encoded).unwrap();
        let mut pages = Pages::default();
        let root = write_value(
            &mut &encoded[..],
            encoded.len() as u64,
            &mut pages,
            &memory,
            &mut quota(),
            || false,
        )
        .unwrap()
        .digest()
        .unwrap();
        for interrupted in [true, false] {
            let cancelled = Cell::new(false);
            let mut output = CancelOutput {
                cancelled: &cancelled,
                interrupted,
                calls: 0,
            };
            let mut io = quota();
            let mut context = ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            };
            let result = read_ledger_value_tree_v1(&root, &mut context, 100, &mut output, || {
                cancelled.get()
            });
            assert!(matches!(result, Err(ValueTreeError::Cancelled)));
            assert_eq!(output.calls, 1, "interrupted={interrupted}");
        }
    }
}
