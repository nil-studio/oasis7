//! Selected actual Current primitives. Every read uses the original qualified
//! codec, every write uses the actual expected cell, and every work counter is
//! owned by the enclosing same-Common adapter.
use super::super::incremental_normal_tail::NormalTailWorkV1;
use super::*;
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
    incremental_agent_subtree::*,
    incremental_diplomacy::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_value_mutation::ValueMutationContextV1,
    incremental_world_number_maps::*,
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Current gameplay selected primitive")
}
pub(in crate::world) fn text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    column: DiplomacyScalarV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<Option<ReservedValue<String>>, GameplayCycleErrorV1> {
    text_with_limit(id, field, column, proof, state, ctx, work.max_text_bytes, tail)
}
pub(in crate::world) fn text_with_limit<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    column: DiplomacyScalarV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_text_bytes: usize,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<Option<ReservedValue<String>>, GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let mut read = WorldFieldReadContextV1 {
        root: proof.root(),
        state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let Some(measured) = read_qualified_diplomacy_scalar_v1(
        id,
        field,
        column,
        proof,
        &mut read,
        max_text_bytes as u64,
        &mut std::io::sink(),
    )?
    else {
        return Ok(None);
    };
    let DiplomacyValueV1::Text { utf8_bytes: n } = measured else {
        return Err(invalid().into());
    };
    let n = usize::try_from(n).map_err(|_| invalid())?;
    if n > max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("gameplay selected text").into());
    }
    tail.charge(n)?;
    let fee = tail.reserve(
        n.checked_add(std::mem::size_of::<String>())
            .ok_or_else(invalid)?,
        MemoryClass::Ordinary,
    )?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(n)
        .map_err(|_| ValueTreeError::ResourceLimited("gameplay selected text"))?;
    if bytes.capacity() > n {
        return Err(invalid().into());
    }
    let actual = read_qualified_diplomacy_scalar_v1(
        id, field, column, proof, &mut read, n as u64, &mut bytes,
    )?;
    if actual
        != Some(DiplomacyValueV1::Text {
            utf8_bytes: n as u64,
        })
        || bytes.len() != n
    {
        return Err(invalid().into());
    }
    read.check()?;
    Ok(Some(
        fee.retain(String::from_utf8(bytes).map_err(|_| invalid())?),
    ))
}
pub(in crate::world) fn scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    column: DiplomacyScalarV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<Option<DiplomacyValueV1>, GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let mut read = WorldFieldReadContextV1 {
        root: proof.root(),
        state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let value = read_qualified_diplomacy_scalar_v1(
        id,
        field,
        column,
        proof,
        &mut read,
        8,
        &mut std::io::sink(),
    )?;
    read.check()?;
    Ok(value)
}
pub(super) fn reputation<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<(Option<WorldNumberV1>, i64), GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let before = read_world_number_map_overlay_v1(
        WorldNumberMapFieldV1::ReputationScores,
        WorldNumberMapKeyRefV1::Text(id),
        ctx,
    )?;
    let amount = match before {
        None => 0,
        Some(WorldNumberV1::Signed(n)) => n,
        _ => return Err(invalid().into()),
    };
    Ok((before, amount))
}
pub(super) fn change_reputation<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    id: &str,
    before: Option<WorldNumberV1>,
    after: i64,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let guard = ctx.transaction.guard();
    if !change_world_number_map_overlay_v1(
        WorldNumberMapChangeV1 {
            field: WorldNumberMapFieldV1::ReputationScores,
            key: WorldNumberMapKeyRefV1::Text(id),
            expected: before,
            after: Some(WorldNumberV1::Signed(after)),
        },
        ctx,
        write,
        &guard,
        live,
    )? {
        return Err(invalid().into());
    }
    Ok(())
}
pub(super) fn electricity_or_data<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    ordinal: u64,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<Option<i64>, GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let query = AgentSubtreeQualifiedRequestV1 {
        cell: AgentSubtreeScalarRequestV1 {
            agent: id,
            field: AgentFieldV1::ResourceAmount,
            ordinal,
        },
        qualification: sources.agents,
    };
    let guard = ctx.transaction.guard();
    match read_agent_subtree_scalar_overlay_v1(query, ctx, 8, &mut std::io::sink(), &guard, live)? {
        None => Ok(None),
        Some(AgentSubtreeValueV1::I64(n)) => Ok(Some(n)),
        _ => Err(invalid().into()),
    }
}
pub(super) fn change_resource<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    id: &str,
    ordinal: u64,
    before: Option<i64>,
    after: i64,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let guard = ctx.transaction.guard();
    let query = AgentSubtreeQualifiedRequestV1 {
        cell: AgentSubtreeScalarRequestV1 {
            agent: id,
            field: AgentFieldV1::ResourceAmount,
            ordinal,
        },
        qualification: sources.agents,
    };
    let mut output = ValueMutationContextV1 {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        cancelled: &mut write.cancelled,
    };
    if !change_agent_subtree_scalar_v1(
        AgentSubtreeCellChangeV1 {
            query,
            expected: before.map(AgentScalarRefV1::I64),
            after: (after != 0).then_some(AgentScalarRefV1::I64(after)),
            max_scalar_bytes: 8,
        },
        ctx,
        &mut output,
        &guard,
        live,
    )? {
        return Err(invalid().into());
    }
    Ok(())
}
pub(super) fn touch<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    id: &str,
    required: bool,
    now: u64,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let guard = ctx.transaction.guard();
    let present = read_current_agent_presence_v1(
        id,
        sources.agents,
        sources.roots,
        ctx,
        parent,
        &mut work.pending_entries_remaining,
        &guard,
        live,
    )? == CurrentAgentPresenceV1::Existing;
    if !present {
        if required {
            let fee = tail.reserve(
                std::mem::size_of::<crate::WorldError>() + id.len(),
                MemoryClass::Ordinary,
            )?;
            return Err(GameplayCycleErrorV1::Business(fee.retain(
                crate::WorldError::AgentNotFound {
                    agent_id: id.to_owned(),
                },
            )));
        }
        return Ok(());
    }
    tail.charge(oasis7_world_state::incremental_agent_body::AGENT_BODY_VALUE_BYTES)?;
    let key = oasis7_world_state::incremental_agent_body::AgentBodyKeyV1::new(id)
        .map_err(BodyMutationError::Invalid)?;
    let mut mutation = crate::world::incremental_body::BodyMutationContext {
        sink: &mut *write.sink,
        memory: write.memory,
        io: &mut *write.io,
        cancelled: &mut write.cancelled,
    };
    let before =
        oasis7_world_state::incremental_agent_body::AgentBodyValueV1::from_canonical_bytes(
            &crate::world::incremental_body_event::read_fixed::<
                { oasis7_world_state::incremental_agent_body::AGENT_BODY_VALUE_BYTES },
                _,
                _,
                _,
                _,
            >(
                ctx.transaction,
                oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                key.as_bytes(),
                ctx.values.source,
                &mut mutation,
            )?,
        )
        .map_err(BodyMutationError::Invalid)?;
    let after = crate::world::incremental_body::write_body_value(
        &oasis7_world_state::incremental_agent_body::AgentBodyValueV1 {
            body_view: before.body_view,
            last_active: now,
        },
        &mut mutation,
    )?;
    ctx.transaction.write(
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
        key.as_bytes(),
        Some(after),
    )?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(())
}

