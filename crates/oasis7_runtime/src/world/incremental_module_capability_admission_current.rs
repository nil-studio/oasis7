//! Complete fixed Current module-capability consumer; no supplied decision.
use super::*;
use crate::world::{
    incremental_authorization_root_current::{
        AuthorizationRootSourceReadWorkV1, CurrentAuthorizationProjectionV1, SourceError,
    },
    incremental_body::BodyMutationError,
    incremental_normal_action_factory_schedule::{
        FactoryScheduleReadErrorV1, FactoryScheduleWorkV1, with_module_lifecycle_read_io_v1,
    },
    incremental_normal_action_module_lifecycle::{ModuleLifecycleSourcesV1, ModuleLifecycleWorkV1},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_governance::{
        RecoveredRuntimeGovernanceRecordV1, RuntimeGovernanceKeyV1,
        read_current_governance_record_v1,
    },
    incremental_runtime_revocation::{
        ModuleAdmissionRevocationRequestV1 as Request, ModuleAdmissionRevocationValueV1 as Value,
    },
};
pub(in crate::world) struct ModuleCapabilityAdmissionSourcesV1<'a>{
    pub current:&'a CurrentAuthorizationProjectionV1<'a>,
    pub typed:&'a ReservedValue<oasis7_world_state::incremental_runtime_typed_complete::RuntimeTypedCompleteQualificationV1>,
    pub governance:&'a ReservedValue<oasis7_world_state::incremental_runtime_governance::RuntimeGovernanceQualificationV1>,
    pub proposals:&'a ReservedValue<oasis7_world_state::incremental_runtime_proposal::RuntimeProposalsQualificationV1>,
    pub native:&'a ReservedValue<oasis7_world_state::incremental_world_native::WorldNativeFieldsQualificationV1>,
    pub roots:&'a RootSetV1,
    pub legacy:&'a ReservedValue<oasis7_world_state::incremental_runtime_module_authorization::ModuleAuthorizationFamilyQualificationV1>,
    pub primitive:&'a ReservedValue<oasis7_world_state::incremental_owner_closure::WorldPrimitiveQualificationV1>,
}
pub(in crate::world) struct ModuleCapabilityAdmissionFeesV1 {
    fees: Vec<MemoryReservation>,
    slots: MemoryReservation,
    _layout: MemoryReservation,
}
pub(in crate::world) struct PaidModuleCapabilityAdmissionV1 {
    result: Result<(), WorldError>,
    fees: ModuleCapabilityAdmissionFeesV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
}
impl PaidModuleCapabilityAdmissionV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fees._layout.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn into_parts(
        self,
    ) -> (Result<(), WorldError>, ModuleCapabilityAdmissionFeesV1) {
        (self.result, self.fees)
    }
}
enum Error {
    World(WorldError),
    Body(BodyMutationError),
}
impl From<WorldError> for Error {
    fn from(e: WorldError) -> Self {
        Self::World(e)
    }
}
impl From<BodyMutationError> for Error {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<ValueTreeError> for Error {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
fn source(e: SourceError) -> BodyMutationError {
    match e {
        SourceError::Overlay(e) => overlay(e),
        SourceError::Execution(e) => e.into(),
        other => {
            #[derive(Debug)]
            struct Cause(SourceError);
            impl std::fmt::Display for Cause {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self.0)
                }
            }
            impl std::error::Error for Cause {}
            BodyMutationError::Original(Box::new(Cause(other)))
        }
    }
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Current module capability source")
}
struct Current<'a, 's, 't, 'v, 'm, 'p, 'b, 'mw, M, P, R, E, C, PE, PC> {
    sources: &'a ModuleLifecycleSourcesV1<'s>,
    ctx: &'a mut WorldFieldOverlayContextV1<'t, 'v, 'm, M, R, E, C>,
    parent: &'a mut WorldFieldReadContextV1<'p, P, R, PE, PC>,
    factory: &'a mut FactoryScheduleWorkV1,
    module: &'a mut ModuleLifecycleWorkV1<'mw>,
    common: &'a mut FormalBoundedContextV1<'b>,
    live: &'a ExecutionFence,
    held: ModuleCapabilityAdmissionFeesV1,
    bootstrap_canonical: Option<&'a mut oasis7_wasm_abi::CanonicalCborWorkV1>,
}
impl<M, P, R, E, C, PE, PC> sealed::Sealed
    for Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
{
}
impl<M, P, R, E, C, PE, PC> Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn hold(&mut self, fee: MemoryReservation) -> Result<(), Error> {
        if self.held.fees.len() == self.held.fees.capacity() {
            let cap = self.held.fees.capacity().checked_mul(2).ok_or_else(bad)?;
            let n = cap
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .ok_or_else(bad)?;
            self.common
                .charge(1, n as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let fee = self
                .common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            self.held.fees.reserve_exact(cap - self.held.fees.len());
            if self.held.fees.capacity() > cap {
                return Err(bad().into());
            }
            self.held.slots = fee;
        }
        self.held.fees.push(fee);
        Ok(())
    }
    fn selected(&mut self, request: Request<'_>) -> Result<ReservedValue<Value>, Error> {
        let s = self.sources.capability;
        s.current
            .read_module_admission_revocation_v1(
                request,
                s.typed,
                s.roots,
                self.ctx,
                self.parent,
                self.module.capability_authorization,
                self.common,
                self.live,
            )
            .map_err(source)
            .map_err(Into::into)
    }
    fn governance(
        &mut self,
        key: RuntimeGovernanceKeyV1,
    ) -> Result<
        Option<
            oasis7_world_state::incremental_runtime_governance::RecoveredColumnsV1<
                RecoveredRuntimeGovernanceRecordV1,
            >,
        >,
        Error,
    > {
        let s = self.sources.capability;
        let live = self.live;
        with_module_lifecycle_read_io_v1(
            s.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, factory, module| {
                let guard = ctx.transaction.guard();
                Ok(read_current_governance_record_v1(
                    s.governance,
                    s.roots,
                    key,
                    parent.state,
                    ctx,
                    &mut factory.lifecycle.native_scan,
                    module.max_selected_heap,
                    &guard,
                    live,
                )
                .map_err(overlay)?)
            },
        )
        .map_err(read_error)
    }
    fn local_certificate(
        &mut self,
        id: u64,
        counter: &crate::world::incremental_world_event_counter::WorldEventCounterV1,
    ) -> Result<GovernanceFinalityCertificate, Error> {
        use oasis7_world_state::incremental_runtime_artifact_config::*;
        let s = self.sources;
        let live = self.live;
        let allowed = with_module_lifecycle_read_io_v1(
            s.families.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, factory, _| {
                let body = open_artifact_config_body_overlay_v1(
                    ArtifactConfigSelectedBodyRequestV1 {
                        field: ArtifactConfigFieldV1::Security,
                        key: ArtifactConfigRecordKeyV1::Global,
                        family: s.families.config,
                        parent_roots: s.families.roots,
                        current: None,
                    },
                    parent.state,
                    ctx,
                    &mut factory.config_scan,
                    &mut factory.config,
                )
                .map_err(overlay)?
                .ok_or_else(bad)?;
                let guard = ctx.transaction.guard();
                let value = read_qualified_artifact_config_overlay_v1(
                    ArtifactConfigCellRequestV1 {
                        field: ArtifactConfigFieldV1::Security,
                        key: ArtifactConfigRecordKeyV1::Global,
                        address: ArtifactConfigAddressV1 {
                            section: "security",
                            ordinal: None,
                            inner_ordinal: None,
                            member: None,
                            column: "allow_local_finality_signing",
                        },
                        family: s.families.config,
                        qualification: &body,
                        max_payload_bytes: 1,
                    },
                    ctx,
                    &guard,
                    live,
                    &mut std::io::sink(),
                )
                .map_err(overlay)?;
                match &*value {
                    ArtifactConfigValueV1::Boolean(value) => Ok(*value),
                    _ => Err(bad().into()),
                }
            },
        )
        .map_err(read_error)?;
        if !allowed {
            return Err(WorldError::GovernancePolicyInvalid{reason:format!("apply_proposal local finality path is disabled by release policy proposal_id={id}")}.into());
        }
        let cap = s.capability;
        let present=with_module_lifecycle_read_io_v1(cap.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,_|{
            let mut read=WorldFieldReadContextV1{root:cap.roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),state:parent.state,values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
            let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_world_native::current_governance_finality_registry_present_v1(cap.native,cap.roots,ctx.transaction,&mut read,&mut factory.lifecycle.native_scan,&mut factory.lifecycle.native_read,&guard,live).map_err(overlay)?)
        }).map_err(read_error)?;
        if present {
            return Err(WorldError::GovernancePolicyInvalid{reason:"local finality certificate builder is unavailable when governance finality signer registry is configured".to_string()}.into());
        }
        let proposal=with_module_lifecycle_read_io_v1(cap.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,_,module|{
            let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_proposal::read_current_runtime_proposal_v1(id,cap.proposals,cap.roots,parent.state,ctx,module.proposal_scan,module.proposal_read,&guard,live).map_err(overlay)?)
        }).map_err(read_error)?;
        let proposal = proposal
            .value()
            .ok_or(WorldError::ProposalNotFound { proposal_id: id })?;
        let hash = match &proposal.status {
            ProposalStatus::Approved { manifest_hash, .. } => manifest_hash,
            other => {
                return Err(WorldError::ProposalInvalidState {
                    proposal_id: id,
                    expected: "approved".to_string(),
                    found: other.label(),
                }
                .into());
            }
        };
        self.record(hash)?;
        let height = counter
            .journal_height(self.ctx.transaction)?
            .checked_add(1)
            .ok_or_else(bad)?;
        let now = self.tick()?;
        let length=with_module_lifecycle_read_io_v1(cap.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,module|{
            let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_governance::read_current_governance_epoch_length_v1(cap.governance,cap.roots,parent.state,ctx,&mut factory.lifecycle.native_scan,module.max_selected_heap,&guard,live).map_err(overlay)?)
        }).map_err(read_error)?;
        let epoch = now / length.max(1);
        let selected = self.governance(RuntimeGovernanceKeyV1::Epoch(epoch))?;
        let fallback;
        let snapshot = match &selected {
            Some(value) => {
                let RecoveredRuntimeGovernanceRecordV1::EpochSnapshot(snapshot) = value.value()
                else {
                    return Err(bad().into());
                };
                snapshot
            }
            None => {
                let (records, length) = self.finality_records(3)?;
                let view =
                    crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1 {
                        registry: &records.registry,
                        admissions: &records.admissions,
                        rotations: &records.rotations,
                        bootstrap_rotations: &records.bootstrap_rotations,
                        epoch_length: length,
                    };
                // Admit the complete derived snapshot clone while its source graph is still held.
                self.record(&(
                    &records.registry,
                    &records.admissions,
                    &records.rotations,
                    &records.bootstrap_rotations,
                ))?;
                fallback = view.fallback_epoch_snapshot_v1(epoch, epoch);
                &fallback
            }
        };
        self.record(snapshot)?;
        // Covers the original three signing payloads, Ed25519 work, key seeds and output graph before allocation.
        let signing = hash
            .len()
            .checked_add(snapshot.validator_set_hash.len())
            .and_then(|n| n.checked_add(snapshot.stake_root.len()))
            .and_then(|n| n.checked_add(16384))
            .and_then(|n| n.checked_mul(4096))
            .ok_or_else(bad)?;
        self.work(signing)?;
        let fee = self
            .common
            .memory()
            .reserve(signing, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.hold(fee)?;
        crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1::build_local_certificate_v1(snapshot,id,hash,height,epoch).map_err(Into::into)
    }
    fn finality_records(
        &mut self,
        signature_count: usize,
    ) -> Result<
        (
            ReservedValue<
                oasis7_world_state::incremental_world_native::CurrentGovernanceFinalityRecordsV1,
            >,
            u64,
        ),
        Error,
    > {
        use oasis7_world_state::incremental_world_native::recover_current_governance_finality_records_v1;
        let s = self.sources.capability;
        let live = self.live;
        let records = with_module_lifecycle_read_io_v1(
            s.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, parent, factory, module| {
                let mut read = WorldFieldReadContextV1 {
                    root: s.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
                    ),
                    state: parent.state,
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
                let guard = ctx.transaction.guard();
                Ok(recover_current_governance_finality_records_v1(
                    s.native,
                    s.roots,
                    ctx.transaction,
                    &mut read,
                    &mut factory.lifecycle.native_scan,
                    &mut factory.lifecycle.native_read,
                    module.pending_entries,
                    &guard,
                    live,
                )
                .map_err(overlay)?)
            },
        )
        .map_err(read_error)?;
        let n = self.record(&(
            &records.registry,
            &records.admissions,
            &records.rotations,
            &records.bootstrap_rotations,
        ))?;
        let count = records
            .admissions
            .len()
            .checked_add(records.rotations.len())
            .and_then(|n| n.checked_add(records.bootstrap_rotations.len()))
            .and_then(|n| n.checked_add(signature_count + 1))
            .ok_or_else(bad)?;
        let cycles = count
            .checked_mul(count)
            .and_then(|c| c.checked_mul(n.checked_add(1)?))
            .and_then(|n| n.checked_mul(512))
            .ok_or_else(bad)?;
        self.work(cycles)?;
        let epoch_length=with_module_lifecycle_read_io_v1(s.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,factory,module|{
            let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_governance::read_current_governance_epoch_length_v1(s.governance,s.roots,parent.state,ctx,&mut factory.lifecycle.native_scan,module.max_selected_heap,&guard,live).map_err(overlay)?)
        }).map_err(read_error)?;
        Ok((records, epoch_length))
    }
    fn verify_certificate(
        &mut self,
        proposal_id: u64,
        approved_hash: &str,
        epoch: u64,
        certificate: &GovernanceFinalityCertificate,
    ) -> Result<(), Error> {
        let selected_snapshot = self.governance(RuntimeGovernanceKeyV1::Epoch(epoch))?;
        self.record(certificate)?;
        if let Some(selected) = &selected_snapshot {
            let RecoveredRuntimeGovernanceRecordV1::EpochSnapshot(snapshot) = selected.value()
            else {
                return Err(bad().into());
            };
            self.record(snapshot)?;
            crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1::validate_certificate_header_v1(snapshot,proposal_id,approved_hash,epoch,certificate)?;
        }
        let (records, epoch_length) = self.finality_records(certificate.signatures.len())?;
        let view = crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1 {
            registry: &records.registry,
            admissions: &records.admissions,
            rotations: &records.rotations,
            bootstrap_rotations: &records.bootstrap_rotations,
            epoch_length,
        };
        let fallback;
        let snapshot = match &selected_snapshot {
            Some(selected) => {
                let RecoveredRuntimeGovernanceRecordV1::EpochSnapshot(value) = selected.value()
                else {
                    return Err(bad().into());
                };
                value
            }
            None => {
                let now = self.tick()?;
                fallback = view.fallback_epoch_snapshot_v1(epoch, now / epoch_length.max(1));
                self.record(&fallback)?;
                &fallback
            }
        };
        view.validate_certificate_from_source_bound_v1(
            snapshot,
            proposal_id,
            approved_hash,
            epoch,
            certificate,
            self,
        )
    }
}
fn read_error(e: FactoryScheduleReadErrorV1) -> Error {
    match e {
        FactoryScheduleReadErrorV1::Body(e) => Error::Body(e),
        FactoryScheduleReadErrorV1::World(e) => Error::World(e),
    }
}
impl<'s, M, P, R, E, C, PE, PC> FactsV1<'s>
    for Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn scope_contains(
        &mut self,
        parent: &oasis7_wasm_abi::CapabilityScope,
        child: &oasis7_wasm_abi::CapabilityScope,
    ) -> Result<bool, Error> {
        if self.bootstrap_canonical.is_none() {
            return Ok(parent.contains_subset(child));
        }
        parent
            .contains_subset_with_fallible_work_v1(child, &mut |s, b| {
                self.common
                    .charge(s, b)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))
            })
            .map_err(Into::into)
    }
    fn validate_grant(&mut self, grant: &oasis7_wasm_abi::CapabilityGrantV2) -> Result<(), Error> {
        if self.bootstrap_canonical.is_none() {
            return grant.validate().map_err(|e| {
                WorldError::CapabilityAuthorizationDenied {
                    reason: format!("parent grant validation: {e}"),
                }
                .into()
            });
        }
        let result = grant.validate_with_fallible_work_v1(&mut |s, b| {
            self.common
                .charge(s, b)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))
        })?;
        result.map_err(|e| {
            WorldError::CapabilityAuthorizationDenied {
                reason: format!("parent grant validation: {e}"),
            }
            .into()
        })
    }
    fn grant_body_matches(
        &mut self,
        grant: &oasis7_wasm_abi::CapabilityGrantV2,
    ) -> Result<bool, Error> {
        match self.bootstrap_canonical.as_deref_mut() {
            Some(work) => crate::world::normal_bootstrap_current::hash::grant_hash_matches_v1(
                grant,
                &grant.canonical_body_hash,
                self.common,
                work,
            )
            .map_err(Into::into),
            None => grant.body_hash_matches().map_err(|e| {
                WorldError::CapabilityAuthorizationDenied {
                    reason: format!("parent grant body hash: {e}"),
                }
                .into()
            }),
        }
    }
    fn grant_id_matches(
        &mut self,
        grant: &oasis7_wasm_abi::CapabilityGrantV2,
    ) -> Result<bool, Error> {
        match self.bootstrap_canonical.as_deref_mut() {
            Some(work) => crate::world::normal_bootstrap_current::hash::grant_hash_matches_v1(
                grant,
                &grant.grant_id,
                self.common,
                work,
            )
            .map_err(Into::into),
            None => grant
                .expected_grant_id()
                .map(|v| v == grant.grant_id)
                .map_err(|e| {
                    WorldError::CapabilityAuthorizationDenied {
                        reason: format!("parent grant id hash: {e}"),
                    }
                    .into()
                }),
        }
    }

    type Error = Error;
    fn tick(&mut self) -> Result<u64, Error> {
        let s = self.sources.capability;
        with_module_lifecycle_read_io_v1(
            s.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, _| {
                s.primitive
                    .validate_binding(s.roots)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                Ok(
                    oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(
                        oasis7_world_state::incremental_world_u64::WorldU64FieldV1::Time,
                        ctx,
                    )
                    .map_err(overlay)?,
                )
            },
        )
        .map_err(read_error)
    }
    fn grant(&mut self, id: &str) -> Result<Option<GrantDataV1<'s>>, Error> {
        let s = self.sources.capability;
        s.current
            .read_module_admission_grant_v1(
                id,
                s.typed,
                s.roots,
                self.ctx,
                self.parent,
                self.module.capability_authorization,
                self.common,
                self.live,
            )
            .map(|v| v.map(GrantDataV1::Current))
            .map_err(source)
            .map_err(Into::into)
    }
    fn grant_exists(&mut self, id: &str) -> Result<bool, Error> {
        let s = self.sources.capability;
        let presence = s
            .current
            .module_admission_grant_presence_v1(
                id,
                s.typed,
                s.roots,
                self.ctx,
                self.parent,
                self.module.capability_authorization,
                self.common,
                self.live,
            )
            .map_err(source)?;
        Ok(presence.is_present())
    }
    fn root(&mut self) -> Result<(), Error> {
        let s = self.sources.capability;
        s.current
            .validate_module_admission_root_v1(
                s.typed,
                s.roots,
                self.ctx,
                self.parent,
                self.module.capability_authorization,
                self.common,
                self.live,
            )
            .map_err(source)??;
        Ok(())
    }
    fn legacy(&mut self, id: &str) -> Result<(), Error> {
        let s = self.sources.capability;
        let grant = s
            .current
            .read_module_admission_legacy_v1(
                id,
                s.legacy,
                s.roots,
                self.ctx,
                self.parent,
                self.module.capability_authorization,
                self.common,
                self.live,
            )
            .map_err(source)?
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!("module cap missing {id}"),
            })?;
        let tick = self.tick()?;
        if grant.is_expired(tick) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module cap expired {id}"),
            }
            .into());
        }
        Ok(())
    }
    fn authority(&mut self, id: &str) -> Result<Option<AuthorityDataV1<'s>>, Error> {
        let value = self.selected(Request::Authority(id))?;
        match &*value {
            Value::Authority(Some(_)) => Ok(Some(AuthorityDataV1::Current(value))),
            Value::Authority(None) => Ok(None),
            _ => Err(bad().into()),
        }
    }
    fn proof(&mut self, id: &str) -> Result<Option<ProofDataV1<'s>>, Error> {
        let value = self.selected(Request::Proof(id))?;
        match &*value {
            Value::Proof(Some(_)) => Ok(Some(ProofDataV1::Current(value))),
            Value::Proof(None) => Ok(None),
            _ => Err(bad().into()),
        }
    }
    fn epoch(&mut self) -> Result<u64, Error> {
        match &*self.selected(Request::Epoch)? {
            Value::Epoch(v) => Ok(*v),
            _ => Err(bad().into()),
        }
    }
    fn revoked(&mut self, id: &str) -> Result<bool, Error> {
        match &*self.selected(Request::Revoked(id))? {
            Value::Revoked(v) => Ok(*v),
            _ => Err(bad().into()),
        }
    }
    fn superseded(&mut self, id: &str) -> Result<bool, Error> {
        match &*self.selected(Request::Superseded(id))? {
            Value::Superseded(v) => Ok(*v),
            _ => Err(bad().into()),
        }
    }
    fn node(&mut self, id: &str) -> Result<Option<String>, Error> {
        use oasis7_world_state::incremental_world_text_maps::{
            WorldTextMapFieldV1, read_world_text_map_overlay_v1,
        };
        let s = self.sources.capability;
        let value = with_module_lifecycle_read_io_v1(
            s.roots,
            self.ctx,
            self.parent,
            self.factory,
            self.common,
            self.module,
            |ctx, _, _, module| {
                struct Count(usize);
                impl std::io::Write for Count {
                    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                        self.0 = self
                            .0
                            .checked_add(b.len())
                            .ok_or_else(|| std::io::Error::other("node key size"))?;
                        Ok(b.len())
                    }
                    fn flush(&mut self) -> std::io::Result<()> {
                        Ok(())
                    }
                }
                let mut count = Count(0);
                let first = read_world_text_map_overlay_v1(
                    WorldTextMapFieldV1::NodeIdentityBindings,
                    id,
                    ctx,
                    module.max_selected_heap as u64,
                    &mut count,
                )
                .map_err(overlay)?;
                if first.is_none() {
                    return Ok(None);
                }
                let n = count
                    .0
                    .checked_mul(3)
                    .and_then(|n| n.checked_add(4096))
                    .ok_or_else(bad)?;
                let fee = ctx
                    .values
                    .memory
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let mut text = Vec::with_capacity(count.0);
                let second = read_world_text_map_overlay_v1(
                    WorldTextMapFieldV1::NodeIdentityBindings,
                    id,
                    ctx,
                    module.max_selected_heap as u64,
                    &mut text,
                )
                .map_err(overlay)?;
                if first != second || text.len() != count.0 {
                    return Err(bad().into());
                }
                Ok(Some((String::from_utf8(text).map_err(|_| bad())?, fee)))
            },
        )
        .map_err(read_error)?;
        match value {
            Some((text, fee)) => {
                self.hold(fee)?;
                Ok(Some(text))
            }
            None => Ok(None),
        }
    }
    fn snapshot_exists(&mut self, epoch: u64) -> Result<bool, Error> {
        Ok(self
            .governance(RuntimeGovernanceKeyV1::Epoch(epoch))?
            .is_some())
    }
    fn proposal_hash(&mut self, id: u64) -> Result<Option<Result<String, ()>>, Error> {
        let s = self.sources.capability;
        let live = self.live;
        let proposal=with_module_lifecycle_read_io_v1(s.roots,self.ctx,self.parent,self.factory,self.common,self.module,|ctx,parent,_,module|{
            let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_proposal::read_current_runtime_proposal_v1(id,s.proposals,s.roots,parent.state,ctx,module.proposal_scan,module.proposal_read,&guard,live).map_err(overlay)?)
        }).map_err(read_error)?;
        let Some(proposal) = proposal.value() else {
            return Ok(None);
        };
        match &proposal.status {
            ProposalStatus::Approved { manifest_hash, .. }
            | ProposalStatus::Applied { manifest_hash } => {
                let n = manifest_hash.len().checked_add(4096).ok_or_else(bad)?;
                self.work(n)?;
                let fee = self
                    .common
                    .memory()
                    .reserve(n, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                self.hold(fee)?;
                Ok(Some(Ok(manifest_hash.clone())))
            }
            _ => Ok(Some(Err(()))),
        }
    }
    fn certificate(&mut self, certificate: &GovernanceFinalityCertificate) -> Result<(), Error> {
        self.verify_certificate(
            certificate.proposal_id,
            &certificate.manifest_hash,
            certificate.epoch_id,
            certificate,
        )
        .map_err(|e| match e {
            Error::World(e) => Error::World(deny(format!("authority finality certificate: {e:?}"))),
            Error::Body(e) => Error::Body(e),
        })
    }
    fn work(&mut self, bytes: usize) -> Result<(), Error> {
        self.common
            .charge(1, bytes as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(())
    }
    fn record<T: serde::Serialize>(&mut self, value: &T) -> Result<usize, Error> {
        struct Count<'a, 'b> {
            n: usize,
            common: &'a mut FormalBoundedContextV1<'b>,
            first: Option<BodyMutationError>,
        }
        impl std::io::Write for Count<'_, '_> {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if let Err(e) = self.common.charge(1, b.len() as u64) {
                    self.first = Some(BodyMutationError::Original(Box::new(e)));
                    return Err(std::io::Error::other("capability record work"));
                }
                self.n = self
                    .n
                    .checked_add(b.len())
                    .ok_or_else(|| std::io::Error::other("capability record size"))?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count {
            n: 0,
            common: self.common,
            first: None,
        };
        let result = serde_json::to_writer(&mut count, value);
        if let Some(error) = count.first {
            return Err(error.into());
        }
        result.map_err(|_| bad())?;
        let n = count.n;
        let heap = n
            .checked_mul(32)
            .and_then(|n| n.checked_add(65536))
            .ok_or_else(bad)?;
        self.work(heap)?;
        let fee = self
            .common
            .memory()
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.hold(fee)?;
        Ok(n)
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_current_inner<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    cap: &str,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidModuleCapabilityAdmissionV1, BodyMutationError> {
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    if !ctx.values.memory.same_pool(common.memory()) {
        return Err(bad());
    }
    let n = cap
        .len()
        .checked_mul(32)
        .and_then(|n| n.checked_add(std::mem::size_of::<PaidModuleCapabilityAdmissionV1>() + 16384))
        .ok_or_else(bad)?;
    common
        .charge(1, n as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let layout = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let slots = common
        .memory()
        .reserve(
            32 * std::mem::size_of::<MemoryReservation>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut current = Current {
        sources,
        ctx,
        parent,
        factory,
        module,
        common,
        live,
        bootstrap_canonical: None,
        held: ModuleCapabilityAdmissionFeesV1 {
            fees: Vec::with_capacity(32),
            slots,
            _layout: layout,
        },
    };
    let result = match admission(&mut current, cap) {
        Ok(v) => Ok(v),
        Err(Error::World(e)) => Err(e),
        Err(Error::Body(e)) => return Err(e),
    };
    let result = match result {
        Ok(v) => Ok(v),
        Err(e) => {
            // Actual record fees already retain the variable error graph; the cap
            // prefix was separately admitted before any family read.
            Err(WorldError::ModuleChangeInvalid {
                reason: format!("module cap admission failed {cap}: {e:?}"),
            })
        }
    };
    current
        .ctx
        .transaction
        .validate_guard(&current.ctx.transaction.guard(), live)?;
    Ok(PaidModuleCapabilityAdmissionV1 {
        result,
        guard: current.ctx.transaction.guard(),
        core: current.common.work().identity(),
        memory: current.common.memory().clone(),
        fees: current.held,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn validate_current_module_required_capability_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    cap: &str,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidModuleCapabilityAdmissionV1, BodyMutationError> {
    ctx.transaction.contribute(|tx| {
        let mut current = WorldFieldOverlayContextV1 {
            transaction: tx,
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
        validate_current_inner(
            cap,
            sources,
            &mut current,
            parent,
            factory,
            module,
            common,
            live,
        )
    })
}

/// Source verification only; governance approval policy remains with the caller's fixed driver.
pub(in crate::world) enum CurrentModuleGovernanceFinalityErrorV1 {
    Body(BodyMutationError),
    World {
        error: WorldError,
        fees: ModuleCapabilityAdmissionFeesV1,
    },
}
pub(in crate::world) struct PaidCurrentModuleGovernanceFinalityV1 {
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    proposal_id: u64,
    approved_hash: String,
    epoch: u64,
    fees: ModuleCapabilityAdmissionFeesV1,
}
impl PaidCurrentModuleGovernanceFinalityV1 {
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        id: u64,
        approved_hash: &str,
        epoch: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || !self.fees._layout.uses_budget(memory)
            || self.core != core
            || self.now != now
            || self.proposal_id != id
            || self.approved_hash != approved_hash
            || self.epoch != epoch
        {
            return Err(bad());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn validate_current_module_governance_finality_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    proposal_id: u64,
    approved_hash: &str,
    epoch: u64,
    certificate: &GovernanceFinalityCertificate,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentModuleGovernanceFinalityV1, CurrentModuleGovernanceFinalityErrorV1> {
    let result = ctx
        .transaction
        .contribute(|tx| -> Result<_, BodyMutationError> {
            tx.validate_guard(&tx.guard(), live)?;
            if !ctx.values.memory.same_pool(common.memory()) {
                return Err(bad());
            }
            let n = approved_hash
                .len()
                .checked_add(std::mem::size_of::<PaidCurrentModuleGovernanceFinalityV1>() + 16384)
                .ok_or_else(bad)?;
            common
                .charge(1, n as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let layout = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let slots = common
                .memory()
                .reserve(
                    32 * std::mem::size_of::<MemoryReservation>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: tx,
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
            let mut current = Current {
                sources,
                ctx: &mut overlay,
                parent,
                factory,
                module,
                common,
                live,
                bootstrap_canonical: None,
                held: ModuleCapabilityAdmissionFeesV1 {
                    fees: Vec::with_capacity(32),
                    slots,
                    _layout: layout,
                },
            };
            let business =
                match current.verify_certificate(proposal_id, approved_hash, epoch, certificate) {
                    Ok(()) => None,
                    Err(Error::World(error)) => Some(error),
                    Err(Error::Body(error)) => return Err(error),
                };
            if let Some(error) = business {
                return Ok(Err((error, current.held)));
            }
            let now = match current.tick() {
                Ok(now) => now,
                Err(Error::Body(error)) => return Err(error),
                Err(Error::World(error)) => return Ok(Err((error, current.held))),
            };
            current
                .ctx
                .transaction
                .validate_guard(&current.ctx.transaction.guard(), live)?;
            Ok(Ok(PaidCurrentModuleGovernanceFinalityV1 {
                guard: current.ctx.transaction.guard(),
                core: current.common.work().identity(),
                memory: current.common.memory().clone(),
                now,
                proposal_id,
                approved_hash: approved_hash.to_owned(),
                epoch,
                fees: current.held,
            }))
        })
        .map_err(CurrentModuleGovernanceFinalityErrorV1::Body)?;
    result.map_err(|(error, fees)| CurrentModuleGovernanceFinalityErrorV1::World { error, fees })
}

pub(in crate::world) struct PaidCurrentLocalModuleGovernanceFinalityV1 {
    certificate: GovernanceFinalityCertificate,
    fees: ModuleCapabilityAdmissionFeesV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
}
impl PaidCurrentLocalModuleGovernanceFinalityV1 {
    pub(in crate::world) fn certificate(&self) -> &GovernanceFinalityCertificate {
        &self.certificate
    }
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        id: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.core != core
            || self.now != now
            || self.certificate.proposal_id != id
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn into_parts(
        self,
    ) -> (
        GovernanceFinalityCertificate,
        ModuleCapabilityAdmissionFeesV1,
    ) {
        (self.certificate, self.fees)
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn build_current_local_module_governance_finality_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    proposal_id: u64,
    counter: &crate::world::incremental_world_event_counter::WorldEventCounterV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentLocalModuleGovernanceFinalityV1, CurrentModuleGovernanceFinalityErrorV1> {
    let result = ctx
        .transaction
        .contribute(|tx| -> Result<_, BodyMutationError> {
            tx.validate_guard(&tx.guard(), live)?;
            if !ctx.values.memory.same_pool(common.memory()) {
                return Err(bad());
            }
            let n = std::mem::size_of::<PaidCurrentLocalModuleGovernanceFinalityV1>() + 16384;
            common
                .charge(1, n as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let layout = common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let slots = common
                .memory()
                .reserve(
                    32 * std::mem::size_of::<MemoryReservation>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: tx,
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
            let mut current = Current {
                sources,
                ctx: &mut overlay,
                parent,
                factory,
                module,
                common,
                live,
                bootstrap_canonical: None,
                held: ModuleCapabilityAdmissionFeesV1 {
                    fees: Vec::with_capacity(32),
                    slots,
                    _layout: layout,
                },
            };
            let certificate = match current.local_certificate(proposal_id, counter) {
                Ok(value) => value,
                Err(Error::World(error)) => return Ok(Err((error, current.held))),
                Err(Error::Body(error)) => return Err(error),
            };
            let now = match current.tick() {
                Ok(now) => now,
                Err(Error::Body(error)) => return Err(error),
                Err(Error::World(error)) => return Ok(Err((error, current.held))),
            };
            current
                .ctx
                .transaction
                .validate_guard(&current.ctx.transaction.guard(), live)?;
            Ok(Ok(PaidCurrentLocalModuleGovernanceFinalityV1 {
                certificate,
                guard: current.ctx.transaction.guard(),
                core: current.common.work().identity(),
                memory: current.common.memory().clone(),
                now,
                fees: current.held,
            }))
        })
        .map_err(CurrentModuleGovernanceFinalityErrorV1::Body)?;
    result.map_err(|(error, fees)| CurrentModuleGovernanceFinalityErrorV1::World { error, fees })
}

/// Actual Current epoch authority data for the release attestation threshold.
/// This is neither an accepted release nor a finality certificate.
pub(in crate::world) struct PaidCurrentModuleEpochSnapshotV1 {
    snapshot: GovernanceFinalityEpochSnapshot,
    fees: ModuleCapabilityAdmissionFeesV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    actual_epoch: u64,
}
impl PaidCurrentModuleEpochSnapshotV1 {
    pub(in crate::world) fn snapshot(&self) -> &GovernanceFinalityEpochSnapshot {
        &self.snapshot
    }
    pub(in crate::world) fn now(&self) -> u64 {
        self.now
    }
    pub(in crate::world) fn epoch_id(&self) -> u64 {
        self.actual_epoch
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.core != core
            || self.now != now
            || !self.fees._layout.uses_budget(memory)
        {
            return Err(bad());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_module_epoch_snapshot_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentModuleEpochSnapshotV1, CurrentModuleGovernanceFinalityErrorV1> {
    let result=ctx.transaction.contribute(|tx|->Result<_,BodyMutationError>{
        tx.validate_guard(&tx.guard(),live)?;
        if !ctx.values.memory.same_pool(common.memory()){return Err(bad())}
        let n=std::mem::size_of::<PaidCurrentModuleEpochSnapshotV1>()+16384;
        common.charge(1,n as u64).map_err(|error|BodyMutationError::Original(Box::new(error)))?;
        let layout=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let slots=common.memory().reserve(32*std::mem::size_of::<MemoryReservation>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
        let mut overlay=WorldFieldOverlayContextV1{transaction:tx,values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};
        let mut current=Current{sources,ctx:&mut overlay,parent,factory,module,common,live,bootstrap_canonical:None,held:ModuleCapabilityAdmissionFeesV1{fees:Vec::with_capacity(32),slots,_layout:layout}};
        let make=|current:&mut Current<'_,'_,'_,'_,'_,'_,'_,'_,M,P,R,_,_,PE,PC>|->Result<_,Error>{
            let now=current.tick()?;
            let cap=current.sources.capability;
            let length=with_module_lifecycle_read_io_v1(cap.roots,current.ctx,current.parent,current.factory,current.common,current.module,|ctx,parent,factory,module|{
                let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_governance::read_current_governance_epoch_length_v1(cap.governance,cap.roots,parent.state,ctx,&mut factory.lifecycle.native_scan,module.max_selected_heap,&guard,live).map_err(self::overlay)?)
            }).map_err(read_error)?;
            let epoch=now/length.max(1);
            if let Some(selected)=current.governance(RuntimeGovernanceKeyV1::Epoch(epoch))? {
                let RecoveredRuntimeGovernanceRecordV1::EpochSnapshot(snapshot)=selected.value()else{return Err(bad().into())};
                current.record(snapshot)?;
                return Ok((snapshot.clone(),now,epoch));
            }
            let (records,length)=current.finality_records(0)?;
            // The original registry projection includes retained rotations and
            // the explicit legacy local fallback. Pay before its map copies.
            current.record(&(&records.registry,&records.admissions,&records.rotations,&records.bootstrap_rotations))?;
            let view=crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1{registry:&records.registry,admissions:&records.admissions,rotations:&records.rotations,bootstrap_rotations:&records.bootstrap_rotations,epoch_length:length};
            let snapshot=view.fallback_epoch_snapshot_v1(epoch,epoch);
            current.record(&snapshot)?;
            Ok((snapshot,now,epoch))
        };
        let (snapshot,now,actual_epoch)=match make(&mut current){Ok(value)=>value,Err(Error::World(error))=>return Ok(Err((error,current.held))),Err(Error::Body(error))=>return Err(error)};
        Ok(Ok(PaidCurrentModuleEpochSnapshotV1{snapshot,fees:current.held,guard:current.ctx.transaction.guard(),core:current.common.work().identity(),memory:current.common.memory().clone(),now,actual_epoch}))
    }).map_err(CurrentModuleGovernanceFinalityErrorV1::Body)?;
    result.map_err(|(error, fees)| CurrentModuleGovernanceFinalityErrorV1::World { error, fees })
}

#[path = "normal_bootstrap_current_capability.rs"]
mod bootstrap;
pub(in crate::world) use bootstrap::{
    BootstrapCapabilityCheckV1, verify_current_bootstrap_capability_v1,
};

#[path = "normal_bootstrap_current_policy.rs"]
mod bootstrap_policy;
pub(in crate::world) use bootstrap_policy::{
    CurrentBootstrapPolicyErrorV1, PaidCurrentBootstrapPolicyV1, read_current_bootstrap_policy_v1,
};

#[path = "normal_bootstrap_current_registry.rs"]
mod bootstrap_registry;
pub(in crate::world) use bootstrap_registry::{
    PaidCurrentBootstrapRegistryV1, read_current_bootstrap_registry_v1,
};
