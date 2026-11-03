//! Dense live mailbox interval relations over a qualified parent.
use super::*;
use crate::incremental_event_fields::{
    MAILBOX_EVENT_REFERENCE_BYTES, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1,
    mailbox_event_id_v1,
};
pub(super) struct MailboxDelta<'a> {
    pub agent: &'a str,
    old: MailboxFrontierV1,
    new: MailboxFrontierV1,
    added: u64,
    removed: u64,
}
impl<'a> MailboxDelta<'a> {
    pub(super) fn new<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        record: &TransitionRecordV1,
        agent: &'a str,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        let key = MailboxKeyV1::new(agent, None).map_err(ValueTreeError::Invalid)?;
        let old = ctx
            .lookup_raw(
                record.before.state_root(
                    oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
                ),
                key.as_bytes(),
            )?
            .ok_or_else(invalid)?;
        let new = changed_key(
            record,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
            key.as_bytes(),
        )
        .map(|(_, after)| after)
        .unwrap_or(Some(old))
        .ok_or_else(invalid)?;
        let mut values = [MailboxFrontierV1 { head: 0, tail: 0 }; 2];
        for (slot, value) in [old, new].into_iter().enumerate() {
            ctx.values
                .verify_closure(&value, MAILBOX_FRONTIER_BYTES as u64, &mut ctx.cancelled)?;
            values[slot] = MailboxFrontierV1::from_canonical_bytes(&read::fixed_tree::<
                MAILBOX_FRONTIER_BYTES,
                _,
                _,
            >(
                &value,
                &mut ctx.values,
                &mut ctx.cancelled,
            )?)
            .map_err(ValueTreeError::Invalid)?;
        }
        let [old, new] = values;
        if new.head < old.head || new.tail < old.tail {
            return Err(invalid());
        }
        Ok(Self {
            agent,
            old,
            new,
            added: 0,
            removed: 0,
        })
    }
    pub(super) fn account<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        &mut self,
        ordinal: Option<u64>,
        mutation: &oasis7_formal_protocol::incremental_state::StateWriteV1,
        ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    ) -> Result<(), ValueTreeError> {
        let Some(ordinal) = ordinal else {
            return Ok(());
        };
        let before = self.old.head <= ordinal && ordinal < self.old.tail;
        let after = self.new.head <= ordinal && ordinal < self.new.tail;
        if mutation.before.is_some() != before || mutation.after.is_some() != after {
            return Err(invalid());
        }
        if let Some(value) = mutation.after {
            ctx.values.verify_closure(
                &value,
                MAILBOX_EVENT_REFERENCE_BYTES as u64,
                &mut ctx.cancelled,
            )?;
            mailbox_event_id_v1(&read::fixed_tree::<MAILBOX_EVENT_REFERENCE_BYTES, _, _>(
                &value,
                &mut ctx.values,
                &mut ctx.cancelled,
            )?)
            .map_err(ValueTreeError::Invalid)?;
        }
        match (mutation.before.is_some(), mutation.after.is_some()) {
            (false, true) => self.added = self.added.checked_add(1).ok_or_else(invalid)?,
            (true, false) => self.removed = self.removed.checked_add(1).ok_or_else(invalid)?,
            (true, true) => (),
            (false, false) => return Err(invalid()),
        }
        ctx.check()?;
        Ok(())
    }
    pub(super) fn finish(&self) -> Result<i128, ValueTreeError> {
        let added = self.new.tail - self.old.tail.max(self.new.head);
        let removed = self.old.tail.min(self.new.head) - self.old.head;
        if self.added != added || self.removed != removed {
            return Err(invalid());
        }
        Ok(i128::from(self.new.tail - self.new.head) - i128::from(self.old.tail - self.old.head))
    }
}
