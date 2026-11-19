//! Original rollback wire validation without allocating outcome DTOs or vectors.
use crate::{
    incremental_field_member::MemberPayloadCodecV1 as Codec,
    incremental_rollback_qualification::scope, incremental_rollback_read::read_rollback_cell_v1,
    incremental_rollback_records::RollbackPathV1 as Path,
    incremental_rollback_recovery::RollbackDecodeContextV1,
};
use oasis7_formal_protocol::{
    incremental_state::StateRootV1,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use std::io::{Cursor, Write};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("rollback streaming shape")
}
// Incremental UTF8 decoder carries at most one incomplete codepoint.
#[derive(Default)]
struct Utf8 {
    pending: [u8; 4],
    len: usize,
    invalid: bool,
}
impl Write for Utf8 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        for &byte in bytes {
            self.pending[self.len] = byte;
            self.len += 1;
            match std::str::from_utf8(&self.pending[..self.len]) {
                Ok(_) => self.len = 0,
                Err(e) if e.error_len().is_none() && self.len < 4 => {}
                Err(_) => {
                    self.invalid = true;
                    return Err(std::io::Error::other("rollback UTF8"));
                }
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Validator<'a, 'b, 'v, R, C> {
    root: &'a StateRootV1,
    before: Option<&'a StateRootV1>,
    delta_cells: Option<&'a mut u64>,
    schema: [u8; 32],
    ctx: &'a mut RollbackDecodeContextV1<'b, 'v, R, C>,
}
impl<R: ValueNodeReader, C: FnMut() -> bool> Validator<'_, '_, '_, R, C> {
    fn check(&mut self) -> Result<(), ValueTreeError> {
        if (self.ctx.cancelled)() {
            Err(ValueTreeError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn read(
        &mut self,
        path: &[Path],
        codec: Codec,
        max: u64,
        out: &mut impl Write,
    ) -> Result<u64, ValueTreeError> {
        self.check()?;
        let result = read_rollback_cell_v1(
            self.root,
            self.schema,
            path,
            codec,
            self.ctx.values,
            self.ctx.keys,
            self.ctx.work,
            max,
            self.ctx.limits.max_record_bytes,
            out,
            self.ctx.cancelled,
        );
        self.check()?;
        Ok(result?.payload_bytes)
    }
    fn number(&mut self, path: &[Path]) -> Result<u64, ValueTreeError> {
        let mut b = [0; 8];
        if self.read(path, Codec::U64, 8, &mut Cursor::new(&mut b[..]))? != 8 {
            return Err(invalid());
        }
        Ok(u64::from_be_bytes(b))
    }
    fn boolean(&mut self, path: &[Path]) -> Result<bool, ValueTreeError> {
        let mut b = [0];
        if self.read(path, Codec::Bool, 1, &mut Cursor::new(&mut b[..]))? != 1 {
            return Err(invalid());
        }
        match b[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid()),
        }
    }
    fn text(&mut self, path: &[Path]) -> Result<(), ValueTreeError> {
        if self.unchanged(path)? {
            return Ok(());
        }
        let mut out = Utf8::default();
        self.read(path, Codec::Text, self.ctx.limits.max_text_bytes, &mut out)?;
        if out.invalid || out.len != 0 {
            Err(invalid())
        } else {
            Ok(())
        }
    }
    fn shape(&mut self, path: &[Path], children: u64) -> Result<(), ValueTreeError> {
        self.check()?;
        let root = scope(self.root, self.schema, path, self.ctx)?;
        if root.leaf_count != children.checked_add(2).ok_or_else(invalid)? {
            return Err(invalid());
        }
        // Check both mandatory original markers at every scope, not just its count.
        super::markers(&root, self.ctx)?;
        self.check()
    }
    fn unchanged(&mut self, path: &[Path]) -> Result<bool, ValueTreeError> {
        let Some(before) = self.before else {
            return Ok(false);
        };
        let _scratch = self
            .ctx
            .values
            .memory
            .reserve(8192, self.ctx.values.class)?;
        // A newly added ordinal/Option child has no before scope. Probe each
        // old edge without treating legitimate absence as malformed state.
        let mut old = before.clone();
        for (i, part) in path[..path.len() - 1].iter().enumerate() {
            let mut reader = crate::incremental_inventory_value_read::InventoryValueReader {
                values: self.ctx.values,
                key_work: self.ctx.keys,
                semantic_bytes: self.ctx.limits.max_record_bytes,
                record_bytes: self.ctx.limits.max_record_bytes,
                cancelled: &mut *self.ctx.cancelled,
            };
            let key = match part {
                Path::Column(name) => reader.key(
                    crate::incremental_rollback_genesis::descriptor(),
                    crate::incremental_world_field_key::WorldFieldRecordKindV1::Entry,
                    &("column", *name),
                )?,
                Path::Ordinal(n) => reader.key(
                    crate::incremental_rollback_genesis::descriptor(),
                    crate::incremental_world_field_key::WorldFieldRecordKindV1::Entry,
                    &("ordinal", *n),
                )?,
            };
            if crate::incremental_record_tree::lookup(
                &old,
                &key.0,
                &mut reader,
                crate::incremental_rollback_genesis::LABELS,
            )?
            .is_none()
            {
                return Ok(false);
            }
            old = scope(before, self.schema, &path[..i + 1], self.ctx)?;
        }
        let new = scope(self.root, self.schema, &path[..path.len() - 1], self.ctx)?;
        let mut reader = crate::incremental_inventory_value_read::InventoryValueReader {
            values: self.ctx.values,
            key_work: self.ctx.keys,
            semantic_bytes: self.ctx.limits.max_record_bytes,
            record_bytes: self.ctx.limits.max_record_bytes,
            cancelled: &mut *self.ctx.cancelled,
        };
        let key = match path[path.len() - 1] {
            Path::Column(name) => reader.key(
                crate::incremental_rollback_genesis::descriptor(),
                crate::incremental_world_field_key::WorldFieldRecordKindV1::Entry,
                &("column", name),
            )?,
            Path::Ordinal(i) => reader.key(
                crate::incremental_rollback_genesis::descriptor(),
                crate::incremental_world_field_key::WorldFieldRecordKindV1::Entry,
                &("ordinal", i),
            )?,
        };
        let a = crate::incremental_record_tree::lookup(
            &old,
            &key.0,
            &mut reader,
            crate::incremental_rollback_genesis::LABELS,
        )?;
        let b = crate::incremental_record_tree::lookup(
            &new,
            &key.0,
            &mut reader,
            crate::incremental_rollback_genesis::LABELS,
        )?;
        self.check()?;
        Ok(a.is_some() && a == b)
    }
    fn strings(&mut self, prefix: &[Path], name: &'static str) -> Result<(), ValueTreeError> {
        let mut p = [Path::Column(""); 4];
        p[..prefix.len()].copy_from_slice(prefix);
        let n = prefix.len();
        p[n] = Path::Column(name);
        let inherited = if prefix.is_empty() {
            true
        } else if let Some(before) = self.before {
            validator(before, None, self.ctx)
                .boolean(&[Path::Column("receipt"), Path::Column("present")])?
        } else {
            false
        };
        if inherited
            && let (Some(before), Some(cells)) = (self.before, self.delta_cells.as_deref_mut())
        {
            return super::vector_delta::advance_vector(
                before,
                self.root,
                &p[..n + 1],
                false,
                self.ctx,
                cells,
            );
        }
        if self.unchanged(&p[..n + 1])? {
            return Ok(());
        }
        p[n + 1] = Path::Column("count");
        let count = self.number(&p[..n + 2])?;
        usize::try_from(count).map_err(|_| invalid())?;
        self.shape(&p[..n + 1], count.checked_add(1).ok_or_else(invalid)?)?;
        for i in 0..count {
            p[n + 1] = Path::Ordinal(i);
            self.text(&p[..n + 2])?;
        }
        Ok(())
    }
    fn optional_text(&mut self, prefix: &[Path], name: &'static str) -> Result<(), ValueTreeError> {
        let mut p = [Path::Column(""); 4];
        let n = prefix.len();
        p[..n].copy_from_slice(prefix);
        p[n] = Path::Column(name);
        p[n + 1] = Path::Column("present");
        let present = self.boolean(&p[..n + 2])?;
        self.shape(&p[..n + 1], if present { 2 } else { 1 })?;
        if present {
            p[n + 1] = Path::Column("value");
            self.text(&p[..n + 2])?;
        }
        Ok(())
    }
    fn disposition(&mut self, i: u64) -> Result<(), ValueTreeError> {
        use Path::{Column as K, Ordinal as O};
        let p = [K("dispositions"), O(i)];
        self.shape(&p, 7)?;
        self.text(&[p[0], p[1], K("source_batch_id")])?;
        self.number(&[p[0], p[1], K("source_event_id")])?;
        if self.number(&[p[0], p[1], K("status")])? > 3 {
            return Err(invalid());
        }
        self.boolean(&[p[0], p[1], K("system_authored")])?;
        self.optional_text(&p, "player_id")?;
        self.optional_text(&p, "action_id")?;
        let cp = [p[0], p[1], K("compensation")];
        let some = self.boolean(&[cp[0], cp[1], cp[2], K("present")])?;
        self.shape(&cp, if some { 4 } else { 1 })?;
        if some {
            for name in ["owner_id", "ticket_id"] {
                self.text(&[cp[0], cp[1], cp[2], K(name)])?;
            }
            if self.number(&[cp[0], cp[1], cp[2], K("state")])? > 4 {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn outcome(&mut self) -> Result<(), ValueTreeError> {
        use Path::Column as K;
        for name in [
            "canonical_intent_hash",
            "rollback_checkpoint_batch_id",
            "rollback_checkpoint_snapshot_hash",
            "rollback_ticket",
            "target_journal_commitment",
            "target_state_root",
            "affected_census_digest",
            "target_batch_id",
        ] {
            self.text(&[K(name)])?;
        }
        for name in [
            "rollback_checkpoint_journal_len",
            "target_journal_len",
            "affected_census_count",
        ] {
            usize::try_from(self.number(&[K(name)])?).map_err(|_| invalid())?;
        }
        for name in [
            "rollback_event_id",
            "prior_reorg_epoch",
            "committed_reorg_epoch",
        ] {
            self.number(&[K(name)])?;
        }
        self.strings(&[], "readiness_blockers")?;
        self.strings(&[], "invalidated_batch_ids")?;
        if let (Some(before), Some(cells)) = (self.before, self.delta_cells.as_deref_mut()) {
            super::vector_delta::advance_vector(
                before,
                self.root,
                &[K("dispositions")],
                true,
                self.ctx,
                cells,
            )?;
        } else if !self.unchanged(&[K("dispositions")])? {
            let count = self.number(&[K("dispositions"), K("count")])?;
            usize::try_from(count).map_err(|_| invalid())?;
            self.shape(
                &[K("dispositions")],
                count.checked_add(1).ok_or_else(invalid)?,
            )?;
            for i in 0..count {
                self.disposition(i)?;
            }
        }
        if !self.unchanged(&[K("receipt")])? {
            let receipt = self.boolean(&[K("receipt"), K("present")])?;
            self.shape(&[K("receipt")], if receipt { 17 } else { 1 })?;
            if receipt {
                for name in [
                    "receipt_id",
                    "canonical_intent_digest",
                    "rollback_checkpoint_batch_id",
                    "rollback_checkpoint_snapshot_hash",
                    "replay_target_batch_id",
                    "replay_target_state_root",
                    "replay_target_journal_commitment",
                    "affected_census_digest",
                    "snapshot_hash",
                ] {
                    self.text(&[K("receipt"), K(name)])?;
                }
                for name in [
                    "rollback_checkpoint_journal_len",
                    "replay_target_journal_len",
                    "affected_census_count",
                ] {
                    usize::try_from(self.number(&[K("receipt"), K(name)])?)
                        .map_err(|_| invalid())?;
                }
                for name in ["snapshot_height", "log_cursor", "acknowledged_at_tick"] {
                    self.number(&[K("receipt"), K(name)])?;
                }
                self.strings(&[K("receipt")], "readiness_blockers")?;
            }
        }
        self.shape(&[], 18)?;
        self.check()
    }
}
pub(super) fn qualify_outcome<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    let _charge = ctx.values.memory.reserve(16384, ctx.values.class)?;
    Validator {
        root,
        before: None,
        delta_cells: None,
        schema: *blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
        ctx,
    }
    .outcome()
}

/// Only the private complete-family receiver may inherit this before graph.
pub(super) fn advance_outcome<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
    diff_cells: &mut u64,
) -> Result<(), ValueTreeError> {
    let _charge = ctx.values.memory.reserve(16384, ctx.values.class)?;
    Validator {
        root: after,
        before: Some(before),
        delta_cells: Some(diff_cells),
        schema: *blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
        ctx,
    }
    .outcome()
}

fn validator<'a, 'b, 'v, R, C>(
    root: &'a StateRootV1,
    before: Option<&'a StateRootV1>,
    ctx: &'a mut RollbackDecodeContextV1<'b, 'v, R, C>,
) -> Validator<'a, 'b, 'v, R, C> {
    Validator {
        root,
        before,
        delta_cells: None,
        schema: *blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
        ctx,
    }
}
pub(super) fn vector_count<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    prefix: &[Path],
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<u64, ValueTreeError> {
    let mut p = [Path::Column(""); 4];
    p[..prefix.len()].copy_from_slice(prefix);
    p[prefix.len()] = Path::Column("count");
    let count = validator(root, None, ctx).number(&p[..prefix.len() + 1])?;
    usize::try_from(count).map_err(|_| invalid())?;
    Ok(count)
}
pub(super) fn vector_shape<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    prefix: &[Path],
    count: u64,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    validator(root, None, ctx).shape(prefix, count.checked_add(1).ok_or_else(invalid)?)
}
pub(super) fn validate_disposition_at<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    i: u64,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    validator(after, Some(before), ctx).disposition(i)
}
pub(super) fn validate_vector_text_at<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    prefix: &[Path],
    i: u64,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    let mut p = [Path::Column(""); 4];
    p[..prefix.len()].copy_from_slice(prefix);
    p[prefix.len()] = Path::Ordinal(i);
    validator(after, Some(before), ctx).text(&p[..prefix.len() + 1])
}
