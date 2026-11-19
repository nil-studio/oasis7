//! Original pruning/quota policy over private actual Current inputs.
use super::*;
use crate::{
    cognition_scheduler::{CognitionScheduler, SchedulerWakeV1},
    world::{
        incremental_normal_cognition_expiry::values,
        incremental_normal_tick_finalize::commitment::PaidIwsCurrentCommitmentV1,
    },
};
use oasis7_agent_api::iws_scheduler_v2::{
    IWS_CURRENT_COMMITMENT_SCHEME_V1, IwsWakeConditionValidatorV2, IwsWakeEvaluationContextV2,
    IwsWakeEvaluationV2,
};
use serde::{Serialize, ser::SerializeMap};
use std::cell::RefCell;
pub(super) enum SelectionErrorV2 {
    Body(BodyMutationError),
}
impl From<BodyMutationError> for SelectionErrorV2 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<ValueTreeError> for SelectionErrorV2 {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}
pub(super) fn root_error<OE>(e: SelectionErrorV2) -> RootModuleExecutionErrorV1<OE> {
    match e {
        SelectionErrorV2::Body(e) => RootModuleExecutionErrorV1::Body(e),
    }
}
pub(super) struct PaidSelectedCognitionWakesV2 {
    wakes: Vec<SchedulerWakeV1>,
    _fee: ReservedValue<()>,
}
impl PaidSelectedCognitionWakesV2 {
    pub(super) fn wakes(&self) -> &[SchedulerWakeV1] {
        &self.wakes
    }
    pub(super) fn empty(
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        common.charge(1, 0).map_err(formal)?;
        Ok(Self {
            wakes: Vec::new(),
            _fee: common
                .memory()
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?
                .retain(()),
        })
    }
}
pub(super) enum CurrentServiceJournalEventV2<'a> {
    Deactivated(&'a SchedulerWakeV1),
    Selected(Option<&'a SchedulerWakeV1>),
    Woken(&'a SchedulerWakeV1),
}
pub(super) struct CurrentCognitionSelectionPlanV2 {
    pub(super) state: state::CurrentServiceStateV2,
    pub(super) inputs: inputs::CurrentServiceInputsV2,
    cut: ReservedValue<PaidIwsCurrentCommitmentV1>,
    commitment: ReservedValue<String>,
    selected: PaidSelectedCognitionWakesV2,
    stale: Vec<SchedulerWakeV1>,
    pub(super) scheduler_after: values::OwnedJson,
    pub(super) evaluations_after: Option<values::OwnedJson>,
    _evaluation_fee: ReservedValue<()>,
    _fee: ReservedValue<()>,
}
impl CurrentCognitionSelectionPlanV2 {
    pub(super) fn scheduler(&self) -> &CognitionScheduler {
        &self.inputs.scheduler.value
    }
    pub(super) fn binding(&self) -> Option<&serde_json::Value> {
        self.inputs.binding()
    }
    pub(super) fn into_selected(self) -> PaidSelectedCognitionWakesV2 {
        self.selected
    }
    pub(super) fn journal_events(&self) -> impl Iterator<Item = CurrentServiceJournalEventV2<'_>> {
        self.stale
            .iter()
            .map(CurrentServiceJournalEventV2::Deactivated)
            .chain(self.selected.wakes.iter().flat_map(|wake| {
                [
                    Some(CurrentServiceJournalEventV2::Selected(Some(wake))),
                    (!wake.continuation_id.trim().is_empty())
                        .then_some(CurrentServiceJournalEventV2::Woken(wake)),
                ]
                .into_iter()
                .flatten()
            }))
            .chain(
                self.selected
                    .wakes
                    .is_empty()
                    .then_some(CurrentServiceJournalEventV2::Selected(None)),
            )
    }
    pub(super) fn validate_before_write<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.inputs.validate_evidence_identity(tx, common, live)?;
        self.cut
            .validate(tx, sources.parent, sources.input, common, live)?;
        if !self.commitment.uses_budget(common.memory()) || !self._fee.uses_budget(common.memory())
        {
            return Err(bad().into());
        }
        Ok(())
    }
}
fn commitment_text(
    cut: &PaidIwsCurrentCommitmentV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    common.charge(1, 71).map_err(formal)?;
    let fee = common
        .memory()
        .reserve(std::mem::size_of::<String>() + 71, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut text = String::with_capacity(71);
    text.push_str("sha256:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in cut.digest() {
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 15) as usize] as char);
    }
    if text.capacity() > 71 {
        return Err(ValueTreeError::ResourceLimited("IWS evaluation commitment capacity").into());
    }
    Ok(fee.retain(text))
}
fn find(
    rows: &[inputs::PreparedWakeInputV2],
    wake: &SchedulerWakeV1,
) -> Result<usize, BodyMutationError> {
    rows.iter()
        .position(|row| row.wake_id() == wake.wake_id)
        .ok_or_else(|| bad().into())
}
#[derive(Serialize)]
struct EvaluationRecordV2<'a> {
    schema_version: &'static str,
    commitment_scheme: &'static str,
    current_commitment: &'a str,
    current_frame:
        &'a oasis7_world_state::incremental_iws_tick_consensus::IwsCurrentCommitmentFrameV1,
    evaluation_tick: u64,
    evaluation_digest: &'a str,
    conditions_digest: &'a str,
    registration: &'a oasis7_agent_api::iws_scheduler_v2::IwsJournalReferenceV2,
}
struct EvaluationOverlay<'a> {
    before: &'a serde_json::Map<String, serde_json::Value>,
    updates: &'a [(String, values::OwnedJson)],
}
impl Serialize for EvaluationOverlay<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let retained = self
            .before
            .keys()
            .filter(|key| !self.updates.iter().any(|(id, _)| id == *key))
            .count();
        let mut out = s.serialize_map(Some(retained + self.updates.len()))?;
        for (key, value) in self.before {
            if !self.updates.iter().any(|(id, _)| id == key) {
                out.serialize_entry(key, value)?
            }
        }
        for (key, value) in self.updates {
            out.serialize_entry(key, &value.value)?
        }
        out.end()
    }
}
pub(super) fn prepare_selection_v2(
    state: state::CurrentServiceStateV2,
    mut inputs: inputs::CurrentServiceInputsV2,
    cut: ReservedValue<PaidIwsCurrentCommitmentV1>,
    tick: u64,
    work: &mut CurrentCognitionServiceWorkV2<'_>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<CurrentCognitionSelectionPlanV2, SelectionErrorV2> {
    let commitment = commitment_text(&cut, common)?;
    let count = inputs.rows().len();
    let eval_bytes = count
        .checked_mul(std::mem::size_of::<Option<IwsWakeEvaluationV2>>() + 4096)
        .ok_or_else(bad)?;
    common
        .charge(count as u64, eval_bytes as u64)
        .map_err(formal)?;
    let eval_fee = common
        .memory()
        .reserve(eval_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?
        .retain(());
    let mut evaluations = Vec::new();
    evaluations
        .try_reserve_exact(count)
        .map_err(|_| ValueTreeError::ResourceLimited("IWS evaluations"))?;
    if evaluations.capacity() > count {
        return Err(ValueTreeError::ResourceLimited("IWS evaluations capacity").into());
    }
    for row in inputs.rows() {
        let Some(spec) = row.evaluation.as_ref() else {
            evaluations.push(None);
            continue;
        };
        if spec.valid_until.is_some_and(|until| tick > until) {
            evaluations.push(None);
            continue;
        }
        common.charge(32768, 256 << 10).map_err(formal)?;
        let _phase = common
            .memory()
            .reserve(256 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut context = IwsWakeEvaluationContextV2::at(tick, inputs.reorg_epoch, &commitment)
            .map_err(formal)?;
        context = spec
            .predicates
            .enrich(context, &cut, tick, inputs.reorg_epoch, common)?;
        for event in &spec.events {
            common.charge(1, event.len() as u64).map_err(formal)?;
            context = context.with_event(event);
        }
        for receipt in &spec.receipts {
            common.charge(1, receipt.len() as u64).map_err(formal)?;
            context = context.with_receipt(receipt);
        }
        evaluations.push(Some(
            IwsWakeConditionValidatorV2::evaluate(&spec.conditions, &context).map_err(formal)?,
        ));
    }
    // Prepay actual source rows, never a configured capacity as actual work.
    let mut text = 0usize;
    let mut max_key = 0usize;
    for wake in inputs.scheduler.value.snapshot_order_wakes_v2() {
        let bytes = crate::cognition_scheduler::scheduler_wake_text_bytes_v1(wake);
        text = text.checked_add(bytes).ok_or_else(bad)?;
        max_key = max_key.max(bytes);
    }
    let raw = state.scheduler.as_ref().ok_or_else(bad)?.value();
    let mut metadata = count;
    if let Some(rows) = raw
        .get("recovered_wakes")
        .and_then(serde_json::Value::as_array)
    {
        metadata = metadata.checked_add(rows.len()).ok_or_else(bad)?;
        for row in rows {
            if let Some(key) = row.as_str() {
                common.charge(1, key.len() as u64).map_err(formal)?;
                max_key = max_key.max(key.len());
            }
        }
    }
    if let Some(rows) = raw
        .get("backpressure_priority")
        .and_then(serde_json::Value::as_object)
    {
        metadata = metadata.checked_add(rows.len()).ok_or_else(bad)?;
        for key in rows.keys() {
            common.charge(1, key.len() as u64).map_err(formal)?;
            max_key = max_key.max(key.len());
        }
    }
    let n = count as u64;
    let log = 64 - ((metadata as u64).max(1)).leading_zeros() as u64;
    let comparisons = n
        .checked_mul(n)
        .and_then(|v| v.checked_mul(64 + 32 * log))
        .and_then(|v| v.checked_add(1))
        .ok_or_else(bad)?;
    common
        .charge(
            comparisons,
            comparisons.checked_mul(max_key as u64).ok_or_else(bad)?,
        )
        .map_err(formal)?;
    let heap = count
        .checked_mul(std::mem::size_of::<SchedulerWakeV1>())
        .and_then(|n| n.checked_mul(32))
        .and_then(|n| text.checked_mul(16).and_then(|text| n.checked_add(text)))
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(bad)?;
    let policy_fee = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?
        .retain(());
    let errors = RefCell::new(None);
    let (stale, selected) = {
        let (scheduler, rows) = inputs.split_policy_rows_v2();
        for wake in scheduler.snapshot_order_wakes_v2() {
            find(rows, wake)?;
        }
        let stale = scheduler.prune_wakes_if(|wake| match find(rows, wake) {
            Ok(index) => rows[index].keep(),
            Err(error) => {
                if errors.borrow().is_none() {
                    *errors.borrow_mut() = Some(error)
                }
                false
            }
        });
        if let Some(error) = errors.borrow_mut().take() {
            return Err(error.into());
        }
        let selected = scheduler.select_ready_if(tick, |wake| match find(rows, wake) {
            Ok(index) => evaluations[index]
                .as_ref()
                .is_some_and(|value| value.status == "ready"),
            Err(error) => {
                if errors.borrow().is_none() {
                    *errors.borrow_mut() = Some(error)
                }
                false
            }
        });
        (stale, selected)
    };
    if let Some(error) = errors.borrow_mut().take() {
        return Err(error.into());
    }
    let mut scheduler_after = values::json(
        &inputs.scheduler.value,
        work.expiry.max_selected_heap,
        common,
    )?;
    let _count_fee = common
        .memory()
        .reserve(2048, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    common.charge(1, 64).map_err(formal)?;
    scheduler_after
        .value
        .as_object_mut()
        .ok_or_else(bad)?
        .insert(
            "backpressure_count".into(),
            serde_json::json!(inputs.scheduler.value.pending_backpressure_count()),
        );
    let empty = serde_json::Map::new();
    let before = state
        .evaluations
        .as_ref()
        .and_then(|record| record.value().as_object())
        .unwrap_or(&empty);
    let update_bytes = selected
        .len()
        .checked_mul(std::mem::size_of::<(String, values::OwnedJson)>() + 128)
        .ok_or_else(bad)?;
    let _updates_fee = common
        .memory()
        .reserve(update_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut updates = Vec::new();
    updates
        .try_reserve_exact(selected.len())
        .map_err(|_| ValueTreeError::ResourceLimited("IWS evaluation updates"))?;
    if updates.capacity() > selected.len() {
        return Err(ValueTreeError::ResourceLimited("IWS evaluation updates capacity").into());
    }
    for wake in &selected {
        let index = find(inputs.rows(), wake)?;
        let row = &inputs.rows()[index];
        let value = evaluations[index].as_ref().ok_or_else(bad)?;
        let spec = row.evaluation.as_ref().ok_or_else(bad)?;
        common
            .charge(1, row.continuation_id().len() as u64)
            .map_err(formal)?;
        let output = values::json(
            &EvaluationRecordV2 {
                schema_version: "iws-continuation-evaluation.v2",
                commitment_scheme: IWS_CURRENT_COMMITMENT_SCHEME_V1,
                current_commitment: &commitment,
                current_frame: cut.frame(),
                evaluation_tick: value.evaluation_tick,
                evaluation_digest: &value.evaluation_digest,
                conditions_digest: &value.conditions_digest,
                registration: &spec.registration,
            },
            work.expiry.max_selected_heap,
            common,
        )?;
        updates.push((row.continuation_id().to_owned(), output));
    }
    // Borrowed serialization preserves all previous in-flight entries and keeps
    // both input/output graph admissions live throughout the exact map overlay.
    let evaluations_after = if before.is_empty() && updates.is_empty() {
        None
    } else {
        let pairs = before
            .len()
            .checked_mul(updates.len())
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(bad)?;
        let old_text = before
            .keys()
            .try_fold(0usize, |n, key| n.checked_add(key.len()).ok_or_else(bad))?;
        let new_text = updates.iter().try_fold(0usize, |n, (key, _)| {
            n.checked_add(key.len()).ok_or_else(bad)
        })?;
        let bytes = old_text
            .checked_mul(updates.len())
            .and_then(|n| {
                new_text
                    .checked_mul(before.len())
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(bad)?;
        common.charge(pairs as u64, bytes as u64).map_err(formal)?;
        Some(values::json(
            &EvaluationOverlay {
                before,
                updates: &updates,
            },
            work.expiry.max_selected_heap,
            common,
        )?)
    };
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<CurrentCognitionSelectionPlanV2>() + 2048,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?
        .retain(());
    Ok(CurrentCognitionSelectionPlanV2 {
        state,
        inputs,
        cut,
        commitment,
        selected: PaidSelectedCognitionWakesV2 {
            wakes: selected,
            _fee: policy_fee,
        },
        stale,
        scheduler_after,
        evaluations_after,
        _evaluation_fee: eval_fee,
        _fee: fee,
    })
}
