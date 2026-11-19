//! Admitted decoding of one actual sandbox result before any typed allocation.
//! This is payload/execution IO, not an invocation permission or accepted head.
#[path = "incremental_module_execution_output_budget.rs"]
mod state_budget;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::{
    cbor_decode_preflight::{
        CborPreflightErrorV1, CborPreflightWorkV1, preflight_definite_cbor_v1,
    },
    memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue},
};
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleEffectIntent, ModuleEmit,
    ModuleOutput, ModuleTickLifecycleDirective,
    module_raw_output::{
        ModuleRawOutputSinkV1, ModuleSandboxRawOutputV1, RAW_SANDBOX_FAILURE_DETAIL_MAX_BYTES_V1,
    },
};

pub struct RootModuleOutputWorkV1 {
    pub calls_remaining: u64,
    pub structure: CborPreflightWorkV1,
    pub max_identity_bytes: usize,
    pub max_original_output_bytes: usize,
    pub max_output_heap_bytes: usize,
}
pub enum RootModuleOutputErrorV1 {
    Value(ValueTreeError),
    Sandbox(ReservedValue<ModuleCallFailure>),
}
impl std::fmt::Debug for RootModuleOutputErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(e) => e.fmt(f),
            Self::Sandbox(e) => e.fmt(f),
        }
    }
}
/// Charges the actual raw result, typed output and serde's buffered internal
/// lifecycle-tag Content. It never allocates an output Vec before preflight.
struct OutputSink<'a, C> {
    request: &'a ModuleCallRequest,
    memory: &'a MemoryBudget,
    work: &'a mut RootModuleOutputWorkV1,
    cancelled: C,
    seen: bool,
    failed: bool,
    output: Option<ReservedValue<ModuleOutput>>,
    error: Option<ValueTreeError>,
    limit_failure: Option<ModuleCallFailure>,
    failure_charge: MemoryReservation,
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("root module sandbox output")
}
fn add(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_add(b)
        .ok_or(ValueTreeError::ResourceLimited("module output heap bound"))
}
fn mul(a: usize, b: usize) -> Result<usize, ValueTreeError> {
    a.checked_mul(b)
        .ok_or(ValueTreeError::ResourceLimited("module output heap bound"))
}
fn count(n: u64) -> Result<usize, ValueTreeError> {
    usize::try_from(n).map_err(|_| ValueTreeError::ResourceLimited("module output count"))
}
impl<'a, C: FnMut() -> bool> OutputSink<'a, C> {
    fn new(
        request: &'a ModuleCallRequest,
        memory: &'a MemoryBudget,
        work: &'a mut RootModuleOutputWorkV1,
        cancelled: C,
    ) -> Result<Self, ValueTreeError> {
        if request.module_id.len() > work.max_identity_bytes
            || request.trace_id.len() > work.max_identity_bytes
        {
            return Err(ValueTreeError::ResourceLimited(
                "module invocation identity bytes",
            ));
        }
        work.calls_remaining = work
            .calls_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("module invocation calls"))?;
        // Admission precedes backend invocation and every returned diagnostic.
        let identity = add(request.module_id.len(), request.trace_id.len())?;
        let failure_bytes = add(
            add(
                mul(identity, 4)?,
                mul(RAW_SANDBOX_FAILURE_DETAIL_MAX_BYTES_V1, 2)?,
            )?,
            8192,
        )?;
        let failure_charge = memory.reserve(failure_bytes, MemoryClass::Ordinary)?;
        Ok(Self {
            request,
            memory,
            work,
            cancelled,
            seen: false,
            failed: false,
            output: None,
            error: None,
            limit_failure: None,
            failure_charge,
        })
    }
    fn failure(&self, code: ModuleCallErrorCode) -> ModuleCallFailure {
        ModuleCallFailure {
            module_id: self.request.module_id.clone(),
            trace_id: self.request.trace_id.clone(),
            code,
            detail: "root module output admission or decoding failed".into(),
        }
    }
    fn decode(&mut self, bytes: &[u8]) -> Result<(), ValueTreeError> {
        if self.failed || self.seen {
            return Err(invalid());
        }
        self.seen = true;
        self.failed = true;
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        if bytes.len() > self.work.max_original_output_bytes
            || bytes.len() as u64 > self.request.limits.max_output_bytes
        {
            return Err(ValueTreeError::ResourceLimited(
                "module original output bytes",
            ));
        }
        oasis7_world_state::incremental_world_event_decode::verify_module_output_layout_inventory_v1(
            self.memory, MemoryClass::Ordinary,
        )?;
        let shape =
            preflight_definite_cbor_v1(bytes, &mut self.work.structure, &mut self.cancelled)
                .map_err(|e| match e {
                    CborPreflightErrorV1::Invalid => invalid(),
                    CborPreflightErrorV1::ResourceLimited => {
                        ValueTreeError::ResourceLimited("module output CBOR work")
                    }
                    CborPreflightErrorV1::Cancelled => ValueTreeError::Cancelled,
                })?;
        let (shape, state_heap) = match state_budget::module_output_state_budget_v1(
            bytes,
            shape,
            &mut self.work.structure,
            &mut self.cancelled,
        )? {
            Some(budget) => (budget.remainder, budget.owned_bytes),
            None => (shape, 0),
        };
        // Closed schema: root struct, Vec<u8>, Vec<EffectIntent>, Vec<Emit>,
        // JSON Value (BTree nodes conservatively covered), one internally
        // tagged lifecycle enum with only u64/no heap default. Content clones
        // are bounded by observed nesting+3 stages. String factor16 includes
        // serde's worst escaped unknown-tag diagnostics and capacity growth.
        const _: () = assert!(!std::mem::needs_drop::<serde_json::Number>());
        type Content = serde::__private229::de::Content<'static>;
        let element = [
            std::mem::size_of::<ModuleEffectIntent>(),
            std::mem::size_of::<ModuleEmit>(),
            std::mem::size_of::<serde_json::Value>(),
            std::mem::size_of::<Content>(),
            1,
        ]
        .into_iter()
        .max()
        .unwrap();
        let stages = add(shape.max_depth, 3)?;
        let sequences = mul(mul(count(shape.sequence_entries)?, element)?, 4)?;
        let maps = mul(
            count(shape.map_entries)?,
            add(1024, mul(std::mem::size_of::<(Content, Content)>(), 4)?)?,
        )?;
        let values = mul(
            count(shape.values)?,
            add(mul(std::mem::size_of::<Content>(), 4)?, 128)?,
        )?;
        let strings = mul(count(shape.string_bytes)?, 16)?;
        let heap = add(
            add(mul(add(add(sequences, maps)?, values)?, stages)?, strings)?,
            add(std::mem::size_of::<ModuleOutput>(), 8192)?,
        )?;
        let heap = add(heap, state_heap)?;
        if heap > self.work.max_output_heap_bytes {
            return Err(ValueTreeError::ResourceLimited("module owned output heap"));
        }
        let charge = self.memory.reserve(heap, MemoryClass::Ordinary)?;
        let mut output: ModuleOutput = serde_cbor::from_slice(bytes).map_err(|_| invalid())?;
        // The actual encoded byte count comes from the executed VM result,
        // never a module-supplied output_bytes value.
        output.output_bytes = bytes.len() as u64;
        // Exhaustive DTO layout guards: new owned fields force review.
        let ModuleOutput {
            new_state,
            effects,
            emits,
            tick_lifecycle,
            output_bytes,
        } = &output;
        let _: &Option<Vec<u8>> = new_state;
        let _: &Vec<ModuleEffectIntent> = effects;
        let _: &Vec<ModuleEmit> = emits;
        let _: &Option<ModuleTickLifecycleDirective> = tick_lifecycle;
        let _: &u64 = output_bytes;
        for ModuleEffectIntent {
            kind,
            params,
            cap_ref,
            cap_slot,
        } in effects
        {
            let _: &String = kind;
            let _: &serde_json::Value = params;
            let _: &String = cap_ref;
            let _: &Option<String> = cap_slot;
        }
        for ModuleEmit { kind, payload } in emits {
            let _: &String = kind;
            let _: &serde_json::Value = payload;
        }
        if let Some(value) = tick_lifecycle {
            match value {
                ModuleTickLifecycleDirective::WakeAfterTicks { ticks } => {
                    let _: &u64 = ticks;
                }
                ModuleTickLifecycleDirective::Suspend => {}
            }
        }
        if let Err((code, detail)) =
            oasis7_wasm_abi::module_raw_output::validate_module_output_limits_v1(
                self.request,
                &output,
            )
        {
            self.limit_failure = Some(ModuleCallFailure {
                module_id: self.request.module_id.clone(),
                trace_id: self.request.trace_id.clone(),
                code,
                detail: detail.to_owned(),
            });
            return Err(invalid());
        }
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        self.output = Some(charge.retain(output));
        self.failed = false;
        Ok(())
    }
    fn finish(
        mut self,
        result: Result<(), ModuleCallFailure>,
    ) -> Result<ReservedValue<ModuleOutput>, RootModuleOutputErrorV1> {
        if let Some(error) = self.error.take() {
            return Err(RootModuleOutputErrorV1::Value(error));
        }
        if let Err(failure) = result {
            let actual = add(
                add(failure.module_id.capacity(), failure.trace_id.capacity())
                    .map_err(RootModuleOutputErrorV1::Value)?,
                failure.detail.capacity(),
            )
            .map_err(RootModuleOutputErrorV1::Value)?;
            if failure.module_id != self.request.module_id
                || failure.trace_id != self.request.trace_id
                || failure.detail.len() > RAW_SANDBOX_FAILURE_DETAIL_MAX_BYTES_V1
                || actual > self.failure_charge.bytes()
            {
                return Err(RootModuleOutputErrorV1::Value(invalid()));
            }
            return Err(RootModuleOutputErrorV1::Sandbox(
                self.failure_charge.retain(failure),
            ));
        }
        if self.failed || !self.seen || (self.cancelled)() {
            return Err(RootModuleOutputErrorV1::Value(invalid()));
        }
        self.output
            .take()
            .ok_or(RootModuleOutputErrorV1::Value(invalid()))
    }
}
impl<C: FnMut() -> bool> ModuleRawOutputSinkV1 for OutputSink<'_, C> {
    fn consume_output(&mut self, bytes: &[u8]) -> Result<(), ModuleCallFailure> {
        match self.decode(bytes) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.failed = true;
                if let Some(failure) = self.limit_failure.take() {
                    return Err(failure);
                }
                let code = if matches!(&error, ValueTreeError::Cancelled) {
                    ModuleCallErrorCode::Interrupted
                } else if matches!(
                    &error,
                    ValueTreeError::ResourceLimited(_) | ValueTreeError::Memory(_)
                ) {
                    ModuleCallErrorCode::OutputTooLarge
                } else {
                    ModuleCallErrorCode::InvalidOutput
                };
                self.error = Some(error);
                Err(self.failure(code))
            }
        }
    }
}
/// Calls a real raw-output backend, not a caller-supplied success callback.
/// Payload and VM linear-memory/output-buffer admission remain independently
/// held through the call. Compiler/JIT/cache memory follows the existing
/// platform contract and is not falsely attributed to this payload fee.
pub fn call_admitted_root_module_output_v1<S: ModuleSandboxRawOutputV1, C: FnMut() -> bool>(
    sandbox: &mut S,
    request: &ModuleCallRequest,
    memory: &MemoryBudget,
    work: &mut RootModuleOutputWorkV1,
    mut cancelled: C,
) -> Result<ReservedValue<ModuleOutput>, RootModuleOutputErrorV1> {
    if cancelled() {
        return Err(RootModuleOutputErrorV1::Value(ValueTreeError::Cancelled));
    }
    let output = usize::try_from(request.limits.max_output_bytes).map_err(|_| {
        RootModuleOutputErrorV1::Value(ValueTreeError::ResourceLimited(
            "module output platform bound",
        ))
    })?;
    let linear = usize::try_from(request.limits.max_mem_bytes).map_err(|_| {
        RootModuleOutputErrorV1::Value(ValueTreeError::ResourceLimited(
            "module memory platform bound",
        ))
    })?;
    let phase = add(
        add(linear, output).map_err(RootModuleOutputErrorV1::Value)?,
        8192,
    )
    .map_err(RootModuleOutputErrorV1::Value)?;
    let _vm_phase = memory
        .reserve(phase, MemoryClass::Ordinary)
        .map_err(|e| RootModuleOutputErrorV1::Value(ValueTreeError::Memory(e)))?;
    let mut sink = OutputSink::new(request, memory, work, &mut cancelled)
        .map_err(RootModuleOutputErrorV1::Value)?;
    let result = sandbox.call_to(request, &mut sink);
    sink.finish(result)
}

#[cfg(test)]
#[path = "incremental_module_execution_output_tests.rs"]
mod tests;
