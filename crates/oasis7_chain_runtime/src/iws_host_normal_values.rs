//! Fixed normal execution's readers and writers borrow one actual page session.
//! Handles never copy physical allowances or turn provisional pages into a head.
use oasis7_distfs::incremental_page_store::ValuePageSession;
use oasis7_formal_protocol::{
    incremental_schedule::{ScheduleDeltaV1, SchedulePageWriter},
    incremental_state::{StateDeltaV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, sync::Arc};

pub(in crate::iws_host_runtime) struct HostNormalValuesV1<'a, 'store> {
    session: RefCell<&'a mut ValuePageSession<'store>>,
    replica: Option<RefCell<&'a mut ValuePageSession<'store>>>,
    memory: &'a MemoryBudget,
    _fee: MemoryReservation,
}
impl<'a, 'store> HostNormalValuesV1<'a, 'store> {
    pub(in crate::iws_host_runtime) fn new(
        session: &'a mut ValuePageSession<'store>,
        memory: &'a MemoryBudget,
    ) -> Result<Self, ValueTreeError> {
        // One reader plus the members, fee, event and final-state writer views
        // coexist in the fixed consumer. All are stack-only borrowed handles.
        let bytes = std::mem::size_of::<Self>()
            .checked_add(5 * std::mem::size_of::<HostNormalValueHandleV1<'_, '_, '_>>())
            .ok_or(ValueTreeError::ResourceLimited("normal value IO layout"))?;
        let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
        Ok(Self {
            session: RefCell::new(session),
            replica: None,
            memory,
            _fee: fee,
        })
    }
    pub(super) fn new_pair(
        primary: &'a mut ValuePageSession<'store>,
        replica: &'a mut ValuePageSession<'store>,
        memory: &'a MemoryBudget,
    ) -> Result<Self, ValueTreeError> {
        let mut views = Self::new(primary, memory)?;
        views.replica = Some(RefCell::new(replica));
        Ok(views)
    }
    pub(in crate::iws_host_runtime) fn handle(&self) -> HostNormalValueHandleV1<'_, 'a, 'store> {
        HostNormalValueHandleV1(self)
    }
    fn check_memory(&self, memory: &MemoryBudget) -> Result<(), ValueTreeError> {
        if self.memory.same_pool(memory) {
            Ok(())
        } else {
            Err(ValueTreeError::Invalid("normal value IO pool"))
        }
    }
}
fn reentry() -> ValueTreeError {
    ValueTreeError::Invalid("normal value IO reentry")
}
pub(in crate::iws_host_runtime) struct HostNormalValueHandleV1<'s, 'a, 'store>(
    &'s HostNormalValuesV1<'a, 'store>,
);
impl ValueNodeReader for HostNormalValueHandleV1<'_, '_, '_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.0.check_memory(memory)?;
        if class != MemoryClass::Ordinary {
            return Err(ValueTreeError::Invalid("normal value IO class"));
        }
        let session = self.0.session.try_borrow().map_err(|_| reentry())?;
        ValueNodeReader::read_node(&**session, id, max_bytes, memory, class)
    }
}
impl ValueNodeWriter for HostNormalValueHandleV1<'_, '_, '_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if !bytes.uses_budget(self.0.memory) {
            return Err(ValueTreeError::Invalid("normal value producer pool"));
        }
        let mut session = self.0.session.try_borrow_mut().map_err(|_| reentry())?;
        session.put_node_borrowed(id, &bytes)?;
        if let Some(replica) = &self.0.replica {
            replica
                .try_borrow_mut()
                .map_err(|_| reentry())?
                .put_node_borrowed(id, &bytes)?;
        }
        Ok(())
    }
}
impl StatePageWriter for HostNormalValueHandleV1<'_, '_, '_> {
    fn put_state_pages(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        self.put_state_pages_cancelled(delta, memory, &mut || false)
    }
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.0.check_memory(memory)?;
        let mut session = self.0.session.try_borrow_mut().map_err(|_| reentry())?;
        // Preserve the actual store's typed batch validation, cancellation and
        // physical accounting, rather than the trait's per-node copy default.
        StatePageWriter::put_state_pages_cancelled(&mut **session, delta, memory, cancelled)?;
        if let Some(replica) = &self.0.replica {
            StatePageWriter::put_state_pages_cancelled(
                &mut **replica.try_borrow_mut().map_err(|_| reentry())?,
                delta,
                memory,
                cancelled,
            )?;
        }
        Ok(())
    }
}
impl SchedulePageWriter for HostNormalValueHandleV1<'_, '_, '_> {
    fn put_schedule_pages(
        &mut self,
        delta: &ReservedValue<ScheduleDeltaV1>,
        memory: &MemoryBudget,
    ) -> Result<(), ValueTreeError> {
        self.0.check_memory(memory)?;
        let mut session = self.0.session.try_borrow_mut().map_err(|_| reentry())?;
        SchedulePageWriter::put_schedule_pages(&mut **session, delta, memory)?;
        if let Some(replica) = &self.0.replica {
            SchedulePageWriter::put_schedule_pages(
                &mut **replica.try_borrow_mut().map_err(|_| reentry())?,
                delta,
                memory,
            )?;
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
#[path = "iws_host_normal_values_tests.rs"]
mod tests;
