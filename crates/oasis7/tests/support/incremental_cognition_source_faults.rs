use super::*;
use oasis7_formal_protocol::incremental_execution::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    incremental_cognition_control_overlay::*,
    incremental_cognition_family::*,
    incremental_cognition_json_overlay::*,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_json_records::*,
};
use std::{cell::Cell, sync::Arc};
#[derive(Clone, Copy, Debug)]
enum Fault {
    Foreign,
    Undercharged,
    Quota,
    Cancelled,
    Missing,
}
struct Hostile<'a, R> {
    source: &'a R,
    armed: &'a Cell<bool>,
    foreign: &'a MemoryBudget,
    fault: Fault,
    copies: Cell<u64>,
}
impl<R: ValueNodeReader> ValueNodeReader for Hostile<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if !self.armed.get() {
            return self.source.read_node(id, max, memory, class);
        }
        match self.fault {
            Fault::Quota => {
                return Err(ValueTreeError::ResourceLimited(
                    "hostile Cognition source quota",
                ));
            }
            Fault::Cancelled => return Err(ValueTreeError::Cancelled),
            Fault::Missing => {
                return Err(ValueTreeError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "hostile Cognition missing node",
                )));
            }
            _ => {}
        }
        let bytes = self.source.read_node(id, max, memory, class)?;
        self.copies.set(self.copies.get() + 1);
        assert_eq!(self.copies.get(), 1);
        // The enclosing adversarial fixture separately retains the copy's actual
        // heap admission. The returned carrier deliberately lies about its credit.
        let charge = match self.fault {
            Fault::Foreign => self.foreign.reserve(bytes.len() + 8192, class)?,
            Fault::Undercharged => memory.reserve(0, class)?,
            _ => unreachable!(),
        };
        Ok(Arc::new(charge.retain(bytes.as_slice().to_vec())))
    }
}
pub fn exercise(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    family: &ReservedValue<CognitionCurrentStateQualificationV1>,
) {
    let field = CognitionJsonFieldV1::FeedbackOutbox;
    let entry = family
        .json_entry(field, &parent.roots.digest().unwrap())
        .unwrap()
        .unwrap();
    for fault in [
        Fault::Foreign,
        Fault::Undercharged,
        Fault::Quota,
        Fault::Cancelled,
        Fault::Missing,
    ] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let original = store.value_session(physical());
        let mut writer = store.value_session(physical());
        let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let armed = Cell::new(false);
        let _copy_cover = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let source = Hostile {
            source: &original,
            armed: &armed,
            foreign: &foreign,
            fault,
            copies: Cell::new(0),
        };
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [0x81; 32],
            &state,
            memory,
            TransactionLimits {
                max_reads: 8,
                max_writes: 2,
                max_facts: 0,
                max_pending_node_bytes: 2 << 20,
            },
        )
        .unwrap();
        let live = parent.fence().unwrap();
        let mut io = fixture::logical();
        let mut write_io = fixture::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        {
            let mut context = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 4096,
                    record_bytes: 8192,
                },
                cancelled: || false,
            };
            let guard = context.transaction.guard();
            assert!(
                change_cognition_control_v1(
                    CognitionControlFieldV1::JournalHeadSeq,
                    (
                        CognitionControlValueV1::Unsigned(u64::MAX),
                        CognitionControlValueV1::Unsigned(0)
                    ),
                    &mut context,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
            let before = write.sink.remaining_io().unwrap();
            armed.set(true);
            let guard = context.transaction.guard();
            let error = change_cognition_json_scalar_v1(
                CognitionJsonScalarChangeV1 {
                    field,
                    qualification: entry,
                    path: &[JsonPathSegmentV1::Name("")],
                    change: (JsonScalarRefV1::Bool(true), JsonScalarRefV1::Unsigned(2)),
                },
                &mut context,
                &mut write,
                &mut CognitionJsonPointWorkV1 {
                    point: JsonWriteWorkV1 {
                        cells_remaining: 128,
                        json_nodes_remaining: 16,
                        max_json_depth: 4,
                        max_pending_node_bytes: 2 << 20,
                    },
                    changed: JsonChangeWorkV1 {
                        json: JsonReadWorkV1 {
                            cells_remaining: 256,
                            json_nodes_remaining: 32,
                            max_depth: 4,
                            max_payload_bytes: 4096,
                        },
                        paths_remaining: 4096,
                        max_pending_pairs: 8,
                    },
                },
                &guard,
                &live,
            )
            .unwrap_err();
            let value = match error {
                WorldFieldOverlayErrorV1::Value(value)
                | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(value)) => value,
                other => panic!("unexpected source error: {other:?}"),
            };
            match fault {
                Fault::Foreign | Fault::Undercharged => {
                    assert!(matches!(value, ValueTreeError::Invalid(_)))
                }
                Fault::Quota => assert!(matches!(
                    value,
                    ValueTreeError::ResourceLimited("hostile Cognition source quota")
                )),
                Fault::Cancelled => assert!(matches!(value, ValueTreeError::Cancelled)),
                Fault::Missing => assert!(
                    matches!(value,ValueTreeError::Io(ref e) if e.kind()==std::io::ErrorKind::NotFound)
                ),
            }
            let after = write.sink.remaining_io().unwrap();
            assert_eq!(before.writes_remaining, after.writes_remaining);
            assert_eq!(before.write_bytes_remaining, after.write_bytes_remaining);
        }
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
        eprintln!(
            "Cognition actual typed point rejected {fault:?}, no suffix publication, successful prefix aborted"
        );
    }
}

