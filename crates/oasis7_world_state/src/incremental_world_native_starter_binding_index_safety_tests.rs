use super::*;
use std::cell::{Cell, RefCell};
struct Recovery<'a> {
    base: &'a Pages,
    memory: MemoryBudget,
    error: RefCell<Option<ValueTreeError>>,
}
struct Copied(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Copied {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Copied;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let Some(page) = MerkleNodeReader::read_node(self.base, digest, max)? else {
            return Ok(None);
        };
        let fee = match self
            .memory
            .reserve(page.as_ref().len() + 256, MemoryClass::Recovery)
        {
            Ok(fee) => fee,
            Err(error) => {
                *self.error.borrow_mut() = Some(ValueTreeError::Memory(error));
                return Err("recovery admission".into());
            }
        };
        Ok(Some(Copied(fee.retain(page.as_ref().to_vec()))))
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(&self.memory) {
            return Err(invalid());
        }
        let value = ValueNodeReader::read_node(self.base, id, max, memory, MemoryClass::Ordinary)?;
        Ok(Arc::new(
            memory
                .reserve(value.len() + 256, MemoryClass::Recovery)?
                .retain(value.as_slice().to_vec()),
        ))
    }
}
#[test]
fn starter_binding_recovery_requires_paid_readmission_and_checks_empty_final_cancel() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    for values in [
        Vec::new(),
        vec![("outer".into(), claim("player", Some(""), "agent"))],
    ] {
        let rows = rows::<Field>(&memory, &pages, &values);
        let roots = install(&memory, &pages, StateNamespaceV1::Authorization, &rows);
        let recovery = Recovery {
            base: &pages,
            memory: memory.clone(),
            error: RefCell::new(None),
        };
        let mut recovery_io = io();
        let mut recovery_keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut recovery_io,
            },
            key_work: &mut recovery_keys,
            semantic_bytes: 1 << 20,
            record_bytes: 3 << 20,
            take_state_error: || recovery.error.borrow_mut().take(),
            cancelled: || false,
        };
        let mut native = work();
        let field = family::qualify_world_native_field_v1::<Field>(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut native,
        )
        .unwrap();
        let index = build_starter_binding_index_v1(
            &field,
            &roots,
            &mut ctx,
            &mut scan(),
            &mut native,
            &mut index_work(),
        )
        .unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent(&roots), [41; 32], &pages, &memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ordinary = context(
            &memory,
            &pages,
            roots.state_root(StateNamespaceV1::Authorization),
            &mut io,
            &mut keys,
        );
        assert!(
            observe_current_starter_binding_v1(
                "player",
                "",
                &index,
                &roots,
                &mut tx,
                &mut ordinary,
                &mut scan(),
                &mut work(),
                &mut index_work(),
                &guard,
                &live
            )
            .is_err()
        );
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
        let foreign = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
        assert!(
            readmit_starter_binding_index_ordinary_bounded_v1(&index, &roots, &foreign, || false)
                .is_err()
        );
        let before = memory.used();
        let ordinary_index =
            readmit_starter_binding_index_ordinary_bounded_v1(&index, &roots, &memory, || false)
                .unwrap();
        assert!(memory.used() > before);
        assert_eq!(ordinary_index.records(), index.records());
        {
            let mut current_tx =
                ExecutionTransaction::begin(parent(&roots), [43; 32], &pages, &memory, limits())
                    .unwrap();
            let observed = observe(
                &memory,
                &pages,
                &roots,
                &ordinary_index,
                &mut current_tx,
                "player",
                "",
                &mut index_work(),
            );
            assert_eq!(observed.matching_claims(), values.len() as u64);
            assert_eq!(
                observed.unique_agent_id(),
                (!values.is_empty()).then_some("agent")
            );
        }
        drop(ordinary_index);
        assert_eq!(memory.used(), before);
        let limit = (256 << 20) - (32 << 20);
        let fill = memory
            .reserve(limit - memory.used(), MemoryClass::Ordinary)
            .unwrap();
        assert!(
            readmit_starter_binding_index_ordinary_bounded_v1(&index, &roots, &memory, || false)
                .is_err()
        );
        drop(fill);
        if values.is_empty() {
            let checks = Cell::new(0);
            assert!(matches!(
                readmit_starter_binding_index_ordinary_v1(
                    &index,
                    &roots,
                    &memory,
                    &mut index_work(),
                    || {
                        checks.set(checks.get() + 1);
                        checks.get() >= 2
                    }
                ),
                Err(ValueTreeError::Cancelled)
            ));
            assert_eq!(checks.get(), 2);
        }
    }
}
#[test]
fn starter_current_cancellation_and_original_storage_error_abort() {
    let memory = MemoryBudget::new(256 << 20, 32 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 32768);
    let rows = rows::<Field>(
        &memory,
        &pages,
        &[("outer".into(), claim("player", Some("key"), "agent"))],
    );
    let roots = install(&memory, &pages, StateNamespaceV1::Authorization, &rows);
    let index = build_index(&memory, &pages, &roots);
    for cancellation in [false, true] {
        let mut tx =
            ExecutionTransaction::begin(parent(&roots), [42; 32], &pages, &memory, limits())
                .unwrap();
        let live = tx.parent_fence();
        let guard = tx.guard();
        let checks = Cell::new(0);
        let errors = Cell::new(0);
        let mut io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 32 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 3 << 20,
            take_state_error: || {
                errors.set(errors.get() + 1);
                (!cancellation && errors.get() == 3)
                    .then_some(ValueTreeError::Invalid("actual source failure"))
            },
            cancelled: || {
                checks.set(checks.get() + 1);
                cancellation && checks.get() >= 8
            },
        };
        let result = observe_current_starter_binding_v1(
            "player",
            "key",
            &index,
            &roots,
            &mut tx,
            &mut ctx,
            &mut scan(),
            &mut work(),
            &mut index_work(),
            &guard,
            &live,
        );
        if cancellation {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
            ));
        } else {
            assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
                    "actual source failure"
                )))
            ));
        }
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
    }
}
