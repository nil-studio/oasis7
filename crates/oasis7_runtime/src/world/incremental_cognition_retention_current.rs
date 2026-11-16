//! Fixed DATA-only dispatch of original paid retention plans into Current.
//! The caller binds the original terminal/source authority before choosing a
//! plan. A real pre-plan cut rejects reuse after any source mutation.
use super::{
    incremental_body::BodyMutationError,
    incremental_cognition_retention_plan::{PaidCognitionRetentionPlanV1, RetentionMutationRefV1},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_retention_ledger::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{JsonPathSegmentV1, JsonSubtreeEditV1},
};
use serde_json::Value;
use std::{collections::BTreeSet, mem::size_of};
fn invalid(s: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(s)
}
fn error(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
fn pay(c: &mut FormalBoundedContextV1<'_>, s: u64, b: usize) -> Result<(), BodyMutationError> {
    c.charge(
        s,
        u64::try_from(b).map_err(|_| invalid("retention DATA byte length"))?,
    )
    .map_err(error)
}
/// Mint only before reading the actual original-plan facts. This is a DATA cut,
/// never a terminal permission or an accepted-source receipt.
struct CurrentRetentionPlanCutV1 {
    root: Option<StateRootV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
fn capture_current_retention_plan_cut_v1<M: MerkleNodeReader>(
    current: &CurrentCognitionRetentionLedgerV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionPlanCutV1, BodyMutationError> {
    current.validate_context(tx, common, live)?;
    if current.payload_root_data_v1().is_none() {
        return Err(invalid(
            "retention original operation requires actual present payload",
        ));
    }
    pay(common, 1024, 8192)?;
    let fee = common
        .memory()
        .reserve(
            size_of::<CurrentRetentionPlanCutV1>() + 8192,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(CurrentRetentionPlanCutV1 {
        root: current.payload_root_data_v1().cloned(),
        guard: tx.guard(),
        core: common.work().identity(),
        fee,
    })
}
fn validate_cut<M: MerkleNodeReader>(
    cut: &CurrentRetentionPlanCutV1,
    current: &CurrentCognitionRetentionLedgerV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    pay(common, 1, 512)?;
    current.validate_context(tx, common, live)?;
    tx.validate_read_source_guard(&cut.guard, live)
        .map_err(error)?;
    if cut.core != common.work().identity()
        || !cut.fee.uses_budget(common.memory())
        || cut.root.as_ref() != current.payload_root_data_v1()
    {
        return Err(invalid("retention original-plan stale Current cut"));
    }
    Ok(())
}
// Chunking arbitrary UTF8 at byte boundaries must not re-parse each chunk.
fn text(value: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<String, BodyMutationError> {
    let mut out = Vec::with_capacity(value.len());
    for part in value.as_bytes().chunks(8192) {
        pay(common, 1, part.len())?;
        out.extend_from_slice(part)
    }
    String::from_utf8(out).map_err(error)
}
fn set_delta(
    value: Option<&Value>,
    member: &str,
    add: bool,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(Value, MemoryReservation), BodyMutationError> {
    let array = match value {
        None => None,
        Some(Value::Array(v)) => Some(v),
        _ => return Err(invalid("retention selected set is not original array")),
    };
    let n = array.map_or(0, Vec::len);
    let mut bytes = member.len();
    let mut max = member.len();
    if let Some(array) = array {
        for value in array {
            pay(common, 1, 0)?;
            let value = value
                .as_str()
                .ok_or_else(|| invalid("retention selected set member is not text"))?;
            bytes = bytes
                .checked_add(value.len())
                .ok_or_else(|| invalid("retention set layout overflow"))?;
            max = max.max(value.len());
        }
    }
    let layout = n
        .checked_add(1)
        .and_then(|n| n.checked_mul(2048))
        .and_then(|n| n.checked_add(bytes))
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| invalid("retention set layout overflow"))?;
    pay(common, 1, layout)?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut values = BTreeSet::new();
    if let Some(array) = array {
        for value in array {
            let value = value
                .as_str()
                .ok_or_else(|| invalid("retention set member"))?;
            let levels = (values.len() + 1usize).ilog2() as usize + 2;
            pay(
                common,
                (levels * 11) as u64,
                max.saturating_add(value.len()).saturating_mul(levels * 11),
            )?;
            values.insert(text(value, common)?);
        }
    }
    let levels = (values.len() + 1usize).ilog2() as usize + 2;
    pay(
        common,
        (levels * 11) as u64,
        max.saturating_add(member.len()).saturating_mul(levels * 11),
    )?;
    if add {
        values.insert(text(member, common)?);
    } else {
        values.remove(member);
    }
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        pay(common, 1, 0)?;
        out.push(Value::String(value));
    }
    Ok((Value::Array(out), fee))
}
fn record_json(
    record: &crate::cognition_retention::RetentionRecordV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(Value, MemoryReservation), BodyMutationError> {
    let mut bytes = 0usize;
    for value in [
        &record.schema_version,
        &record.world_id,
        &record.envelope_idempotency_key,
        &record.envelope_digest,
        &record.status,
        &record.response_artifact_id,
        &record.continuation_id,
    ]
    .into_iter()
    .map(Some)
    .chain([
        record.agent_session_id.as_ref(),
        record.agent_turn_id.as_ref(),
        record.decision_request_id.as_ref(),
        record.terminal_disposition.as_ref(),
        record.receipt_id.as_ref(),
        record.receipt_digest.as_ref(),
        record.commit_record_id.as_ref(),
    ]) {
        pay(common, 1, 0)?;
        if let Some(value) = value {
            for chunk in value.as_bytes().chunks(8192) {
                pay(common, 1, chunk.len())?;
            }
            bytes = bytes
                .checked_add(value.len())
                .ok_or_else(|| invalid("retention record JSON layout"))?;
        }
    }
    let layout = bytes
        .checked_add(65536)
        .ok_or_else(|| invalid("retention record JSON layout"))?;
    pay(common, 1024, layout)?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let value = match serde_json::to_value(record) {
        Ok(value) => value,
        Err(source) => return Err(error(PaidRetentionJsonError { source, _fee: fee })),
    };
    Ok((value, fee))
}
/// Only fixed original paths can be emitted. Selected proof/value/expected-root
/// owners remain alive through the write; there is no string-as-array-index.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn apply_current_retention_plan_v1<M, P, R, E, C, W, D>(
    prepared: CurrentRetentionPreparedPlanV1,
    current: &mut CurrentCognitionRetentionLedgerV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, D>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    D: FnMut() -> bool,
{
    let CurrentRetentionPreparedPlanV1 { plan, cut } = prepared;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        plan.validate(common)?;
        validate_cut(&cut, current, tx, common, live)?;
        if current.payload_root_data_v1().is_none() {
            return Err(invalid(
                "retention original plan requires actual present payload",
            ));
        }
        plan.visit(common, |mutation, common| {
            use JsonPathSegmentV1::Name;
            // Selection is performed before constructing the replacement. It is never
            // refreshed after preparation to let stale expected data pass.
            let (column, key): (&str, Option<&str>) = match &mutation {
                RetentionMutationRefV1::PutRecord { key, .. }
                | RetentionMutationRefV1::RemoveRecord(key) => ("records", Some(key)),
                RetentionMutationRefV1::AddPin { key, .. }
                | RetentionMutationRefV1::RemovePin { key, .. }
                | RetentionMutationRefV1::RemovePinSet(key) => ("pins", Some(key)),
                RetentionMutationRefV1::AddResponseArtifact(_)
                | RetentionMutationRefV1::RemoveResponseArtifact(_) => ("response_artifacts", None),
                RetentionMutationRefV1::SetGcFloor(_) => ("gc_floor_tick", None),
            };
            let path = [Name(column), Name(key.unwrap_or(""))];
            let selected_path = &path[..if key.is_some() { 2 } else { 1 }];
            let selected = read_current_cognition_retention_selection_v1(
                current,
                selected_path,
                tx,
                read,
                work,
                common,
                live,
            )?;
            if selected.value().is_none()
                && matches!(
                    &mutation,
                    RetentionMutationRefV1::RemoveRecord(_)
                        | RetentionMutationRefV1::RemovePinSet(_)
                        | RetentionMutationRefV1::RemovePin { .. }
                        | RetentionMutationRefV1::RemoveResponseArtifact(_)
                )
            {
                return Ok(());
            }
            let mut owned = None;
            let after = match &mutation {
                RetentionMutationRefV1::PutRecord { record, .. } => {
                    let (value, fee) = record_json(record, common)?;
                    owned = Some((value, fee));
                    owned.as_ref().map(|v| &v.0)
                }
                RetentionMutationRefV1::RemoveRecord(_)
                | RetentionMutationRefV1::RemovePinSet(_) => None,
                RetentionMutationRefV1::AddPin { reference, .. }
                | RetentionMutationRefV1::AddResponseArtifact(reference) => {
                    let (value, fee) = set_delta(selected.value(), reference, true, common)?;
                    owned = Some((value, fee));
                    owned.as_ref().map(|v| &v.0)
                }
                RetentionMutationRefV1::RemovePin { reference, .. }
                | RetentionMutationRefV1::RemoveResponseArtifact(reference) => {
                    if selected.value().is_none() {
                        return Ok(());
                    }
                    let (value, fee) = set_delta(selected.value(), reference, false, common)?;
                    owned = Some((value, fee));
                    owned.as_ref().map(|v| &v.0)
                }
                RetentionMutationRefV1::SetGcFloor(value) => {
                    pay(common, 1, size_of::<Value>())?;
                    let fee = common
                        .memory()
                        .reserve(size_of::<Value>(), MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    owned = Some((Value::from(*value), fee));
                    owned.as_ref().map(|v| &v.0)
                }
            };
            selected.validate_context(tx, common, live)?;
            let edit = JsonSubtreeEditV1::ObjectMember {
                name: key.unwrap_or(column),
                expected: selected.expected_root(),
                after,
            };
            let parent_path = &path[..if key.is_some() { 1 } else { 0 }];
            edit_current_cognition_retention_ledger_v1(
                current,
                parent_path,
                edit,
                tx,
                read,
                write,
                work,
                common,
                live,
            )
            .map_err(error)?;
            drop(owned);
            drop(selected);
            Ok(())
        })
    }));
    match result {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}

/// No public binder can pair a previously created plan with a fresh cut. The
/// only constructors below capture the actual Current before invoking its
/// fixed operation's original rule producer.
pub(in crate::world) struct CurrentRetentionPreparedPlanV1 {
    plan: PaidCognitionRetentionPlanV1,
    cut: CurrentRetentionPlanCutV1,
}
use super::incremental_cognition_retention_plan as plans;
use crate::cognition_retention::{
    RetentionExecutionProbe, RetentionFactsV1, RetentionRecordV1, RetentionReplayRequestV1,
    RetentionWorkV1,
};
use serde::Deserialize;
use std::cell::RefCell;

#[derive(Debug)]
struct PaidRetentionJsonError {
    source: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Display for PaidRetentionJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.source, f)
    }
}
impl std::error::Error for PaidRetentionJsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
impl CurrentRetentionPreparedPlanV1 {
    pub(in crate::world) fn gc_report(
        &self,
    ) -> Option<&crate::cognition_retention::RetentionGcReport> {
        self.plan.gc_report()
    }
}
fn decode_record(
    value: &Value,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<ReservedValue<RetentionRecordV1>, BodyMutationError> {
    let phase = row_phase(value, common)?;
    match RetentionRecordV1::deserialize(value) {
        Ok(record) => Ok(phase.retain(record)),
        Err(source) => Err(error(PaidRetentionJsonError {
            source,
            _fee: phase,
        })),
    }
}
fn ordered_keys<'a>(
    rows: &'a serde_json::Map<String, Value>,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<ReservedValue<Vec<&'a str>>, BodyMutationError> {
    let mut max = 0;
    for key in rows.keys() {
        pay(&mut common.borrow_mut(), 1, key.len())?;
        max = max.max(key.len());
    }
    let bytes = rows
        .len()
        .checked_mul(2048)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| invalid("retention ordered keys layout"))?;
    pay(&mut common.borrow_mut(), 1, bytes)?;
    let fee = common
        .borrow()
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut sorted = BTreeSet::new();
    for key in rows.keys() {
        let levels = (sorted.len() + 1usize).ilog2() as usize + 2;
        pay(
            &mut common.borrow_mut(),
            (levels * 11) as u64,
            max.saturating_add(key.len()).saturating_mul(levels * 11),
        )?;
        sorted.insert(key.as_str());
    }
    let mut out = Vec::with_capacity(sorted.len());
    for key in sorted {
        pay(&mut common.borrow_mut(), 1, 0)?;
        out.push(key);
    }
    Ok(fee.retain(out))
}
fn row_phase(
    value: &Value,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<MemoryReservation, BodyMutationError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("retention original record object"))?;
    let mut bytes = 65536usize;
    for (key, value) in object {
        pay(&mut common.borrow_mut(), 1, key.len())?;
        bytes = bytes
            .checked_add(key.len().saturating_mul(2))
            .ok_or_else(|| invalid("retention row layout"))?;
        if let Some(value) = value.as_str() {
            for chunk in value.as_bytes().chunks(8192) {
                pay(&mut common.borrow_mut(), 1, chunk.len())?;
            }
            bytes = bytes
                .checked_add(value.len().saturating_mul(2))
                .ok_or_else(|| invalid("retention row layout"))?;
        }
    }
    pay(&mut common.borrow_mut(), 1024, bytes)?;
    common
        .borrow()
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}
fn read_set(
    value: &Value,
    common: &RefCell<&mut FormalBoundedContextV1<'_>>,
) -> Result<(BTreeSet<String>, MemoryReservation), BodyMutationError> {
    let array = value
        .as_array()
        .ok_or_else(|| invalid("retention original text set array"))?;
    let mut bytes = 4096usize;
    let mut max = 0;
    for item in array {
        pay(&mut common.borrow_mut(), 1, 0)?;
        let item = item
            .as_str()
            .ok_or_else(|| invalid("retention original text set member"))?;
        bytes = bytes
            .checked_add(2048)
            .and_then(|n| n.checked_add(item.len()))
            .ok_or_else(|| invalid("retention set layout"))?;
        max = max.max(item.len());
    }
    pay(&mut common.borrow_mut(), 1, bytes)?;
    let fee = common
        .borrow()
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut out = BTreeSet::new();
    for item in array {
        let item = item
            .as_str()
            .ok_or_else(|| invalid("retention original set member"))?;
        let levels = (out.len() + 1usize).ilog2() as usize + 2;
        pay(
            &mut common.borrow_mut(),
            (levels * 11) as u64,
            max.saturating_add(item.len()).saturating_mul(levels * 11),
        )?;
        out.insert(text(item, &mut common.borrow_mut())?);
    }
    Ok((out, fee))
}
/// Every fact is selected from the same borrowed actual Current. It contains
/// no caller DTO, status/verdict or configurable path resolver.
struct CurrentFacts<'a, 'b, 'c, F> {
    read: F,
    common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    owned: RefCell<Vec<MemoryReservation>>,
    slots: RefCell<Option<MemoryReservation>>,
    artifacts: RefCell<Option<(ReservedValue<BTreeSet<String>>, usize)>>,
}
impl<F> RetentionWorkV1 for CurrentFacts<'_, '_, '_, F> {
    type Error = BodyMutationError;
    fn work(&self, s: u64, b: usize) -> Result<(), Self::Error> {
        pay(&mut self.common.borrow_mut(), s, b)
    }
    fn owned(&self, n: usize) -> Result<(), Self::Error> {
        self.work(1, n)?;
        let fee = self
            .common
            .borrow()
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut fees = self.owned.borrow_mut();
        if fees.len() == fees.capacity() {
            let capacity = fees
                .capacity()
                .checked_mul(2)
                .map(|n| n.max(4))
                .ok_or_else(|| invalid("retention fact fee slots"))?;
            let bytes = capacity
                .checked_mul(size_of::<MemoryReservation>())
                .ok_or_else(|| invalid("retention fact fee slots"))?;
            self.work(1, bytes)?;
            let slots = self
                .common
                .borrow()
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let additional = capacity - fees.len();
            fees.try_reserve_exact(additional)
                .map_err(|_| invalid("retention fact fee slots allocation"))?;
            *self.slots.borrow_mut() = Some(slots);
        }
        fees.push(fee);
        Ok(())
    }
}
impl<F> RetentionFactsV1 for CurrentFacts<'_, '_, '_, F>
where
    F: Fn(
        &[JsonPathSegmentV1<'_>],
    ) -> Result<PaidCurrentCognitionRetentionSelectionV1, BodyMutationError>,
{
    fn horizon(&self) -> Result<u64, Self::Error> {
        let selected = (self.read)(&[JsonPathSegmentV1::Name("horizon")])?;
        selected
            .value()
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("retention actual horizon"))
    }
    fn floor(&self) -> Result<u64, Self::Error> {
        let selected = (self.read)(&[JsonPathSegmentV1::Name("gc_floor_tick")])?;
        match selected.value() {
            None => Ok(0),
            Some(v) => v
                .as_u64()
                .ok_or_else(|| invalid("retention actual GC floor")),
        }
    }
    fn with_record<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&RetentionRecordV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let selected = (self.read)(&[
            JsonPathSegmentV1::Name("records"),
            JsonPathSegmentV1::Name(key),
        ])?;
        match selected.value() {
            None => visit(None),
            Some(value) => {
                let record = decode_record(value, self.common)?;
                visit(Some(&record))
            }
        }
    }
    fn visit_records(
        &self,
        mut visit: impl FnMut(&str, &RetentionRecordV1) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        let selected = (self.read)(&[JsonPathSegmentV1::Name("records")])?;
        let rows = selected
            .value()
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("retention actual ordered records"))?;
        let keys = ordered_keys(rows, self.common)?;
        for key in keys.iter() {
            self.work(1, key.len())?;
            let value = rows
                .get(*key)
                .ok_or_else(|| invalid("retention selected record key"))?;
            let record = decode_record(value, self.common)?;
            visit(key, &record)?
        }
        Ok(())
    }
    fn with_pins<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&BTreeSet<String>>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let selected = (self.read)(&[
            JsonPathSegmentV1::Name("pins"),
            JsonPathSegmentV1::Name(key),
        ])?;
        match selected.value() {
            None => visit(None),
            Some(value) => {
                let (pins, phase) = read_set(value, self.common)?;
                let pins = phase.retain(pins);
                visit(Some(&pins))
            }
        }
    }
    fn visit_pins(
        &self,
        mut visit: impl FnMut(&str, &BTreeSet<String>) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        let selected = (self.read)(&[JsonPathSegmentV1::Name("pins")])?;
        let rows = selected
            .value()
            .and_then(Value::as_object)
            .ok_or_else(|| invalid("retention actual ordered pins"))?;
        let keys = ordered_keys(rows, self.common)?;
        for key in keys.iter() {
            self.work(1, key.len())?;
            let value = rows
                .get(*key)
                .ok_or_else(|| invalid("retention selected pins key"))?;
            let (pins, phase) = read_set(value, self.common)?;
            let pins = phase.retain(pins);
            visit(key, &pins)?
        }
        Ok(())
    }
    fn visit_response_artifacts(
        &self,
        mut visit: impl FnMut(&str) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.with_artifacts(|values, _| {
            for id in values {
                self.work(1, 0)?;
                visit(id)?
            }
            Ok(())
        })
    }
    fn response_artifact_present(&self, key: &str) -> Result<bool, Self::Error> {
        self.with_artifacts(|values, max| {
            let levels = (values.len() + 1usize).ilog2() as usize + 2;
            self.work(
                (levels * 11) as u64,
                max.saturating_add(key.len()).saturating_mul(levels * 11),
            )?;
            Ok(values.contains(key))
        })
    }
}

