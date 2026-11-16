//! Process-local authority source. Neither serialized World data nor an absent
//! optional pin selects this mode. GEN capture is emitted only by the full
//! borrowed World producer; cold configuration requires explicit verification.
use super::{
    DevLocalParticipantAuthority, ReleaseSecurityPolicy, World,
    incremental_body::BodyMutationError, incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError, VerifiedAuthorityV1,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental::authority_binding_bounded,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;
#[derive(Debug)]
pub struct NormalGenesisProcessSourceV1 {
    local_pin: Option<[u8; 32]>,
    policy: ReleaseSecurityPolicy,
    cognition_world: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessMode {
    Governed,
    DevLocal([u8; 32]),
}
pub struct NormalProcessSourceV1 {
    mode: ProcessMode,
    policy: ReleaseSecurityPolicy,
    world: String,
    genesis: String,
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
}
pub struct VerifiedNormalColdProcessConfigurationV1 {
    mode: ProcessMode,
    policy: ReleaseSecurityPolicy,
    world: String,
    genesis: String,
    authority: oasis7_formal_protocol::incremental_execution::AuthorityBinding,
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
}
pub enum NormalProcessStartupSourceV1<'a> {
    GenesisGoverned(&'a ReservedValue<NormalGenesisProcessSourceV1>),
    GenesisDevLocal {
        genesis: &'a ReservedValue<NormalGenesisProcessSourceV1>,
        authority: &'a DevLocalParticipantAuthority,
    },
    Cold(&'a ReservedValue<VerifiedNormalColdProcessConfigurationV1>),
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("normal actual Host process authority source")
}
fn formal(e: FormalError) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
fn copy(s: &str) -> Result<String, BodyMutationError> {
    let mut v = String::new();
    v.try_reserve_exact(s.len()).map_err(|_| bad())?;
    if v.capacity() > s.len() {
        return Err(bad());
    }
    v.push_str(s);
    Ok(v)
}
fn pin(authority: &DevLocalParticipantAuthority) -> Result<[u8; 32], BodyMutationError> {
    let mut p = [0u8; 32];
    hex::decode_to_slice(authority.operator_public_key(), &mut p).map_err(|_| bad())?;
    Ok(p)
}
#[derive(Debug)]
struct CaptureError {
    error: super::WorldError,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.error)
    }
}
impl std::error::Error for CaptureError {}
/// Private entry: only the same full typed GEN World producer may call it.
pub(super) fn capture_genesis_process_v1(
    world: &World,
    remaining: &mut u64,
    bytes_remaining: &mut u64,
    memory: &MemoryBudget,
) -> Result<ReservedValue<NormalGenesisProcessSourceV1>, ValueTreeError> {
    *remaining = remaining
        .checked_sub(64)
        .ok_or(ValueTreeError::ResourceLimited("GEN process source work"))?;
    let error_fee = memory.reserve(4096, MemoryClass::Ordinary)?;
    if let Err(error) = world.validate_devlocal_participant_authority_context() {
        return Err(ValueTreeError::Io(std::io::Error::other(CaptureError {
            error,
            _fee: error_fee,
        })));
    }
    let local_pin = world
        .devlocal_participant_authority
        .as_ref()
        .map(|authority| {
            let mut out = [0u8; 32];
            hex::decode_to_slice(authority.operator_public_key(), &mut out)
                .map_err(|_| ValueTreeError::Invalid("GEN actual process operator pin"))?;
            Ok::<_, ValueTreeError>(out)
        })
        .transpose()?;
    let cognition_world = world
        .cognition
        .get("runtime_binding")
        .and_then(|b| b.get("world_id"))
        .and_then(serde_json::Value::as_str);
    if cognition_world.is_some_and(|w| w.is_empty() || w.len() > 256) {
        return Err(ValueTreeError::Invalid(
            "GEN process Runtime world identity",
        ));
    }
    let bytes =
        std::mem::size_of::<NormalGenesisProcessSourceV1>() + cognition_world.map_or(0, str::len);
    *bytes_remaining =
        bytes_remaining
            .checked_sub(bytes as u64 + 128)
            .ok_or(ValueTreeError::ResourceLimited(
                "GEN process source copy bytes",
            ))?;
    let fee = memory.reserve(bytes, MemoryClass::Ordinary)?;
    let cognition_world = cognition_world
        .map(|s| copy(s).map_err(|_| ValueTreeError::Invalid("GEN process world copy")))
        .transpose()?;
    Ok(fee.retain(NormalGenesisProcessSourceV1 {
        local_pin,
        policy: world.release_security_policy.clone(),
        cognition_world,
    }))
}
fn actual_world<'a>(
    parent: &'a BoundNormalTickParentV1<'_>,
) -> &'a oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1 {
    &parent.refs().head.record().world
}
fn check_governed(
    authority: &VerifiedAuthorityV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<oasis7_formal_protocol::incremental_execution::AuthorityBinding, BodyMutationError> {
    common.consume_steps(4).map_err(formal)?;
    if authority.initial_body().is_none() {
        return Err(bad());
    }
    authority_binding_bounded(authority, common).map_err(formal)
}
impl NormalProcessSourceV1 {
    pub(in crate::world) fn release_security_policy_for_parent_v1<'a>(
        &'a self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<&'a ReleaseSecurityPolicy, BodyMutationError> {
        self.is_devlocal_for_current_parent_v1(parent, common)?;
        Ok(&self.policy)
    }

    pub(in crate::world) fn bind_startup_v1(
        source: NormalProcessStartupSourceV1<'_>,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Arc<ReservedValue<Self>>, BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        let identity = actual_world(parent);
        common
            .charge(
                16,
                identity
                    .world_id
                    .len()
                    .saturating_add(identity.genesis_digest.len()) as u64
                    + 8192,
            )
            .map_err(formal)?;
        let (mode, policy) = match source {
            NormalProcessStartupSourceV1::GenesisGoverned(genesis) => {
                if !genesis.uses_budget(common.memory())
                    || parent.refs().core.transition().is_some()
                    || genesis.local_pin.is_some()
                    || genesis
                        .cognition_world
                        .as_ref()
                        .is_some_and(|w| w != &identity.world_id)
                {
                    return Err(bad());
                }
                if check_governed(parent.refs().authority, common)? != parent.fence().authority {
                    return Err(bad());
                }
                (ProcessMode::Governed, genesis.policy.clone())
            }
            NormalProcessStartupSourceV1::GenesisDevLocal { genesis, authority } => {
                if !genesis.uses_budget(common.memory())
                    || parent.refs().core.transition().is_some()
                    || genesis.local_pin != Some(pin(authority)?)
                    || &genesis.policy != authority.release_security_policy()
                    || genesis
                        .cognition_world
                        .as_ref()
                        .is_some_and(|w| w != &identity.world_id)
                {
                    return Err(bad());
                }
                (
                    ProcessMode::DevLocal(pin(authority)?),
                    genesis.policy.clone(),
                )
            }
            NormalProcessStartupSourceV1::Cold(config) => {
                if !config.uses_budget(common.memory())
                    || !config.fee.uses_budget(common.memory())
                    || config.core != common.work().identity()
                    || config.world != identity.world_id
                    || config.genesis != identity.genesis_digest
                    || config.authority != parent.fence().authority
                {
                    return Err(bad());
                }
                (
                    match config.mode {
                        ProcessMode::Governed => ProcessMode::Governed,
                        ProcessMode::DevLocal(p) => ProcessMode::DevLocal(p),
                    },
                    config.policy.clone(),
                )
            }
        };
        let bytes = std::mem::size_of::<Self>()
            + std::mem::size_of::<ReservedValue<Self>>()
            + 2 * std::mem::size_of::<usize>()
            + 4 * std::mem::align_of::<ReservedValue<Self>>()
            + identity.world_id.len()
            + identity.genesis_digest.len();
        let outer = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(());
        Ok(Arc::new(outer.retain(Self {
            mode,
            policy,
            world: copy(&identity.world_id)?,
            genesis: copy(&identity.genesis_digest)?,
            core: common.work().identity(),
            fee,
        })))
    }
    pub(in crate::world) fn is_devlocal_for_current_parent_v1(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, BodyMutationError> {
        parent.validate_context(common).map_err(formal)?;
        let identity = actual_world(parent);
        common
            .charge(
                1,
                (identity.world_id.len() + identity.genesis_digest.len()) as u64,
            )
            .map_err(formal)?;
        if self.world != identity.world_id
            || self.genesis != identity.genesis_digest
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(matches!(self.mode, ProcessMode::DevLocal(_)))
    }
    pub(in crate::world) fn same_configuration_v1(
        &self,
        other: &Self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .charge(
                1,
                (self.world.len() + other.world.len() + self.genesis.len() + other.genesis.len())
                    as u64
                    + 128,
            )
            .map_err(formal)?;
        if self.mode != other.mode
            || self.policy != other.policy
            || self.world != other.world
            || self.genesis != other.genesis
            || self.core != common.work().identity()
            || other.core != self.core
            || !self.fee.uses_budget(common.memory())
            || !other.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
}

fn cold_output(
    mode: ProcessMode,
    policy: &ReleaseSecurityPolicy,
    authority: &VerifiedAuthorityV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, BodyMutationError> {
    let world = &authority.definition().world;
    common
        .charge(
            8,
            (world.world_id.len() + world.genesis_digest.len()) as u64 + 8192,
        )
        .map_err(formal)?;
    let binding = authority_binding_bounded(authority, common).map_err(formal)?;
    let bytes = std::mem::size_of::<VerifiedNormalColdProcessConfigurationV1>()
        + world.world_id.len()
        + world.genesis_digest.len();
    let outer = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<VerifiedNormalColdProcessConfigurationV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?
        .retain(());
    Ok(outer.retain(VerifiedNormalColdProcessConfigurationV1 {
        mode,
        policy: policy.clone(),
        world: copy(&world.world_id)?,
        genesis: copy(&world.genesis_digest)?,
        authority: binding,
        core: common.work().identity(),
        fee,
    }))
}
/// Explicit Formal startup branch. A missing local pin is not an argument.
pub fn prepare_verified_normal_cold_governed_configuration_v1(
    authority: &VerifiedAuthorityV1,
    policy: &ReleaseSecurityPolicy,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, BodyMutationError> {
    check_governed(authority, common)?;
    cold_output(ProcessMode::Governed, policy, authority, common)
}
#[derive(Debug)]
struct ColdConfigurationRefusal {
    message: String,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl std::fmt::Display for ColdConfigurationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ColdConfigurationRefusal {}
/// Independently pinned Host configuration and its original signed startup
/// policy. This never takes a mode flag or infers authority from World roots.
pub fn prepare_verified_normal_cold_devlocal_configuration_v1(
    local: &DevLocalParticipantAuthority,
    signed: &crate::service_admission::SignedReadRequest<
        crate::service_admission::participant_policy::DevLocalParticipantPolicyAuthorizationV1,
    >,
    authority: &VerifiedAuthorityV1,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, BodyMutationError> {
    use crate::service_admission::participant_policy::devlocal_policy_signing_domain;
    let identity = &authority.definition().world;
    let comparison_bytes = [
        local.operator_public_key().len(),
        signed.subject_public_key.len(),
        signed.request.policy.world_id.len(),
        identity.world_id.len(),
        signed.request.policy.genesis_digest.len(),
        identity.genesis_digest.len(),
    ]
    .into_iter()
    .try_fold(0u64, |n, b| n.checked_add(b as u64))
    .ok_or_else(bad)?;
    common.charge(4, comparison_bytes).map_err(formal)?;
    if local.release_security_policy().is_production_hardened()
        || signed.subject_public_key != local.operator_public_key()
        || signed.request.policy.world_id != identity.world_id
        || signed.request.policy.genesis_digest != identity.genesis_digest
    {
        return Err(bad());
    }
    // Preserve the original outer object-domain cap before signature parsing.
    // Cryptographic signing-image encoding/verification below runs inside the
    // actual continuing SDK meter, not inside an exterior estimate.
    let _wire = oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
        &signed.request,
        1 << 20,
        common,
        "normal process signed policy bound",
        "normal process signed policy encode",
    )
    .map_err(formal)?;
    crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter| {
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
            devlocal_policy_signing_domain(signed.request.policy.generation),
            &signed.request,&signed.subject_public_key,&signed.signature_hex,meter,
        )
    })?.map_err(|error|BodyMutationError::Original(Box::new(error)))?;
    cold_output(
        ProcessMode::DevLocal(pin(local)?),
        local.release_security_policy(),
        authority,
        common,
    )
}

/// Refinement of an independently verified private Host pin. The Chain caller
/// must carry its unforgeable CLI-success stamp; Runtime still binds the actual
/// verified authority/world before this config can enter a resident owner.
pub fn prepare_verified_normal_cold_devlocal_pinned_configuration_v1(
    local: &DevLocalParticipantAuthority,
    authority: &VerifiedAuthorityV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<VerifiedNormalColdProcessConfigurationV1>, BodyMutationError> {
    common.charge(4, 64).map_err(formal)?;
    if local.release_security_policy().is_production_hardened() {
        return Err(bad());
    }
    cold_output(
        ProcessMode::DevLocal(pin(local)?),
        local.release_security_policy(),
        authority,
        common,
    )
}

/// Readonly projection facts minted only from the actual Host process source.
/// No caller flag/JSON/absence can select a provider security mode.
pub(in crate::world) struct PaidProjectionProcessFactsV1 {
    hardened: bool,
    local_finality: bool,
    fence: oasis7_formal_protocol::incremental_execution::ExecutionFence,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl PaidProjectionProcessFactsV1 {
    pub(in crate::world) fn validate_for_tx_v1<
        M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    >(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.charge(1, 64).map_err(formal)?;
        if tx.parent_fence() != self.fence
            || !tx.uses_budget(common.memory())
            || self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn production_hardened(&self) -> bool {
        self.hardened
    }
    pub(in crate::world) fn allow_local_finality_signing(&self) -> bool {
        self.local_finality
    }
}
impl NormalProcessSourceV1 {
    pub(in crate::world) fn capture_projection_facts_v1(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<PaidProjectionProcessFactsV1, BodyMutationError> {
        let policy = self.release_security_policy_for_parent_v1(parent, common)?;
        let n = std::mem::size_of::<PaidProjectionProcessFactsV1>();
        common.charge(4, n as u64).map_err(formal)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(PaidProjectionProcessFactsV1 {
            hardened: policy.is_production_hardened(),
            local_finality: policy.allow_local_finality_signing,
            fence: parent.fence(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            fee,
        })
    }
}