pub(super) fn change_diplomacy<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    column: DiplomacyScalarV1,
    proof: &mut ReservedValue<DiplomacyRecordQualificationV1>,
    before: Option<DiplomacyScalarValueV1<'_>>,
    after: Option<DiplomacyScalarValueV1<'_>>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let guard = ctx.transaction.guard();
    let next = change_qualified_diplomacy_scalar_v1(
        DiplomacyScalarChangeV1 {
            field,
            id,
            column,
            qualification: proof,
            expected: before,
            after,
            max_current_payload_bytes: work.max_text_bytes as u64,
            max_pending_node_bytes: work.max_pending_node_bytes,
        },
        ctx,
        write,
        &guard,
        live,
    )?
    .ok_or_else(invalid)?;
    *proof = next;
    Ok(())
}
pub(super) fn contract_status(
    text: &str,
) -> Result<crate::EconomicContractStatus, GameplayCycleErrorV1> {
    Ok(match text {
        "open" => crate::EconomicContractStatus::Open,
        "accepted" => crate::EconomicContractStatus::Accepted,
        "settled" => crate::EconomicContractStatus::Settled,
        "expired" => crate::EconomicContractStatus::Expired,
        _ => return Err(invalid().into()),
    })
}
pub(super) fn crisis_status(text: &str) -> Result<crate::CrisisStatus, GameplayCycleErrorV1> {
    Ok(match text {
        "active" => crate::CrisisStatus::Active,
        "resolved" => crate::CrisisStatus::Resolved,
        "timed_out" => crate::CrisisStatus::TimedOut,
        _ => return Err(invalid().into()),
    })
}

