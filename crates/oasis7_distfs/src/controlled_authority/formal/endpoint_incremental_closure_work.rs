//! Stack-only loans of the original candidate accounts. A failed operation
//! keeps every observable debit; an unobservable or unwinding lane stays zero.
use super::super::super::originals::DurableOriginalWorkV1;
use super::super::super::{NativeChunkReadWorkV1, StoreSessions};
use super::{FormalError, page_store_formal_error};
use crate::incremental_page_store::{IncrementalPageStore, PageIoBudget, ValuePageSession};
use std::{
    cell::RefCell,
    ops::{Deref, DerefMut},
    sync::atomic::AtomicBool,
};

#[derive(Default)]
pub(super) struct Settlement {
    error: RefCell<Option<FormalError>>,
}
impl Settlement {
    fn failed(&self, error: FormalError) {
        // Only these stack loans can borrow this cell. Keep the first failed
        // balance observation without replacing a producer's original error.
        self.error.borrow_mut().get_or_insert(error);
    }
    pub(super) fn finish<T>(self, result: Result<T, FormalError>) -> Result<T, FormalError> {
        let value = result?;
        if let Some(error) = self.error.into_inner() {
            return Err(error);
        }
        Ok(value)
    }
}

fn empty_io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 0,
        read_bytes_remaining: 0,
        writes_remaining: 0,
        write_bytes_remaining: 0,
    }
}

pub(super) struct OriginalsLoan<'a> {
    occurrences: &'a mut u64,
    bytes: &'a mut u64,
    pub(super) work: DurableOriginalWorkV1,
}
impl<'a> OriginalsLoan<'a> {
    pub(super) fn new(occurrences: &'a mut u64, bytes: &'a mut u64) -> Self {
        let work = DurableOriginalWorkV1 {
            occurrences_remaining: std::mem::take(occurrences),
            bytes_remaining: std::mem::take(bytes),
        };
        Self {
            occurrences,
            bytes,
            work,
        }
    }
}
impl Drop for OriginalsLoan<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            *self.occurrences = self.work.occurrences_remaining;
            *self.bytes = self.work.bytes_remaining;
        }
    }
}

pub(super) struct NativeLoan<'a> {
    occurrences: &'a mut u64,
    bytes: &'a mut u64,
    frozen_bytes: &'a mut u64,
    pub(super) work: NativeChunkReadWorkV1,
}
impl<'a> NativeLoan<'a> {
    pub(super) fn new(
        occurrences: &'a mut u64,
        bytes: &'a mut u64,
        frozen_bytes: &'a mut u64,
    ) -> Self {
        let work = NativeChunkReadWorkV1 {
            occurrences_remaining: std::mem::take(occurrences),
            bytes_remaining: std::mem::take(bytes),
            frozen_wire_bytes_remaining: std::mem::take(frozen_bytes),
        };
        Self {
            occurrences,
            bytes,
            frozen_bytes,
            work,
        }
    }
}
impl Drop for NativeLoan<'_> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            *self.occurrences = self.work.occurrences_remaining;
            *self.bytes = self.work.bytes_remaining;
            *self.frozen_bytes = self.work.frozen_wire_bytes_remaining;
        }
    }
}

pub(super) struct WriterLoan<'work, 'pages> {
    target: &'work mut PageIoBudget,
    settlement: &'work Settlement,
    writer: ValuePageSession<'pages>,
}
impl<'work, 'pages> WriterLoan<'work, 'pages> {
    pub(super) fn new(
        pages: &'pages IncrementalPageStore,
        target: &'work mut PageIoBudget,
        settlement: &'work Settlement,
    ) -> Self {
        let writer = pages.value_session(std::mem::replace(target, empty_io()));
        Self {
            target,
            settlement,
            writer,
        }
    }
}
impl<'pages> Deref for WriterLoan<'_, 'pages> {
    type Target = ValuePageSession<'pages>;
    fn deref(&self) -> &Self::Target {
        &self.writer
    }
}
impl DerefMut for WriterLoan<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.writer
    }
}
impl Drop for WriterLoan<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        match self.writer.remaining_io() {
            Ok(remaining) => *self.target = remaining,
            Err(error) => self.settlement.failed(page_store_formal_error(error)),
        }
    }
}

pub(super) struct PublicationLoan<'work, 'pages> {
    target: &'work mut PageIoBudget,
    memo_steps: &'work mut u64,
    settlement: &'work Settlement,
    sessions: Option<StoreSessions<'pages>>,
}
impl<'work, 'pages> PublicationLoan<'work, 'pages> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        pages: &'pages IncrementalPageStore,
        state_io: PageIoBudget,
        value_io: PageIoBudget,
        target: &'work mut PageIoBudget,
        max_synced_pages: usize,
        memo_steps: &'work mut u64,
        cancelled: &'pages AtomicBool,
        settlement: &'work Settlement,
    ) -> Result<Self, FormalError> {
        // Failed constructors do not expose their consumed sessions. Leave only
        // these transferred lanes burned instead of reconstructing their caps.
        let sessions = StoreSessions::new(
            pages,
            state_io,
            value_io,
            std::mem::replace(target, empty_io()),
            max_synced_pages,
            std::mem::take(memo_steps),
            cancelled,
        )?;
        Ok(Self {
            target,
            memo_steps,
            settlement,
            sessions: Some(sessions),
        })
    }
    pub(super) fn with_state_read_memo(
        mut self,
        entries: usize,
        bytes: usize,
    ) -> Result<Self, FormalError> {
        let sessions = self.sessions.take().expect("live publication sessions");
        self.sessions = Some(sessions.with_state_read_memo(entries, bytes)?);
        Ok(self)
    }
}
impl<'pages> Deref for PublicationLoan<'_, 'pages> {
    type Target = StoreSessions<'pages>;
    fn deref(&self) -> &Self::Target {
        self.sessions.as_ref().expect("live publication sessions")
    }
}
impl Drop for PublicationLoan<'_, '_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        if let Some(sessions) = self.sessions.as_ref() {
            match sessions.remaining_publish_io() {
                Ok(physical) => *self.target = physical,
                Err(error) => self.settlement.failed(error),
            }
            match sessions.remaining_memo_steps() {
                Ok(steps) => *self.memo_steps = steps,
                Err(error) => self.settlement.failed(error),
            }
        }
    }
}

#[cfg(all(test, unix))]
#[path = "endpoint_incremental_closure_work_tests.rs"]
mod tests;
