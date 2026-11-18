//! Complete original public + every current owner/delegate leaf. Formatter
//! and terminal-task branches are fixed Current consumers, never callbacks.
use super::*;
use oasis7_client_api::formal_projection::ProjectionAudienceV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1;
use std::mem::size_of;
#[path = "incremental_genesis_projection_commitment.rs"]
pub(in crate::world) mod genesis;
struct Leaves {
    values: Vec<(ProjectionAudienceV1, String)>,
    fees: Vec<MemoryReservation>,
    slots: MemoryReservation,
    fee_slots: MemoryReservation,
    _fee: MemoryReservation,
}
fn pay(
    common: &mut FormalBoundedContextV1<'_>,
    n: usize,
) -> Result<MemoryReservation, BodyMutationError> {
    common.charge(1, n as u64).map_err(paid)?;
    common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}
fn grow<T>(
    values: &mut Vec<T>,
    slots: &mut MemoryReservation,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    if values.len() < values.capacity() {
        return Ok(());
    }
    let capacity = values
        .capacity()
        .checked_mul(2)
        .unwrap_or(usize::MAX)
        .max(4);
    let n = capacity
        .checked_mul(size_of::<T>())
        .ok_or_else(|| invalid("projection leaf slots layout"))?;
    let fee = pay(common, n)?;
    values
        .try_reserve_exact(capacity - values.len())
        .map_err(|_| invalid("projection leaf slots allocation"))?;
    if values.capacity() != capacity {
        return Err(invalid("projection leaf slots capacity"));
    }
    *slots = fee;
    Ok(())
}
impl Leaves {
    fn new(common: &mut FormalBoundedContextV1<'_>) -> Result<Self, BodyMutationError> {
        let fee = pay(common, size_of::<Self>())?;
        let slots = pay(common, 0)?;
        let fee_slots = pay(common, 0)?;
        Ok(Self {
            values: Vec::new(),
            fees: Vec::new(),
            slots,
            fee_slots,
            _fee: fee,
        })
    }
    fn keep(
        &mut self,
        fee: MemoryReservation,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        grow(&mut self.fees, &mut self.fee_slots, common)?;
        self.fees.push(fee);
        Ok(())
    }
    fn push(
        &mut self,
        audience: ProjectionAudienceV1,
        hash: &ReservedValue<String>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        if !hash.uses_budget(common.memory()) {
            return Err(invalid("projection leaf digest pool"));
        }
        let fee = pay(common, size_of::<String>() + hash.len())?;
        let text = hash.as_str().to_owned();
        common.consume_steps(0).map_err(paid)?;
        self.keep(fee, common)?;
        grow(&mut self.values, &mut self.slots, common)?;
        self.values.push((audience, text));
        Ok(())
    }
}
pub(super) fn compute_current_projection_root_v1<
    M: MerkleNodeReader,
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    state: &super::state_columns::OriginalStateColumnsDataV1,
    electricity: &super::original_rows::OriginalProjectionRowsDataV1,
    agent_intents: &super::original_rows::OriginalProjectionRowsDataV1,
    observation: &ProjectionColumnDataV1,
    aux: &ProjectionColumnDataV1,
    template: &super::template::StaticProjectionSchemaDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    process: &crate::world::incremental_normal_process_source::PaidProjectionProcessFactsV1,
    authorization: &crate::world::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
    finality: &crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1,
    raw_state: &super::raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    tx: &ExecutionTransaction<'_, M>,
    retained: &mut R,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
    live: &mut dyn FnMut() -> ExecutionFence,
) -> Result<ReservedValue<String>, BodyMutationError> {
    let source = crate::world::incremental_normal_projection_commitment::facts::CurrentProjectionReadSourceV1::new(tx, live).with_security(process, authorization, finality);
    compute_projection_root_data_v1(
        state,
        electricity,
        agent_intents,
        observation,
        aux,
        template,
        metadata,
        raw_state,
        binding,
        &source,
        retained,
        common,
        canonical,
    )
}

pub(super) fn compute_projection_root_data_v1<
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    state: &super::state_columns::OriginalStateColumnsDataV1,
    electricity: &super::original_rows::OriginalProjectionRowsDataV1,
    agent_intents: &super::original_rows::OriginalProjectionRowsDataV1,
    observation: &ProjectionColumnDataV1,
    aux: &ProjectionColumnDataV1,
    template: &super::template::StaticProjectionSchemaDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    raw_state: &super::raw_hash::PaidOriginalStateHashDataV1,
    binding: Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    retained: &mut R,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<ReservedValue<String>, BodyMutationError> {
    origin.validate_metadata(metadata, common)?;
    raw_state.validate_context(common)?;
    let mut leaves = Leaves::new(common)?;
    // The original public formatter/hash happens before audience discovery.
    let public_fee = pay(common, size_of::<ProjectionAudienceV1>() + "public".len())?;
    let public = ProjectionAudienceV1::public();
    leaves.keep(public_fee, common)?;
    let projection = super::format::format_projection_data_v1(
        state,
        electricity,
        agent_intents,
        observation,
        aux,
        template,
        metadata,
        raw_state,
        binding,
        None,
        "",
        retained,
        canonical,
        origin,
        common,
    )?;
    let hash = super::leaf_hash::leaf_data_v1(&public, projection.projection(), common, canonical)?;
    leaves.push(public, &hash, common)?;
    drop(hash);
    drop(projection);
    let mut audiences = super::audiences::collect_actual_projection_audiences_v1(
        state,
        electricity,
        aux,
        metadata,
        binding,
        origin,
        common,
    )?;
    // The actual audience carrier and all its B-tree/String fees remain alive
    // until the consuming original tree kernel has released its moved inputs.
    for audience in std::mem::take(&mut audiences.values) {
        let agent = audience
            .scope_id
            .strip_prefix("agent:")
            .ok_or_else(|| invalid("original private audience scope"))?;
        let reader_key = audience
            .reader_public_key
            .as_deref()
            .ok_or_else(|| invalid("original private audience key"))?;
        let hash = if let Some(closed) = super::format::current_closed_projection_data_v1(
            state,
            electricity,
            aux,
            metadata,
            raw_state,
            binding,
            agent,
            retained,
            canonical,
            origin,
            common,
        )? {
            super::leaf_hash::leaf_data_v1(&audience, closed.projection(), common, canonical)?
        } else {
            let projection = super::format::format_projection_data_v1(
                state,
                electricity,
                agent_intents,
                observation,
                aux,
                template,
                metadata,
                raw_state,
                binding,
                Some(agent),
                reader_key,
                retained,
                canonical,
                origin,
                common,
            )?;
            super::leaf_hash::leaf_data_v1(&audience, projection.projection(), common, canonical)?
        };
        leaves.push(audience, &hash, common)?;
    }
    let output =
        super::leaf_hash::tree_root_data_v1(std::mem::take(&mut leaves.values), common, canonical)?;
    origin.validate_metadata(metadata, common)?;
    Ok(output)
}