pub(in crate::world) fn open_diplomacy<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<DiplomacyRecordQualificationV1>>, GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let mut read = WorldFieldReadContextV1 {
        root: sources
            .roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
        state: parent.state,
        values: ValueReadContext {
            source: parent.values.source,
            memory: parent.values.memory,
            class: parent.values.class,
            io: &mut *parent.values.io,
        },
        key_work: &mut *parent.key_work,
        semantic_bytes: parent.semantic_bytes,
        record_bytes: parent.record_bytes,
        take_state_error: &mut parent.take_state_error,
        cancelled: &mut parent.cancelled,
    };
    let guard = ctx.transaction.guard();
    Ok(match field {
        DiplomacyFieldV1::Alliances | DiplomacyFieldV1::Wars => {
            open_current_diplomacy_relation_record_v1(
                field,
                id,
                sources.diplomacy,
                sources.roots,
                ctx.transaction,
                &mut read,
                &mut work.diplomacy_index,
                &guard,
                live,
            )?
        }
        DiplomacyFieldV1::Crises => open_current_diplomacy_crisis_record_v1(
            id,
            sources.diplomacy,
            sources.roots,
            ctx.transaction,
            &mut read,
            &mut work.selected_scan,
            &mut work.cells_remaining,
            work.max_text_bytes as u64,
            &guard,
            live,
        )?,
        DiplomacyFieldV1::EconomicContracts => open_current_diplomacy_contract_record_v1(
            id,
            sources.diplomacy,
            sources.roots,
            ctx.transaction,
            &mut read,
            &mut work.selected_scan,
            &mut work.cells_remaining,
            work.max_text_bytes as u64,
            &guard,
            live,
        )?,
    })
}

pub(super) fn open_governance<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    kind: oasis7_world_state::incremental_governance_records::GovernanceRecordKindV1,
    id: &str,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<
    Option<
        ReservedValue<
            oasis7_world_state::incremental_governance_records::GovernanceRecordQualificationV1,
        >,
    >,
    GameplayCycleErrorV1,
> {
    tail.charge(id.len())?;
    let mut read = WorldFieldReadContextV1 {
        root: sources.roots.state_root(kind.namespace()),
        state: parent.state,
        values: ValueReadContext {
            source: parent.values.source,
            memory: parent.values.memory,
            class: parent.values.class,
            io: &mut *parent.values.io,
        },
        key_work: &mut *parent.key_work,
        semantic_bytes: parent.semantic_bytes,
        record_bytes: parent.record_bytes,
        take_state_error: &mut parent.take_state_error,
        cancelled: &mut parent.cancelled,
    };
    let guard = ctx.transaction.guard();
    Ok(oasis7_world_state::incremental_governance_records::open_current_gameplay_governance_record_v1(oasis7_world_state::incremental_governance_records::GovernanceRecordKeyV1{kind,id},sources.governance,sources.roots,ctx.transaction,&mut read,&mut work.selected_scan,&mut work.cells_remaining,work.max_text_bytes as u64,&guard,live)?)
}
pub(super) fn governance_scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: oasis7_world_state::incremental_governance_records::GovernanceRecordKeyV1<'_>,
    cell: oasis7_world_state::incremental_governance_records::GovernanceCellV1<'_>,
    proof: &ReservedValue<
        oasis7_world_state::incremental_governance_records::GovernanceRecordQualificationV1,
    >,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<
    oasis7_world_state::incremental_governance_records::GovernanceValueV1,
    GameplayCycleErrorV1,
