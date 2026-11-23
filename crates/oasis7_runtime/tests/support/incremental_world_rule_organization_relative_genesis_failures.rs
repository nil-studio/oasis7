use super::*;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueNodeWriter};
use std::cell::Cell;
use std::sync::Arc;
struct FailAfterPut<'a, W> {
    inner: &'a mut W,
    put: &'a Cell<bool>,
}
impl<W: ValueNodeWriter> ValueNodeWriter for FailAfterPut<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.put.set(true);
        Err(ValueTreeError::Invalid("durable put then injected failure"))
    }
}
impl<W: ValueNodeWriter> StatePageWriter for FailAfterPut<'_, W> {}
impl<W: ValueNodeReader> ValueNodeReader for FailAfterPut<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.inner.read_node(id, max, memory, class)
    }
}
pub(super) fn exercise() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    for mode in 0..8 {
        // Each mode owns a fresh store. The put-failure oracle must reach a
        // novel physical node, not an earlier mode's deduplicated value.
        let store = IncrementalPageStore::create(
            &directory.0.join(format!("genesis-error-{mode}")),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let mut source = source();
        let rules = source.world_rules.as_mut().unwrap();
        let value = rules.organization_estate.remove(ESTATE).unwrap();
        rules
            .organization_estate
            .insert(format!("estate-error-{mode}"), value);
        let mut rows = produce(&store, &memory, &source);
        assert_eq!(rows.len(), 3);
        let mut remaining = if mode == 2 { 0 } else { 128 };
        let mut io = logical();
        if mode == 0 {
            io.write_bytes_remaining = 0;
        }
        let mut keys = SemanticKeyBudget {
            bytes_remaining: if mode == 1 { 0 } else { 1 << 20 },
        };
        let mut bounds = limits();
        bounds.payload_bytes = 165;
        if mode == 3 {
            bounds.payload_bytes = 0;
        }
        let cancelled = Cell::new(mode == 4);
        let put = Cell::new(false);
        let mut values = store.value_session(physical());
        let mut sink = |row: ReservedValue<ExecutionWriteV1>| {
            rows.push((row.mutation.key.clone(), row.mutation.after.unwrap()));
            if mode == 5 {
                return Err(ValueTreeError::Invalid(
                    "injected organization genesis sink",
                ));
            }
            if mode == 6 {
                cancelled.set(true);
            }
            Ok(())
        };
        let result = if mode == 7 {
            let mut writer = FailAfterPut {
                inner: &mut values,
                put: &put,
            };
            prepare_qualified_organization_genesis_v1(
                &source,
                &mut MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut io,
                    key_work: &mut keys,
                    limits: bounds,
                    cancelled: || cancelled.get(),
                },
                &mut remaining,
                4 << 20,
                &mut sink,
            )
        } else {
            prepare_qualified_organization_genesis_v1(
                &source,
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io,
                    key_work: &mut keys,
                    limits: bounds,
                    cancelled: || cancelled.get(),
                },
                &mut remaining,
                4 << 20,
                &mut sink,
            )
        };
        assert!(result.is_err(), "mode {mode}");
        if matches!(mode, 4 | 6) {
            assert!(
                matches!(
                    result,
                    Err(MemberRecordError::Value(ValueTreeError::Cancelled))
                ),
                "mode{mode}: {result:?}"
            );
        }
        assert!(rows.len() >= 3);
        if mode == 7 {
            assert!(put.get());
        }
        // No StateDelta or authoritative root is built from any completed-looking prefix.
    }
}
