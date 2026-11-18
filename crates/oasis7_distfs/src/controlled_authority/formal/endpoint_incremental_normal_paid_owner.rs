//! Private mutable owner storage admitted before the first allocation.
//! The box is dropped before its charge, including on recovery and unwind.
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
use std::mem::size_of;

pub(super) struct PaidNormalBoxV1<T> {
    pub(super) value: Box<T>,
    pub(super) fee: MemoryReservation,
}
impl<T> PaidNormalBoxV1<T> {
    pub(super) fn layout() -> usize {
        size_of::<T>() + size_of::<Self>()
    }
    pub(super) fn new(value: T, fee: MemoryReservation) -> Self {
        // All constructors use the existing phase's admission before mutation.
        assert!(fee.bytes() >= Self::layout(), "normal owner prepaid layout");
        Self {
            value: Box::new(value),
            fee,
        }
    }
    pub(super) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.fee.uses_budget(memory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_proto::memory_budget::MemoryClass;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct DropProbe {
        memory: MemoryBudget,
        observed: Arc<AtomicUsize>,
    }
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.observed.store(self.memory.used(), Ordering::SeqCst);
        }
    }
    #[test]
    fn failed_owner_move_keeps_charge_until_its_value_drops() {
        let bytes = PaidNormalBoxV1::<DropProbe>::layout();
        let memory = MemoryBudget::new(bytes, 0).unwrap();
        let observed = Arc::new(AtomicUsize::new(0));
        let fee = memory.reserve(bytes, MemoryClass::Ordinary).unwrap();
        let owner = PaidNormalBoxV1::new(
            DropProbe {
                memory: memory.clone(),
                observed: observed.clone(),
            },
            fee,
        );
        let failure: Result<(), _> = Err(owner);
        assert_eq!(memory.used(), bytes);
        assert!(failure.as_ref().unwrap_err().uses_budget(&memory));
        drop(failure);
        assert_eq!(observed.load(Ordering::SeqCst), bytes);
        assert_eq!(memory.used(), 0);
    }
}
