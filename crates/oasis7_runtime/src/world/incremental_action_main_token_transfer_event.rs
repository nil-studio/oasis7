//! Expected-value monetary writes and the original sender-only mailbox route.
//! Publication accepts only the private post-write producer from this owner.
use super::*;
use crate::world::{
    incremental_body::write_field_bytes,
    incremental_body_event::{original, read_fixed},
    incremental_module_execution_event::contribute_main_token_transfer_event_v1,
};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Ns;
use oasis7_world_state::{
    incremental_event_fields::{
        MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1, mailbox_event_ref_v1,
    },
    incremental_world_balances::{
        WorldBalanceChangeV1, WorldBalanceFieldV1, WorldBalanceRefV1,
        change_world_balance_overlay_v1,
    },
    incremental_world_number_maps::{
        WorldNumberMapChangeV1, WorldNumberMapFieldV1, WorldNumberMapKeyRefV1, WorldNumberV1,
        change_world_number_map_overlay_v1,
    },
};

fn before_ref(
    account: &str,
    value: Option<WorldBalanceReadV1>,
) -> Result<Option<WorldBalanceRefV1<'_>>, BodyMutationError> {
    match value {
        None => Ok(None),
        Some(WorldBalanceReadV1::MainToken {
            liquid_balance,
            vested_balance,
            restricted_starter_claim_balance,
            ..
        }) => Ok(Some(WorldBalanceRefV1::MainToken {
            account_id: account,
            liquid_balance,
            vested_balance,
            restricted_starter_claim_balance,
        })),
        _ => Err(BodyMutationError::Invalid(
            "main token expected balance kind",
        )),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn install_delta<M, P, V, W, E, C, D, CW>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
    current: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    members: &mut MemberRecordWriteContext<'_, W, CW>,
    delta: &MainTokenTransferDeltaV1,
    before: [Option<WorldBalanceReadV1>; 2],
    before_nonce: Option<u64>,
    work: &mut MainTokenTransferWorkV1,
    common: &FormalBoundedWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    // The original monetary publisher also normalizes the world material
    // mirror. Consume the actual retained material owner, never a World clone.
    let guard = step.transaction.guard();
    materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
    for (row, before) in [(&delta.source, before[0]), (&delta.target, before[1])] {
        work.spend(
            common,
            row.account_id.len().checked_add(32).ok_or_else(limited)?,
            &mut || (current.cancelled)() || (step.mutation.cancelled)() || (members.cancelled)(),
        )?;
        let expected = before_ref(&row.account_id, before)?;
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: step.transaction,
            values: ValueReadContext {
                source: current.values.source,
                memory: current.values.memory,
                class: current.values.class,
                io: &mut *current.values.io,
            },
            key_work: &mut *current.key_work,
            semantic_bytes: current.semantic_bytes,
            record_bytes: current.record_bytes,
            take_state_error: &mut current.take_state_error,
            cancelled: || (current.cancelled)() || (step.mutation.cancelled)(),
        };
        let guard = overlay.transaction.guard();
        let changed = change_world_balance_overlay_v1(
            WorldBalanceChangeV1 {
                field: WorldBalanceFieldV1::MainToken,
                key: &row.account_id,
                expected,
                after: Some(WorldBalanceRefV1::from(row)),
                max_current_identity_bytes: work.max_identity_bytes,
            },
            &mut overlay,
            members,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
        if !changed {
            return Err(BodyMutationError::Invalid(
                "main token transfer balance unchanged",
            ));
        }
    }
    work.spend(
        common,
        delta
            .source
            .account_id
            .len()
            .checked_add(8)
            .ok_or_else(limited)?,
        &mut || (current.cancelled)() || (step.mutation.cancelled)() || (members.cancelled)(),
    )?;
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: step.transaction,
        values: ValueReadContext {
            source: current.values.source,
            memory: current.values.memory,
            class: current.values.class,
            io: &mut *current.values.io,
        },
        key_work: &mut *current.key_work,
        semantic_bytes: current.semantic_bytes,
        record_bytes: current.record_bytes,
        take_state_error: &mut current.take_state_error,
        cancelled: || (current.cancelled)() || (step.mutation.cancelled)(),
    };
    let guard = overlay.transaction.guard();
    let changed = change_world_number_map_overlay_v1(
        WorldNumberMapChangeV1 {
            field: WorldNumberMapFieldV1::MainTokenTransferNonces,
            key: WorldNumberMapKeyRefV1::Text(&delta.source.account_id),
            expected: before_nonce.map(WorldNumberV1::Unsigned),
            after: Some(WorldNumberV1::Unsigned(delta.nonce)),
        },
        &mut overlay,
        members,
        &guard,
        live,
    )
    .map_err(overlay_error)?;
    if !changed {
        return Err(BodyMutationError::Invalid(
            "main token transfer nonce unchanged",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish<M, V, W, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    prepared: &PreparedMainTokenTransferEventV1,
    sender_present: bool,
    work: &mut MainTokenTransferWorkV1,
    common: &FormalBoundedWorkV1,
    max_event_bytes: usize,
    live: &ExecutionFence,
    persist: &mut F,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    prepared.validate(step.transaction, step.mutation.memory, step.time(), live)?;
    let event = prepared.event();
    let crate::WorldEventBody::Domain(crate::DomainEvent::MainTokenTransferred {
        from_account_id,
        ..
    }) = &event.body
    else {
        return Err(BodyMutationError::Invalid(
            "main token transfer original body",
        ));
    };
    let _keys = step
        .mutation
        .memory
        .reserve(
            from_account_id
                .len()
                .checked_mul(3)
                .and_then(|n| n.checked_add(3 * 1024))
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    // As in the existing MaterialTransfer sink, both passes debit the same
    // CanonicalCborWork. The first supplies the real mailbox original ID.
    let measured = original::measure(
        event,
        step.mutation.memory,
        step.original_work,
        &mut step.mutation.cancelled,
        max_event_bytes,
    )?;
    let original_id = measured.id();
    contribute_main_token_transfer_event_v1(
        step.time(),
        prepared,
        live,
        step.transaction,
        step.values,
        step.mutation,
        step.original_work,
        max_event_bytes,
        persist,
    )?;
    if sender_present {
        work.spend(
            common,
            from_account_id.len().checked_add(64).ok_or_else(limited)?,
            &mut step.mutation.cancelled,
        )?;
        let key = MailboxKeyV1::new(from_account_id, None).map_err(BodyMutationError::Invalid)?;
        let mut frontier = MailboxFrontierV1::from_canonical_bytes(&read_fixed::<
            MAILBOX_FRONTIER_BYTES,
            _,
            _,
            _,
            _,
        >(
            step.transaction,
            Ns::Scheduler,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?)
        .map_err(BodyMutationError::Invalid)?;
        let item = MailboxKeyV1::new(from_account_id, Some(frontier.tail))
            .map_err(BodyMutationError::Invalid)?;
        frontier.tail = frontier
            .tail
            .checked_add(1)
            .ok_or(BodyMutationError::Invalid("main token mailbox overflow"))?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        if step
            .transaction
            .read(Ns::Scheduler, item.as_bytes())?
            .is_some()
        {
            return Err(BodyMutationError::Invalid("main token mailbox occupied"));
        }
        let item_ref = write_field_bytes(&mailbox_event_ref_v1(original_id), step.mutation)?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        step.transaction
            .write(Ns::Scheduler, item.as_bytes(), Some(item_ref))?;
        let frontier_ref = write_field_bytes(
            &frontier
                .canonical_bytes()
                .map_err(BodyMutationError::Invalid)?,
            step.mutation,
        )?;
        let guard = step.transaction.guard();
        step.transaction.validate_guard(&guard, live)?;
        step.transaction
            .write(Ns::Scheduler, key.as_bytes(), Some(frontier_ref))?;
    }
    if (step.mutation.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
