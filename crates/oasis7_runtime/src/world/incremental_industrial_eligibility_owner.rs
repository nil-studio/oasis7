//! Current owner observations for the incremental industrial eligibility path.
//!
//! These readers mirror the two original owner algorithms independently. A
//! returned value is retained business data only; it is not an authority
//! capability and cannot qualify a later transaction.
use super::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1;
use super::{
    IndustrialEligibilityErrorV1, IndustrialEligibilitySourcesV1, IndustrialEligibilityWorkV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_subtree::{CurrentAgentPresenceV1, read_current_agent_presence_v1},
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_artifact_resources::{
        RuntimeArtifactChildRootV1, RuntimeArtifactMemberProfileV1, RuntimeArtifactRecordV1,
        RuntimeArtifactValueV1, open_current_chain_resource_manifest_v1,
        read_runtime_artifact_member_v1,
    },
    incremental_runtime_revocation::{
        read_current_revocation_agent_identity_v1, resolve_manufactured_identity_generation_v1,
    },
    incremental_world_native::{
        recover_current_agent_claim_v1, recover_current_starter_oc_claim_v1,
    },
    incremental_world_number_maps::{
        WorldNumberMapFieldV1, WorldNumberMapKeyRefV1, WorldNumberV1,
        read_world_number_map_overlay_v1,
    },
    incremental_world_rule_composite_family::read_current_agent_order_v1,
    world_rules::AgentIndustrialPhaseV1,
};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as FmtWrite,
    io::{self, Write},
};

/// State owner fields consumed by the original production-order predicate.
/// The primary ID and generation are retained internally because Runtime's
/// identity algorithm uses them before the later production predicate.
pub(in crate::world) struct OwnedCurrentIndustrialStateOwnerV1 {
    pub(super) primary_agent_id: String,
    pub(super) player_id: String,
    pub(super) source_id: String,
    pub(super) public_key: Option<String>,
    pub(super) generation: u64,
}

#[derive(Clone, Copy)]
struct SelectedOrderIds<'a> {
    manufactured: bool,
    runtime_identity: Option<&'a str>,
    state_owner: Option<&'a str>,
}

