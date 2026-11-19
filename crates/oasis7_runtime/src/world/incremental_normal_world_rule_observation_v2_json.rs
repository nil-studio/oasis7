//! The sole format-2 JSON grammar. Byte arrays are JSON integer arrays, never
//! caller text; fixed field/namespace order and transaction BTree key order.
use super::*;
use std::cell::RefCell;
struct Encoder<'a, 'b, 'c, 'd, W> {
    out: &'a mut W,
    common: &'b RefCell<&'c mut FormalBoundedContextV1<'d>>,
    buffer: [u8; 4096],
    used: usize,
}
impl<W: Write> Encoder<'_, '_, '_, '_, W> {
    fn flush_buffer(&mut self) -> Result<(), ObservationCurrentSourceErrorV2> {
        if self.used == 0 {
            return Ok(());
        }
        self.common.borrow_mut().charge(1, self.used as u64)?;
        self.out.write_all(&self.buffer[..self.used])?;
        self.used = 0;
        Ok(())
    }
    fn raw(&mut self, value: &[u8]) -> Result<(), ObservationCurrentSourceErrorV2> {
        for &byte in value {
            if self.used == self.buffer.len() {
                self.flush_buffer()?;
            }
            self.buffer[self.used] = byte;
            self.used += 1;
        }
        Ok(())
    }
    fn number(&mut self, n: u8) -> Result<(), ObservationCurrentSourceErrorV2> {
        let mut digits = [0u8; 3];
        let count = if n >= 100 {
            digits[0] = b'0' + n / 100;
            digits[1] = b'0' + n / 10 % 10;
            digits[2] = b'0' + n % 10;
            3
        } else if n >= 10 {
            digits[0] = b'0' + n / 10;
            digits[1] = b'0' + n % 10;
            2
        } else {
            digits[0] = b'0' + n;
            1
        };
        self.raw(&digits[..count])
    }
    fn bytes(&mut self, value: &[u8]) -> Result<(), ObservationCurrentSourceErrorV2> {
        self.common.borrow_mut().charge(1, value.len() as u64)?;
        self.raw(b"[")?;
        for (i, &byte) in value.iter().enumerate() {
            if i != 0 {
                self.raw(b",")?;
            }
            self.number(byte)?;
        }
        self.raw(b"]")
    }
    fn optional(&mut self, value: Option<[u8; 32]>) -> Result<(), ObservationCurrentSourceErrorV2> {
        match value {
            Some(value) => self.bytes(&value),
            None => self.raw(b"null"),
        }
    }
}
pub(super) fn write_original<M: MerkleNodeReader, W: Write>(
    tx: &ExecutionTransaction<'_, M>,
    parent: ParentIds,
    out: &mut W,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), ObservationCurrentSourceErrorV2> {
    // Keep one continuous caller meter in both visitor callbacks. No fresh work
    // or caller-supplied change rows can enter this grammar.
    write_inner(tx, parent, out, common)
}
fn write_inner<'c, M: MerkleNodeReader, W: Write>(
    tx: &ExecutionTransaction<'_, M>,
    parent: ParentIds,
    out: &mut W,
    common: &'c mut FormalBoundedContextV1<'_>,
) -> Result<(), ObservationCurrentSourceErrorV2> {
    let common = RefCell::new(common);
    let mut encoder = Encoder {
        out,
        common: &common,
        buffer: [0; 4096],
        used: 0,
    };
    encoder.raw(b"{\"domain\":\"")?;
    encoder.raw(OBSERVATION_CURRENT_DOMAIN_V2.as_bytes())?;
    encoder.raw(b"\",\"format\":2,\"parent_record_id\":")?;
    encoder.bytes(&parent.record)?;
    encoder.raw(b",\"parent_body_id\":")?;
    encoder.bytes(&parent.body)?;
    encoder.raw(b",\"parent_roots\":")?;
    encoder.bytes(&parent.roots)?;
    encoder.raw(b",\"namespaces\":[")?;
    let mut remaining = EXECUTION_MAX_READS
        .checked_add(EXECUTION_MAX_WRITES)
        .ok_or_else(limited)?;
    for (index, namespace) in NAMESPACES.into_iter().enumerate() {
        if index != 0 {
            encoder.raw(b",")?;
        }
        encoder.raw(b"{\"namespace\":")?;
        encoder.number((index + 1) as u8)?;
        encoder.raw(b",\"changes\":[")?;
        let mut first = true;
        tx.visit_pending_writes(
            namespace,
            &mut remaining,
            || {
                common.borrow_mut().consume_steps(1)?;
                Ok::<_, ObservationCurrentSourceErrorV2>(())
            },
            |key, before, after| {
                if !first {
                    encoder.raw(b",")?;
                }
                first = false;
                encoder.raw(b"[")?;
                encoder.bytes(key)?;
                encoder.raw(b",")?;
                encoder.optional(before)?;
                encoder.raw(b",")?;
                encoder.optional(after)?;
                encoder.raw(b"]")
            },
        )?;
        encoder.raw(b"]}")?;
    }
    encoder.raw(b"]}")?;
    encoder.flush_buffer()?;
    common.borrow_mut().consume_steps(0)?;
    Ok(())
}
