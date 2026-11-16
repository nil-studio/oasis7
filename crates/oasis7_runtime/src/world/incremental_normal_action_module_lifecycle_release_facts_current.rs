//! Fixed Current observers for the one shared release business kernel.
use super::super::current::{
    CurrentModuleMarketAgentV1, read_artifact_owner_v1, read_artifact_registered_v1,
    read_market_agent_v1,
};
use super::super::release_rules::*;
use super::super::*;
use crate::world::{
    incremental_module_invocation::AcceptedInvocationDirectoryV1,
    incremental_normal_action_factory_schedule::{
        FactoryScheduleReadErrorV1, with_module_lifecycle_read_io_v1,
    },
};
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_native::CurrentFactoryNativeRecordV1, state::ModuleReleaseRequestState,
};
type Error = FactoryScheduleReadErrorV1;
pub(in crate::world::incremental_normal_action_module_lifecycle) struct CurrentReleaseFactsPartsV1 {
    pub agents: [Option<CurrentModuleMarketAgentV1>; 2],
    pub request: Option<ReservedValue<CurrentFactoryNativeRecordV1<ModuleReleaseRequestState>>>,
    pub next_raw: Option<u64>,
    pub fees: Vec<MemoryReservation>,
    pub slots: MemoryReservation,
    pub output_fee: Option<MemoryReservation>,
    pub profiles: Vec<super::super::release_profiles_current::PaidCurrentReleaseProfilesV1>,
    pub shadows: Vec<super::super::release_shadow_current::PaidCurrentReleaseShadowHashV1>,
    pub instances: Vec<super::super::instance_current::ModuleInstanceFactsFeesV1>,
    pub epochs: Vec<std::sync::Arc<crate::world::PaidCurrentModuleEpochSnapshotV1>>,
}
pub(in crate::world::incremental_normal_action_module_lifecycle) struct CurrentModuleReleaseFactsV1<
    'a,
    's,
    't,
    'v,
    'm,
    'p,
    'b,
    'mw,
    M,
    P,
    R,
    E,
    C,
    PE,
    PC,
