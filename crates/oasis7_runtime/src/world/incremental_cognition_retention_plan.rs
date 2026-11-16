//! Paid original retention decisions. This owner contains DATA only; matching
//! a real terminal marker, Current source epoch and TX is the fixed caller's
//! obligation, never a boolean or JSON setter supplied by this plan.
use crate::cognition_recovery::WorldCommitRecordV1;
use crate::cognition_retention::*;
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use std::{cell::RefCell, collections::BTreeSet, mem::size_of};
fn invalid(note: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(note)
}
fn error(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
struct Meter<'a, 'b, 'c> {
    common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    fees: RefCell<Vec<MemoryReservation>>,
    slots: RefCell<MemoryReservation>,
}
impl<'a, 'b, 'c> Meter<'a, 'b, 'c> {
    fn new(
        common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    ) -> Result<Self, BodyMutationError> {
        let zero = common
            .borrow()
            .memory()
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            common,
            fees: RefCell::new(Vec::new()),
            slots: RefCell::new(zero),
        })
    }
    fn finish(self, contents: Contents) -> Result<PaidCognitionRetentionPlanV1, BodyMutationError> {
        self.work(1, 0)?;
        let common = self.common.borrow();
        let core = common.work().identity();
        let memory = common.memory().clone();
        drop(common);
        Ok(PaidCognitionRetentionPlanV1 {
            contents,
            core,
            memory,
            _fees: self.fees.into_inner(),
            _slots: self.slots.into_inner(),
        })
    }
}
impl RetentionWorkV1 for Meter<'_, '_, '_> {
    type Error = BodyMutationError;
    fn work(&self, steps: u64, bytes: usize) -> Result<(), Self::Error> {
        self.common
            .borrow_mut()
            .charge(
                steps,
                u64::try_from(bytes).map_err(|_| invalid("retention processing length"))?,
            )
            .map_err(error)
    }
    fn owned(&self, bytes: usize) -> Result<(), Self::Error> {
        self.work(1, bytes)?;
        let fee = self
            .common
            .borrow()
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut fees = self.fees.borrow_mut();
        if fees.len() == fees.capacity() {
            let capacity = fees
                .capacity()
                .checked_mul(2)
                .and_then(|n| n.max(4).checked_add(0))
                .ok_or_else(|| invalid("retention fee slot capacity"))?;
            let n = capacity
                .checked_mul(size_of::<MemoryReservation>())
                .ok_or_else(|| invalid("retention fee slots layout"))?;
            self.work(1, n)?;
            let slots = self
                .common
                .borrow()
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            {
                let additional = capacity - fees.len();
                fees.try_reserve_exact(additional)
            }
            .map_err(|_| invalid("retention fee slots allocation"))?;
            if fees.capacity() != capacity {
                return Err(invalid("retention fee slots exact capacity"));
            }
            *self.slots.borrow_mut() = slots
        }
        fees.push(fee);
        Ok(())
    }
}
struct Facts<'s, 'm, 'a, 'b, 'c, S> {
    source: &'s S,
    meter: &'m Meter<'a, 'b, 'c>,
}
impl<S: RetentionFactsV1<Error = BodyMutationError>> RetentionWorkV1
    for Facts<'_, '_, '_, '_, '_, S>
{
    type Error = BodyMutationError;
    fn work(&self, s: u64, b: usize) -> Result<(), Self::Error> {
        self.meter.work(s, b)
    }
    fn owned(&self, n: usize) -> Result<(), Self::Error> {
        self.meter.owned(n)
    }
}
impl<S: RetentionFactsV1<Error = BodyMutationError>> RetentionFactsV1
    for Facts<'_, '_, '_, '_, '_, S>
{
    fn horizon(&self) -> Result<u64, Self::Error> {
        self.source.horizon()
    }
    fn floor(&self) -> Result<u64, Self::Error> {
        self.source.floor()
    }
    fn with_record<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&RetentionRecordV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.with_record(key, visit)
    }
    fn visit_records(
        &self,
        visit: impl FnMut(&str, &RetentionRecordV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.source.visit_records(visit)
    }
    fn with_pins<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&BTreeSet<String>>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.with_pins(key, visit)
    }
    fn visit_pins(
        &self,
        visit: impl FnMut(&str, &BTreeSet<String>) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.source.visit_pins(visit)
    }
    fn visit_response_artifacts(
        &self,
        visit: impl FnMut(&str) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.source.visit_response_artifacts(visit)
    }
    fn response_artifact_present(&self, key: &str) -> Result<bool, Self::Error> {
        self.source.response_artifact_present(key)
    }
}
pub(in crate::world) enum RetentionMutationRefV1<'a> {
    PutRecord {
        key: &'a str,
        record: &'a RetentionRecordV1,
    },
    AddResponseArtifact(&'a str),
    RemoveResponseArtifact(&'a str),
    AddPin {
        key: &'a str,
        reference: &'a str,
    },
    RemovePin {
        key: &'a str,
        reference: &'a str,
    },
    RemovePinSet(&'a str),
    RemoveRecord(&'a str),
    SetGcFloor(u64),
}
enum Contents {
    Insert(RetentionInsertDecisionV1),
    Pin(RetentionPinDecisionV1),
    Clear(Vec<RetentionClearPinDecisionV1>),
    Gc {
        decision: RetentionGcDecisionV1,
        add: Vec<String>,
        remove: Vec<String>,
    },
}
pub(in crate::world) struct PaidCognitionRetentionPlanV1 {
    contents: Contents,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fees: Vec<MemoryReservation>,
    _slots: MemoryReservation,
}
impl PaidCognitionRetentionPlanV1 {
    pub(in crate::world) fn validate(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(error)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._slots.uses_budget(common.memory())
        {
            return Err(invalid("retention DATA plan context"));
        }
        for fee in &self._fees {
            common.consume_steps(1).map_err(error)?;
            if !fee.uses_budget(common.memory()) {
                return Err(invalid("retention DATA plan member pool"));
            }
        }
        Ok(())
    }
    pub(in crate::world) fn gc_report(&self) -> Option<&RetentionGcReport> {
        match &self.contents {
            Contents::Gc { decision, .. } => Some(&decision.report),
            _ => None,
        }
    }
    pub(in crate::world) fn visit(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
        mut emit: impl FnMut(
            RetentionMutationRefV1<'_>,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<(), BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.validate(common)?;
        match &self.contents {
            Contents::Insert(v) => {
                if let Some(id) = &v.response_artifact {
                    common.consume_steps(1).map_err(error)?;
                    emit(RetentionMutationRefV1::AddResponseArtifact(id), common)?
                }
                common.consume_steps(1).map_err(error)?;
                emit(
                    RetentionMutationRefV1::PutRecord {
                        key: &v.key,
                        record: &v.record,
                    },
                    common,
                )?
            }
            Contents::Pin(v) => {
                common.consume_steps(1).map_err(error)?;
                emit(
                    RetentionMutationRefV1::AddPin {
                        key: &v.key,
                        reference: &v.reference,
                    },
                    common,
                )?
            }
            Contents::Clear(values) => {
                for value in values {
                    common.consume_steps(1).map_err(error)?;
                    if value.remove_set {
                        emit(RetentionMutationRefV1::RemovePinSet(&value.key), common)?
                    } else {
                        for reference in &value.references {
                            common.consume_steps(1).map_err(error)?;
                            emit(
                                RetentionMutationRefV1::RemovePin {
                                    key: &value.key,
                                    reference,
                                },
                                common,
                            )?
                        }
                    }
                }
            }
            Contents::Gc {
                decision,
                add,
                remove,
            } => {
                common.consume_steps(1).map_err(error)?;
                emit(RetentionMutationRefV1::SetGcFloor(decision.floor), common)?;
                for key in &decision.report.deleted_keys {
                    common.consume_steps(1).map_err(error)?;
                    emit(RetentionMutationRefV1::RemoveRecord(key), common)?;
                    emit(RetentionMutationRefV1::RemovePinSet(key), common)?
                }
                for id in remove {
                    common.consume_steps(1).map_err(error)?;
                    emit(RetentionMutationRefV1::RemoveResponseArtifact(id), common)?
                }
                for id in add {
                    common.consume_steps(1).map_err(error)?;
                    emit(RetentionMutationRefV1::AddResponseArtifact(id), common)?
                }
            }
        }
        self.validate(common)
    }
}
fn header(meter: &Meter<'_, '_, '_>) -> Result<(), BodyMutationError> {
    meter.owned(size_of::<PaidCognitionRetentionPlanV1>())
}
fn admit_record(
    meter: &Meter<'_, '_, '_>,
    record: &RetentionRecordV1,
) -> Result<(), BodyMutationError> {
    let mut n = size_of::<RetentionRecordV1>();
    for value in [
        &record.schema_version,
        &record.world_id,
        &record.envelope_idempotency_key,
        &record.envelope_digest,
        &record.status,
        &record.response_artifact_id,
        &record.continuation_id,
    ] {
        meter.work(1, 0)?;
        n = n
            .checked_add(value.capacity())
            .ok_or_else(|| invalid("retention record layout"))?
    }
    for value in [
        &record.agent_session_id,
        &record.agent_turn_id,
        &record.decision_request_id,
        &record.terminal_disposition,
        &record.receipt_id,
        &record.receipt_digest,
        &record.commit_record_id,
    ] {
        meter.work(1, 0)?;
        if let Some(value) = value {
            n = n
                .checked_add(value.capacity())
                .ok_or_else(|| invalid("retention record layout"))?
        }
    }
    meter.owned(n)
}
pub(in crate::world) fn prepare_retention_insert_plan_v1(
    record: RetentionRecordV1,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<PaidCognitionRetentionPlanV1, BodyMutationError> {
    let meter = Meter::new(common)?;
    header(&meter)?;
    admit_record(&meter, &record)?;
    let decision = prepare_retention_insert_with_work_v1(record, &meter)?;
    meter.finish(Contents::Insert(decision))
}
pub(in crate::world) fn prepare_retention_commit_plan_v1(
    marker: &WorldCommitRecordV1,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<PaidCognitionRetentionPlanV1, BodyMutationError> {
    let meter = Meter::new(common)?;
    header(&meter)?;
    let record = retention_record_from_commit_with_work_v1(marker, &meter)?;
    let decision = prepare_retention_insert_with_work_v1(record, &meter)?;
    meter.finish(Contents::Insert(decision))
}
pub(in crate::world) fn prepare_retention_pin_plan_v1(
    key: &str,
    reference: &str,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<PaidCognitionRetentionPlanV1, BodyMutationError> {
    let meter = Meter::new(common)?;
    header(&meter)?;
    let decision = prepare_retention_pin_with_work_v1(key, reference, &meter)?;
    meter.finish(Contents::Pin(decision))
}
pub(in crate::world) fn prepare_retention_clear_prefix_plan_v1<
    S: RetentionFactsV1<Error = BodyMutationError>,
>(
    source: &S,
    prefix: &str,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<PaidCognitionRetentionPlanV1, BodyMutationError> {
    let meter = Meter::new(common)?;
    header(&meter)?;
    let facts = Facts {
        source,
        meter: &meter,
    };
    let changes = prepare_retention_clear_prefix_with_work_v1(&facts, prefix)?;
    meter.finish(Contents::Clear(changes))
}
pub(in crate::world) fn prepare_retention_gc_plan_v1<
    S: RetentionFactsV1<Error = BodyMutationError>,
>(
    source: &S,
    now_tick: u64,
    gc_floor_tick: u64,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<PaidCognitionRetentionPlanV1, BodyMutationError> {
    let meter = Meter::new(common)?;
    header(&meter)?;
    let facts = Facts {
        source,
        meter: &meter,
    };
    let decision = prepare_retention_gc_with_work_v1(&facts, now_tick, gc_floor_tick)?;
    let mut add = Vec::new();
    let mut remove = Vec::new();
    meter.owned(2 * size_of::<Vec<String>>())?;
    for id in &decision.artifacts {
        meter.work(1, 0)?;
        if !facts.response_artifact_present(id)? {
            plan_push(&meter, &mut add, retention_text_with_work_v1(&meter, id)?)?
        }
    }
    let mut max = 0;
    for id in &decision.artifacts {
        meter.work(1, 0)?;
        max = max.max(id.len());
    }
    facts.visit_response_artifacts(|id| {
        let levels = (decision.artifacts.len() + 1).ilog2() as usize + 2;
        meter.work(
            (levels * 11) as u64,
            (max + id.len()).saturating_mul(levels * 11),
        )?;
        if !decision.artifacts.contains(id) {
            plan_push(
                &meter,
                &mut remove,
                retention_text_with_work_v1(&meter, id)?,
            )?
        }
        Ok(())
    })?;
    meter.finish(Contents::Gc {
        decision,
        add,
        remove,
    })
}
fn plan_push(
    meter: &Meter<'_, '_, '_>,
    values: &mut Vec<String>,
    value: String,
) -> Result<(), BodyMutationError> {
    if values.len() == values.capacity() {
        let capacity = values.capacity().saturating_mul(2).max(4);
        meter.owned(capacity.saturating_mul(size_of::<String>()))?;
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| invalid("retention artifact delta slots"))?;
        if values.capacity() != capacity {
            return Err(invalid("retention artifact delta exact capacity"));
        }
    }
    meter.work(1, 0)?;
    values.push(value);
    Ok(())
}

/// A replay observation never grants provider/effect/link execution. The exact
/// original business result is nested inside this paid DATA owner.
pub(in crate::world) struct PaidCognitionRetentionReplayDataV1 {
    result: Result<RetentionReplayResult, RetentionError>,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fees: Vec<MemoryReservation>,
    _slots: MemoryReservation,
}
impl PaidCognitionRetentionReplayDataV1 {
    pub(in crate::world) fn result(&self) -> Result<&RetentionReplayResult, &RetentionError> {
        self.result.as_ref()
    }
    pub(in crate::world) fn validate(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(error)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._slots.uses_budget(common.memory())
        {
            return Err(invalid("retention replay DATA context"));
        }
        for fee in &self._fees {
            common.consume_steps(1).map_err(error)?;
            if !fee.uses_budget(common.memory()) {
                return Err(invalid("retention replay member pool"));
            }
        }
        Ok(())
    }
}
pub(in crate::world) fn prepare_retention_replay_data_v1<
    S: RetentionFactsV1<Error = BodyMutationError>,
>(
    source: &S,
    request: &RetentionReplayRequestV1,
    probe: &RetentionExecutionProbe,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<PaidCognitionRetentionReplayDataV1, BodyMutationError> {
    let meter = Meter::new(common)?;
    meter.owned(size_of::<PaidCognitionRetentionReplayDataV1>())?;
    let facts = Facts {
        source,
        meter: &meter,
    };
    let result = replay_retention_v1_with_work(&facts, request, probe)?;
    let c = common.borrow();
    let core = c.work().identity();
    let memory = c.memory().clone();
    drop(c);
    Ok(PaidCognitionRetentionReplayDataV1 {
        result,
        core,
        memory,
        _fees: meter.fees.into_inner(),
        _slots: meter.slots.into_inner(),
    })
}