#[derive(Clone, Copy)]
enum StateOwnerSourceIdV1<'a> {
    Starter { agent: &'a str, claimed_at: u64 },
    Manufactured(&'a str),
}

/// Iterate the authenticated, sorted Current directory in the same BTree key
/// order as `WorldState::agent_orders.values()`. Runtime identity deliberately
/// accepts any delivered timestamp; the state owner requires Delivered phase.
fn select_orders<'a>(
    orders: impl Iterator<Item = (&'a str, &'a str, AgentIndustrialPhaseV1, Option<u64>)>,
    target: &str,
    read_work: &mut IndustrialEligibilityReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<SelectedOrderIds<'a>, ValueTreeError> {
    let mut selected = SelectedOrderIds {
        manufactured: false,
        runtime_identity: None,
        state_owner: None,
    };
    for (id, target_agent_id, phase, delivered_at) in orders {
        read_work.cell()?;
        let text_bytes =
            id.len()
                .checked_add(target_agent_id.len())
                .ok_or(ValueTreeError::ResourceLimited(
                    "industrial owner order work",
                ))?;
        if id.len() as u64 > read_work.max_text_bytes
            || target_agent_id.len() as u64 > read_work.max_text_bytes
        {
            return Err(ValueTreeError::ResourceLimited(
                "industrial owner order text",
            ));
        }
        read_work.comparison_bytes_remaining = read_work
            .comparison_bytes_remaining
            .checked_sub(text_bytes as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "industrial owner order comparison",
            ))?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if target_agent_id == target {
            selected.manufactured = true;
            if delivered_at.is_some() {
                selected.runtime_identity.get_or_insert(id);
                if phase == AgentIndustrialPhaseV1::Delivered {
                    selected.state_owner.get_or_insert(id);
                }
            }
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(selected)
}

struct ManifestTextV1 {
    value: Option<String>,
    _allocation: MemoryReservation,
}

/// Point-read one registered manifest member using the child root opened from
/// this transaction. The fixed-root read context is intentionally scoped to
/// that child, never to the old outer artifacts namespace.
#[allow(clippy::too_many_arguments)]
fn manifest_member<M, R, E, C, P, V, ER, CR, K>(
    child: &ReservedValue<RuntimeArtifactChildRootV1>,
    key: &K,
    profile: RuntimeArtifactMemberProfileV1,
    required: bool,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ManifestTextV1>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    K: oasis7_world_state::incremental_semantic_key::SemanticKeyV1 + ?Sized,
{
    overlay.transaction.validate_guard(guard, live)?;
    if (overlay.cancelled)() || (parent_read.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    work.read.cell()?;
    let maximum = usize::try_from(work.read.max_text_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("industrial manifest text"))?;
    let allocation = overlay
        .values
        .memory
        .reserve(
            maximum
                .checked_add(std::mem::size_of::<String>())
                .ok_or(ValueTreeError::ResourceLimited("industrial manifest text"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(maximum)
        .map_err(|_| ValueTreeError::ResourceLimited("industrial manifest text"))?;
    if bytes.capacity() > maximum {
        return Err(ValueTreeError::ResourceLimited("industrial manifest text capacity").into());
    }
    let value = {
        let mut child_read = WorldFieldReadContextV1 {
            root: child.root(),
            state: parent_read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: parent_read.values.source,
                memory: overlay.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *parent_read.values.io,
            },
            key_work: &mut *parent_read.key_work,
            semantic_bytes: parent_read.semantic_bytes,
            record_bytes: parent_read.record_bytes,
            take_state_error: &mut parent_read.take_state_error,
            cancelled: &mut parent_read.cancelled,
        };
        read_runtime_artifact_member_v1(
            RuntimeArtifactRecordV1::ChainResourceManifest,
            child,
            key,
            profile,
            &mut child_read,
            work.read.max_text_bytes,
            &mut bytes,
        )?
    };
    if (overlay.cancelled)() || (parent_read.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let value = match value {
        Some(RuntimeArtifactValueV1::Text { utf8_bytes }) => {
            if usize::try_from(utf8_bytes).ok() != Some(bytes.len()) {
                return Err(ValueTreeError::Invalid("industrial manifest text length").into());
            }
            work.read.comparison_bytes_remaining = work
                .read
                .comparison_bytes_remaining
                .checked_sub(utf8_bytes)
                .ok_or(ValueTreeError::ResourceLimited(
                    "industrial manifest text work",
                ))?;
            Some(
                String::from_utf8(bytes)
                    .map_err(|_| ValueTreeError::Invalid("industrial manifest UTF-8"))?,
            )
        }
        Some(_) => {
            return Err(ValueTreeError::Invalid("industrial manifest member type").into());
        }
        None if !required => return Ok(None),
        None => return Err(ValueTreeError::Invalid("industrial manifest member missing").into()),
    };
    Ok(Some(ManifestTextV1 {
        value,
        _allocation: allocation,
    }))
}

struct HashWriter<'a, C> {
    hash: &'a mut Sha256,
    cancelled: C,
}
impl<C: FnMut() -> bool> Write for HashWriter<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if (self.cancelled)() {
            return Err(io::Error::other("industrial world binding cancelled"));
        }
        let written = bytes.len().min(1024);
        self.hash.update(&bytes[..written]);
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Hash the same JSON tuple as `World::world_rule_binding` without building an
/// intermediate byte vector. The six-times text bound covers JSON escaping of
/// every input byte; tuple punctuation and quotes are included separately.
fn world_rule_binding_digest(
    world_id: &str,
    chain_id: &str,
    genesis: &str,
    work: &mut u64,
    mut cancelled: impl FnMut() -> bool,
) -> Result<[u8; 32], ValueTreeError> {
    let text_bytes = "oasis7-world-rule-world-v1"
        .len()
        .checked_add(world_id.len())
        .and_then(|n| n.checked_add(chain_id.len()))
        .and_then(|n| n.checked_add(genesis.len()))
        .ok_or(ValueTreeError::ResourceLimited(
            "industrial world binding work",
        ))?;
    let output_bound = text_bytes
        .checked_mul(6)
        .and_then(|n| n.checked_add(16))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(ValueTreeError::ResourceLimited(
            "industrial world binding work",
        ))?;
    *work = work
        .checked_sub(output_bound)
        .ok_or(ValueTreeError::ResourceLimited(
            "industrial world binding work",
        ))?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let tuple = ("oasis7-world-rule-world-v1", world_id, chain_id, genesis);
    let mut hash = Sha256::new();
    let serialized = {
        let mut writer = HashWriter {
            hash: &mut hash,
            cancelled: &mut cancelled,
        };
        serde_json::to_writer(&mut writer, &tuple)
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    serialized.map_err(|_| ValueTreeError::Invalid("industrial world binding JSON"))?;
    Ok(hash.finalize().into())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_industrial_world_binding_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    _state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<String>>, IndustrialEligibilityErrorV1>
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
        if overlay.values.class != MemoryClass::Ordinary
            || !overlay.transaction.uses_budget(overlay.values.memory)
            || !parent_read.values.memory.same_pool(overlay.values.memory)
            || !sources.artifacts.uses_budget(overlay.values.memory)
        {
            return Err(ValueTreeError::Invalid("industrial manifest pool/class").into());
        }
        let child =
            open_current_chain_resource_manifest_v1(sources.artifacts, overlay, guard, live)?;
        let guard = &overlay.transaction.guard();
        let world_id = manifest_member(
            &child,
            "world_id",
            RuntimeArtifactMemberProfileV1::Text,
            true,
            parent_read,
            overlay,
            work,
            guard,
            live,
        )?
        .ok_or(ValueTreeError::Invalid("industrial world id member"))?;
        let guard = &overlay.transaction.guard();
        let chain_id = manifest_member(
            &child,
            "chain_id",
            RuntimeArtifactMemberProfileV1::Text,
            true,
            parent_read,
            overlay,
            work,
            guard,
            live,
        )?
        .ok_or(ValueTreeError::Invalid("industrial chain id member"))?;
        // Boolean cells use a separate typed reader so the closed Option pair
        // is checked rather than inferring presence from the value cell alone.
        let guard = &overlay.transaction.guard();
        let present = match read_manifest_presence(&child, parent_read, overlay, work, guard, live)?
        {
            Some(value) => value,
            None => return Err(ValueTreeError::Invalid("industrial genesis presence").into()),
        };
        let guard = &overlay.transaction.guard();
        let genesis = manifest_member(
            &child,
            &("genesis_ref", "value"),
            RuntimeArtifactMemberProfileV1::Text,
            false,
            parent_read,
            overlay,
            work,
            guard,
            live,
        )?;
        // Original option_text writes a Bool plus a Text value ONLY for Some.
        // None is false+absent, never a Unit cell. Do not infer a missing
        // required Some payload as None, or accept an orphan false+value.
        if present != genesis.is_some() {
            return Err(ValueTreeError::Invalid("industrial genesis option pair").into());
        }
        let (Some(world_id), Some(chain_id), Some(genesis)) = (
            world_id.value.as_deref(),
            chain_id.value.as_deref(),
            genesis.as_ref().and_then(|text| text.value.as_deref()),
        ) else {
            return retain_world_binding(None, overlay.values.memory);
        };
        if world_id.is_empty() || chain_id.is_empty() || genesis.is_empty() {
            return retain_world_binding(None, overlay.values.memory);
        }
        let _output = overlay
            .values
            .memory
            .reserve(
                std::mem::size_of::<Option<String>>() + 64,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let digest = world_rule_binding_digest(
            world_id,
            chain_id,
            genesis,
            &mut work.read.comparison_bytes_remaining,
            || (overlay.cancelled)() || (parent_read.cancelled)(),
        )?;
        let mut binding = String::new();
        binding
            .try_reserve_exact(64)
            .map_err(|_| ValueTreeError::ResourceLimited("industrial world binding output"))?;
        if binding.capacity() > 64 {
            return Err(ValueTreeError::ResourceLimited(
                "industrial world binding output capacity",
            )
            .into());
        }
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in digest {
            binding.push(HEX[(byte >> 4) as usize] as char);
            binding.push(HEX[(byte & 0x0f) as usize] as char);
        }
        Ok(_output.retain(Some(binding)))
    })();
    poison_on_error(&mut overlay.transaction, result.is_err());
    result.map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn read_manifest_presence<M, R, E, C, P, V, ER, CR>(
    child: &ReservedValue<RuntimeArtifactChildRootV1>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    overlay: &WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<bool>, IndustrialEligibilityErrorV1>
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
    use oasis7_world_state::incremental_runtime_artifact_resources::RuntimeArtifactValueV1;
    overlay.transaction.validate_guard(guard, live)?;
    work.read.cell()?;
    let mut output = io::sink();
    let value = {
        let mut child_read = WorldFieldReadContextV1 {
            root: child.root(),
            state: parent_read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: parent_read.values.source,
                memory: overlay.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *parent_read.values.io,
            },
            key_work: &mut *parent_read.key_work,
            semantic_bytes: parent_read.semantic_bytes,
            record_bytes: parent_read.record_bytes,
            take_state_error: &mut parent_read.take_state_error,
            cancelled: &mut parent_read.cancelled,
        };
        read_runtime_artifact_member_v1(
            RuntimeArtifactRecordV1::ChainResourceManifest,
            child,
            &("genesis_ref", "present"),
            RuntimeArtifactMemberProfileV1::Boolean,
            &mut child_read,
            1,
            &mut output,
        )?
    };
    match value {
        Some(RuntimeArtifactValueV1::Bool(value)) => Ok(Some(value)),
        _ => Err(ValueTreeError::Invalid("industrial manifest option presence").into()),
    }
}

fn retain_world_binding(
    value: Option<String>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<ReservedValue<Option<String>>, IndustrialEligibilityErrorV1> {
    let fee = memory
        .reserve(std::mem::size_of::<Option<String>>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(value))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_current_industrial_identity_owner_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    agent: &str,
    state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<String>>, IndustrialEligibilityErrorV1>
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
    with_current_industrial_identity_v1(
        sources,
        agent,
        state,
        overlay,
        parent_read,
        work,
        guard,
        live,
        |identity, memory, work| match identity {
            Some((owner, _generation)) => retain_owner_binding(owner, memory, work),
            None => retain_world_binding(None, memory),
        },
    )
}

// All identity predicates and Current reads stay in this one fixed flow.
// The private callback only constructs a prepaid DATA output.
#[allow(clippy::too_many_arguments)]
fn with_current_industrial_identity_v1<M, R, E, C, P, V, ER, CR, OUT>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    agent: &str,
    state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    output: impl FnOnce(
        Option<(&str, u64)>,
        &oasis7_proto::memory_budget::MemoryBudget,
        &mut IndustrialEligibilityWorkV1,
    ) -> Result<OUT, IndustrialEligibilityErrorV1>,
) -> Result<OUT, IndustrialEligibilityErrorV1>
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
            &mut || (overlay.cancelled)() || (parent_read.cancelled)(),
        )?;
        if !selection.manufactured {
            let guard = &overlay.transaction.guard();
            let revocation = read_current_revocation_agent_identity_v1(
                agent,
                sources.revocation,
                overlay,
                parent_read,
                &mut work.scan,
                &mut work.revocation,
                guard,
                live,
            )?;
            let Some(identity) = revocation.identity() else {
                return output(None, overlay.values.memory, work);
            };
            return output(
                Some((identity.owner_binding.as_str(), identity.generation)),
                overlay.values.memory,
                work,
            );
        }
        let Some(order_id) = selection.runtime_identity else {
            return output(None, overlay.values.memory, work);
        };
        let guard = &overlay.transaction.guard();
        let order = read_current_agent_order_v1(
            sources.orders.source(),
            order_id,
            state,
            overlay,
            &mut work.scan,
            &mut work.typed,
            guard,
            live,
        )?
        .ok_or(ValueTreeError::Invalid(
            "selected Current identity order absent",
        ))?;
        if order.value().target_agent_id != agent || order.value().delivered_at.is_none() {
            return Err(ValueTreeError::Invalid("Current identity order index mismatch").into());
        }
        let guard = &overlay.transaction.guard();
        let binding = read_current_industrial_world_binding_v1(
            sources,
            state,
            overlay,
            parent_read,
            work,
            guard,
            live,
        )?;
        if order.value().quote.world_binding.as_deref() != binding.as_deref() {
            return output(None, overlay.values.memory, work);
        }
        let guard = &overlay.transaction.guard();
        let owner = resolve_current_industrial_state_owner_inner(
            &CurrentStateOwnerSourcesV1::from_industrial(sources),
            agent,
            overlay,
            parent_read,
            work,
            guard,
            live,
        )?;
        let Some(owner) = owner.as_ref() else {
            return output(None, overlay.values.memory, work);
        };
        let guard = &overlay.transaction.guard();
        let root_identity = read_current_revocation_agent_identity_v1(
            &owner.primary_agent_id,
            sources.revocation,
            overlay,
            parent_read,
            &mut work.scan,
            &mut work.revocation,
            guard,
            live,
        )?;
        if root_identity
            .identity()
            .is_none_or(|identity| identity.owner_binding != owner.player_id)
        {
            return output(None, overlay.values.memory, work);
        }
        let guard = &overlay.transaction.guard();
        let target_identity = read_current_revocation_agent_identity_v1(
            agent,
            sources.revocation,
            overlay,
            parent_read,
            &mut work.scan,
            &mut work.revocation,
            guard,
            live,
        )?;
        let Some(generation) = resolve_manufactured_identity_generation_v1(
            target_identity.identity(),
            target_identity.acquisition_sequence(),
            owner.generation,
        ) else {
            return output(None, overlay.values.memory, work);
        };
        output(
            Some((&owner.player_id, generation)),
            overlay.values.memory,
            work,
        )
    })();
    poison_on_error(&mut overlay.transaction, result.is_err());
    result.map_err(Into::into)
}

fn retain_owner_binding(
    value: &str,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &mut IndustrialEligibilityWorkV1,
) -> Result<ReservedValue<Option<String>>, IndustrialEligibilityErrorV1> {
    work.read.comparison_bytes_remaining = work
        .read
        .comparison_bytes_remaining
        .checked_sub(value.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "industrial owner output work",
        ))?;
    if value.len() as u64 > work.read.max_text_bytes {
        return Err(ValueTreeError::ResourceLimited("industrial owner output text").into());
    }
    let fee = memory
        .reserve(
            std::mem::size_of::<Option<String>>()
                .checked_add(value.len())
                .ok_or(ValueTreeError::ResourceLimited("industrial owner output"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| ValueTreeError::ResourceLimited("industrial owner output"))?;
    if output.capacity() > value.len() {
        return Err(ValueTreeError::ResourceLimited("industrial owner output capacity").into());
    }
    output.push_str(value);
    Ok(fee.retain(Some(output)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_current_industrial_state_owner_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    agent: &str,
    state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>, IndustrialEligibilityErrorV1>
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
    let result = resolve_current_industrial_state_owner_inner(
        &CurrentStateOwnerSourcesV1::from_industrial(sources),
        agent,
        overlay,
        parent_read,
        work,
        guard,
        live,
    );
    poison_on_error(&mut overlay.transaction, result.is_err());
    result.map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn resolve_current_industrial_state_owner_inner<M, R, E, C, P, V, ER, CR>(
    sources: &CurrentStateOwnerSourcesV1<'_, '_>,
    agent: &str,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>, IndustrialEligibilityErrorV1>
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
    overlay.transaction.validate_guard(guard, live)?;
    sources.orders.validate_current(overlay, guard, live)?;
    let guard = &overlay.transaction.guard();
    let presence = read_current_agent_presence_v1(
        agent,
        sources.agents,
        sources.roots,
        overlay,
        parent_read,
        &mut work.pending_entries_remaining,
        guard,
        live,
    )?;
    if presence != CurrentAgentPresenceV1::Existing {
        return retain_state_owner(None, overlay.values.memory);
    }
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
        &mut || (overlay.cancelled)() || (parent_read.cancelled)(),
    )?;
    if !selection.manufactured {
        let starter = {
            let mut claims = WorldFieldReadContextV1 {
                root: sources.roots.state_root(StateNamespaceV1::Authorization),
                state: parent_read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: parent_read.values.source,
                    memory: overlay.values.memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *parent_read.values.io,
                },
                key_work: &mut *parent_read.key_work,
                semantic_bytes: parent_read.semantic_bytes,
                record_bytes: parent_read.record_bytes,
                take_state_error: &mut parent_read.take_state_error,
                cancelled: &mut parent_read.cancelled,
            };
            let guard = &overlay.transaction.guard();
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
        let Some(starter) = starter else {
            return retain_state_owner(None, overlay.values.memory);
        };
        let claim = starter.value().value();
        if claim.agent_id != agent || claim.player_id.is_empty() {
            return retain_state_owner(None, overlay.values.memory);
        }
        let output = state_owner_from_claim(
            agent,
            &claim.player_id,
            claim.public_key.as_deref(),
            StateOwnerSourceIdV1::Starter {
                agent,
                claimed_at: claim.claimed_at,
            },
            1,
            overlay,
            work,
        )?;
        return Ok(output);
    }
    if selection.state_owner.is_none() {
        return retain_state_owner(None, overlay.values.memory);
    }
    let claim = {
        let mut claims = WorldFieldReadContextV1 {
            root: sources.roots.state_root(StateNamespaceV1::Business),
            state: parent_read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: parent_read.values.source,
                memory: overlay.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *parent_read.values.io,
            },
            key_work: &mut *parent_read.key_work,
            semantic_bytes: parent_read.semantic_bytes,
            record_bytes: parent_read.record_bytes,
            take_state_error: &mut parent_read.take_state_error,
            cancelled: &mut parent_read.cancelled,
        };
        let guard = &overlay.transaction.guard();
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
    let Some(claim) = claim else {
        return retain_state_owner(None, overlay.values.memory);
    };
    let claim = claim.value().value();
    let Some(source) = claim.control_source.as_ref() else {
        return retain_state_owner(None, overlay.values.memory);
    };
    if claim.target_agent_id != agent
        || claim.claim_owner_id != source.root_agent_id
        || source.acquired_event_digest.len() != 64
        || source.acquisition_sequence == 0
    {
        return retain_state_owner(None, overlay.values.memory);
    }
    let sequence = read_world_number_map_overlay_v1(
        WorldNumberMapFieldV1::AgentClaimAcquisitionSequences,
        WorldNumberMapKeyRefV1::Text(agent),
        overlay,
    )?;
    if sequence != Some(WorldNumberV1::Unsigned(source.acquisition_sequence)) {
        return retain_state_owner(None, overlay.values.memory);
    }
    let root = {
        let mut claims = WorldFieldReadContextV1 {
            root: sources.roots.state_root(StateNamespaceV1::Authorization),
            state: parent_read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: parent_read.values.source,
                memory: overlay.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *parent_read.values.io,
            },
            key_work: &mut *parent_read.key_work,
            semantic_bytes: parent_read.semantic_bytes,
            record_bytes: parent_read.record_bytes,
            take_state_error: &mut parent_read.take_state_error,
            cancelled: &mut parent_read.cancelled,
        };
        let guard = &overlay.transaction.guard();
        recover_current_starter_oc_claim_v1(
            &source.root_agent_id,
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
    let Some(root) = root else {
        return retain_state_owner(None, overlay.values.memory);
    };
    let root = root.value().value();
    if root.agent_id != source.root_agent_id
        || root.player_id != source.player_id
        || root.claimed_at != source.root_claimed_at
        || root.player_id.is_empty()
        || root.public_key.as_ref().is_none_or(String::is_empty)
    {
        return retain_state_owner(None, overlay.values.memory);
    }
    let guard = &overlay.transaction.guard();
    let root_presence = read_current_agent_presence_v1(
        &source.root_agent_id,
        sources.agents,
        sources.roots,
        overlay,
        parent_read,
        &mut work.pending_entries_remaining,
        guard,
        live,
    )?;
    if root_presence != CurrentAgentPresenceV1::Existing {
        return retain_state_owner(None, overlay.values.memory);
    }
    let output = state_owner_from_claim(
        &source.root_agent_id,
        &source.player_id,
        root.public_key.as_deref(),
        StateOwnerSourceIdV1::Manufactured(&source.acquired_event_digest),
        source.acquisition_sequence,
        overlay,
        work,
    )?;
    Ok(output)
}

fn state_owner_from_claim<M, R, E, C>(
    primary_agent_id: &str,
    player_id: &str,
    public_key: Option<&str>,
    source: StateOwnerSourceIdV1<'_>,
    generation: u64,
    overlay: &WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityWorkV1,
) -> Result<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let source_id_len = match source {
        StateOwnerSourceIdV1::Starter { agent, claimed_at } => "starter_claim:"
            .len()
            .checked_add(agent.len())
            .and_then(|n| n.checked_add(1 + decimal_digits(claimed_at)))
            .ok_or(ValueTreeError::ResourceLimited(
                "industrial state owner source",
            ))?,
        StateOwnerSourceIdV1::Manufactured(digest) => "manufactured_claim:"
            .len()
            .checked_add(digest.len())
            .ok_or(ValueTreeError::ResourceLimited(
                "industrial state owner source",
            ))?,
    };
    let text_bytes = primary_agent_id
        .len()
        .checked_add(player_id.len())
        .and_then(|n| n.checked_add(source_id_len))
        .and_then(|n| n.checked_add(public_key.map_or(0, str::len)))
        .ok_or(ValueTreeError::ResourceLimited(
            "industrial state owner output",
        ))?;
    let text_bytes_u64 = u64::try_from(text_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("industrial state owner work"))?;
    work.read.comparison_bytes_remaining = work
        .read
        .comparison_bytes_remaining
        .checked_sub(text_bytes_u64)
        .ok_or(ValueTreeError::ResourceLimited(
            "industrial state owner work",
        ))?;
    if text_bytes as u64 > work.read.max_text_bytes.saturating_mul(4) {
        return Err(ValueTreeError::ResourceLimited("industrial state owner text").into());
    }
    let bytes = std::mem::size_of::<OwnedCurrentIndustrialStateOwnerV1>()
        .checked_add(text_bytes)
        .ok_or(ValueTreeError::ResourceLimited(
            "industrial state owner output",
        ))?;
    let fee = overlay
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let source_id = match source {
        StateOwnerSourceIdV1::Starter { agent, claimed_at } => {
            let mut output = String::new();
            output
                .try_reserve_exact(source_id_len)
                .map_err(|_| ValueTreeError::ResourceLimited("industrial state owner source"))?;
            if output.capacity() > source_id_len {
                return Err(ValueTreeError::ResourceLimited(
                    "industrial state owner source capacity",
                )
                .into());
            }
            write!(&mut output, "starter_claim:{agent}:{claimed_at}")
                .map_err(|_| ValueTreeError::Invalid("industrial state owner source"))?;
            output
        }
        StateOwnerSourceIdV1::Manufactured(digest) => {
            let mut output = String::new();
            output
                .try_reserve_exact(source_id_len)
                .map_err(|_| ValueTreeError::ResourceLimited("industrial state owner source"))?;
            if output.capacity() > source_id_len {
                return Err(ValueTreeError::ResourceLimited(
                    "industrial state owner source capacity",
                )
                .into());
            }
            write!(&mut output, "manufactured_claim:{digest}")
                .map_err(|_| ValueTreeError::Invalid("industrial state owner source"))?;
            output
        }
    };
    if source_id.len() != source_id_len {
        return Err(ValueTreeError::Invalid("industrial state owner source length").into());
    }
    let output = OwnedCurrentIndustrialStateOwnerV1 {
        primary_agent_id: copy_text(primary_agent_id)?,
        player_id: copy_text(player_id)?,
        source_id,
        public_key: public_key.map(copy_text).transpose()?,
        generation,
    };
    Ok(fee.retain(Some(output)))
}

fn decimal_digits(value: u64) -> usize {
    if value == 0 {
        1
    } else {
        value.ilog10() as usize + 1
    }
}

fn copy_text(value: &str) -> Result<String, ValueTreeError> {
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| ValueTreeError::ResourceLimited("industrial owner text"))?;
    if output.capacity() > value.len() {
        return Err(ValueTreeError::ResourceLimited(
            "industrial owner text capacity",
        ));
    }
    output.push_str(value);
    Ok(output)
}

fn retain_state_owner(
    value: Option<OwnedCurrentIndustrialStateOwnerV1>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<ReservedValue<Option<OwnedCurrentIndustrialStateOwnerV1>>, IndustrialEligibilityErrorV1>
{
    let fee = memory
        .reserve(
            std::mem::size_of::<Option<OwnedCurrentIndustrialStateOwnerV1>>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(value))
}

fn poison_on_error<M: MerkleNodeReader>(
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    error: bool,
) {
    if error {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
}

#[cfg(test)]
#[path = "incremental_industrial_eligibility_owner_tests.rs"]
mod tests;

#[path = "incremental_current_state_owner.rs"]
mod current_state_owner;
pub(in crate::world) use current_state_owner::{
    CurrentStateOwnerSourcesV1, resolve_current_state_owner_v1,
};

#[path = "incremental_safe_alternative_identity.rs"]
pub(in crate::world) mod safe_alternative_identity;