> {
    pub sources: &'a ModuleLifecycleSourcesV1<'s>,
    pub ctx: &'a mut WorldFieldOverlayContextV1<'t, 'v, 'm, M, R, E, C>,
    pub parent: &'a mut WorldFieldReadContextV1<'p, P, R, PE, PC>,
    pub factory: &'a mut FactoryScheduleWorkV1,
    pub module: &'a mut ModuleLifecycleWorkV1<'mw>,
    pub common: &'a mut FormalBoundedContextV1<'b>,
    pub directory: &'a ReservedValue<AcceptedInvocationDirectoryV1>,
    pub live: &'a ExecutionFence,
    held: CurrentReleaseFactsPartsV1,
}
impl<'a, 's, 't, 'v, 'm, 'p, 'b, 'mw, M, P, R, E, C, PE, PC>
    CurrentModuleReleaseFactsV1<'a, 's, 't, 'v, 'm, 'p, 'b, 'mw, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sources: &'a ModuleLifecycleSourcesV1<'s>,
        ctx: &'a mut WorldFieldOverlayContextV1<'t, 'v, 'm, M, R, E, C>,
        parent: &'a mut WorldFieldReadContextV1<'p, P, R, PE, PC>,
        factory: &'a mut FactoryScheduleWorkV1,
        module: &'a mut ModuleLifecycleWorkV1<'mw>,
        common: &'a mut FormalBoundedContextV1<'b>,
        directory: &'a ReservedValue<AcceptedInvocationDirectoryV1>,
        live: &'a ExecutionFence,
    ) -> Result<Self, Error> {
        if !ctx.values.memory.same_pool(common.memory()) {
            return Err(invalid().into());
        }
        let bytes = std::mem::size_of::<Self>()
            .checked_add(32 * std::mem::size_of::<MemoryReservation>())
            .and_then(|n| {
                n.checked_add(
                    2 * std::mem::size_of::<
                        super::super::release_profiles_current::PaidCurrentReleaseProfilesV1,
                    >(),
                )
            })
            .and_then(|n| {
                n.checked_add(
                    2 * std::mem::size_of::<
                        super::super::release_shadow_current::PaidCurrentReleaseShadowHashV1,
                    >(),
                )
            })
            .and_then(|n| {
                n.checked_add(
                    4 * std::mem::size_of::<
                        super::super::instance_current::ModuleInstanceFactsFeesV1,
                    >(),
                )
            })
            .ok_or_else(invalid)?;
        pay(common, bytes)?;
        let slots = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        pay(common, 16384)?;
        let output_fee = common
            .memory()
            .reserve(16384, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            sources,
            ctx,
            parent,
            factory,
            module,
            common,
            directory,
            live,
            held: CurrentReleaseFactsPartsV1 {
                agents: [None, None],
                request: None,
                next_raw: None,
                output_fee: Some(output_fee),
                fees: Vec::with_capacity(32),
                slots,
                profiles: Vec::with_capacity(2),
                shadows: Vec::with_capacity(2),
                instances: Vec::with_capacity(4),
                epochs: Vec::new(),
            },
        })
    }
    pub fn into_parts(self) -> CurrentReleaseFactsPartsV1 {
        self.held
    }
    fn retain_instance_facts(
        &mut self,
        fees: super::super::instance_current::ModuleInstanceFactsFeesV1,
    ) -> Result<(), Error> {
        if self.held.instances.len() == self.held.instances.capacity() {
            return Err(invalid().into());
        }
        self.held.instances.push(fees);
        Ok(())
    }
    fn hold(&mut self, fee: MemoryReservation) -> Result<(), Error> {
        if self.held.fees.len() == self.held.fees.capacity() {
            let cap = self
                .held
                .fees
                .capacity()
                .checked_mul(2)
                .ok_or_else(invalid)?;
            let bytes = cap
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .ok_or_else(invalid)?;
            pay(self.common, bytes)?;
            let mut replacement = self
                .common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            self.held.fees.reserve_exact(cap - self.held.fees.len());
            if self.held.fees.capacity() > cap {
                return Err(invalid().into());
            }
            self.held
                .slots
                .merge(&mut replacement)
                .map_err(BodyMutationError::Invalid)?;
        }
        self.held.fees.push(fee);
        Ok(())
    }
    /// Admission covers simultaneous original serializer graph and final kernel
    /// output/error copies. Only closed Action/Request DTOs call this method.
    pub fn reserve<T: serde::Serialize>(&mut self, value: &T) -> Result<(), Error> {
        let counted = super::super::governance_json::module_json_value_paid_v1(value, self.common)
            .map_err(json_error)?;
        let bytes = counted
            .fee
            .bytes()
            .checked_mul(8)
            .and_then(|n| n.checked_add(16384))
            .ok_or_else(invalid)?;
        self.common
            .charge(
                u64::try_from(bytes).map_err(|_| invalid())?,
                u64::try_from(bytes).map_err(|_| invalid())?,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = self
            .common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.hold(fee)?;
        let output = counted
            .fee
            .bytes()
            .checked_mul(4)
            .and_then(|n| n.checked_add(8192))
            .ok_or_else(invalid)?;
        pay(self.common, output)?;
        let mut extra = self
            .common
            .memory()
            .reserve(output, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.held
            .output_fee
            .as_mut()
            .ok_or_else(invalid)?
            .merge(&mut extra)
            .map_err(BodyMutationError::Invalid)?;
        drop(counted);
        Ok(())
    }
    fn clone_text(&mut self, value: &str) -> Result<String, Error> {
        let bytes = value
            .len()
            .checked_add(std::mem::size_of::<String>())
            .ok_or_else(invalid)?;
        pay(self.common, bytes)?;
        let fee = self
            .common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.hold(fee)?;
        Ok(value.to_owned())
    }
}
struct JsonCause {
    error: serde_json::Error,
    _fee: MemoryReservation,
}
impl std::fmt::Debug for JsonCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.error, f)
    }
}
impl std::fmt::Display for JsonCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.error, f)
    }
}
impl std::error::Error for JsonCause {}
fn json_error(
    error: super::super::governance_json::ModuleGovernanceJsonErrorV1,
) -> BodyMutationError {
    match error {
        super::super::governance_json::ModuleGovernanceJsonErrorV1::Body(e) => e,
        super::super::governance_json::ModuleGovernanceJsonErrorV1::Json { error, _fee } => {
            BodyMutationError::Original(Box::new(JsonCause { error, _fee }))
        }
    }
}
impl<M, P, R, E, C, PE, PC> sealed::Sealed
    for CurrentModuleReleaseFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
{
}
impl<M, P, R, E, C, PE, PC> ModuleReleaseBusinessFactsV1
    for CurrentModuleReleaseFactsV1<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    type Error = Error;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Error> {
        if self.held.agents.iter().flatten().any(|a| a.id == id) {
            return Ok(true);
        }
        let slot = self
            .held
            .agents
            .iter()
            .position(Option::is_none)
            .ok_or_else(invalid)?;
        let s = self.sources;
        let live = self.live;
        let agent = with_module_lifecycle_read_io_v1(
            s.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, factory, _| Ok(read_market_agent_v1(id, s, ctx, parent, factory, live)?),
        )?;
        let present = agent.is_some();
        self.held.agents[slot] = agent;
        Ok(present)
    }
    fn artifact_exists(&mut self, hash: &str) -> Result<bool, Error> {
        let s = self.sources;
        let live = self.live;
        with_module_lifecycle_read_io_v1(
            s.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| Ok(read_artifact_registered_v1(hash, ctx)?),
        )
    }
    fn owner(&mut self, hash: &str) -> Result<Option<String>, Error> {
        let s = self.sources;
        let owner = with_module_lifecycle_read_io_v1(
            s.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, module| Ok(read_artifact_owner_v1(hash, ctx, module.max_selected_heap)?),
        )?;
        let (value, fee) = owner.into_value_v1();
        self.hold(fee)?;
        Ok(value)
    }
    fn frozen_admission(&mut self, manifest: &crate::ModuleManifest) -> Result<(), Error> {
        use super::super::instance_rules::ModuleInstallFactsV1;
        let mut facts = super::super::instance_current::CurrentModuleInstanceFactsV1::new(
            self.sources,
            self.ctx,
            self.parent,
            self.factory,
            self.module,
            self.common,
            self.live,
            self.directory,
        )?;
        let result = facts.frozen_admission(manifest);
        let fees = facts.into_parts();
        // Keep opaque original fact owners without detaching their reservations.
        // A dedicated slot is added by the outer source holder below.
        self.retain_instance_facts(fees)?;
        result
    }
    fn request(&mut self, id: u64) -> Result<Option<ModuleReleaseRequestSnapshotV1>, Error> {
        let before = super::super::release_current::read_current_module_release_request_v1(
            id,
            self.sources,
            self.ctx,
            self.parent,
            self.factory,
            self.module,
            self.common,
            self.live,
        )?;
        let Some(value) = before.value() else {
            self.held.request = Some(before);
            return Ok(None);
        };
        self.reserve(value)?;
        let counted = super::super::governance_json::module_json_value_paid_v1(value, self.common)
            .map_err(json_error)?;
        let bytes = counted
            .fee
            .bytes()
            .checked_mul(2)
            .and_then(|n| n.checked_add(std::mem::size_of::<ModuleReleaseRequestState>()))
            .ok_or_else(invalid)?;
        pay(self.common, bytes)?;
        let fee = self
            .common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let snapshot = fee.retain(value.clone());
        drop(counted);
        self.held.request = Some(before);
        Ok(Some(ModuleReleaseRequestSnapshotV1::Current(snapshot)))
    }
    fn next_request_id(&mut self) -> Result<u64, Error> {
        let s = self.sources;
        let raw = with_module_lifecycle_read_io_v1(
            s.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| {
                Ok(oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(oasis7_world_state::incremental_world_u64::WorldU64FieldV1::NextModuleReleaseRequestId,ctx).map_err(overlay_error)?)
            },
        )?;
        self.held.next_raw = Some(raw);
        Ok(raw.max(1))
    }
    fn node_identity_present(&mut self, id: &str) -> Result<bool, Error> {
        let s = self.sources;
        with_module_lifecycle_read_io_v1(
            s.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, module| {
                let value=oasis7_world_state::incremental_world_text_maps::read_world_text_map_overlay_v1(oasis7_world_state::incremental_world_text_maps::WorldTextMapFieldV1::NodeIdentityBindings,id,ctx,module.max_selected_heap as u64,&mut std::io::sink()).map_err(overlay_error)?;
                Ok(value.is_some())
            },
        )
    }
    fn role_membership(
        &mut self,
        agent: &str,
        role: &str,
    ) -> Result<
        oasis7_world_state::incremental_authorization_mapsets::AuthorizationMembershipV1,
        Error,
    > {
        let s = self.sources;
        with_module_lifecycle_read_io_v1(
            s.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| {
                Ok(oasis7_world_state::incremental_authorization_mapsets::read_authorization_membership_overlay_v1(oasis7_world_state::incremental_authorization_mapsets::AuthorizationMapSetFieldV1::ModuleReleaseRoleBindings,agent,role,ctx).map_err(overlay_error)?)
            },
        )
    }
    fn profile_changes(
        &mut self,
        changes: &crate::ModuleProfileChanges,
    ) -> Result<Result<(), String>, Error> {
        let owner = super::super::release_profiles_current::validate_current_release_profiles_v1(
            changes,
            self.sources,
            self.ctx,
            self.parent,
            self.factory,
            self.module,
            self.common,
            self.live,
        )?;
        let result = match owner.result() {
            Ok(()) => Ok(()),
            Err(error) => Err(self.clone_text(error)?),
        };
        if self.held.profiles.len() == self.held.profiles.capacity() {
            return Err(invalid().into());
        }
        self.held.profiles.push(owner);
        Ok(result)
    }
    fn shadow_hash(
        &mut self,
        manifest: &crate::ModuleManifest,
        activate: bool,
        changes: &crate::ModuleProfileChanges,
    ) -> Result<Result<String, String>, Error> {
        self.reserve(&(manifest, changes))?;
        let owner = super::super::release_shadow_current::evaluate_current_release_shadow_hash_v1(
            manifest,
            activate,
            changes,
            self.sources,
            self.ctx,
            self.parent,
            self.factory,
            self.module,
            self.common,
            self.directory,
            self.live,
        )?;
        let result = match owner.result() {
            Ok(hash) => Ok(self.clone_text(hash)?),
            Err(error) => Err(self.clone_text(error)?),
        };
        if self.held.shadows.len() == self.held.shadows.capacity() {
            return Err(invalid().into());
        }
        self.held.shadows.push(owner);
        Ok(result)
    }
}
