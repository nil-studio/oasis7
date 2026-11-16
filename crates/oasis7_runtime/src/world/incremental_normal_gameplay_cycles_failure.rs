//! Original invalid-directive diagnostic, from the selected physical emit and
//! the actual bounded serde error. No raw failure/permission constructor.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::incremental_world_event_decode::DecodedWorldEventV1;
use std::{
    fmt::{self, Write},
    sync::Arc,
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("gameplay directive failure source")
}
struct Diagnostic<'a, 'b, 'c> {
    tail: &'a mut NormalTailWorkV1<'b, 'c>,
    count: usize,
    output: Option<&'a mut String>,
    failure: Option<BodyMutationError>,
}
impl Write for Diagnostic<'_, '_, '_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let result = (|| {
            self.count = self.count.checked_add(text.len()).ok_or_else(bad)?;
            self.tail.charge(text.len())?;
            if let Some(output) = &mut self.output {
                if self.count > output.capacity() {
                    return Err(bad());
                }
                output.push_str(text);
            }
            Ok::<(), BodyMutationError>(())
        })();
        if let Err(e) = result {
            self.failure = Some(e);
            return Err(fmt::Error);
        }
        Ok(())
    }
}
fn diagnostic(
    error: &serde_json::Error,
    writer: &mut Diagnostic<'_, '_, '_>,
) -> Result<(), BodyMutationError> {
    let result = write!(
        writer,
        "decode {} payload failed: {}",
        kernel::GAMEPLAY_LIFECYCLE_EMIT_KIND,
        error
    );
    if let Some(e) = writer.failure.take() {
        return Err(e);
    }
    result.map_err(|_| bad())
}
pub(in crate::world) struct PreparedGameplayDirectiveFailureV1 {
    event: crate::WorldEvent,
    origin: ReservedValue<DecodedWorldEventV1>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _event_fee: MemoryReservation,
    _decode_error: serde_json::Error,
    _decode_fee: Arc<ReservedValue<()>>,
}
impl PreparedGameplayDirectiveFailureV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        if self.now != now
            || self.event.time != now
            || self.event.caused_by.is_some()
            || self.identity != identity
            || !self.memory.same_pool(memory)
            || !self.origin.uses_budget(memory)
            || !self._decode_fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || !matches!(&self.event.body,crate::WorldEventBody::ModuleCallFailed(f)if f.code==oasis7_wasm_abi::ModuleCallErrorCode::InvalidOutput)
        {
            return Err(bad());
        }
        Ok(())
    }
}
pub(super) struct GameplayDirectiveFailureDataV1 {
    failure: oasis7_wasm_abi::ModuleCallFailure,
    business: ReservedValue<crate::WorldError>,
    origin: ReservedValue<DecodedWorldEventV1>,
    event_fee: MemoryReservation,
    error: serde_json::Error,
    decode_fee: Arc<ReservedValue<()>>,
}
impl GameplayDirectiveFailureDataV1 {
    pub(super) fn finish(
        self,
        id: u64,
        now: u64,
        guard: PreparedGuard,
        identity: WorkIdentityV1,
        memory: MemoryBudget,
    ) -> (
        PreparedGameplayDirectiveFailureV1,
        ReservedValue<crate::WorldError>,
    ) {
        (
            PreparedGameplayDirectiveFailureV1 {
                event: crate::WorldEvent {
                    id,
                    time: now,
                    caused_by: None,
                    body: crate::WorldEventBody::ModuleCallFailed(self.failure),
                },
                origin: self.origin,
                guard,
                identity,
                memory,
                now,
                _event_fee: self.event_fee,
                _decode_error: self.error,
                _decode_fee: self.decode_fee,
            },
            self.business,
        )
    }
}
pub(super) fn prepare_directive_failure_v1(
    origin: ReservedValue<DecodedWorldEventV1>,
    error: serde_json::Error,
    decode_fee: Arc<ReservedValue<()>>,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<GameplayDirectiveFailureDataV1, GameplayCycleErrorV1> {
    if !origin.uses_budget(tail.memory()) || !decode_fee.uses_budget(tail.memory()) {
        return Err(bad().into());
    }
    let crate::WorldEventBody::ModuleEmitted(emit) = &origin.event().body else {
        return Err(bad().into());
    };
    let mut count = Diagnostic {
        tail,
        count: 0,
        output: None,
        failure: None,
    };
    diagnostic(&error, &mut count)?;
    let len = count.count;
    drop(count);
    let strings = len
        .checked_add(emit.module_id.len())
        .and_then(|n| n.checked_add(emit.trace_id.len()))
        .ok_or_else(bad)?;
    let event_fee = tail.reserve(
        std::mem::size_of::<crate::WorldEvent>()
            .checked_add(strings)
            .ok_or_else(bad)?,
        MemoryClass::Ordinary,
    )?;
    let business_fee = tail.reserve(
        std::mem::size_of::<crate::WorldError>()
            .checked_add(strings)
            .ok_or_else(bad)?,
        MemoryClass::Ordinary,
    )?;
    let mut detail = String::new();
    detail.try_reserve_exact(len).map_err(|_| bad())?;
    if detail.capacity() > len {
        return Err(bad().into());
    }
    let mut writer = Diagnostic {
        tail,
        count: 0,
        output: Some(&mut detail),
        failure: None,
    };
    diagnostic(&error, &mut writer)?;
    if writer.count != len {
        return Err(bad().into());
    }
    drop(writer);
    tail.charge(strings.checked_mul(2).ok_or_else(bad)?)?;
    let business = business_fee.retain(crate::WorldError::ModuleCallFailed {
        module_id: emit.module_id.clone(),
        trace_id: emit.trace_id.clone(),
        code: oasis7_wasm_abi::ModuleCallErrorCode::InvalidOutput,
        detail: detail.clone(),
    });
    let failure = oasis7_wasm_abi::ModuleCallFailure {
        module_id: emit.module_id.clone(),
        trace_id: emit.trace_id.clone(),
        code: oasis7_wasm_abi::ModuleCallErrorCode::InvalidOutput,
        detail,
    };
    Ok(GameplayDirectiveFailureDataV1 {
        failure,
        business,
        origin,
        event_fee,
        error,
        decode_fee,
    })
}
