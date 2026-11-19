//! Exact-parent rollback receiver; untouched original subgraphs inherit qualification.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{TransitionRecordV1, TransitionVerifier};
#[allow(clippy::too_many_arguments)]
pub fn accept_rollback_family_transition_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RollbackFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, M>,
    record: &TransitionRecordV1,
    state: &M,
    scan: &mut StateScanBudget,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
    diff_cells: &mut u64,
    mut take_state_error: impl FnMut() -> Option<ValueTreeError>,
) -> Result<ReservedValue<RollbackFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(e) = take_state_error() {
            return Err(e.into());
        }
        parent.validate_binding(&record.before)?;
        let memory = ctx.values.memory;
        if parent.class != MemoryClass::Ordinary
            || ctx.values.class != MemoryClass::Ordinary
            || verifier.class != MemoryClass::Ordinary
            || !parent.uses_budget(memory)
            || parent.reserved_bytes() < 512
            || !verifier.memory.same_pool(memory)
            || state.allocation_class() != Some(MemoryClass::Ordinary)
            || !state
                .allocation_budget()
                .is_some_and(|b| b.same_pool(memory))
        {
            return Err(invalid().into());
        }
        let witness = {
            let mut verify=crate::incremental_verified_world_transition::WorldTransitionVerificationContextV1 {
                after_root:record.after.state_root(StateNamespaceV1::Authorization),memory:ctx.values.memory,class:ctx.values.class,
                take_state_error:&mut take_state_error,cancelled:&mut *ctx.cancelled
            };
            crate::incremental_verified_world_transition::verify_world_transition_in_v1(
                verifier,
                record,
                &mut verify,
            )?
        };
        accept_rollback_family_verified_v1(
            parent,
            &witness,
            state,
            scan,
            ctx,
            diff_cells,
            &mut take_state_error,
        )
    })();
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(e) = take_state_error() {
        return Err(e.into());
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn accept_rollback_family_verified_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RollbackFamilyQualificationV1>,
    witness: &crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,
    state: &M,
    scan: &mut StateScanBudget,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
    diff_cells: &mut u64,
    mut take_state_error: impl FnMut() -> Option<ValueTreeError>,
) -> Result<ReservedValue<RollbackFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    witness.check_pool_class(ctx.values.memory, ctx.values.class)?;
    let record = witness.record();

    let result = (|| {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(e) = take_state_error() {
            return Err(e.into());
        }
        parent.validate_binding(&record.before)?;
        let memory = ctx.values.memory;
        if parent.class != MemoryClass::Ordinary
            || ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(memory)
            || parent.reserved_bytes() < 512
            || state.allocation_class() != Some(MemoryClass::Ordinary)
            || !state
                .allocation_budget()
                .is_some_and(|b| b.same_pool(memory))
        {
            return Err(invalid().into());
        }

        let charge = memory
            .reserve(512, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _scratch = memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut consumed = i128::from(parent.consumed_count);
        let mut outcomes = i128::from(parent.outcome_count);
        let mut registry = parent.registry_root.clone();
        for write in &record.writes {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if let Some(e) = take_state_error() {
                return Err(e.into());
            }
            ctx.work.cells_remaining = ctx
                .work
                .cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("rollback receiver rows"))?;
            // Other owners contribute dedicated raw keys to this same transaction.
            // The enclosing key router closes those grammars; this family still
            // rejects malformed keys in its own generic field-key domain.
            if !write
                .mutation
                .key
                .starts_with(b"oasis7.iws.world-field-key.v1")
            {
                continue;
            }
            let key = WorldFieldKeyV1::from_bytes(&write.mutation.key)
                .map_err(ValueTreeError::Invalid)?;
            if key.owner != FieldOwner::Runtime
                || !matches!(
                    key.name,
                    "rollback_authority_registry"
                        | "consumed_rollback_nonces"
                        | "rollback_nonce_outcomes"
                )
            {
                continue;
            }
            if write.namespace != StateNamespaceV1::Authorization {
                return Err(invalid().into());
            }
            if key.kind == WorldFieldRecordKindV1::Metadata {
                if write.mutation.before.is_none() || write.mutation.after != write.mutation.before
                {
                    return Err(invalid().into());
                }
                continue;
            }
            if write.mutation.before == write.mutation.after {
                continue;
            }
            let probe = oasis7_formal_protocol::incremental_state::StateEntryV1 {
                namespace: write.namespace,
                key: write.mutation.key.clone(),
                value: write
                    .mutation
                    .after
                    .or(write.mutation.before)
                    .ok_or_else(invalid)?,
            };
            let key_limit = ctx.limits.max_output_bytes;
            let original = row(&probe, ctx, key_limit)?;
            match original.key {
                Key::Unit => {
                    if key.name != "rollback_authority_registry" || write.mutation.after.is_none() {
                        return Err(invalid().into());
                    }
                    let reference = original.reference.ok_or_else(invalid)?;
                    let child = reference
                        .validate_binding(
                            blake3::hash(b"runtime.rollback.registry.v1").as_bytes(),
                            &key.semantic_key.ok_or_else(invalid)?,
                            StateNamespaceV1::Authorization,
                        )
                        .map_err(|_| invalid())?;
                    validate_registry(child, state, scan, ctx)?;
                    registry = child.clone();
                }
                Key::Text(_) => {
                    if key.name == "rollback_authority_registry" {
                        return Err(invalid().into());
                    }
                    if key.name == "consumed_rollback_nonces" {
                        if original.reference.is_some()
                            || original.codec != MemberPayloadCodecV1::Bool
                        {
                            return Err(invalid().into());
                        }
                        consumed += i128::from(write.mutation.after.is_some())
                            - i128::from(write.mutation.before.is_some());
                    } else {
                        if let Some(after) = write.mutation.after {
                            let _ = after;
                            let reference = original.reference.ok_or_else(invalid)?;
                            let child = reference
                                .validate_binding(
                                    blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
                                    &key.semantic_key.ok_or_else(invalid)?,
                                    StateNamespaceV1::Authorization,
                                )
                                .map_err(|_| invalid())?;
                            if let Some(before) = write.mutation.before {
                                let oldprobe =
                                    oasis7_formal_protocol::incremental_state::StateEntryV1 {
                                        value: before,
                                        ..probe
                                    };
                                let old = row(&oldprobe, ctx, key_limit)?
                                    .reference
                                    .ok_or_else(invalid)?;
                                let oldroot = old
                                    .validate_binding(
                                        blake3::hash(b"runtime.rollback.outcome.v1").as_bytes(),
                                        &key.semantic_key.ok_or_else(invalid)?,
                                        StateNamespaceV1::Authorization,
                                    )
                                    .map_err(|_| invalid())?;
                                stream::advance_outcome(oldroot, child, ctx, diff_cells)?;
                            } else {
                                stream::qualify_outcome(child, ctx)?;
                            }
                        }
                        outcomes += i128::from(write.mutation.after.is_some())
                            - i128::from(write.mutation.before.is_some());
                    }
                }
            }
        }
        Ok(charge.retain(RollbackFamilyQualificationV1 {
            root_set_digest: record.after.digest().map_err(|_| invalid())?,
            registry_root: registry,
            class: MemoryClass::Ordinary,
            consumed_count: u64::try_from(consumed).map_err(|_| invalid())?,
            outcome_count: u64::try_from(outcomes).map_err(|_| invalid())?,
        }))
    })();
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(e) = take_state_error() {
        return Err(e.into());
    }
    result
}
