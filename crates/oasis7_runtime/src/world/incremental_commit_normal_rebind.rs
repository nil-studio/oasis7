//! One exclusive Common child for the post-seal directory work loans. Logical
//! limits are temporarily capped, then only actual debits return to the same
//! original counters. No counter store or Common identity is created here.
use super::*;
use crate::world::incremental_factory_phase::io::{
    FactoryPhaseMeterLaneV1 as Lane, FactoryPhaseMeterLanesV1,
};
pub(super) struct RebindLanes<'a> {
    pub material: &'a mut InventoryMaterialScopeWorkV1,
    pub directories: NormalHostDirectoryWorkLoanV1<'a>,
}
impl FactoryPhaseMeterLanesV1 for RebindLanes<'_> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(Lane<'a>)) {
        use Lane::{Bytes as B, Entries as N, Steps as S};
        v(N(&mut self.material.pending_entries_remaining));
        v(B(&mut self.material.comparison_bytes_remaining));
        let d = &mut self.directories;
        v(S(&mut d.inventory.visits_remaining));
        v(B(&mut d.inventory.comparison_bytes_remaining));
        v(N(&mut d.inventory.pending_entries_remaining));
        v(S(&mut d.claims.visits_remaining));
        v(B(&mut d.claims.comparison_bytes_remaining));
        v(N(&mut d.claims.pending_entries_remaining));
        v(S(d.grant_cells));
        v(S(&mut d.gameplay.visits_remaining));
        v(B(&mut d.gameplay.comparison_bytes_remaining));
        v(N(&mut d.gameplay.pending_entries_remaining));
        v(S(&mut d.threats.visits_remaining));
        v(B(&mut d.threats.comparison_bytes_remaining));
        v(N(&mut d.threats.pending_entries_remaining));
        v(S(&mut d.builds.cells_remaining));
        v(B(&mut d.builds.comparison_bytes_remaining));
        v(S(&mut d.receipts.cells_remaining));
        v(B(&mut d.receipts.bytes_remaining));
    }
}
fn invalid() -> FormalError {
    FormalError::Invalid("normal seal work lane inventory".into())
}
#[derive(Clone, Copy, Default)]
struct Slot {
    before: u128,
    cap: u128,
    kind: u8,
}
fn value(lane: &Lane<'_>) -> (u128, u8) {
    match lane {
        Lane::Steps(n) => (**n as u128, 1),
        Lane::Bytes(n) => (**n as u128, 2),
        Lane::Entries(n) => (**n as u128, 3),
    }
}
fn set(lane: Lane<'_>, n: u128) -> bool {
    match lane {
        Lane::Steps(p) | Lane::Bytes(p) => match u64::try_from(n) {
            Ok(n) => {
                *p = n;
                true
            }
            Err(_) => false,
        },
        Lane::Entries(p) => match usize::try_from(n) {
            Ok(n) => {
                *p = n;
                true
            }
            Err(_) => false,
        },
    }
}
/// Even operation Err returns actual remaining work. Unwind exhausts the
/// Common child through its original Drop contract and is never caught as data.
pub(super) fn run<T, E, X: FactoryPhaseMeterLanesV1>(
    common: &mut FormalBoundedContextV1<'_>,
    lanes: &mut X,
    run: impl FnOnce(&mut X, &mut dyn FnMut() -> bool) -> Result<T, E>,
) -> Result<Result<T, E>, FormalError> {
    const MAX: usize = 64;
    // The eight original rebinders validate/hash the fixed RootSet repeatedly
    // outside their variable row counters. RootSet admits <=256 world-ID bytes
    // and exactly six fixed state roots; this conservative fixed phase covers
    // 128 complete <=2KiB header visits. Variable material/build row work remains
    // separately capped and debited below, with no whole-World traversal.
    common.charge(512, 256 * 1024 + std::mem::size_of::<[Slot; MAX]>() as u64)?;
    let (memory, meter, stop) = common.parts();
    let _fee = memory
        .reserve(std::mem::size_of::<[Slot; MAX]>(), MemoryClass::Ordinary)
        .map_err(|_| invalid())?;
    let lease = meter.begin_child()?;
    let mut slots = [Slot::default(); MAX];
    let mut count = 0usize;
    let mut steps = 0u64;
    let mut bytes = 0u64;
    lanes.visit_lanes(&mut |lane| {
        let (n, kind) = value(&lane);
        if let Some(slot) = slots.get_mut(count) {
            *slot = Slot {
                before: n,
                cap: 0,
                kind,
            };
            if kind == 2 { bytes += 1 } else { steps += 1 }
        }
        count += 1;
    });
    if count > MAX || steps == 0 || bytes == 0 {
        return Err(invalid());
    }
    let step_cap = lease.remaining_steps() / steps;
    let byte_cap = lease.remaining_read_bytes() / bytes;
    let mut index = 0;
    let mut okay = true;
    lanes.visit_lanes(&mut |lane| {
        if let Some(slot) = slots.get_mut(index) {
            let (_, kind) = value(&lane);
            okay &= slot.kind == kind;
            slot.cap = slot
                .before
                .min(if kind == 2 { byte_cap } else { step_cap } as u128);
            okay &= set(lane, slot.cap)
        } else {
            okay = false
        }
        index += 1;
    });
    if !okay || index != count {
        return Err(invalid());
    }
    let mut cancelled = false;
    let mut latched_stop = || {
        cancelled = cancelled || stop();
        cancelled
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(lanes, &mut latched_stop)
    }));
    let mut used_steps = 0u64;
    let mut used_bytes = 0u64;
    let mut index = 0;
    lanes.visit_lanes(&mut |lane| {
        if let Some(slot) = slots.get(index) {
            let (after, kind) = value(&lane);
            if kind != slot.kind || after > slot.cap {
                okay = false
            } else {
                let delta = slot.cap - after;
                if let Ok(delta) = u64::try_from(delta) {
                    let total = if kind == 2 {
                        &mut used_bytes
                    } else {
                        &mut used_steps
                    };
                    if let Some(sum) = total.checked_add(delta) {
                        *total = sum
                    } else {
                        okay = false
                    }
                } else {
                    okay = false
                }
                okay &= set(lane, slot.before - delta);
            }
        } else {
            okay = false
        }
        index += 1;
    });
    if let Err(payload) = result {
        drop(lease);
        std::panic::resume_unwind(payload)
    }
    if !okay || index != count {
        return Err(invalid());
    }
    let debit = lease.consume(used_steps, used_bytes);
    lease.finish();
    if cancelled {
        return Err(FormalError::Invalid(
            "bounded archive read cancelled".into(),
        ));
    }
    debit?;
    match result {
        Ok(result) => Ok(result),
        Err(_) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
    struct TestLanes {
        steps: u64,
        bytes: u64,
    }
    impl FactoryPhaseMeterLanesV1 for TestLanes {
        fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(Lane<'a>)) {
            v(Lane::Steps(&mut self.steps));
            v(Lane::Bytes(&mut self.bytes));
        }
    }
    #[test]
    fn operation_error_keeps_actual_debit_and_restores_only_unused_logical_cap() {
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10_000, 2 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut lanes = TestLanes {
            steps: 10_000,
            bytes: 2 << 20,
        };
        let result: Result<Result<(), ()>, _> = run(&mut common, &mut lanes, |lanes, _| {
            lanes.steps -= 3;
            lanes.bytes -= 7;
            Err(())
        });
        assert!(matches!(result, Ok(Err(()))));
        assert_eq!(lanes.steps, 9997);
        assert_eq!(lanes.bytes, (2 << 20) - 7);
        assert_eq!(work.remaining_steps(), 10_000 - 512 - 3);
        assert_eq!(
            work.remaining_read_bytes(),
            (2 << 20) - 256 * 1024 - std::mem::size_of::<[Slot; 64]>() as u64 - 7
        );
        assert_eq!(memory.used(), 0);
    }
    #[test]
    fn ignored_once_only_cancellation_still_rejects_and_latches() {
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10_000, 2 << 20).unwrap();
        let mut calls = 0;
        let mut stop = || {
            calls += 1;
            calls == 2
        };
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut lanes = TestLanes {
            steps: 10_000,
            bytes: 2 << 20,
        };
        let result: Result<Result<(), ()>, _> = run(&mut common, &mut lanes, |_, stop| {
            assert!(stop());
            assert!(stop());
            Ok(())
        });
        assert!(
            matches!(result,Err(FormalError::Invalid(ref s)) if s=="bounded archive read cancelled")
        );
        assert_eq!(lanes.steps, 10_000);
        assert_eq!(memory.used(), 0);
    }
    #[test]
    fn unwind_exhausts_actual_common_and_preserves_real_logical_spend() {
        let memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10_000, 2 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut lanes = TestLanes {
            steps: 10_000,
            bytes: 2 << 20,
        };
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<Result<(), ()>, _> = run(&mut common, &mut lanes, |lanes, _| {
                lanes.steps -= 1;
                panic!("real child unwind")
            });
        }));
        assert!(panic.is_err());
        assert_eq!(lanes.steps, 9999);
        assert_eq!(work.remaining_steps(), 0);
        assert_eq!(work.remaining_read_bytes(), 0);
        assert!(!work.is_child_active());
        assert_eq!(memory.used(), 0);
    }
}