struct CancelWriter<'a> {
    session: ValuePageSession<'a>,
    armed: &'a Cell<bool>,
    cancelled: &'a Cell<bool>,
    events: Cell<u64>,
    target: u64,
}
impl CancelWriter<'_> {
    fn after(&self, before: &PageIoBudget) {
        if !self.armed.get() {
            return;
        }
        let n = self.events.get() + 1;
        self.events.set(n);
        if n == self.target {
            let after = self.session.remaining_io().unwrap();
            assert!(
                after.writes_remaining < before.writes_remaining,
                "cancellation must follow actual physical publication"
            );
            self.cancelled.set(true);
        }
    }
}
impl ValueNodeReader for CancelWriter<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.session.read_node(id, max, memory, class)
    }
}
impl ValueNodeWriter for CancelWriter<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let before = self.session.remaining_io().unwrap();
        self.session.put_node(id, bytes)?;
        self.after(&before);
        Ok(())
    }
}
impl StatePageWriter for CancelWriter<'_> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        let before = self.session.remaining_io().unwrap();
        self.session
            .put_state_pages_cancelled(delta, memory, cancelled)?;
        self.after(&before);
        Ok(())
    }
}
pub fn real_put_cancellation(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    family: &ReservedValue<CognitionCurrentStateQualificationV1>,
) {
    let n = put_attempt(store, memory, parent, family, 0, u64::MAX, 1);
    assert!(n > 1);
    for channel in 0..2 {
        for target in [1, n] {
            assert_eq!(
                put_attempt(
                    store,
                    memory,
                    parent,
                    family,
                    channel,
                    target,
                    2 + channel as u64 * 2 + u64::from(target == n)
                ),
                target
            );
        }
    }
}
fn put_attempt(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ExecutionParent,
    family: &ReservedValue<CognitionCurrentStateQualificationV1>,
    channel: u8,
    target: u64,
    after: u64,
) -> u64 {
    let field = CognitionJsonFieldV1::FeedbackOutbox;
    let entry = family
        .json_entry(field, &parent.roots.digest().unwrap())
        .unwrap()
        .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let armed = Cell::new(false);
    let cancelled = Cell::new(false);
    let mut writer = CancelWriter {
        session: store.value_session(physical()),
        armed: &armed,
        cancelled: &cancelled,
        events: Cell::new(0),
        target,
    };
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [0x82; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 2,
            max_facts: 0,
            max_pending_node_bytes: 2 << 20,
        },
    )
    .unwrap();
    let live = parent.fence().unwrap();
    let mut io = fixture::logical();
    let mut write_io = fixture::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    {
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || channel == 0 && cancelled.get(),
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || channel == 1 && cancelled.get(),
        };
        let guard = context.transaction.guard();
        assert!(
            change_cognition_control_v1(
                CognitionControlFieldV1::JournalHeadSeq,
                (
                    CognitionControlValueV1::Unsigned(u64::MAX),
                    CognitionControlValueV1::Unsigned(0)
                ),
                &mut context,
                &mut write,
                &guard,
                &live
            )
            .unwrap()
        );
        armed.set(true);
        let guard = context.transaction.guard();
        let result = change_cognition_json_scalar_v1(
            CognitionJsonScalarChangeV1 {
                field,
                qualification: entry,
                path: &[JsonPathSegmentV1::Name("")],
                change: (
                    JsonScalarRefV1::Bool(true),
                    JsonScalarRefV1::Unsigned(after),
                ),
            },
            &mut context,
            &mut write,
            &mut CognitionJsonPointWorkV1 {
                point: JsonWriteWorkV1 {
                    cells_remaining: 128,
                    json_nodes_remaining: 16,
                    max_json_depth: 4,
                    max_pending_node_bytes: 2 << 20,
                },
                changed: JsonChangeWorkV1 {
                    json: JsonReadWorkV1 {
                        cells_remaining: 256,
                        json_nodes_remaining: 32,
                        max_depth: 4,
                        max_payload_bytes: 4096,
                    },
                    paths_remaining: 4096,
                    max_pending_pairs: 8,
                },
            },
            &guard,
            &live,
        );
        if target == u64::MAX {
            assert!(result.unwrap().is_some());
        } else {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                    | Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                        ValueTreeError::Cancelled
                    )))
            ));
        }
    }
    let events = writer.events.get();
    if target != u64::MAX {
        assert!(cancelled.get());
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
        eprintln!(
            "Cognition real physical publication event={events}, channel={channel}, prefix aborted"
        );
    } else {
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 2);
        eprintln!("Cognition successful point calibrated real sink events N={events}");
    }
    events
}
