//! Actual qualified Current agent and complete ContractAccept record reads.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn agent<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    id: &str,
    sources: &ContractAcceptSourcesV1<'_>,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut ContractAcceptWorkV1,
    live: &ExecutionFence,
) -> Result<bool, BodyMutationError> {
    work.spend(id.len())?;
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: step.mutation.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: || (read.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = ctx.transaction.guard();
    Ok(read_current_agent_presence_v1(
        id,
        sources.agents,
        roots,
        &mut ctx,
        parent_read,
        &mut work.pending_entries_remaining,
        &guard,
        live,
    )
    .map_err(overlay_error)?
        == CurrentAgentPresenceV1::Existing)
}

/// Decode every fixed column from the authenticated child, preserving embedded ID.
pub(in crate::world) fn contract_with_work<P, V, E, C>(
    id: &str,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut ContractAcceptWorkV1,
    before_column: &mut impl FnMut(usize) -> Result<(), BodyMutationError>,
) -> Result<
    ReservedValue<oasis7_world_state::gameplay_state::EconomicContractState>,
    BodyMutationError,
>
where
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let cap = usize::try_from(work.max_text_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("contract_accept text bound"))?;
    let bytes = cap
        .checked_mul(7)
        .and_then(|n| {
            n.checked_add(std::mem::size_of::<
                oasis7_world_state::gameplay_state::EconomicContractState,
            >())
        })
        .ok_or(ValueTreeError::ResourceLimited(
            "contract_accept record layout",
        ))?;
    let charge = read
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut ctx = WorldFieldReadContextV1 {
        root: proof.root(),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    let mut get = |column| -> Result<(Option<RecordValueV1>, String), BodyMutationError> {
        before_column(id.len())?;
        work.spend(id.len())?;
        let mut output = Vec::with_capacity(cap);
        let v = read_qualified_diplomacy_scalar_v1(
            id,
            DiplomacyFieldV1::EconomicContracts,
            DiplomacyScalarV1::Contract(column),
            proof,
            &mut ctx,
            work.max_text_bytes,
            &mut output,
        )?;
        let text = String::from_utf8(output)
            .map_err(|_| BodyMutationError::Invalid("contract_accept scalar utf8"))?;
        Ok((v, text))
    };
    fn uint(v: Option<RecordValueV1>) -> Result<u64, BodyMutationError> {
        match v {
            Some(RecordValueV1::Unsigned(v)) => Ok(v),
            _ => Err(BodyMutationError::Invalid(
                "contract_accept unsigned scalar",
            )),
        }
    }
    fn text(v: (Option<RecordValueV1>, String)) -> Result<String, BodyMutationError> {
        match v {
            (Some(RecordValueV1::Text { .. }), s) => Ok(s),
            _ => Err(BodyMutationError::Invalid("contract_accept text scalar")),
        }
    }
    fn optional(v: (Option<RecordValueV1>, String)) -> Result<Option<String>, BodyMutationError> {
        match v {
            (None, _) => Ok(None),
            (Some(RecordValueV1::Text { .. }), s) => Ok(Some(s)),
            _ => Err(BodyMutationError::Invalid(
                "contract_accept optional text scalar",
            )),
        }
    }

    fn signed(v: Option<RecordValueV1>) -> Result<i64, BodyMutationError> {
        match v {
            Some(RecordValueV1::Signed(v)) => Ok(v),
            _ => Err(BodyMutationError::Invalid("contract signed scalar")),
        }
    }
    fn opt_uint(v: Option<RecordValueV1>) -> Result<Option<u64>, BodyMutationError> {
        match v {
            None => Ok(None),
            v => Ok(Some(uint(v)?)),
        }
    }
    let contract_id = text(get(ContractScalarV1::ContractId)?)?;
    let creator_agent_id = text(get(ContractScalarV1::CreatorAgentId)?)?;
    let counterparty_agent_id = text(get(ContractScalarV1::CounterpartyAgentId)?)?;
    let fulfillment_kind = match text(get(ContractScalarV1::FulfillmentKind)?)?.as_str() {
        "atomic_exchange" => {
            oasis7_world_state::gameplay_state::EconomicContractFulfillmentKind::AtomicExchange
        }
        "service" => oasis7_world_state::gameplay_state::EconomicContractFulfillmentKind::Service,
        _ => return Err(BodyMutationError::Invalid("contract fulfillment kind")),
    };
    let settlement_kind = match text(get(ContractScalarV1::SettlementKind)?)?.as_str() {
        "electricity" => oasis7_agent_api::world_action::ResourceKind::Electricity,
        "data" => oasis7_agent_api::world_action::ResourceKind::Data,
        _ => return Err(BodyMutationError::Invalid("contract settlement kind")),
    };
    let settlement_amount = signed(get(ContractScalarV1::SettlementAmount)?.0)?;
    let reputation_stake = signed(get(ContractScalarV1::ReputationStake)?.0)?;
    let expires_at = uint(get(ContractScalarV1::ExpiresAt)?.0)?;
    let description = text(get(ContractScalarV1::Description)?)?;
    let status = match text(get(ContractScalarV1::Status)?)?.as_str() {
        "open" => oasis7_world_state::gameplay_state::EconomicContractStatus::Open,
        "accepted" => oasis7_world_state::gameplay_state::EconomicContractStatus::Accepted,
        "settled" => oasis7_world_state::gameplay_state::EconomicContractStatus::Settled,
        "expired" => oasis7_world_state::gameplay_state::EconomicContractStatus::Expired,
        _ => return Err(BodyMutationError::Invalid("contract status")),
    };
    let accepted_at = opt_uint(get(ContractScalarV1::AcceptedAt)?.0)?;
    let settled_at = opt_uint(get(ContractScalarV1::SettledAt)?.0)?;
    let settlement_success = match get(ContractScalarV1::SettlementSuccess)?.0 {
        None => None,
        Some(RecordValueV1::Bool(v)) => Some(v),
        _ => return Err(BodyMutationError::Invalid("contract settlement success")),
    };
    let transfer_amount = signed(get(ContractScalarV1::TransferAmount)?.0)?;
    let tax_amount = signed(get(ContractScalarV1::TaxAmount)?.0)?;
    let settlement_notes = optional(get(ContractScalarV1::SettlementNotes)?)?;
    Ok(
        charge.retain(oasis7_world_state::gameplay_state::EconomicContractState {
            contract_id,
            creator_agent_id,
            counterparty_agent_id,
            fulfillment_kind,
            settlement_kind,
            settlement_amount,
            reputation_stake,
            expires_at,
            description,
            status,
            accepted_at,
            settled_at,
            settlement_success,
            transfer_amount,
            tax_amount,
            settlement_notes,
        }),
    )
}

/// Legacy component wrapper preserves its original finite scalar work contract.
pub(super) fn contract<
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut ContractAcceptWorkV1,
) -> Result<ReservedValue<oasis7_world_state::EconomicContractState>, BodyMutationError> {
    contract_with_work(id, proof, read, work, &mut |_| Ok(()))
}
