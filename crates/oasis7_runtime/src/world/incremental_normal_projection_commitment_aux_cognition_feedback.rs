//! Complete fixed Some(Agent) feedback observation over the actual auxiliary
//! graph. Receipt correlation remains DATA, never input/commit qualification.
use super::*;
use crate::world::cognition_persistence::receipt_observation::{self, ReceiptReadErrorV1};
use oasis7_client_api::world_service_feedback::{
    FeedbackHistoryWorkV1, WorldServiceFeedbackHistory, WorldServiceFeedbackReplayRecord,
};
use oasis7_world_state::cognition_feedback_contract::readonly_work::FeedbackReadErrorV1;
#[path = "incremental_normal_projection_commitment_aux_cognition_feedback_work.rs"]
mod work;
pub(in crate::world) struct CurrentFeedbackHistoryFeesV1 {
    _facts: CurrentProjectionFactsFeesV1,
    _outbox: Option<MemoryReservation>,
}
pub(in crate::world) struct PaidCurrentFeedbackHistoryDataV1 {
    history: WorldServiceFeedbackHistory,
    fees: CurrentFeedbackHistoryFeesV1,
}
impl PaidCurrentFeedbackHistoryDataV1 {
    pub(in crate::world) fn history(&self) -> &WorldServiceFeedbackHistory {
        &self.history
    }
    pub(in crate::world) fn into_parts_v1(
        self,
    ) -> (WorldServiceFeedbackHistory, CurrentFeedbackHistoryFeesV1) {
        (self.history, self.fees)
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_feedback_history_data_v1(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    aux: &ProjectionColumnDataV1,
    metadata: &OriginalProjectionReadMetadataV1<'_>,
    raw_state: &raw_hash::PaidOriginalStateHashDataV1,
    agent: &str,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<PaidCurrentFeedbackHistoryDataV1, BodyMutationError> {
    raw_state.validate_context(common)?;
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?;
    facts.before_owned(size_of::<PaidCurrentFeedbackHistoryDataV1>())?;
    let view = values::View::new(&facts, aux)?;
    let source = work::Source::new(&view, canonical);
    let field = match view.optional(&["cognition", "feedback_outbox"])? {
        None => None,
        Some(row) => Some(match view.decode::<serde_json::Value>(&row)? {
            Ok(v) => v,
            Err(error) => return Err(view.business(format_args!("{error}"))),
        }),
    };
    let outbox = source.receipt(
        crate::world::cognition_feedback::observation::parse_outbox_field_with_work_v1(
            field.as_ref(),
            &source,
        ),
    )?;
    let (records, outbox_fee) = outbox.into_parts_v1();
    let mut output = Vec::new();
    let mut snapshot = None;
    for record in records.into_values() {
        facts.work(1, record.agent_subject.len() + agent.len())?;
        if record.agent_subject != agent {
            continue;
        }
        // The original formatter validates again after the complete outbox census.
        let result = record.validate_with_work_v1(&source);
        source.check()?;
        match result {
            Ok(()) => {}
            Err(FeedbackReadErrorV1::Business(error)) => {
                return Err(view.business(format_args!("{error}")));
            }
            Err(FeedbackReadErrorV1::Work(error)) => return Err(paid(error)),
        }
        view.before_clone(&record.payload)?;
        let feedback: oasis7_agent_api::FeedbackEnvelopeV1 = serde_json::from_value(record.payload)
            .map_err(|error| view.business(format_args!("{error}")))?;
        let lineage = if feedback.status == "committed" {
            let receipt_id = feedback
                .runtime_receipt_id
                .as_deref()
                .ok_or_else(|| view.business(format_args!("feedback receipt missing")))?;
            if snapshot.is_none() {
                let row = view.required(&["cognition"])?;
                let value = match view.decode::<serde_json::Value>(&row)? {
                    Ok(v) => v,
                    Err(error) => return Err(view.business(format_args!("{error}"))),
                };
                snapshot = Some(source.receipt(
                    receipt_observation::parse_receipt_observation_with_work_v1(value, &source),
                )?);
            }
            let selected =
                source.receipt(receipt_observation::read_receipt_observation_with_work_v1(
                    snapshot
                        .as_ref()
                        .ok_or_else(|| invalid("actual receipt observation snapshot"))?,
                    receipt_id,
                    &source,
                ))?;
            view.before_clone(selected.value())?;
            Some(selected.value().agent_api_projection())
        } else {
            None
        };
        values::push(
            &facts,
            &mut output,
            WorldServiceFeedbackReplayRecord {
                feedback,
                original_envelope_digest: record.envelope_digest,
                delivery_state: record.state,
                receipt_lineage: lineage,
            },
        )?;
    }
    let history = WorldServiceFeedbackHistory {
        agent_id: view.copy(agent)?,
        records: output,
    };
    let metadata = history.metadata_with_work_v1(agent, &mut |item| match item {
        FeedbackHistoryWorkV1::Feedback(v) => view.before_clone(v),
        FeedbackHistoryWorkV1::Lineage(v) => view.before_clone(v),
        FeedbackHistoryWorkV1::Text(v) => {
            facts.work(1, v.len())?;
            facts.before_owned(size_of::<String>() + v.len())
        }
        FeedbackHistoryWorkV1::Slots(n) => {
            let bytes = n
                .checked_mul(size_of::<(
                    oasis7_agent_api::FeedbackEnvelopeV1,
                    String,
                    String,
                )>())
                .ok_or_else(|| invalid("feedback metadata output slots"))?;
            facts.work(1, bytes)?;
            facts.before_owned(bytes)
        }
        FeedbackHistoryWorkV1::ActionId(_) => {
            facts.work(32, 32)?;
            facts.before_owned(64)
        }
    })?;
    if let Err(note) = metadata {
        return Err(view.business(format_args!("{note}")));
    }
    source.check()?;
    facts.check()?;
    drop(snapshot);
    drop(source);
    drop(view);
    Ok(PaidCurrentFeedbackHistoryDataV1 {
        history,
        fees: CurrentFeedbackHistoryFeesV1 {
            _facts: facts.into_fees(),
            _outbox: outbox_fee,
        },
    })
}
