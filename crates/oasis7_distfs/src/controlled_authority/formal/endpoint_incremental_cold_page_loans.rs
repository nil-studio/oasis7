//! Actual quota moves into physical sessions, returning only their real
//! remaining counters. The emptied caller slot cannot be concurrently used.
use crate::incremental_page_store::{
    IncrementalPageStore, PageIoBudget, PageStoreError, StatePageSession, ValuePageSession,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::sync::atomic::AtomicBool;
fn take(slot: &mut PageIoBudget) -> PageIoBudget {
    std::mem::replace(
        slot,
        PageIoBudget {
            reads_remaining: 0,
            read_bytes_remaining: 0,
            writes_remaining: 0,
            write_bytes_remaining: 0,
        },
    )
}
fn snapshot(io: &PageIoBudget) -> [u64; 4] {
    [
        io.reads_remaining,
        io.read_bytes_remaining,
        io.writes_remaining,
        io.write_bytes_remaining,
    ]
}
fn restore(
    slot: &mut PageIoBudget,
    remaining: Result<PageIoBudget, PageStoreError>,
    ceiling: [u64; 4],
) {
    if let Ok(remaining) = remaining
        && snapshot(&remaining)
            .into_iter()
            .zip(ceiling)
            .all(|(after, before)| after <= before)
    {
        *slot = remaining;
    }
    // A poisoned/untrusted session keeps the emptied slot empty. The Common
    // exclusive child also fences its full balance on unwind.
}
pub(super) struct StateLoan<'work, 'pages> {
    session: StatePageSession<'pages>,
    slot: &'work mut PageIoBudget,
    ceiling: [u64; 4],
}
impl<'work, 'pages> StateLoan<'work, 'pages> {
    pub(super) fn new(
        pages: &'pages IncrementalPageStore,
        slot: &'work mut PageIoBudget,
        class: MemoryClass,
        cancelled: &'pages AtomicBool,
        memo: Option<(usize, usize)>,
    ) -> Result<Self, PageStoreError> {
        let io = take(slot);
        let ceiling = snapshot(&io);
        // Constructor/memo installation performs no page IO. Failed admission
        // returns the untouched original finite counters rather than minting.
        let session = match pages.state_session_with_cancelled(io, class, cancelled) {
            Ok(session) => session,
            Err(error) => {
                *slot = PageIoBudget {
                    reads_remaining: ceiling[0],
                    read_bytes_remaining: ceiling[1],
                    writes_remaining: ceiling[2],
                    write_bytes_remaining: ceiling[3],
                };
                return Err(error);
            }
        };
        let session = if let Some((entries, bytes)) = memo {
            match session.with_read_memo(entries, bytes) {
                Ok(session) => session,
                Err(error) => {
                    *slot = PageIoBudget {
                        reads_remaining: ceiling[0],
                        read_bytes_remaining: ceiling[1],
                        writes_remaining: ceiling[2],
                        write_bytes_remaining: ceiling[3],
                    };
                    return Err(error);
                }
            }
        } else {
            session
        };
        Ok(Self {
            session,
            slot,
            ceiling,
        })
    }
    pub(super) fn source(&self) -> &StatePageSession<'pages> {
        &self.session
    }
}
impl Drop for StateLoan<'_, '_> {
    fn drop(&mut self) {
        restore(self.slot, self.session.remaining_io(), self.ceiling);
    }
}
pub(super) struct ValueLoan<'work, 'pages> {
    session: ValuePageSession<'pages>,
    slot: &'work mut PageIoBudget,
    ceiling: [u64; 4],
}
impl<'work, 'pages> ValueLoan<'work, 'pages> {
    pub(super) fn new(
        pages: &'pages IncrementalPageStore,
        slot: &'work mut PageIoBudget,
        cancelled: &'pages AtomicBool,
    ) -> Self {
        let io = take(slot);
        let ceiling = snapshot(&io);
        let session = pages.value_session_with_cancelled(io, cancelled);
        Self {
            session,
            slot,
            ceiling,
        }
    }
    pub(super) fn source(&self) -> &ValuePageSession<'pages> {
        &self.session
    }
    pub(super) fn sink(&mut self) -> &mut ValuePageSession<'pages> {
        &mut self.session
    }
}
impl Drop for ValueLoan<'_, '_> {
    fn drop(&mut self) {
        restore(self.slot, self.session.remaining_io(), self.ceiling);
    }
}
