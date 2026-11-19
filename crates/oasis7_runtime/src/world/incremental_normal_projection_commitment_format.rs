//! Fixed actual-source formatter consumers. Closed uses the complete original
//! historical reducer and permanent terminal validation, with paid source owners.
pub(super) use super::facts::task_reduce::prepare_current_closed_projection_data_v1 as current_closed_projection_data_v1;
use super::*;

use super::{
    original_rows::OriginalProjectionRowsDataV1, state_columns::OriginalStateColumnsDataV1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::observation_projection::{
    WorldServiceProjection,
    current_commitment::{ProjectionColumnDataV1, ProjectionColumnErrorV1},
};
use std::mem::size_of;
#[path = "incremental_normal_projection_commitment_format_events.rs"]
mod events;
struct PrivateFees {
    _owner: Option<super::facts::CurrentProjectionFactsFeesV1>,
    _feedback: Option<super::facts::CurrentFeedbackHistoryFeesV1>,
    _native: Option<super::facts::CurrentProjectionFactsFeesV1>,
    _key: Option<super::facts::PaidCurrentOwnerKeyMatchDataV1>,
    _task: Option<super::facts::CurrentProjectionFactsFeesV1>,
    _cognition: Option<super::facts::CognitionProjectionFeesV1>,
}
pub(super) struct PaidCurrentProjectionFormatDataV1 {
    projection: WorldServiceProjection<ProjectionColumnDataV1>,
    _private: PrivateFees,
    _status: super::facts::CurrentWorldRuleStatusFeesV1,
    _rules: super::facts::visible_rules::PaidCurrentRuleVisibleDataV1,
    _query: super::facts::query::CurrentGameplayQueryFeesV1,
    _events: events::CurrentEventsFeesV1,
    _governance: MemoryReservation,
    _strings: Vec<MemoryReservation>,
    _fee: MemoryReservation,
}
impl PaidCurrentProjectionFormatDataV1 {
    pub(super) fn projection(&self) -> &WorldServiceProjection<ProjectionColumnDataV1> {
        &self.projection
    }
}
fn column(e: ProjectionColumnErrorV1<BodyMutationError>) -> BodyMutationError {
    match e {
        ProjectionColumnErrorV1::Work(e) => e,
        ProjectionColumnErrorV1::Memory(e) => ValueTreeError::Memory(e).into(),
        ProjectionColumnErrorV1::Invalid(note) => invalid(note),
    }
}
fn copy_text(
    value: &str,
    fees: &mut Vec<MemoryReservation>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<String, BodyMutationError> {
    let n = size_of::<String>()
        .checked_add(value.len())
        .ok_or_else(|| invalid("formatter binding text layout"))?;
    common.charge(1, n as u64).map_err(paid)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(value.len());
    if bytes.capacity() != value.len() {
        return Err(invalid("formatter binding text capacity"));
    }
    for chunk in value.as_bytes().chunks(8192) {
        common.charge(1, chunk.len() as u64).map_err(paid)?;
        bytes.extend_from_slice(chunk)
    }
    if fees.len() == fees.capacity() {
        return Err(invalid("formatter fixed binding fee slots"));
    }
    fees.push(fee);
    String::from_utf8(bytes).map_err(paid)
}
fn copy_binding(
    value: &oasis7_agent_api::RuntimeBindingV1,
    fees: &mut Vec<MemoryReservation>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<oasis7_agent_api::RuntimeBindingV1, BodyMutationError> {
    Ok(oasis7_agent_api::RuntimeBindingV1 {
        world_id: copy_text(&value.world_id, fees, common)?,
        branch_id: copy_text(&value.branch_id, fees, common)?,
        finality_epoch: value.finality_epoch,
        finality_block_hash: value
            .finality_block_hash
            .as_ref()
            .map(|v| copy_text(&v.0, fees, common).map(oasis7_agent_api::Digest32))
            .transpose()?,
        finality_status: copy_text(&value.finality_status, fees, common)?,
        base_tick: value.base_tick,
        base_world_hash: oasis7_agent_api::Digest32(copy_text(
            &value.base_world_hash.0,
            fees,
            common,
        )?),
        reorg_epoch: value.reorg_epoch,
        runtime_manifest_hash: oasis7_agent_api::Digest32(copy_text(
            &value.runtime_manifest_hash.0,
            fees,
            common,
        )?),
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) fn format_projection_data_v1<
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    agent_intents: &OriginalProjectionRowsDataV1,
    observation: &ProjectionColumnDataV1,
    aux: &ProjectionColumnDataV1,
    template: &super::template::StaticProjectionSchemaDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    raw_state: &super::raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    reader: Option<&str>,
    reader_key: &str,
    retained: &mut R,
    canonical: &mut CanonicalCborWorkV1,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidCurrentProjectionFormatDataV1, BodyMutationError> {
    origin.validate_metadata(metadata, common)?;
    raw_state.validate_context(common)?;
    state.validate_context(common)?;
    let expected = metadata.runtime_binding_for_state_hash_bounded_v1(
        raw_state.as_str(),
        common,
        canonical,
    )?;
    if let Some(binding) = binding {
        if !binding.uses_budget(common.memory()) {
            return Err(invalid("formatter binding pool"));
        }
    }
    common.consume_steps(1).map_err(paid)?;
    match (expected.as_ref(), binding) {
        (Some(expected), Some(binding)) => {
            for value in [&**expected, &**binding] {
                for text in [
                    &value.world_id,
                    &value.branch_id,
                    &value.finality_status,
                    &value.base_world_hash.0,
                    &value.runtime_manifest_hash.0,
                ] {
                    for chunk in text.as_bytes().chunks(8192) {
                        common.charge(1, chunk.len() as u64).map_err(paid)?
                    }
                }
                if let Some(hash) = &value.finality_block_hash {
                    for chunk in hash.0.as_bytes().chunks(8192) {
                        common.charge(1, chunk.len() as u64).map_err(paid)?
                    }
                }
                common.consume_steps(9).map_err(paid)?;
            }
            if **expected != **binding {
                return Err(invalid("formatter runtime binding source mismatch"));
            }
        }
        (None, None) => {}
        _ => return Err(invalid("formatter runtime binding source mismatch")),
    }
    let n = size_of::<PaidCurrentProjectionFormatDataV1>() + 16 * size_of::<MemoryReservation>();
    common.charge(1, n as u64).map_err(paid)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut strings = Vec::with_capacity(16);
    if strings.capacity() != 16 {
        return Err(invalid("formatter fixed fee capacity"));
    }
    let rules = super::facts::visible_rules::prepare_current_rule_visible_data_v1(
        state,
        electricity,
        metadata,
        observation,
        aux,
        reader,
        origin,
        common,
    )?;
    let mut visible = super::visible_state::compose_non_rule_visible_state_data_v1(
        observation,
        template.data(),
        reader,
        common,
    )?;
    let memory = common.memory().clone();
    visible = visible
        .with_object_member_data("world_rules", rules.value(), &memory, &mut |s, b| {
            common.charge(s, b).map_err(paid)
        })
        .map_err(column)?;
    let status = super::facts::prepare_current_world_rule_status_data_v1(
        state,
        electricity,
        aux,
        metadata,
        raw_state,
        reader,
        origin,
        common,
        canonical,
    )?;
    let (status, status_fees) = status.into_parts_v1();
    let mut private = PrivateFees {
        _owner: None,
        _feedback: None,
        _native: None,
        _key: None,
        _task: None,
        _cognition: None,
    };
    let mut owner = None;
    let mut chat = None;
    let mut feedback = None;
    if let Some(agent) = reader {
        let source = super::facts::prepare_current_owner_chat_projection_data_v1(
            state,
            electricity,
            agent_intents,
            aux,
            metadata,
            raw_state,
            binding,
            agent,
            retained,
            origin,
            common,
            canonical,
        )?;
        let (fence, view, fees) = source.into_parts_v1();
        owner = fence;
        chat = view;
        private._owner = Some(fees);
        let source = super::facts::prepare_current_feedback_history_data_v1(
            state,
            electricity,
            aux,
            metadata,
            raw_state,
            agent,
            origin,
            common,
            canonical,
        )?;
        let (history, fees) = source.into_parts_v1();
        feedback = Some(history);
        private._feedback = Some(fees);
    }
    let runtime_binding = binding
        .map(|value| copy_binding(value, &mut strings, common))
        .transpose()?;
    let mut projection =
        oasis7_world_state::observation_projection::original_formatter::base_data_v1(
            visible,
            status,
            owner,
            feedback,
            runtime_binding,
        );
    let mut execution_actor = None;
    if let Some(agent) = reader {
        let source = super::facts::prepare_current_native_authorizations_data_v1(
            state,
            electricity,
            aux,
            metadata,
            agent,
            origin,
            common,
        )?;
        let (native, fees) = source.into_parts_v1();
        projection.native_delegation_authorizations = native;
        private._native = Some(fees);
        let key = super::facts::prepare_current_owner_key_match_data_v1(
            state,
            electricity,
            aux,
            metadata,
            agent,
            reader_key,
            origin,
            common,
        )?;
        if key.matches_data_v1() {
            projection.canonical_agent_chat = chat;
            execution_actor = Some(agent)
        }
        private._key = Some(key);
        let task = super::facts::task_reduce::prepare_current_task_control_data_v1(
            state,
            electricity,
            aux,
            metadata,
            binding.map(|v| &**v),
            agent,
            reader_key,
            retained,
            canonical,
            origin,
            common,
        )?;
        let (control, fees) = task.into_parts_v1();
        projection.game_task_control = control;
        private._task = Some(fees);
    }
    let query = super::facts::query::prepare_current_gameplay_query_data_v1(
        state,
        electricity,
        aux,
        metadata,
        raw_state,
        binding,
        reader,
        execution_actor,
        origin,
        common,
        canonical,
    )?;
    let (query, query_fees) = query.into_parts_v1();
    oasis7_world_state::observation_projection::original_formatter::attach_gameplay_query_data_v1(
        &mut projection,
        query,
    );
    if let Some(agent) = reader {
        let cognition = super::facts::prepare_current_cognition_projection_data_v1(
            state,
            electricity,
            aux,
            metadata,
            raw_state,
            binding,
            agent,
            origin,
            common,
            canonical,
        )?;
        private._cognition = Some(cognition.apply_to_projection_v1(&mut projection));
    }
    let (governance, governance_fee) = metadata
        .governance_membership_parts_for_state_hash_bounded_v1(raw_state.as_str(), common)?;
    let (events, event_fees) = events::events_data_v1(aux, metadata, reader, common)?;
    let header = oasis7_client_api::world_service::FormalProjectionCommitHeaderV1 {
        authority_generation: metadata.authority_generation(),
        permission_generation: metadata.permission_generation(),
        event_id_era: metadata.event_id_era(),
        journal_retention_floor: metadata.journal_retention_floor(),
        journal_high_watermark: metadata.journal_high_watermark(),
        governance_membership: Some(governance),
    };
    oasis7_world_state::observation_projection::original_formatter::attach_formal_header_data_v1(
        &mut projection,
        header,
        events,
    );
    origin.validate_metadata(metadata, common)?;
    Ok(PaidCurrentProjectionFormatDataV1 {
        projection,
        _private: private,
        _status: status_fees,
        _rules: rules,
        _query: query_fees,
        _events: event_fees,
        _governance: governance_fee,
        _strings: strings,
        _fee: fee,
    })
}
