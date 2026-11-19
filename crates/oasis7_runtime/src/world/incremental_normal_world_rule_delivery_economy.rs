//! Selected original economy reducer and its complete exact Current write set.
use super::*;
use oasis7_world_state::{
    AgentCell, AgentClaimState, AgentState, MainTokenAccountBalance, MainTokenSupplyState,
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::*,
    incremental_fixed_economy_records::*,
    incremental_world_balances::*,
    incremental_world_number_maps::*,
    incremental_world_u64::*,
    world_rules::{AgentIndustrialOrderV1, AgentIndustrialPhaseV1},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Write},
};
type ClaimBefore = ReservedValue<CurrentIndustrialClaimRecordV1<AgentClaimState>>;
pub(super) struct PreparedEconomy {
    account_before: Option<MainTokenAccountBalance>,
    account_after: MainTokenAccountBalance,
    supply_before: MainTokenSupplyState,
    supply_after: MainTokenSupplyState,
    supply: ReservedValue<FixedEconomyRecordQualificationV1>,
    treasury_before: Option<WorldNumberV1>,
    treasury_after: BTreeMap<String, u64>,
    claim: AgentClaimState,
    claim_before: ClaimBefore,
    sequence_before: Option<WorldNumberV1>,
    sequence: u64,
    last_before: u64,
    last_after: u64,
    cell: AgentCell,
    _account_fee: MemoryReservation,
    _fee: MemoryReservation,
}
fn num(v: Option<WorldNumberV1>) -> Result<u64, IndustrialDeliveryErrorV1> {
    match v {
        Some(WorldNumberV1::Unsigned(v)) => Ok(v),
        None => Ok(0),
        _ => Err(invalid()),
    }
}
fn btree_node<K, V>() -> usize {
    11 * (size_of::<K>() + size_of::<V>()) + 12 * size_of::<usize>() + 64
}
/// Complete claim inner-targets plus all original nonterminal order targets.
/// Only resident metadata is scanned; selected claim roots are Current-joined.
pub(in crate::world) fn capacity<M, P, V, R, E, C, PE, PC>(
    order: &AgentIndustrialOrderV1,
    sources: &IndustrialDeliverySourcesV1<'_, '_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialDeliveryWorkV1,
    primary:oasis7_world_state::world_rules::industrial_eligibility_kernel::PrimaryProductionFactsV1<'_>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    live: &ExecutionFence,
) -> Result<(), IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let claims = {
        let mut read = WorldFieldReadContextV1 {
            root: sources
                .eligibility
                .roots
                .state_root(industrial_claim_namespace_v1()?),
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
        recover_current_industrial_claim_targets_v1(
            &order.owner_id,
            sources.claims,
            sources.eligibility.native,
            sources.eligibility.roots,
            ctx.transaction,
            &mut read,
            &mut work.eligibility.scan,
            &mut work.eligibility.typed,
            &mut work.claims,
            &guard,
            live,
        )?
    };
    let guard = ctx.transaction.guard();
    sources
        .eligibility
        .orders
        .validate_current(ctx, &guard, live)
        .map_err(IndustrialEligibilityErrorV1::from)?;
    let mut count = claims.len().checked_add(2).ok_or_else(invalid)?;
    let mut bytes = order
        .owner_id
        .len()
        .checked_add(order.target_agent_id.len())
        .ok_or_else(invalid)?;
    for id in claims.iter() {
        tail.charge(id.len())?;
        bytes = bytes.checked_add(id.len()).ok_or_else(invalid)?;
    }
    for (_, index) in sources.eligibility.orders.iter_order_indices() {
        tail.charge(index.owner_id().len().min(order.owner_id.len()))?;
        if index.owner_id() == order.owner_id
            && !matches!(
                index.phase(),
                AgentIndustrialPhaseV1::Delivered
                    | AgentIndustrialPhaseV1::Cancelled
                    | AgentIndustrialPhaseV1::Disposed
            )
        {
            count = count.checked_add(1).ok_or_else(invalid)?;
            bytes = bytes
                .checked_add(index.target_agent_id().len())
                .ok_or_else(invalid)?;
        }
    }
    let bound = btree_node::<String, ()>()
        .checked_mul(count.checked_add(2).ok_or_else(invalid)?)
        .and_then(|n| n.checked_add(bytes))
        .ok_or_else(invalid)?;
    // Original union allocates at most this many entries. Both copies and all
    // conservative BTree comparisons are prepaid from the same Common child.
    tail.charge(
        bytes
            .checked_mul(usize::BITS as usize * 12)
            .ok_or_else(invalid)?,
    )?;
    let _fee = tail.reserve(bound, MemoryClass::Ordinary)?;
    let set = oasis7_world_state::world_rules::manufacturing_existing_targets_from_facts_v1(
        &order.owner_id,
        primary,
        claims.iter().map(String::as_str),
    )?;
    let score = match read_world_number_map_overlay_v1(
        WorldNumberMapFieldV1::ReputationScores,
        WorldNumberMapKeyRefV1::Text(&order.owner_id),
        ctx,
    )? {
        Some(WorldNumberV1::Signed(v)) => v,
        None => 0,
        _ => return Err(invalid()),
    };
    oasis7_world_state::world_rules::manufacturing_capacity_from_targets_v1(
        set,
        sources
            .eligibility
            .orders
            .iter_order_indices()
            .filter(|(_, i)| {
                i.owner_id() == order.owner_id
                    && !matches!(
                        i.phase(),
                        AgentIndustrialPhaseV1::Delivered
                            | AgentIndustrialPhaseV1::Cancelled
                            | AgentIndustrialPhaseV1::Disposed
                    )
            })
            .map(|(_, i)| i.target_agent_id()),
        Some(&order.target_agent_id),
        false,
        score,
    )?;
    Ok(())
}
pub(in crate::world) fn balance<M, R, E, C>(
    actor: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: usize,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<(Option<MainTokenAccountBalance>, MemoryReservation), IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let first = read_world_balance_overlay_v1(
        WorldBalanceFieldV1::MainToken,
        actor,
        ctx,
        max as u64,
        &mut io::sink(),
    )?;
    let bytes = match first {
        Some(WorldBalanceReadV1::MainToken {
            identity_utf8_bytes,
            ..
        }) => usize::try_from(identity_utf8_bytes).map_err(|_| invalid())?,
        None => 0,
        _ => return Err(invalid()),
    };
    tail.charge(bytes)?;
    let fee = tail.reserve(
        size_of::<MainTokenAccountBalance>()
            .checked_add(bytes)
            .ok_or_else(invalid)?,
        MemoryClass::Ordinary,
    )?;
    let mut identity = Vec::new();
    identity.try_reserve_exact(bytes).map_err(|_| invalid())?;
    if identity.capacity() > bytes {
        return Err(invalid());
    }
    let second = read_world_balance_overlay_v1(
        WorldBalanceFieldV1::MainToken,
        actor,
        ctx,
        max as u64,
        &mut identity,
    )?;
    if first != second || identity.len() != bytes {
        return Err(invalid());
    }
    let value = match second {
        Some(WorldBalanceReadV1::MainToken {
            liquid_balance,
            vested_balance,
            restricted_starter_claim_balance,
            ..
        }) => Some(MainTokenAccountBalance {
            account_id: String::from_utf8(identity).map_err(|_| invalid())?,
            liquid_balance,
            vested_balance,
            restricted_starter_claim_balance,
        }),
        None => None,
        _ => return Err(invalid()),
    };
    Ok((value, fee))
}
fn scalar<M, R, E, C>(
    actor: &str,
    field: AgentFieldV1,
    sources: &IndustrialDeliverySourcesV1<'_, '_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: usize,
    live: &ExecutionFence,
) -> Result<i64, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let guard = ctx.transaction.guard();
    match read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent: actor,
                field,
                ordinal: 0,
            },
            qualification: sources.eligibility.agents,
        },
        ctx,
        max as u64,
        &mut io::sink(),
        &guard,
        live,
    )? {
        Some(AgentSubtreeValueV1::I64(v)) => Ok(v),
        _ => Err(invalid()),
    }
}
fn acquisition_hash(
    event: &crate::DomainEvent,
    sequence: u64,
    tail: &mut NormalTailWorkV1<'_, '_>,
) -> Result<String, BodyMutationError> {
    struct Output<'a, 'b, 'c> {
        tail: &'a mut NormalTailWorkV1<'b, 'c>,
        hash: Sha256,
        error: Option<BodyMutationError>,
    }
    impl Write for Output<'_, '_, '_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if let Err(e) = self.tail.charge(b.len()) {
                self.error = Some(e);
                return Err(io::Error::other("claim acquisition meter"));
            }
            self.hash.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Output {
        tail,
        hash: Sha256::new(),
        error: None,
    };
    let encoded = serde_json::to_writer(
        &mut output,
        &("oasis7.agent-claim.acquisition.v1", event, sequence),
    );
    if let Some(error) = output.error {
        return Err(error);
    }
    encoded.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let digest = output.hash.finalize();
    let mut text = String::new();
    text.try_reserve_exact(64)
        .map_err(|_| BodyMutationError::Invalid("acquisition digest allocation"))?;
    if text.capacity() > 64 {
        return Err(BodyMutationError::Invalid("acquisition digest capacity"));
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for b in digest {
        text.push(HEX[(b >> 4) as usize] as char);
        text.push(HEX[(b & 15) as usize] as char);
    }
    Ok(text)
}
pub(super) fn prepare<M, P, V, R, E, C, PE, PC>(
    order: &AgentIndustrialOrderV1,
    actor: &String,
    epoch: u64,
    starter: Option<&oasis7_world_state::StarterOcClaimState>,
    sources: &IndustrialDeliverySourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialDeliveryWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedEconomy, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let (account_before, account_fee) = balance(actor, ctx, work.max_identity_bytes, tail)?;
    let player = starter.map_or(0, |s| s.player_id.len());
    let identity = account_before.as_ref().map_or(0, |a| a.account_id.len());
    // Exhaustive fixed new-cell constructor layout: humanoid+frame, seven
    // default slots, six installed module IDs (each <=the radiation ID length).
    // Empty ResourceStock/cargo/mailbox/intent and scalar BodyKernelView allocate
    // nothing. No caller-controlled World collection enters this allowance.
    let cell_heap = "humanoid".len()
        + oasis7_world_state::agent_state::DEFAULT_BODY_FRAME_KIND.len()
        + 7 * (size_of::<oasis7_world_state::agent_state::BodyModuleSlot>() + "slot-1".len())
        + 6 * "m1.power.radiation_harvest".len();
    let payload = actor
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(order.target_agent_id.len().checked_mul(4)?))
        .and_then(|n| n.checked_add(player))
        .and_then(|n| n.checked_add(identity))
        .and_then(|n| {
            n.checked_add(
                cell_heap
                    + 64
                    + 2 * btree_node::<String, u64>()
                    + 1024
                    + size_of::<PreparedEconomy>()
                    + size_of::<crate::DomainEvent>(),
            )
        })
        .ok_or_else(invalid)?;
    tail.charge(payload)?;
    let fee = tail.reserve(payload, MemoryClass::Ordinary)?;
    let mut account = Some(account_before.clone().unwrap_or_default());
    let funded = account
        .as_ref()
        .ok_or_else(invalid)?
        .liquid_balance
        .checked_add(order.quote.upfront_liquid)
        .ok_or_else(|| denied("funding_overflow"))?;
    account.as_mut().ok_or_else(invalid)?.liquid_balance = funded;
    let q = &order.quote.claim_cost;
    let funding = oasis7_world_state::agent_claims::split_agent_claim_upfront_funding(
        q.slot_index,
        account.as_ref().ok_or_else(invalid)?.liquid_balance,
        0,
        q.activation_fee_amount,
        q.claim_bond_amount,
        q.upkeep_per_epoch,
    )
    .map_err(denied)?;
    let event = crate::DomainEvent::AgentClaimed {
        acquisition_contract_version: 1,
        claimer_agent_id: actor.clone(),
        target_agent_id: order.target_agent_id.clone(),
        reputation_tier: q.reputation_tier,
        slot_index: q.slot_index,
        activation_fee_amount: q.activation_fee_amount,
        activation_fee_burn_amount: q.activation_fee_burn_amount,
        activation_fee_treasury_amount: q.activation_fee_treasury_amount,
        claim_bond_amount: q.claim_bond_amount,
        upfront_restricted_spent_amount: funding.upfront.restricted_amount,
        upfront_liquid_spent_amount: funding.upfront.liquid_amount,
        auto_issued_restricted_amount: 0,
        auto_issued_restricted_source_treasury_bucket_id: None,
        claim_bond_locked_restricted_amount: funding.claim_bond.restricted_amount,
        claim_bond_locked_liquid_amount: funding.claim_bond.liquid_amount,
        upkeep_per_epoch: q.upkeep_per_epoch,
        claimed_at_epoch: epoch,
        upkeep_paid_through_epoch: epoch,
        release_cooldown_epochs: q.release_cooldown_epochs,
        grace_epochs: q.grace_epochs,
        idle_warning_epochs: q.idle_warning_epochs,
        forced_idle_reclaim_epochs: q.forced_idle_reclaim_epochs,
        forced_reclaim_penalty_bps: q.forced_reclaim_penalty_bps,
    };
    let pos = oasis7_agent_api::world_action::GeoPos::new(
        scalar(
            actor,
            AgentFieldV1::PositionX,
            sources,
            ctx,
            work.max_identity_bytes,
            live,
        )?,
        scalar(
            actor,
            AgentFieldV1::PositionY,
            sources,
            ctx,
            work.max_identity_bytes,
            live,
        )?,
        scalar(
            actor,
            AgentFieldV1::PositionZ,
            sources,
            ctx,
            work.max_identity_bytes,
            live,
        )?,
    );
    let mut cell = AgentCell::new(AgentState::new(&order.target_agent_id, pos), now);
    cell.state.resources = Default::default();
    cell.activity = Some(oasis7_world_state::agent_cell::AgentActivityV1::idle(now));
    let claim_before = {
        let mut read = WorldFieldReadContextV1 {
            root: sources
                .eligibility
                .roots
                .state_root(industrial_claim_namespace_v1()?),
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
        read_current_industrial_agent_claim_v1(
            &order.target_agent_id,
            sources.eligibility.native,
            sources.eligibility.roots,
            ctx.transaction,
            &mut read,
            &mut work.eligibility.scan,
            &mut work.eligibility.typed,
            &guard,
            live,
        )?
    };
    if claim_before.value().is_some() {
        return Err(denied(format!(
            "agent claim already exists: target={}",
            order.target_agent_id
        )));
    }
    for (_, index) in sources.eligibility.orders.iter_order_indices() {
        tail.charge(index.target_agent_id().len().min(actor.len()))?;
        if index.target_agent_id() == actor {
            return Err(denied("manufactured_actor_cannot_acquire_agent"));
        }
    }
    if starter.is_some() {
        capacity(order,sources,ctx,parent,work,oasis7_world_state::world_rules::industrial_eligibility_kernel::PrimaryProductionFactsV1 {agent_exists:true,starter_claim:starter},tail,live)?;
    }
    let sequence_before = read_world_number_map_overlay_v1(
        WorldNumberMapFieldV1::AgentClaimAcquisitionSequences,
        WorldNumberMapKeyRefV1::Text(&order.target_agent_id),
        ctx,
    )?;
    let sequence = num(sequence_before)?
        .checked_add(1)
        .ok_or_else(|| denied("agent_claim_acquisition_sequence_overflow"))?;
    let mut quote_order = None;
    for (id, index) in sources.eligibility.orders.iter_order_indices() {
        tail.charge(
            index
                .target_agent_id()
                .len()
                .min(order.target_agent_id.len()),
        )?;
        if index.target_agent_id() == order.target_agent_id
            && index.phase() == AgentIndustrialPhaseV1::AwaitingDelivery
        {
            let guard = ctx.transaction.guard();
            quote_order=oasis7_world_state::incremental_world_rule_composite_family::read_current_agent_order_v1(sources.eligibility.orders.source(),id,state,ctx,&mut work.eligibility.scan,&mut work.eligibility.typed,&guard,live)?;
            break;
        }
    }
    let quote = &quote_order
        .as_ref()
        .ok_or_else(|| denied("manufacturing_delivery_order_missing"))?
        .value()
        .quote
        .claim_cost;
    let guard = ctx.transaction.guard();
    let supply = open_current_industrial_main_token_supply_v1(
        sources.fixed,
        sources.eligibility.roots,
        ctx,
        state,
        &mut work.fixed_scan,
        &mut work.fixed_cells_remaining,
        &guard,
        live,
    )?;
    let mut values = [0u64; 4];
    for (i, column) in [
        FixedEconomyColumnV1::TotalSupply,
        FixedEconomyColumnV1::CirculatingSupply,
        FixedEconomyColumnV1::TotalIssued,
        FixedEconomyColumnV1::TotalBurned,
    ]
    .into_iter()
    .enumerate()
    {
        let guard = ctx.transaction.guard();
        let value = read_qualified_fixed_economy_overlay_v1(
            FixedEconomyCellRequestV1 {
                column,
                qualification: &supply,
            },
            ctx,
            &guard,
            live,
        )?;
        let FixedEconomyValueV1::U64(v) = value else {
            return Err(invalid());
        };
        values[i] = v;
    }
    let supply_before = MainTokenSupplyState {
        total_supply: values[0],
        circulating_supply: values[1],
        total_issued: values[2],
        total_burned: values[3],
    };
    let mut supply_after = supply_before.clone();
    let treasury_before = read_world_number_map_overlay_v1(
        WorldNumberMapFieldV1::MainTokenTreasuryBalances,
        WorldNumberMapKeyRefV1::Text(oasis7_world_state::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        ctx,
    )?;
    let treasury = BTreeMap::from([(
        oasis7_world_state::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_owned(),
        num(treasury_before)?,
    )]);
    let mut treasury_after = BTreeMap::new();
    let last_before =
        read_world_u64_overlay_v1(WorldU64FieldV1::AgentClaimLastProcessedEpoch, ctx)?;
    // Validated manufacturing orders have slot>=2. Original auto-restricted
    // funding returns 0 before either grant/pool can affect its value. Omitted
    // grant/liveops DATA therefore cannot alter any original predicate or update;
    // no grant qualification or spending permission is inferred from this proof.
    let mut hash_error = None;
    let result=oasis7_world_state::world::agent_claim_economic_publication::prepare_selected_agent_claim_economic_v1(&event,quote,actor,&mut account,&mut supply_after,&treasury,&mut treasury_after,None,starter,true,sequence,last_before,|event,seq|match acquisition_hash(event,seq,tail){Ok(v)=>Ok(v),Err(e)=>{hash_error=Some(e);Err(crate::WorldError::ResourceBalanceInvalid {reason:"bounded acquisition original".into()})}});
    if let Some(error) = hash_error {
        return Err(error.into());
    }
    let (claim, last_after) = result?;
    Ok(PreparedEconomy {
        account_before,
        account_after: account.ok_or_else(invalid)?,
        supply_before,
        supply_after,
        supply,
        treasury_before,
        treasury_after,
        claim,
        claim_before,
        sequence_before,
        sequence,
        last_before,
        last_after,
        cell,
        _account_fee: account_fee,
        _fee: fee,
    })
}
pub(super) fn commit<M, P, V, R, W, E, C, PE, PC, CW, ME>(
    prepared: &PreparedEconomy,
    order: &AgentIndustrialOrderV1,
    actor: &str,
    sources: &IndustrialDeliverySourcesV1<'_, '_>,
    _state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    let mut supply = None;
    for (column, before, after) in [
        (
            FixedEconomyColumnV1::TotalSupply,
            prepared.supply_before.total_supply,
            prepared.supply_after.total_supply,
        ),
        (
            FixedEconomyColumnV1::CirculatingSupply,
            prepared.supply_before.circulating_supply,
            prepared.supply_after.circulating_supply,
        ),
        (
            FixedEconomyColumnV1::TotalIssued,
            prepared.supply_before.total_issued,
            prepared.supply_after.total_issued,
        ),
        (
            FixedEconomyColumnV1::TotalBurned,
            prepared.supply_before.total_burned,
            prepared.supply_after.total_burned,
        ),
    ] {
        let guard = ctx.transaction.guard();
        let next = change_qualified_fixed_economy_cell_v1(
            FixedEconomyCellChangeV1 {
                column,
                qualification: supply.as_ref().unwrap_or(&prepared.supply),
                expected: FixedEconomyValueV1::U64(before),
                after: FixedEconomyValueV1::U64(after),
                max_pending_node_bytes: work.max_pending_node_bytes,
            },
            ctx,
            write,
            &guard,
            live,
        )?;
        if next.is_some() {
            supply = next;
        }
    }
    let guard = ctx.transaction.guard();
    change_world_balance_overlay_v1(
        WorldBalanceChangeV1 {
            field: WorldBalanceFieldV1::MainToken,
            key: actor,
            expected: prepared
                .account_before
                .as_ref()
                .map(WorldBalanceRefV1::from),
            after: Some(WorldBalanceRefV1::from(&prepared.account_after)),
            max_current_identity_bytes: work.max_identity_bytes as u64,
        },
        ctx,
        write,
        &guard,
        live,
    )?;
    let ecosystem = oasis7_world_state::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL;
    let after = prepared
        .treasury_after
        .get(ecosystem)
        .copied()
        .ok_or_else(invalid)?;
    let guard = ctx.transaction.guard();
    change_world_number_map_overlay_v1(
        WorldNumberMapChangeV1 {
            field: WorldNumberMapFieldV1::MainTokenTreasuryBalances,
            key: WorldNumberMapKeyRefV1::Text(ecosystem),
            expected: prepared.treasury_before,
            after: Some(WorldNumberV1::Unsigned(after)),
        },
        ctx,
        write,
        &guard,
        live,
    )?;
    let guard = ctx.transaction.guard();
    change_world_number_map_overlay_v1(
        WorldNumberMapChangeV1 {
            field: WorldNumberMapFieldV1::AgentClaimAcquisitionSequences,
            key: WorldNumberMapKeyRefV1::Text(&order.target_agent_id),
            expected: prepared.sequence_before,
            after: Some(WorldNumberV1::Unsigned(prepared.sequence)),
        },
        ctx,
        write,
        &guard,
        live,
    )?;
    {
        let mut read = WorldFieldReadContextV1 {
            root: sources
                .eligibility
                .roots
                .state_root(industrial_claim_namespace_v1()?),
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
        write_current_industrial_agent_claim_v1(
            &order.target_agent_id,
            &prepared.claim_before,
            &prepared.claim,
            ctx.transaction,
            &mut read,
            write,
            &mut work.records_remaining,
            &mut work.native_write,
            &guard,
            live,
        )?;
    }
    let guard = ctx.transaction.guard();
    change_world_u64_overlay_v1(
        WorldU64FieldV1::AgentClaimLastProcessedEpoch,
        prepared.last_before,
        prepared.last_after,
        ctx,
        write,
        &guard,
        live,
    )?;
    {
        let mut mutation = crate::world::incremental_body::BodyMutationContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            cancelled: &mut write.cancelled,
        };
        let guard = ctx.transaction.guard();
        create_agent_subtree_v1(
            AgentSubtreeCreateV1 {
                agent: &order.target_agent_id,
                cell: &prepared.cell,
                qualification: sources.eligibility.agents,
                max_scalar_bytes: work.max_identity_bytes as u64,
            },
            ctx,
            &mut mutation,
            &mut |_, _| {
                Err(BodyMutationError::Invalid(
                    "new manufactured agent has no mailbox originals",
                ))
            },
            &guard,
            live,
        )?;
        let key = oasis7_world_state::incremental_agent_body::AgentBodyKeyV1::new(actor)
            .map_err(BodyMutationError::Invalid)?;
        let body =
            oasis7_world_state::incremental_agent_body::AgentBodyValueV1::from_canonical_bytes(
                &crate::world::incremental_body_event::read_fixed::<
                    { oasis7_world_state::incremental_agent_body::AGENT_BODY_VALUE_BYTES },
                    _,
                    _,
                    _,
                    _,
                >(
                    ctx.transaction,
                    StateNamespaceV1::Business,
                    key.as_bytes(),
                    ctx.values.source,
                    &mut mutation,
                )?,
            )
            .map_err(BodyMutationError::Invalid)?;
        let after = crate::world::incremental_body::write_body_value(
            &oasis7_world_state::incremental_agent_body::AgentBodyValueV1 {
                body_view: body.body_view,
                last_active: now,
            },
            &mut mutation,
        )?;
        ctx.transaction
            .write(StateNamespaceV1::Business, key.as_bytes(), Some(after))?;
        let guard = ctx.transaction.guard();
        materials.contribute(
            ctx.transaction,
            ctx.values.source,
            &mut mutation,
            &guard,
            live,
        )?;
    }
    tail.charge_record()?;
    Ok(())
}