impl<F> CurrentFacts<'_, '_, '_, F>
where
    F: Fn(
        &[JsonPathSegmentV1<'_>],
    ) -> Result<PaidCurrentCognitionRetentionSelectionV1, BodyMutationError>,
{
    fn with_artifacts<T>(
        &self,
        visit: impl FnOnce(&BTreeSet<String>, usize) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        if self.artifacts.borrow().is_none() {
            let selected = (self.read)(&[JsonPathSegmentV1::Name("response_artifacts")])?;
            let value = selected
                .value()
                .ok_or_else(|| invalid("retention actual response artifacts"))?;
            let (values, phase) = read_set(value, self.common)?;
            let mut max = 0;
            for id in &values {
                self.work(1, 0)?;
                max = max.max(id.len());
            }
            *self.artifacts.borrow_mut() = Some((phase.retain(values), max));
        }
        let held = self.artifacts.borrow();
        let (values, max) = held
            .as_ref()
            .ok_or_else(|| invalid("retention actual artifact scratch"))?;
        visit(values, *max)
    }
}

pub(in crate::world) fn prepare_current_retention_insert_v1<M: MerkleNodeReader>(
    current: &CurrentCognitionRetentionLedgerV1,
    record: RetentionRecordV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionPreparedPlanV1, BodyMutationError> {
    let cut = capture_current_retention_plan_cut_v1(current, tx, common, live)?;
    let plan = plans::prepare_retention_insert_plan_v1(record, &RefCell::new(common))?;
    Ok(CurrentRetentionPreparedPlanV1 { plan, cut })
}
pub(in crate::world) fn prepare_current_retention_commit_v1<M: MerkleNodeReader>(
    current: &CurrentCognitionRetentionLedgerV1,
    record: &crate::cognition_recovery::WorldCommitRecordV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionPreparedPlanV1, BodyMutationError> {
    let cut = capture_current_retention_plan_cut_v1(current, tx, common, live)?;
    let plan = plans::prepare_retention_commit_plan_v1(record, &RefCell::new(common))?;
    Ok(CurrentRetentionPreparedPlanV1 { plan, cut })
}
pub(in crate::world) fn prepare_current_retention_pin_v1<M: MerkleNodeReader>(
    current: &CurrentCognitionRetentionLedgerV1,
    key: &str,
    reference: &str,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionPreparedPlanV1, BodyMutationError> {
    let cut = capture_current_retention_plan_cut_v1(current, tx, common, live)?;
    let plan = plans::prepare_retention_pin_plan_v1(key, reference, &RefCell::new(common))?;
    Ok(CurrentRetentionPreparedPlanV1 { plan, cut })
}
enum CurrentOperation<'a> {
    Gc { now: u64, floor: u64 },
    Clear { prefix: &'a str },
}
fn prepare_current_operation<M, P, R, E, C>(
    current: &CurrentCognitionRetentionLedgerV1,
    operation: CurrentOperation<'_>,
    tx: &ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionPreparedPlanV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let cut = capture_current_retention_plan_cut_v1(current, tx, common, live)?;
    if current.payload_root_data_v1().is_none() {
        return Err(invalid("retention actual Current payload absent"));
    }
    let common = RefCell::new(common);
    let read = RefCell::new(read);
    let work = RefCell::new(work);
    let facts = CurrentFacts {
        common: &common,
        owned: RefCell::new(Vec::new()),
        slots: RefCell::new(None),
        artifacts: RefCell::new(None),
        read: |path: &[JsonPathSegmentV1<'_>]| {
            read_current_cognition_retention_selection_v1(
                current,
                path,
                tx,
                &mut **read.borrow_mut(),
                &mut **work.borrow_mut(),
                &mut common.borrow_mut(),
                live,
            )
            .map_err(Into::into)
        },
    };
    let plan = match operation {
        CurrentOperation::Gc { now, floor } => {
            plans::prepare_retention_gc_plan_v1(&facts, now, floor, &common)?
        }
        CurrentOperation::Clear { prefix } => {
            plans::prepare_retention_clear_prefix_plan_v1(&facts, prefix, &common)?
        }
    };
    Ok(CurrentRetentionPreparedPlanV1 { plan, cut })
}
pub(in crate::world) fn prepare_current_retention_gc_v1<M, P, R, E, C>(
    current: &CurrentCognitionRetentionLedgerV1,
    now: u64,
    floor: u64,
    tx: &ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionPreparedPlanV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    prepare_current_operation(
        current,
        CurrentOperation::Gc { now, floor },
        tx,
        read,
        work,
        common,
        live,
    )
}
pub(in crate::world) fn prepare_current_retention_clear_prefix_v1<M, P, R, E, C>(
    current: &CurrentCognitionRetentionLedgerV1,
    prefix: &str,
    tx: &ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionPreparedPlanV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    prepare_current_operation(
        current,
        CurrentOperation::Clear { prefix },
        tx,
        read,
        work,
        common,
        live,
    )
}
pub(in crate::world) struct CurrentRetentionReplayDataV1 {
    data: plans::PaidCognitionRetentionReplayDataV1,
    cut: CurrentRetentionPlanCutV1,
}
impl CurrentRetentionReplayDataV1 {
    pub(in crate::world) fn result(
        &self,
    ) -> Result<
        &crate::cognition_retention::RetentionReplayResult,
        &crate::cognition_retention::RetentionError,
    > {
        self.data.result()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        current: &CurrentCognitionRetentionLedgerV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.data.validate(common)?;
        validate_cut(&self.cut, current, tx, common, live)
    }
}
pub(in crate::world) fn prepare_current_retention_replay_v1<M, P, R, E, C>(
    current: &CurrentCognitionRetentionLedgerV1,
    request: &RetentionReplayRequestV1,
    probe: &RetentionExecutionProbe,
    tx: &ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut CognitionRetentionWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CurrentRetentionReplayDataV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let cut = capture_current_retention_plan_cut_v1(current, tx, common, live)?;
    let common = RefCell::new(common);
    let read = RefCell::new(read);
    let work = RefCell::new(work);
    let facts = CurrentFacts {
        common: &common,
        owned: RefCell::new(Vec::new()),
        slots: RefCell::new(None),
        artifacts: RefCell::new(None),
        read: |path: &[JsonPathSegmentV1<'_>]| {
            read_current_cognition_retention_selection_v1(
                current,
                path,
                tx,
                &mut **read.borrow_mut(),
                &mut **work.borrow_mut(),
                &mut common.borrow_mut(),
                live,
            )
            .map_err(Into::into)
        },
    };
    let data = plans::prepare_retention_replay_data_v1(&facts, request, probe, &common)?;
    Ok(CurrentRetentionReplayDataV1 { data, cut })
}
