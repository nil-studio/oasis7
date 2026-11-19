//! Selected pending Recipe DTO recovery for the fixed due loop. The callback
//! is private to this module's actual Industry-Q/Current adapter; the result is
//! paid data, not a completion/receipt capability.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::incremental_industry_jobs::{
    IndustryCellReadV1 as Read, IndustryCellV1 as Cell, IndustryCollectionV1 as List,
    IndustryColumnV1 as Col, IndustryStackColumnV1 as StackCol, IndustryValueV1 as Value,
};
use std::{io::Write, mem::size_of};
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("due pending recipe Current record")
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due pending recipe layout").into()
}
struct Decoder<'a, F> {
    read: F,
    max_text: usize,
    max_entries: usize,
    marker: std::marker::PhantomData<&'a ()>,
}
impl<F> Decoder<'_, F>
where
    F: for<'cell> FnMut(Cell<'cell>, &mut dyn Write) -> Result<Read, BodyMutationError>,
{
    fn value(&mut self, cell: Cell<'_>) -> Result<Value, BodyMutationError> {
        match (self.read)(cell, &mut std::io::sink())? {
            Read::Value(value) => Ok(value),
            _ => Err(invalid()),
        }
    }
    fn number(&mut self, column: Col) -> Result<u64, BodyMutationError> {
        match self.value(Cell::Column(column))? {
            Value::Unsigned(n) => Ok(n),
            _ => Err(invalid()),
        }
    }
    fn signed(&mut self, column: Col) -> Result<i64, BodyMutationError> {
        match self.value(Cell::Column(column))? {
            Value::Signed(n) => Ok(n),
            _ => Err(invalid()),
        }
    }
    fn flag(&mut self, column: Col) -> Result<bool, BodyMutationError> {
        match self.value(Cell::Column(column))? {
            Value::Bool(n) => Ok(n),
            _ => Err(invalid()),
        }
    }
    fn optional_text_cell(&mut self, cell: Cell<'_>) -> Result<Option<String>, BodyMutationError> {
        let mut bytes = Vec::with_capacity(self.max_text);
        if bytes.capacity() != self.max_text {
            return Err(limited());
        }
        match (self.read)(cell, &mut bytes)? {
            Read::Value(Value::Unit) if bytes.is_empty() => Ok(None),
            Read::Value(Value::Text { utf8_bytes })
                if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
            {
                Ok(Some(String::from_utf8(bytes).map_err(|_| invalid())?))
            }
            _ => Err(invalid()),
        }
    }
    fn text(&mut self, column: Col) -> Result<String, BodyMutationError> {
        self.optional_text_cell(Cell::Column(column))?
            .ok_or_else(invalid)
    }
    fn optional_text(&mut self, column: Col) -> Result<Option<String>, BodyMutationError> {
        self.optional_text_cell(Cell::Column(column))
    }
    fn length(&mut self, list: List) -> Result<usize, BodyMutationError> {
        match self.value(Cell::Collection(list))? {
            Value::Unsigned(n) => {
                let n = usize::try_from(n).map_err(|_| limited())?;
                if n > self.max_entries {
                    return Err(limited());
                }
                Ok(n)
            }
            _ => Err(invalid()),
        }
    }
    fn texts(&mut self, list: List) -> Result<Vec<String>, BodyMutationError> {
        let n = self.length(list)?;
        let mut result = Vec::with_capacity(n);
        if result.capacity() != n {
            return Err(limited());
        }
        for index in 0..n {
            result.push(
                self.optional_text_cell(Cell::TextItem(list, index as u64))?
                    .ok_or_else(invalid)?,
            );
        }
        Ok(result)
    }
    fn stacks(
        &mut self,
        list: List,
    ) -> Result<Vec<oasis7_world_state::MaterialStack>, BodyMutationError> {
        let n = self.length(list)?;
        let mut result = Vec::with_capacity(n);
        if result.capacity() != n {
            return Err(limited());
        }
        for index in 0..n {
            let kind = self
                .optional_text_cell(Cell::StackItem(list, index as u64, StackCol::Kind))?
                .ok_or_else(invalid)?;
            let amount = match self.value(Cell::StackItem(list, index as u64, StackCol::Amount))? {
                Value::Signed(n) => n,
                _ => return Err(invalid()),
            };
            result.push(oasis7_world_state::MaterialStack { kind, amount });
        }
        Ok(result)
    }
    fn ledger(
        &mut self,
        column: Col,
    ) -> Result<oasis7_world_state::MaterialLedgerId, BodyMutationError> {
        // The original closed typed reader emits its canonical ledger frame. Reuse
        // the original decoder; do not invent colon/text parsing or normalize IDs.
        let mut bytes = Vec::with_capacity(self.max_text);
        if bytes.capacity() != self.max_text {
            return Err(limited());
        }
        match (self.read)(Cell::Column(column), &mut bytes)? {
            Read::Value(Value::Ledger(_)) => {}
            _ => return Err(invalid()),
        };
        use oasis7_world_state::incremental_ledger_value::{
            LedgerValueRefV1, read_ledger_value_v1,
        };
        Ok(
            match read_ledger_value_v1(&bytes).map_err(BodyMutationError::Invalid)? {
                LedgerValueRefV1::World => oasis7_world_state::MaterialLedgerId::World,
                LedgerValueRefV1::Agent(id) => {
                    oasis7_world_state::MaterialLedgerId::Agent(id.to_owned())
                }
                LedgerValueRefV1::Site(id) => {
                    oasis7_world_state::MaterialLedgerId::Site(id.to_owned())
                }
                LedgerValueRefV1::Factory(id) => {
                    oasis7_world_state::MaterialLedgerId::Factory(id.to_owned())
                }
            },
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn decode_pending_recipe_v1<F>(
    outer: u64,
    memory: &MemoryBudget,
    max_text: usize,
    max_entries: usize,
    read: F,
) -> Result<ReservedValue<crate::RecipeJobState>, BodyMutationError>
where
    F: for<'cell> FnMut(Cell<'cell>, &mut dyn Write) -> Result<Read, BodyMutationError>,
{
    let fee_bytes = recipe_decode_layout_v1(max_text, max_entries)?;
    let fee = memory
        .reserve(fee_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut d = Decoder {
        read,
        max_text,
        max_entries,
        marker: std::marker::PhantomData,
    };
    let industrial_root_ref = if d.flag(Col::RecordIndustrialRootRef)? {
        Some(oasis7_agent_api::world_action::IndustrialRefV1 {
            world_binding: d.text(Col::IndustrialRootWorldBinding)?,
            domain: d.text(Col::IndustrialRootDomain)?,
            schema: d.text(Col::IndustrialRootSchema)?,
            id: d.text(Col::IndustrialRootId)?,
            revision: d.number(Col::IndustrialRootRevision)?,
            digest: d.text(Col::IndustrialRootDigest)?,
        })
    } else {
        None
    };
    let emergency_power_world_binding = d.optional_text(Col::RecordEmergencyPowerWorldBinding)?;
    let committed_recipe_origin = if d.flag(Col::RecordCommittedRecipeOrigin)? {
        Some(crate::CommittedRecipeOrigin {
            submission: oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin {
                verified_player_id: d.text(Col::SubmissionVerifiedPlayerId)?,
                public_key: d.text(Col::SubmissionPublicKey)?,
                auth_nonce: d.number(Col::SubmissionAuthNonce)?,
                hosted_registration_nonce: d
                    .optional_text(Col::SubmissionHostedRegistrationNonce)?,
                requester_agent_id: d.text(Col::SubmissionRequesterAgentId)?,
                factory_id: d.text(Col::SubmissionFactoryId)?,
                recipe_id: d.text(Col::SubmissionRecipeId)?,
            },
            consensus_action_id: d.number(Col::OriginConsensusActionId)?,
            consensus_submitter_player_id: d.text(Col::OriginConsensusSubmitterPlayerId)?,
            action_payload_hash: d.text(Col::OriginActionPayloadHash)?,
            committed_height: d.number(Col::OriginCommittedHeight)?,
            action_root: d.text(Col::OriginActionRoot)?,
        })
    } else {
        None
    };
    let job_id = d.number(Col::RecordJobId)?;
    if job_id != outer {
        return Err(invalid());
    }
    let value = crate::RecipeJobState {
        industrial_root_ref,
        emergency_power_world_binding,
        committed_recipe_origin,
        job_id,
        requester_agent_id: d.text(Col::RecordRequesterAgentId)?,
        factory_id: d.text(Col::RecordFactoryId)?,
        recipe_id: d.text(Col::RecordRecipeId)?,
        accepted_batches: u32::try_from(d.number(Col::RecordAcceptedBatches)?)
            .map_err(|_| invalid())?,
        consume: d.stacks(List::Consume)?,
        produce: d.stacks(List::Produce)?,
        byproducts: d.stacks(List::Byproducts)?,
        power_required: d.signed(Col::RecordPowerRequired)?,
        power_owner_agent_id: d.optional_text(Col::RecordPowerOwnerAgentId)?,
        duration_ticks: u32::try_from(d.number(Col::RecordDurationTicks)?)
            .map_err(|_| invalid())?,
        consume_ledger: d.ledger(Col::RecordConsumeLedger)?,
        output_ledger: d.ledger(Col::RecordOutputLedger)?,
        bottleneck_tags: d.texts(List::BottleneckTags)?,
        logistics_route_ids: d.texts(List::LogisticsRouteIds)?,
        logistics_path_ids: d.texts(List::LogisticsPathIds)?,
        ready_at: d.number(Col::RecordReadyAt)?,
    };
    Ok(fee.retain(value))
}

pub(super) fn recipe_decode_layout_v1(
    max_text: usize,
    max_entries: usize,
) -> Result<usize, BodyMutationError> {
    // 21 maximum owned flat Strings: industrial ref5, origin9, emergency1,
    // requester/factory/recipe3, payer1, two ledger identifiers. One extra text
    // buffer covers the concurrently decoded ledger frame. The six original
    // lists each hold at most caller.max_entries, with no history truncation.
    let list_slots = size_of::<oasis7_world_state::MaterialStack>()
        .checked_mul(3)
        .and_then(|n| n.checked_add(3 * size_of::<String>()))
        .ok_or_else(limited)?;
    let text_slots = max_entries
        .checked_mul(6)
        .and_then(|n| n.checked_add(22))
        .ok_or_else(limited)?;
    let fee_bytes = text_slots
        .checked_mul(max_text)
        .and_then(|n| n.checked_add(max_entries.checked_mul(list_slots)?))
        .and_then(|n| {
            n.checked_add(
                size_of::<crate::RecipeJobState>()
                    + size_of::<ReservedValue<crate::RecipeJobState>>(),
            )
        })
        .ok_or_else(limited)?;
    Ok(fee_bytes)
}
