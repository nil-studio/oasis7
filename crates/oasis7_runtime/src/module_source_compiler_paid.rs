//! Metered output data from the actual Host compiler. This grants no admission.
use super::*;
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{
    cell::RefCell,
    io::{self, Write},
};

#[derive(Debug)]
pub(crate) enum PaidSourceCompileErrorV1 {
    Resource(BodyMutationError),
    Compiler(WorldError),
}
struct Output<'a, 'b> {
    bytes: &'a mut Vec<u8>,
    cap: usize,
    debit: &'b dyn Fn(u64, u64) -> Result<(), ModuleSourceCompileError>,
    first: &'b RefCell<Option<BodyMutationError>>,
}
impl Write for Output<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.cap)
        {
            if self.first.borrow().is_none() {
                *self.first.borrow_mut() =
                    Some(BodyMutationError::Invalid("compiler output limit"));
            }
            return Err(io::Error::other("compiler output limit"));
        }
        (self.debit)(1, bytes.len() as u64)
            .map_err(|_| io::Error::other("compiler output work"))?;
        // Capacity was reserved before the Host was invoked. No reallocation.
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl RuntimeSourceCompiler {
    /// The output cap is the caller's explicit resource bound, not a new wire
    /// grammar. Compiler-process memory is a separate Host worker obligation.
    pub(crate) fn compile_paid_output_v1(
        &self,
        module_id: &str,
        package: &ModuleSourcePackage,
        max_output_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, PaidSourceCompileErrorV1> {
        let resource = PaidSourceCompileErrorV1::Resource;
        common
            .consume_steps(1)
            .map_err(|e| resource(BodyMutationError::Original(Box::new(e))))?;
        let layout = max_output_bytes
            .checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(|| resource(BodyMutationError::Invalid("compiler output layout")))?;
        let memory = common.memory().clone();
        let fee = memory
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)
            .map_err(BodyMutationError::from)
            .map_err(resource)?;
        let _scope = memory
            .reserve(8192 + 1024, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)
            .map_err(BodyMutationError::from)
            .map_err(resource)?;
        let mut bytes = Vec::with_capacity(max_output_bytes);
        let (_, work, stop) = common.parts();
        let lease = work
            .begin_child()
            .map_err(|e| resource(BodyMutationError::Original(Box::new(e))))?;
        let stop = RefCell::new(stop);
        let first = RefCell::new(None);
        let debit = |steps, read_bytes| {
            if first.borrow().is_some() {
                return Err(ModuleSourceCompileError::Unavailable(
                    "compiler work already stopped".into(),
                ));
            }
            let result = if (stop.borrow_mut())() {
                Err(ValueTreeError::Cancelled.into())
            } else {
                lease
                    .consume(steps, read_bytes)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))
            };
            if let Err(error) = result {
                *first.borrow_mut() = Some(error);
                return Err(ModuleSourceCompileError::Unavailable(
                    "compiler caller work stopped".into(),
                ));
            }
            Ok(())
        };
        let result = {
            let mut output = Output {
                bytes: &mut bytes,
                cap: max_output_bytes,
                debit: &debit,
                first: &first,
            };
            self.0.compile_to(
                module_id,
                package,
                max_output_bytes,
                &mut output,
                &mut |s, b| debit(s, b),
            )
        };
        // Poll after success only: a later cancellation must not replace an
        // already returned compiler error. Earlier metered failures stay first.
        if result.is_ok() {
            let _ = debit(0, 0);
        }
        lease.finish();
        if let Some(error) = first.into_inner() {
            return Err(resource(error));
        }
        result
            .map_err(map_compile_error)
            .map_err(PaidSourceCompileErrorV1::Compiler)?;
        Ok(fee.retain(bytes))
    }
}
