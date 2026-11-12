//! Actual Current projection of `World::current_agent_owner` for SafeAlternative.
//! Nested under the industrial owner so its fixed point readers are shared.
//! This retained projection is DATA and has no admission constructor.
use super::*;
use oasis7_world_state::incremental_runtime_revocation::{
    RuntimeRevocationCurrentV1, read_current_participant_owner_data_v1,
};

pub(in crate::world) struct CurrentSafeAlternativeOwnerV1 {
    pub(in crate::world) player: String,
    pub(in crate::world) public_key: Option<String>,
}

fn retain_owner_projection(
    player: Option<(&str, Option<&str>)>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &mut IndustrialEligibilityWorkV1,
) -> Result<ReservedValue<Option<CurrentSafeAlternativeOwnerV1>>, IndustrialEligibilityErrorV1> {
    let text = player
        .map_or(Some(0), |(p, k)| p.len().checked_add(k.map_or(0, str::len)))
        .ok_or(ValueTreeError::ResourceLimited(
            "SafeAlternative owner text",
        ))?;
    work.read.comparison_bytes_remaining = work
        .read
        .comparison_bytes_remaining
        .checked_sub(
            u64::try_from(text)
                .map_err(|_| ValueTreeError::ResourceLimited("SafeAlternative owner text"))?,
        )
        .ok_or(ValueTreeError::ResourceLimited(
            "SafeAlternative owner copy",
        ))?;
    let bytes = std::mem::size_of::<Option<CurrentSafeAlternativeOwnerV1>>()
        .checked_add(text)
        .ok_or(ValueTreeError::ResourceLimited(
            "SafeAlternative owner layout",
        ))?;
    let fee = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    fn copy(s: &str) -> Result<String, ValueTreeError> {
        let mut out = String::with_capacity(s.len());
        if out.capacity() > s.len() {
            return Err(ValueTreeError::ResourceLimited(
                "SafeAlternative owner capacity",
            ));
        }
        out.push_str(s);
        Ok(out)
    }
    let out = player
        .map(|(p, k)| {
            Ok::<_, ValueTreeError>(CurrentSafeAlternativeOwnerV1 {
                player: copy(p)?,
                public_key: k.map(copy).transpose()?,
            })
        })
        .transpose()?;
    Ok(fee.retain(out))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_safe_alternative_owner_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    current: &RuntimeRevocationCurrentV1,
    agent: &str,
    state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<CurrentSafeAlternativeOwnerV1>>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    let result = (|| {
        overlay.transaction.validate_guard(guard, live)?;
        // The private derived index selects the first original map key, exactly
        // as the old bindings.values().find(), including duplicate agent IDs.
        let participant = read_current_participant_owner_data_v1(
            agent,
            sources.revocation,
            current,
            overlay,
            read,
            &mut work.scan,
            &mut work.revocation,
            guard,
            live,
        )?;
        if let Some(receipt) = participant.receipt() {
            let Some(policy) = participant.policy() else {
                return retain_owner_projection(None, overlay.values.memory, work);
            };
            let guard = &overlay.transaction.guard();
            let identity =
                super::safe_alternative_identity::read_current_safe_alternative_identity_v1(
                    sources, agent, state, overlay, read, work, guard, live,
                )?;
            let guard = &overlay.transaction.guard();
            let presence = read_current_agent_presence_v1(
                agent,
                sources.agents,
                sources.roots,
                overlay,
                read,
                &mut work.pending_entries_remaining,
                guard,
                live,
            )?;
            let guard = &overlay.transaction.guard();
            sources.orders.validate_current(overlay, guard, live)?;
            let selection = select_orders(
                sources.orders.iter_order_indices().map(|(id, index)| {
                    (
                        id,
                        index.target_agent_id(),
                        index.phase(),
                        index.delivered_at(),
                    )
                }),
                agent,
                &mut work.read,
                &mut || (overlay.cancelled)() || (read.cancelled)(),
            )?;
            let guard = &overlay.transaction.guard();
            let child =
                open_current_chain_resource_manifest_v1(sources.artifacts, overlay, guard, live)?;
            let guard = &overlay.transaction.guard();
            let world = manifest_member(
                &child,
                "world_id",
                RuntimeArtifactMemberProfileV1::Text,
                true,
                read,
                overlay,
                work,
                guard,
                live,
            )?
            .ok_or(ValueTreeError::Invalid("SafeAlternative world id"))?;
            let guard = &overlay.transaction.guard();
            let present = read_manifest_presence(&child, read, overlay, work, guard, live)?;
            let guard = &overlay.transaction.guard();
            let genesis = manifest_member(
                &child,
                &("genesis_ref", "value"),
                RuntimeArtifactMemberProfileV1::Text,
                false,
                read,
                overlay,
                work,
                guard,
                live,
            )?;
            if presence != CurrentAgentPresenceV1::Existing
                || selection.manufactured
                || world.value.as_deref() != Some(receipt.world_id.as_str())
                || present != Some(true)
                || genesis.as_ref().and_then(|g| g.value.as_deref())
                    != Some(receipt.genesis_digest.as_str())
                || !receipt
                    .identity_namespace
                    .same_authority(&policy.identity_namespace)
                || !participant.namespace_was_authorized()
                || identity
                    .as_ref()
                    .is_none_or(|i| i.owner_binding != receipt.owner_binding)
                || identity.as_ref().map(|i| i.generation) != Some(receipt.owner_generation.0)
            {
                return retain_owner_projection(None, overlay.values.memory, work);
            }
            let guard = &overlay.transaction.guard();
            let claim = {
                let mut claims = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(StateNamespaceV1::Business),
                    state: read.state,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: read.values.source,
                        memory: overlay.values.memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: &mut read.cancelled,
                };
                recover_current_agent_claim_v1(
                    agent,
                    sources.native,
                    sources.roots,
                    overlay.transaction,
                    &mut claims,
                    &mut work.scan,
                    &mut work.typed,
                    guard,
                    live,
                )?
            };
            if claim.as_ref().is_some_and(|c| {
                let c = c.value().value();
                c.claim_owner_id != agent || c.control_source.is_some()
            }) {
                return retain_owner_projection(None, overlay.values.memory, work);
            }
            let guard = &overlay.transaction.guard();
            let starter = {
                let mut claims = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(StateNamespaceV1::Authorization),
                    state: read.state,
                    values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                        source: read.values.source,
                        memory: overlay.values.memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: &mut read.cancelled,
                };
                recover_current_starter_oc_claim_v1(
                    agent,
                    sources.native,
                    sources.roots,
                    overlay.transaction,
                    &mut claims,
                    &mut work.scan,
                    &mut work.typed,
                    guard,
                    live,
                )?
            };
            if starter.as_ref().is_some_and(|s| {
                let s = s.value().value();
                s.agent_id != agent
                    || s.player_id != receipt.owner_binding
                    || s.public_key.as_ref() != Some(&receipt.owner_public_key)
            }) {
                return retain_owner_projection(None, overlay.values.memory, work);
            }
            return retain_owner_projection(
                Some((&receipt.owner_binding, Some(&receipt.owner_public_key))),
                overlay.values.memory,
                work,
            );
        }
        let narrow = CurrentStateOwnerSourcesV1::from_industrial(sources);
        let guard = &overlay.transaction.guard();
        let owner = resolve_current_industrial_state_owner_inner(
            &narrow, agent, overlay, read, work, guard, live,
        )?;
        let Some(owner) = owner.as_ref() else {
            return retain_owner_projection(None, overlay.values.memory, work);
        };
        let guard = &overlay.transaction.guard();
        sources.orders.validate_current(overlay, guard, live)?;
        let selection = select_orders(
            sources.orders.iter_order_indices().map(|(id, index)| {
                (
                    id,
                    index.target_agent_id(),
                    index.phase(),
                    index.delivered_at(),
                )
            }),
            agent,
            &mut work.read,
            &mut || (overlay.cancelled)() || (read.cancelled)(),
        )?;
        if selection.manufactured {
            let guard = &overlay.transaction.guard();
            let consistent = with_current_industrial_identity_v1(
                sources,
                agent,
                state,
                overlay,
                read,
                work,
                guard,
                live,
                |identity, _, _| Ok(identity.is_some_and(|(binding, _)| binding == owner.player_id)),
            )?;
            if !consistent {
                return retain_owner_projection(None, overlay.values.memory, work);
            }
        }
        retain_owner_projection(
            Some((&owner.player_id, owner.public_key.as_deref())),
            overlay.values.memory,
            work,
        )
    })();
    poison_on_error(overlay.transaction, result.is_err());
    result
}