> {
    tail.charge(key.id.len())?;
    let guard = ctx.transaction.guard();
    oasis7_world_state::incremental_governance_records::read_qualified_governance_cell_overlay_v1(
        oasis7_world_state::incremental_governance_records::GovernanceCellRequestV1 {
            key,
            cell,
            qualification: proof,
        },
        ctx,
        8,
        &mut std::io::sink(),
        &guard,
        live,
    )?
    .ok_or_else(|| invalid().into())
}
pub(super) fn governance_text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: oasis7_world_state::incremental_governance_records::GovernanceRecordKeyV1<'_>,
    cell: oasis7_world_state::incremental_governance_records::GovernanceCellV1<'_>,
    proof: &ReservedValue<
        oasis7_world_state::incremental_governance_records::GovernanceRecordQualificationV1,
    >,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<String>>, GameplayCycleErrorV1> {
    use oasis7_world_state::incremental_governance_records::*;
    tail.charge(key.id.len())?;
    let guard = ctx.transaction.guard();
    let measured = read_qualified_governance_cell_overlay_v1(
        GovernanceCellRequestV1 {
            key,
            cell,
            qualification: proof,
        },
        ctx,
        work.max_text_bytes as u64,
        &mut std::io::sink(),
        &guard,
        live,
    )?;
    let n = match measured {
        Some(GovernanceValueV1::Unit) => return Ok(None),
        Some(GovernanceValueV1::Text { utf8_bytes }) => {
            usize::try_from(utf8_bytes).map_err(|_| invalid())?
        }
        _ => return Err(invalid().into()),
    };
    if n > work.max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("gameplay governance text").into());
    }
    tail.charge(n)?;
    let fee = tail.reserve(
        n.checked_add(std::mem::size_of::<String>())
            .ok_or_else(invalid)?,
        MemoryClass::Ordinary,
    )?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(n)
        .map_err(|_| ValueTreeError::ResourceLimited("gameplay governance text"))?;
    if bytes.capacity() > n {
        return Err(invalid().into());
    }
    let guard = ctx.transaction.guard();
    let actual = read_qualified_governance_cell_overlay_v1(
        GovernanceCellRequestV1 {
            key,
            cell,
            qualification: proof,
        },
        ctx,
        n as u64,
        &mut bytes,
        &guard,
        live,
    )?;
    if actual
        != Some(GovernanceValueV1::Text {
            utf8_bytes: n as u64,
        })
        || bytes.len() != n
    {
        return Err(invalid().into());
    }
    Ok(Some(
        fee.retain(String::from_utf8(bytes).map_err(|_| invalid())?),
    ))
}
pub(super) fn change_governance<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    key: oasis7_world_state::incremental_governance_records::GovernanceRecordKeyV1<'_>,
    cell: oasis7_world_state::incremental_governance_records::GovernanceCellV1<'_>,
    proof: &mut ReservedValue<
        oasis7_world_state::incremental_governance_records::GovernanceRecordQualificationV1,
    >,
    before: oasis7_world_state::incremental_governance_records::GovernanceValueRefV1<'_>,
    after: oasis7_world_state::incremental_governance_records::GovernanceValueRefV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    use oasis7_world_state::incremental_governance_records::*;
    tail.charge(key.id.len())?;
    let guard = ctx.transaction.guard();
    let next = change_qualified_governance_cell_v1(
        GovernanceCellChangeV1 {
            key,
            cell,
            qualification: proof,
            expected: Some(before),
            after: Some(after),
            max_current_payload_bytes: work.max_text_bytes as u64,
            max_pending_node_bytes: work.max_pending_node_bytes,
        },
        ctx,
        write,
        &guard,
        live,
    )?
    .ok_or_else(invalid)?;
    *proof = next;
    Ok(())
}

