use super::*;
use crate::incremental_state::scan::{Guarded, Stop};
use crate::merkle::VisitError;
use std::cell::{Cell, RefCell};

pub struct ScheduleScanBudget {
    pub nodes_remaining: u64,
    pub read_bytes_remaining: u64,
    pub buckets_remaining: u64,
    pub instances_remaining: u64,
}
pub struct ScheduleScanContext<'a, R> {
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub io: &'a mut ScheduleScanBudget,
}
impl<R: MerkleNodeReader> ScheduleScanContext<'_, R> {
    /// Stream all indexed wakes <= now, in time then instance-digest order.
    /// Outputs are provisional until successful completion. No persistent
    /// growing map or output buffer is allocated; retained copies need admission.
    /// Parent qualification must establish the complete scalar/index bijection.
    pub fn visit_due<E>(
        &mut self,
        root: &ScheduleIndexV1,
        now: u64,
        mut visitor: impl FnMut(u64, &str) -> Result<(), E>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<u64, ScheduleScanError<E>> {
        let _workspace = self
            .memory
            .reserve(256 * 1024, self.class)
            .map_err(ScheduleScanError::Memory)?;
        if self.source.allocation_class() != Some(self.class)
            || !self
                .source
                .allocation_budget()
                .is_some_and(|p| p.same_pool(self.memory))
        {
            return Err(ScheduleScanError::Invalid("schedule source pool/class"));
        }
        root.validate()
            .map_err(|e| ScheduleScanError::Tree(e.to_string()))?;
        let cancelled = RefCell::new(cancelled);
        if (cancelled.borrow_mut())() {
            return Err(ScheduleScanError::Cancelled);
        }
        let stop = Cell::new(None);
        let source = Guarded {
            source: self.source,
            nodes: Cell::from_mut(&mut self.io.nodes_remaining),
            bytes: Cell::from_mut(&mut self.io.read_bytes_remaining),
            cancelled: &cancelled,
            stop: &stop,
        };
        let mut view = Updater::<ScheduleCodec<1>, _>::new(
            &source,
            empty::<1>().map_err(ScheduleScanError::Tree)?,
            0,
        );
        let mut upper = [255; 32];
        upper[..8].copy_from_slice(&now.to_be_bytes());
        let mut total = 0u64;
        let result = view.visit_key_range(&root.node_root, &[0; 32], &upper, |leaf| {
            if (cancelled.borrow_mut())() {
                return Err(ScheduleScanError::Cancelled);
            }
            let Leaf::Time {
                wake,
                count,
                root: bucket,
            } = leaf
            else {
                return Err(ScheduleScanError::Invalid("schedule time type"));
            };
            if *wake > now {
                return Err(ScheduleScanError::Invalid("schedule time range"));
            }
            if self.io.buckets_remaining == 0 {
                return Err(ScheduleScanError::ResourceLimited);
            }
            self.io.buckets_remaining -= 1;
            let mut instances = Updater::<ScheduleCodec<2>, _>::new(
                &source,
                empty::<2>().map_err(ScheduleScanError::Tree)?,
                0,
            );
            let count_found = instances
                .visit_leaves(bucket, |leaf| {
                    if (cancelled.borrow_mut())() {
                        return Err(ScheduleScanError::Cancelled);
                    }
                    let Leaf::Instance(instance) = leaf else {
                        return Err(ScheduleScanError::Invalid("schedule instance type"));
                    };
                    if self.io.instances_remaining == 0 {
                        return Err(ScheduleScanError::ResourceLimited);
                    }
                    self.io.instances_remaining -= 1;
                    total = total
                        .checked_add(1)
                        .ok_or(ScheduleScanError::Invalid("schedule count overflow"))?;
                    if total > root.entries {
                        return Err(ScheduleScanError::Invalid("schedule total count"));
                    }
                    visitor(*wake, instance).map_err(ScheduleScanError::Visitor)
                })
                .map_err(flatten)?;
            if count_found != *count {
                return Err(ScheduleScanError::Invalid("schedule bucket count"));
            }
            Ok(())
        });
        match stop.get() {
            Some(Stop::Limited) => return Err(ScheduleScanError::ResourceLimited),
            Some(Stop::Cancelled) => return Err(ScheduleScanError::Cancelled),
            None => {}
        }
        let buckets = result.map_err(flatten)?;
        if buckets > root.buckets
            || (now == u64::MAX && (buckets != root.buckets || total != root.entries))
        {
            return Err(ScheduleScanError::Invalid("schedule root counts"));
        }
        if (cancelled.borrow_mut())() {
            return Err(ScheduleScanError::Cancelled);
        }
        Ok(total)
    }
}
fn flatten<E>(error: VisitError<ScheduleScanError<E>>) -> ScheduleScanError<E> {
    match error {
        VisitError::Tree(error) => ScheduleScanError::Tree(error),
        VisitError::Visitor(error) => error,
    }
}
