//! Bounded test diagnostics; no protected clock or private values are observed.
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::Write,
    marker::PhantomData,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
const CAPACITY: usize = 4096;
thread_local! {
    static CURRENT: RefCell<Option<Handle>> = const { RefCell::new(None) };
    static CALLBACK: RefCell<Option<(Handle, u64)>> = const { RefCell::new(None) };
}
type Handle = Arc<Collector>;
struct Collector {
    trace: Mutex<Trace>,
    dropped: AtomicU64,
}
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum Method {
    LegacyCommit,
    Configure,
    CurrentContext,
    Build,
    Preview,
    Retain,
    Pending,
    Export,
    EnableMembership,
    LookupPublic,
    RetainReceived,
    Load,
    ValidatePayload,
    ValidateBatch,
    HistoryPage,
    CommitCertificate,
}
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum Stage {
    LedgerReload,
    UsageReserve,
    VerifiedCut,
    InitialHandoff,
    OriginMerge,
    LiveHandoff,
    HistoryReplay,
}
struct Record {
    sequence: u64,
    parent: Option<u64>,
    hook: u64,
    method: Option<Method>,
    stage: Option<Stage>,
    height: Option<u64>,
    entered_ns: u128,
    acquired_ns: Option<u128>,
    returned_ns: Option<u128>,
    ok: Option<bool>,
    count: Option<u64>,
}
struct Trace {
    start: Instant,
    records: VecDeque<Record>,
    next: u64,
    bindings: u64,
    evicted: u64,
    evicted_incomplete: u64,
    closed: bool,
}
impl Collector {
    fn new() -> Handle {
        Arc::new(Self {
            trace: Mutex::new(Trace {
                start: Instant::now(),
                records: VecDeque::new(),
                next: 0,
                bindings: 0,
                evicted: 0,
                evicted_incomplete: 0,
                closed: false,
            }),
            dropped: AtomicU64::new(0),
        })
    }
    fn update(&self, f: impl FnOnce(&mut Trace)) {
        if let Ok(mut trace) = self.trace.try_lock() {
            if !trace.closed {
                f(&mut trace);
            }
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}
fn insert(trace: &mut Trace, mut record: Record) -> u64 {
    if trace.records.len() == CAPACITY
        && let Some(old) = trace.records.pop_front()
    {
        trace.evicted += 1;
        trace.evicted_incomplete += u64::from(old.returned_ns.is_none());
    }
    let sequence = trace.next;
    trace.next += 1;
    record.sequence = sequence;
    record.entered_ns = trace.start.elapsed().as_nanos();
    trace.records.push_back(record);
    sequence
}
pub(crate) struct Scope {
    handle: Handle,
    previous: Option<Handle>,
    label: &'static str,
    _not_send: PhantomData<Rc<()>>,
}
impl Scope {
    pub(crate) fn begin(label: &'static str) -> Self {
        let handle = Collector::new();
        let previous = CURRENT.with(|slot| slot.replace(Some(handle.clone())));
        Self {
            handle,
            previous,
            label,
            _not_send: PhantomData,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|slot| {
            slot.replace(self.previous.take());
        });
        let output = std::thread::panicking()
            || std::env::var("OASIS7_BFT_TEST_STAGE_TRACE").as_deref() == Ok("1");
        // No output is performed on callback threads or while a business lock is held.
        let Ok(mut trace) = self.handle.trace.try_lock() else {
            if output {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "BFT_ACTUAL_HOOK_TRACE scope={} collector=unknown",
                    self.label
                );
            }
            return;
        };
        trace.closed = true;
        let records = std::mem::take(&mut trace.records);
        let (evicted, incomplete, bindings) =
            (trace.evicted, trace.evicted_incomplete, trace.bindings);
        let unfinished = records.iter().filter(|r| r.returned_ns.is_none()).count();
        drop(trace);
        if !output {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(
            stderr,
            "BFT_ACTUAL_HOOK_TRACE scope={} capacity={} records={} bound_hooks={} evicted={} evicted_incomplete={} dropped={} incomplete={} lost_or_unfinished=unknown external_node_mutex_ns=unknown broadcast_ns=unknown unscoped_hooks=unknown",
            self.label,
            CAPACITY,
            records.len(),
            bindings,
            evicted,
            incomplete,
            self.handle.dropped.load(Ordering::Relaxed),
            unfinished
        );
        for r in records {
            let _ = writeln!(
                stderr,
                "BFT_ACTUAL_HOOK_TRACE sequence={} parent={:?} hook={} method={:?} stage={:?} height={:?} entered_ns={} acquired_ns={:?} returned_ns={:?} ok={:?} count={:?} completion={}",
                r.sequence,
                r.parent,
                r.hook,
                r.method.map(|v| v as u8),
                r.stage.map(|v| v as u8),
                r.height,
                r.entered_ns,
                r.acquired_ns,
                r.returned_ns,
                r.ok,
                r.count,
                if r.returned_ns.is_some() {
                    "completed"
                } else {
                    "unknown"
                }
            );
        }
    }
}
pub(crate) struct Binding {
    handle: Handle,
    ordinal: u64,
}
pub(crate) fn current() -> Option<Binding> {
    let handle = CURRENT.with(|slot| slot.borrow().clone())?;
    let mut trace = match handle.trace.try_lock() {
        Ok(trace) => trace,
        Err(_) => {
            handle.dropped.fetch_add(1, Ordering::Relaxed);
            return None;
        }
    };
    let ordinal = trace.bindings;
    trace.bindings += 1;
    drop(trace);
    Some(Binding { handle, ordinal })
}
pub(crate) struct Call {
    handle: Handle,
    sequence: u64,
    start: Instant,
}
impl Call {
    pub(crate) fn enter(
        binding: &Option<Binding>,
        method: Method,
        height: Option<u64>,
    ) -> Option<Self> {
        let binding = binding.as_ref()?;
        let handle = binding.handle.clone();
        let start = Instant::now();
        let mut trace = match handle.trace.try_lock() {
            Ok(trace) => trace,
            Err(_) => {
                handle.dropped.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        };
        if trace.closed {
            return None;
        }
        let sequence = insert(
            &mut trace,
            Record {
                sequence: 0,
                parent: None,
                hook: binding.ordinal,
                method: Some(method),
                stage: None,
                height,
                entered_ns: 0,
                acquired_ns: None,
                returned_ns: None,
                ok: None,
                count: None,
            },
        );
        drop(trace);
        Some(Self {
            handle,
            sequence,
            start,
        })
    }
    pub(crate) fn acquired(call: &Option<Self>) {
        let Some(call) = call else { return };
        call.handle.update(|trace| {
            if let Some(r) = trace
                .records
                .iter_mut()
                .find(|r| r.sequence == call.sequence)
            {
                r.acquired_ns = Some(call.start.elapsed().as_nanos());
            }
        });
    }
    pub(crate) fn returned(call: &Option<Self>, ok: bool) {
        let Some(call) = call else { return };
        call.handle.update(|trace| {
            if let Some(r) = trace
                .records
                .iter_mut()
                .find(|r| r.sequence == call.sequence)
            {
                r.returned_ns = Some(call.start.elapsed().as_nanos());
                r.ok = Some(ok);
            }
        });
    }
}
// Explicitly rebind the originating call on each actual callback thread. The
// guard cannot move to another thread and restores any nested parent context.
pub(crate) struct CallContext {
    previous: Option<(Handle, u64)>,
    _not_send: PhantomData<Rc<()>>,
}
impl CallContext {
    pub(crate) fn bind(call: &Option<Call>) -> Self {
        let next = call
            .as_ref()
            .map(|call| (call.handle.clone(), call.sequence));
        let previous = CALLBACK.with(|slot| slot.replace(next));
        Self {
            previous,
            _not_send: PhantomData,
        }
    }
}
impl Drop for CallContext {
    fn drop(&mut self) {
        CALLBACK.with(|slot| {
            slot.replace(self.previous.take());
        });
    }
}
pub(crate) struct Span {
    handle: Handle,
    sequence: u64,
    start: Instant,
}
impl Span {
    pub(crate) fn enter(stage: Stage, height: u64) -> Option<Self> {
        let (handle, parent) = CALLBACK.with(|slot| slot.borrow().clone())?;
        let start = Instant::now();
        let mut trace = match handle.trace.try_lock() {
            Ok(trace) => trace,
            Err(_) => {
                handle.dropped.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        };
        if trace.closed {
            return None;
        }
        let hook = match trace.records.iter().find(|r| r.sequence == parent) {
            Some(r) => r.hook,
            None => {
                handle.dropped.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        };
        let sequence = insert(
            &mut trace,
            Record {
                sequence: 0,
                parent: Some(parent),
                hook,
                method: None,
                stage: Some(stage),
                height: Some(height),
                entered_ns: 0,
                acquired_ns: None,
                returned_ns: None,
                ok: None,
                count: None,
            },
        );
        drop(trace);
        Some(Self {
            handle,
            sequence,
            start,
        })
    }
    pub(crate) fn complete(span: Option<Self>, count: Option<u64>) {
        let Some(span) = span else { return };
        span.handle.update(|trace| {
            if let Some(r) = trace
                .records
                .iter_mut()
                .find(|r| r.sequence == span.sequence)
            {
                r.returned_ns = Some(span.start.elapsed().as_nanos());
                r.ok = Some(true);
                r.count = count;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Option<Binding> {
        Some(Binding {
            handle: Collector::new(),
            ordinal: 0,
        })
    }
    #[test]
    fn callback_handle_crosses_threads_without_activating_an_unrelated_case() {
        let a = binding();
        let b = binding();
        let call = Call::enter(&a, Method::Preview, Some(9));
        let handle = a.as_ref().unwrap().handle.clone();
        std::thread::spawn(move || {
            assert!(Span::enter(Stage::LedgerReload, 9).is_none());
            let _context = CallContext::bind(&call);
            Span::complete(Span::enter(Stage::LedgerReload, 9), None);
            let other = Call::enter(&b, Method::Build, Some(10));
            {
                let _nested = CallContext::bind(&other);
                Span::complete(Span::enter(Stage::LiveHandoff, 10), None);
            }
            Span::complete(Span::enter(Stage::HistoryReplay, 9), Some(2));
        })
        .join()
        .unwrap();
        let trace = handle.trace.lock().unwrap();
        assert_eq!(trace.records.len(), 3);
        assert!(trace.records.iter().all(|r| r.height == Some(9)));
        assert_eq!(trace.records[2].count, Some(2));
        assert_eq!(trace.records[2].parent, Some(0));
    }
    #[test]
    fn unfinished_and_evicted_spans_remain_unknown_and_capacity_is_bounded() {
        let binding = binding();
        let call = Call::enter(&binding, Method::Preview, Some(9));
        let _context = CallContext::bind(&call);
        let unfinished = Span::enter(Stage::VerifiedCut, 9);
        drop(unfinished);
        let handle = binding.as_ref().unwrap().handle.clone();
        assert!(
            handle.trace.lock().unwrap().records[1]
                .returned_ns
                .is_none()
        );
        // Calls remain bound even when their parent record is evicted.
        for _ in 0..CAPACITY + 2 {
            let _ = Call::enter(&binding, Method::Build, Some(9));
        }
        let trace = handle.trace.lock().unwrap();
        assert_eq!(trace.records.len(), CAPACITY);
        assert_eq!(trace.evicted, 4);
        assert_eq!(trace.evicted_incomplete, 4);
    }
    #[test]
    fn collector_contention_drops_observations_without_waiting_or_completing() {
        let binding = binding();
        let call = Call::enter(&binding, Method::CurrentContext, Some(9));
        let _context = CallContext::bind(&call);
        let handle = binding.as_ref().unwrap().handle.clone();
        let trace = handle.trace.lock().unwrap();
        assert!(Span::enter(Stage::LedgerReload, 9).is_none());
        Call::returned(&call, true);
        assert_eq!(handle.dropped.load(Ordering::Relaxed), 2);
        assert!(trace.records[0].returned_ns.is_none());
    }
}
