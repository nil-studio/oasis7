use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;

// Exercise the same inventory reconciliation used by the genuine cold material
// caller without manufacturing a checkpoint or accepted-source capability.
struct Loans {
    io: u64,
    catalog: CatalogWorkLoanV1,
}
impl Inventory for Loans {
    fn lanes(&mut self, visit: &mut dyn FnMut(&mut u64, LaneKind)) {
        visit(&mut self.io, LaneKind::IoBytes);
        visit(&mut self.catalog.steps, LaneKind::Steps);
        visit(&mut self.catalog.bytes, LaneKind::WorkBytes);
    }
    fn admit_loans(&mut self, steps: u64, bytes: u64) {
        self.catalog.steps = steps;
        self.catalog.bytes = bytes;
    }
}
fn loans() -> Loans {
    Loans {
        io: 32,
        catalog: CatalogWorkLoanV1 { steps: 0, bytes: 0 },
    }
}

#[test]
fn cold_catalog_loan_settles_once_on_original_common_even_on_error() {
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let meter = FormalBoundedWorkV1::new(10_000, 1_000).unwrap();
    let identity = meter.identity();
    let mut work = loans();
    let mut stop = || false;
    let mut context = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    for fail in [false, true] {
        context.charge(7, 11).unwrap();
        let before = (meter.remaining_steps(), meter.remaining_read_bytes());
        let result = run_inventory(&mut work, &mut context, |work, stop| {
            assert!(meter.is_child_active());
            assert!(meter.charge(0, 0, stop).is_err());
            work.io -= 2;
            work.catalog.charge(3, 5)?;
            if fail {
                Err(invalid("original cold source failure"))
            } else {
                Ok(())
            }
        });
        if fail {
            assert!(matches!(result, Err(FormalError::Invalid(message))
                if message == "original cold source failure"));
        } else {
            result.unwrap();
        }
        // Existing physical bytes retain their 16-step weight; the catalog's
        // explicit (steps, bytes) pair is paid once without that IO multiplier.
        assert_eq!(meter.remaining_steps(), before.0 - 1 - 2 * 16 - 3);
        assert_eq!(meter.remaining_read_bytes(), before.1 - 2 - 5);
        assert_eq!(meter.identity(), identity);
        assert!(!meter.is_child_active());
        assert_eq!((work.catalog.steps, work.catalog.bytes), (0, 0));
        assert_eq!(memory.used(), 0);
    }
    assert_eq!(work.io, 28);
}

#[test]
fn cold_catalog_loan_exhaustion_and_unwind_never_refill_common() {
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let meter = FormalBoundedWorkV1::new(10, 20).unwrap();
    let mut work = loans();
    let mut stop = || false;
    let mut context = FormalBoundedContextV1::new(&memory, &meter, &mut stop);
    let result = run_inventory(&mut work, &mut context, |work, _| {
        work.catalog.charge(4, 11)
    });
    assert!(matches!(result, Err(FormalError::Invalid(message))
        if message == "bounded archive read steps"));
    assert_eq!(
        (meter.remaining_steps(), meter.remaining_read_bytes()),
        (6, 10)
    );
    assert!(!meter.is_child_active());
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), FormalError> = run_inventory(&mut work, &mut context, |work, _| {
            work.catalog.charge(1, 1)?;
            panic!("cold source unwound")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(
        (meter.remaining_steps(), meter.remaining_read_bytes()),
        (0, 0)
    );
    assert!(!meter.is_child_active());
    assert_eq!(memory.used(), 0);
}