pub(super) fn member_scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    ordinal: u64,
    column: DiplomacyMemberColumnV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<Option<DiplomacyValueV1>, GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let mut read = WorldFieldReadContextV1 {
        root: proof.root(),
        state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let value = read_qualified_diplomacy_member_v1(
        DiplomacyMemberReadV1 {
            field,
            id,
            ordinal,
            column,
            qualification: proof,
            max_payload_bytes: 8,
        },
        &mut read,
        &mut std::io::sink(),
    )?;
    read.check()?;
    Ok(value)
}

pub(super) fn member_text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    ordinal: u64,
    column: DiplomacyMemberColumnV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<Option<ReservedValue<String>>, GameplayCycleErrorV1> {
    member_text_with_limit(id,field,ordinal,column,proof,state,ctx,work.max_text_bytes,tail)
}
pub(in crate::world) fn member_text_with_limit<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    ordinal: u64,
    column: DiplomacyMemberColumnV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_text_bytes: usize,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<Option<ReservedValue<String>>, GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let mut read = WorldFieldReadContextV1 {
        root: proof.root(),
        state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let Some(measured) = read_qualified_diplomacy_member_v1(
        DiplomacyMemberReadV1 {
            field,
            id,
            ordinal,
            column,
            qualification: proof,
            max_payload_bytes: max_text_bytes as u64,
        },
        &mut read,
        &mut std::io::sink(),
    )?
    else {
        return Ok(None);
    };
    let DiplomacyValueV1::Text { utf8_bytes: n } = measured else {
        return Err(invalid().into());
    };
    let n = usize::try_from(n).map_err(|_| invalid())?;
    if n > max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("gameplay selected text").into());
    }
    tail.charge(n)?;
    let fee = tail.reserve(
        n.checked_add(std::mem::size_of::<String>())
            .ok_or_else(invalid)?,
        MemoryClass::Ordinary,
    )?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(n)
        .map_err(|_| ValueTreeError::ResourceLimited("gameplay selected text"))?;
    if bytes.capacity() > n {
        return Err(invalid().into());
    }
    let actual = read_qualified_diplomacy_member_v1(
        DiplomacyMemberReadV1 {
            field,
            id,
            ordinal,
            column,
            qualification: proof,
            max_payload_bytes: n as u64,
        },
        &mut read,
        &mut bytes,
    )?;
    if actual
        != Some(DiplomacyValueV1::Text {
            utf8_bytes: n as u64,
        })
        || bytes.len() != n
    {
        return Err(invalid().into());
    }
    read.check()?;
    Ok(Some(
        fee.retain(String::from_utf8(bytes).map_err(|_| invalid())?),
    ))
}

pub(in crate::world) fn member_length<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    field: DiplomacyFieldV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<u64, GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let mut read = WorldFieldReadContextV1 {
        root: proof.root(),
        state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let value = read_qualified_diplomacy_length_v1(field, id, proof, &mut read)?;
    read.check()?;
    Ok(value)
}

pub(super) fn require_agent<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    id: &str,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    tail.charge(id.len())?;
    let guard = ctx.transaction.guard();
    let present = read_current_agent_presence_v1(
        id,
        sources.agents,
        sources.roots,
        ctx,
        parent,
        &mut work.pending_entries_remaining,
        &guard,
        live,
    )? == CurrentAgentPresenceV1::Existing;
    if !present {
        let fee = tail.reserve(
            std::mem::size_of::<crate::WorldError>() + id.len(),
            MemoryClass::Ordinary,
        )?;
        return Err(GameplayCycleErrorV1::Business(fee.retain(
            crate::WorldError::AgentNotFound {
                agent_id: id.to_owned(),
            },
        )));
    }
    Ok(())
}
