//! One genuine terminal Current metadata observation shared by result and
//! projection production. No public fields or source constructor are exposed.
use super::*;
use crate::world::incremental_factory_phase::io::{
    FactoryPhaseMeterLaneV1 as Lane, FactoryPhaseMeterLanesV1,
};
use crate::world::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1;
use crate::world::incremental_normal_event_commitment::NormalEventCommitmentV1;
use crate::world::incremental_normal_industry_due::delivery::{
    ProductValidationDeliveryWorkV1, read_gameplay_journal_window_position_v1,
    read_gameplay_world_event_original_v1,
};
use oasis7_client_api::world_service::{
    FormalGovernanceMembershipV1, PublicMembershipCandidate, PublicMembershipRegistry,
};
use oasis7_world_state::{
    incremental_cognition_proof_ledger::{
        CognitionProofLedgerFieldV1, CurrentCognitionProofLedgerV1, ProofLedgerReadWorkV1,
        ProofLedgerRecoveredV1,
    },
    incremental_runtime_history_index::RuntimeHistoryIndexQualificationV1,
    incremental_runtime_revocation::{
        CurrentAuthorityGenerationV1, ModuleAdmissionRevocationRequestV1,
        ModuleAdmissionRevocationValueV1, read_current_authority_generation_v1,
        read_current_module_admission_revocation_v1,
    },
    incremental_world_native::{
        CurrentGovernanceRegistryV1, CurrentGovernanceRowsV1, WorldNativeSelectedReadWorkV1,
        read_current_governance_registry_v1, read_current_governance_rows_v1,
    },
};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    mem::size_of,
};
pub(in crate::world) struct RuntimeAuthorityAliasesV1 {
    pub(in crate::world) world_id: String,
    pub(in crate::world) branch_id: String,
    pub(in crate::world) finality_epoch: u64,
    pub(in crate::world) finality_block_hash: Option<String>,
    pub(in crate::world) finality_status: String,
    pub(in crate::world) reorg_epoch: u64,
    pub(in crate::world) runtime_manifest_hash: String,
}
struct GovernancePartsV1 {
    epoch: u64,
    candidates: BTreeMap<String, PublicMembershipCandidate>,
    selected: BTreeMap<String, String>,
    registry: Option<PublicMembershipRegistry>,
    digest: String,
    _digest_fee: MemoryReservation,
}
pub struct NormalTerminalReadMetadataV1 {
    aliases: RuntimeAuthorityAliasesV1,
    binding: ProofLedgerRecoveredV1<serde_json::Value>,
    authority: CurrentAuthorityGenerationV1,
    permission: ReservedValue<ModuleAdmissionRevocationValueV1>,
    governance_rows: ReservedValue<CurrentGovernanceRowsV1>,
    registry: ReservedValue<CurrentGovernanceRegistryV1>,
    governance: GovernancePartsV1,
    binding_layout: usize,
    governance_layout: usize,
    clock: u64,
    journal: oasis7_world_state::observation_projection::current_commitment_history::ProjectionJournalBoundsV1,
    latest_start: u64,
    retained: u64,
    physical_event_era: u64,
    physical_next_event_id: u64,
    manifest_hash: String,
    manifest_world: String,
    parent_record: [u8; 32],
    parent_fence: ExecutionFence,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _phase: MemoryReservation,
}
impl NormalTerminalReadMetadataV1 {
    pub(super) fn parent_record(&self) -> [u8; 32] {
        self.parent_record
    }
    pub(super) fn parent_fence(&self) -> ExecutionFence {
        self.parent_fence
    }
    pub(super) fn latest_start(&self) -> u64 {
        self.latest_start
    }
    pub(super) fn retained(&self) -> u64 {
        self.retained
    }
    pub(super) fn physical_event_era(&self) -> u64 {
        self.physical_event_era
    }
    pub fn provider_world_id(&self) -> &str {
        &self.aliases.world_id
    }
    pub fn branch_id(&self) -> &str {
        &self.aliases.branch_id
    }
    pub fn finality_epoch(&self) -> u64 {
        self.aliases.finality_epoch
    }
    pub fn finality_status(&self) -> &str {
        &self.aliases.finality_status
    }
    pub fn finality_block_hash(&self) -> Option<&str> {
        self.aliases.finality_block_hash.as_deref()
    }
    pub fn reorg_generation(&self) -> u64 {
        self.aliases.reorg_epoch
    }
    pub fn authority_generation(&self) -> u64 {
        self.authority.generation()
    }
    pub fn permission_generation(&self) -> u64 {
        match &*self.permission {
            ModuleAdmissionRevocationValueV1::Epoch(v) => *v,
            _ => unreachable!(),
        }
    }
    /// Actual allocator state, independent of the retained journal suffix.
    pub(in crate::world) fn next_event_id_v1(&self) -> u64 {
        self.physical_next_event_id
    }
    pub(in crate::world) fn next_event_id_era_v1(&self) -> u64 {
        self.physical_event_era
    }
    pub fn event_id_era(&self) -> u64 {
        self.journal.era
    }
    pub(in crate::world) fn journal_current_era_start_v1(&self) -> usize {
        self.journal.start
    }
    pub fn journal_retention_floor(&self) -> u64 {
        self.journal.floor
    }
    pub fn journal_high_watermark(&self) -> u64 {
        self.journal.high
    }
    pub(in crate::world) fn journal_retained_event_count_v1(&self) -> u64 {
        self.retained
    }
    pub fn current_clock(&self) -> u64 {
        self.clock
    }
    pub fn manifest_hash(&self) -> &str {
        &self.manifest_hash
    }
    pub(in crate::world) fn captured_guard(&self) -> &PreparedGuard {
        &self.guard
    }
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._phase.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._phase.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    /// Pure output DATA; only the enclosing actual-seal facade may select the
    /// real semantic-state hash. This method cannot turn its String into a source.
    pub fn governance_membership_for_state_hash_bounded_v1(
        &self,
        raw_state_hash: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<FormalGovernanceMembershipV1>, BodyMutationError> {
        let (value, fee) =
            self.governance_membership_parts_for_state_hash_bounded_v1(raw_state_hash, common)?;
        Ok(fee.retain(value))
    }
    /// Fixed ownership transfer for the original formal-header DATA. The
    /// caller retains this exact graph fee; no generic reservation detach exists.
    pub(in crate::world) fn governance_membership_parts_for_state_hash_bounded_v1(
        &self,
        raw_state_hash: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        (
            FormalGovernanceMembershipV1,
            oasis7_proto::memory_budget::MemoryReservation,
        ),
        BodyMutationError,
    > {
        self.validate_context(common)?;
        let bytes = self
            .governance_layout
            .checked_add(raw_state_hash.len())
            .ok_or_else(bad)?;
        common
            .charge(bytes as u64, bytes as u64)
            .map_err(original)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok((
            FormalGovernanceMembershipV1 {
                schema_version: 1,
                state_root_hash: raw_state_hash.to_owned(),
                governance_epoch: self.governance.epoch,
                candidates: self.governance.candidates.clone(),
                selected_candidates: self.governance.selected.clone(),
                effective_registry: self.governance.registry.clone(),
                effective_registry_digest: self.governance.digest.clone(),
            },
            fee,
        ))
    }
    pub fn runtime_binding_for_state_hash_bounded_v1(
        &self,
        raw_state_hash: &str,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<ReservedValue<oasis7_agent_api::RuntimeBindingV1>, BodyMutationError> {
        self.validate_context(common)?;
        crate::World::cognition_runtime_binding_from_current_fields_bounded_v1(
            self.binding.value(),
            &self.manifest_world,
            &self.manifest_hash,
            raw_state_hash,
            self.clock,
            common,
            canonical,
        )
    }
}
#[derive(Debug)]
struct MetadataWorldCause(crate::WorldError);
impl std::fmt::Display for MetadataWorldCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for MetadataWorldCause {}
pub struct NormalTerminalMetadataReadWorkV1 {
    pub canonical: oasis7_wasm_abi::CanonicalCborWorkV1,
    pub native: WorldNativeSelectedReadWorkV1,
    pub native_scan: StateScanBudget,
    pub native_cells: u64,
    pub pending_entries: usize,
    pub ledger: ProofLedgerReadWorkV1,
    pub ledger_scan: StateScanBudget,
    pub originals: ProductValidationDeliveryWorkV1,
    pub max_text_bytes: usize,
}
pub(super) fn admit_json_copy(
    value: &serde_json::Value,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(usize, MemoryReservation), BodyMutationError> {
    admit_serializable_phase(value, common)
}
pub(super) fn admit_serializable_phase<T: serde::Serialize>(
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(usize, MemoryReservation), BodyMutationError> {
    struct Count<'a, 'c> {
        common: &'a mut FormalBoundedContextV1<'c>,
        n: usize,
        first: Option<BodyMutationError>,
    }
    impl Write for Count<'_, '_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self.first.is_some() {
                return Err(io::ErrorKind::Other.into());
            }
            for chunk in b.chunks(8192) {
                if let Err(e) = self.common.charge(1, chunk.len() as u64) {
                    self.first = Some(original(e));
                    return Err(io::ErrorKind::Other.into());
                }
                self.n = match self.n.checked_add(chunk.len()) {
                    Some(n) => n,
                    None => {
                        self.first = Some(bad());
                        return Err(io::ErrorKind::Other.into());
                    }
                }
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Count {
        common,
        n: 0,
        first: None,
    };
    let result = serde_json::to_writer(&mut out, value);
    if let Some(e) = out.first {
        return Err(e);
    }
    result.map_err(original)?;
    let n = out.n;
    let bytes = n
        .checked_mul(128)
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or_else(bad)?;
    out.common
        .charge(bytes as u64, bytes as u64)
        .map_err(original)?;
    let fee = out
        .common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok((n, fee))
}
struct NativeLoan<'a, 'v, P, R, E, C> {
    read: &'a mut WorldFieldReadContextV1<'v, P, R, E, C>,
    work: &'a mut NormalTerminalMetadataReadWorkV1,
}
impl<P, R, E, C> FactoryPhaseMeterLanesV1 for NativeLoan<'_, '_, P, R, E, C> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(Lane<'a>)) {
        v(Lane::Steps(&mut self.read.values.io.nodes_remaining));
        v(Lane::Bytes(&mut self.read.values.io.read_bytes_remaining));
        v(Lane::Bytes(&mut self.read.values.io.write_bytes_remaining));
        v(Lane::Bytes(&mut self.read.key_work.bytes_remaining));
        let w = &mut self.work.native_scan;
        v(Lane::Steps(&mut w.nodes_remaining));
        v(Lane::Steps(&mut w.leaves_remaining));
        v(Lane::Bytes(&mut w.read_bytes_remaining));
        v(Lane::Steps(&mut self.work.native.json.cells_remaining));
        v(Lane::Steps(&mut self.work.native.json.json_nodes_remaining));
        v(Lane::Steps(&mut self.work.native_cells));
        v(Lane::Entries(&mut self.work.pending_entries));
    }
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn capture_normal_terminal_metadata_v1<
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        base: &ReservedValue<super::metadata_parent::NormalBusinessMetadataParentSourceV1>,
        revocation: &RuntimeRevocationCurrentV1,
        journal: &ReservedValue<RuntimeHistoryIndexQualificationV1>,
        events: &NormalEventCommitmentV1,
        manifest: &ReservedValue<CurrentWorldManifestHashV1>,
        ledger: Option<&CurrentCognitionProofLedgerV1>,
        read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        business: &mut NormalBusinessReadWorkV1,
        work: &mut NormalTerminalMetadataReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<NormalTerminalReadMetadataV1, RootModuleExecutionErrorV1<O::Error>> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            base.validate_parent(parent, common)?;
            let refs = parent.refs();
            let guard = self.ctx.transaction.guard();
            let live = live_owner();
            let authority = with_business_source_read_work_v1(
                refs.roots,
                self.ctx,
                read,
                &mut business.authorization,
                &mut business.processing,
                common,
                |ctx, read, w, processing| {
                    read_current_authority_generation_v1(
                        base.authority_generation(),
                        refs.storage.current().runtime().revocation(),
                        revocation,
                        ctx,
                        read,
                        &mut w.scan,
                        &mut w.typed,
                        processing,
                        &guard,
                        &live,
                    )
                    .map_err(SourceError::from)
                },
            )
            .map_err(source)?;
            let guard = self.ctx.transaction.guard();
            let live = live_owner();
            let permission = with_authorization_source_read_work_v1(
                refs.roots,
                self.ctx,
                read,
                &mut business.authorization,
                common,
                |ctx, read, w| {
                    read_current_module_admission_revocation_v1(
                        ModuleAdmissionRevocationRequestV1::Epoch,
                        refs.storage.current().runtime().revocation(),
                        ctx,
                        read,
                        &mut w.scan,
                        &mut w.typed,
                        &guard,
                        &live,
                    )
                    .map_err(SourceError::from)
                },
            )
            .map_err(source)?;
            if !matches!(&*permission, ModuleAdmissionRevocationValueV1::Epoch(_)) {
                return Err(bad().into());
            }
            let ledger = match ledger {
                Some(v) => v,
                None => {
                    return Err(note(
                        "cognition validation failed: runtime_binding_missing".into(),
                        common,
                    )?);
                }
            };
            let binding = match ledger.read_field(
                self.ctx.transaction,
                CognitionProofLedgerFieldV1::RuntimeBinding,
                read,
                &mut work.ledger_scan,
                &mut work.ledger,
                common,
                &live_owner(),
            )? {
                Some(v) => v,
                None => {
                    return Err(note(
                        "cognition validation failed: runtime_binding_missing".into(),
                        common,
                    )?);
                }
            };
            let (_, mut phase) = admit_json_copy(binding.value(), common)?;
            let binding_layout = phase.bytes();
            let mut loan = NativeLoan { read, work };
            let (world, mut world_fee) = self.with_factory_prologue_external_meter_v1(
                common,
                live_owner,
                &mut loan,
                |root, live, loan| {
                    Ok(read_manifest_world(
                        root.ctx,
                        loan.read,
                        refs.storage.current().runtime().resources(),
                        loan.work.max_text_bytes,
                        &live(),
                    )?)
                },
            )?;
            phase
                .merge(&mut world_fee)
                .map_err(BodyMutationError::Invalid)?;
            let hash = manifest.hash();
            common.charge(1, hash.len() as u64).map_err(original)?;
            let mut hash_fee = common
                .memory()
                .reserve(hash.len(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let manifest_hash = hash.to_owned();
            phase
                .merge(&mut hash_fee)
                .map_err(BodyMutationError::Invalid)?;
            let (
                world_id,
                branch_id,
                finality_epoch,
                finality_block_hash,
                finality_status,
                reorg_epoch,
                runtime_manifest_hash,
            ) = crate::World::cognition_runtime_authority_aliases_from_current_fields_v1(
                binding.value(),
                &world,
                &manifest_hash,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(MetadataWorldCause(e))))?;
            let aliases = RuntimeAuthorityAliasesV1 {
                world_id,
                branch_id,
                finality_epoch,
                finality_block_hash,
                finality_status,
                reorg_epoch,
                runtime_manifest_hash,
            };
            let (rows,registry,clock,epoch_length)=self.with_factory_prologue_external_meter_v1(common,live_owner,&mut loan,|root,live,loan|{
   let clock=oasis7_world_state::incremental_world_u64::read_world_u64_overlay_v1(oasis7_world_state::incremental_world_u64::WorldU64FieldV1::Time,root.ctx).map_err(current_scope::overlay)?;
   let guard=root.ctx.transaction.guard();let length=oasis7_world_state::incremental_runtime_governance::read_current_governance_epoch_length_v1(refs.storage.current().runtime().governance(),refs.roots,loan.read.state,root.ctx,&mut loan.work.native_scan,loan.work.native.max_tree_heap,&guard,&live()).map_err(current_scope::overlay)?;
   let mut current=WorldFieldReadContextV1{root:refs.roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),state:loan.read.state,values:oasis7_formal_protocol::incremental_value::ValueReadContext{source:loan.read.values.source,memory:loan.read.values.memory,class:loan.read.values.class,io:&mut*loan.read.values.io},key_work:&mut*loan.read.key_work,semantic_bytes:loan.read.semantic_bytes,record_bytes:loan.read.record_bytes,take_state_error:&mut loan.read.take_state_error,cancelled:&mut loan.read.cancelled};
   let guard=root.ctx.transaction.guard();let rows=read_current_governance_rows_v1(refs.storage.current().native().native(),refs.roots,root.ctx.transaction,&mut current,&mut loan.work.native_scan,&mut loan.work.native,&mut loan.work.native_cells,&mut loan.work.pending_entries,&guard,&live()).map_err(current_scope::overlay)?;
   let guard=root.ctx.transaction.guard();let registry=read_current_governance_registry_v1(refs.storage.current().native().native(),refs.roots,root.ctx.transaction,&mut current,&mut loan.work.native_scan,&mut loan.work.native,&guard,&live()).map_err(current_scope::overlay)?;
   Ok((rows,registry,clock,length))
  })?;
            let graph = rows
                .retained_allocation_bytes_v1()?
                .checked_add(registry.retained_allocation_bytes_v1()?)
                .and_then(|n| n.checked_mul(32))
                .ok_or_else(bad)?;
            common
                .charge(graph as u64, graph as u64)
                .map_err(original)?;
            let mut gov_fee = common
                .memory()
                .reserve(graph, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut admissions = BTreeMap::new();
            let mut rotations = BTreeMap::new();
            let mut bootstrap = BTreeMap::new();
            rows.visit_admissions::<ValueTreeError>(
                &mut work.native_cells,
                &mut self.ctx.cancelled,
                &mut |key, row, _| {
                    admissions.insert(key.to_owned(), row.clone());
                    Ok(())
                },
            )?;
            rows.visit_rotations::<ValueTreeError>(
                &mut work.native_cells,
                &mut self.ctx.cancelled,
                &mut |key, row, _| {
                    rotations.insert(key.to_owned(), row.clone());
                    Ok(())
                },
            )?;
            rows.visit_bootstrap_rotations::<ValueTreeError>(
                &mut work.native_cells,
                &mut self.ctx.cancelled,
                &mut |key, row, _| {
                    bootstrap.insert(key.to_owned(), row.clone());
                    Ok(())
                },
            )?;
            let epoch = clock / epoch_length.max(1);
            let records =
                crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1 {
                    registry: registry.registry(),
                    admissions: &admissions,
                    rotations: &rotations,
                    bootstrap_rotations: &bootstrap,
                    epoch_length,
                };
            let effective = records
                .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
                    &admissions,
                    epoch,
                )
                .map_err(|e| BodyMutationError::Original(Box::new(MetadataWorldCause(e))))?;
            let selected =
                crate::World::governance_effective_validator_admission_records(&admissions)
                    .into_iter()
                    .map(|(node, record)| (node, record.candidate_id.clone()))
                    .collect();
            let paid_digest = crate::world::normal_bootstrap_current::hash::with_current_meter_v1(
                common,
                &mut work.canonical,
                |meter| {
                    oasis7_client_api::world_service::request_digest_with_work_v1(
                        "governance-membership-registry-v1",
                        &effective,
                        meter,
                    )
                },
            )?
            .map_err(original)?;
            // The original DTO owns its digest String. Keep this copy's real
            // fee with GovernanceParts after the temporary digest owner drops.
            let digest_bytes = size_of::<String>()
                .checked_add(paid_digest.as_str().len())
                .ok_or_else(bad)?;
            common.charge(1, digest_bytes as u64).map_err(original)?;
            let digest_fee = common
                .memory()
                .reserve(digest_bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let digest = paid_digest.as_str().to_owned();
            common.consume_steps(0).map_err(original)?;
            let candidates = admissions
                .into_iter()
                .map(|(key, row)| (key, crate::governance_projection::public_candidate(row)))
                .collect();
            let effective = effective.map(|r| PublicMembershipRegistry {
                slot_id: r.slot_id,
                threshold: r.threshold,
                threshold_bps: r.threshold_bps,
                signer_bindings: r.signer_bindings,
                validator_stakes: r.validator_stakes,
            });
            phase
                .merge(&mut gov_fee)
                .map_err(BodyMutationError::Invalid)?;
            let ids = events.journal_ids();
            let parent_window=&refs.runtime.manifest().lanes[oasis7_world_state::incremental_runtime_history_manifest::RuntimeHistoryLaneV1::Journal.index()];
            let parent_end = parent_window
                .start
                .checked_add(parent_window.count)
                .ok_or_else(bad)?;
            let latest_start = if ids.wraps() > 0 {
                parent_end
                    .checked_add(
                        ids.count()
                            .checked_sub(ids.suffix_count())
                            .ok_or_else(bad)?,
                    )
                    .ok_or_else(bad)?
            } else {
                base.latest_start()
            };
            let window = journal.window();
            let end = window.start.checked_add(window.count).ok_or_else(bad)?;
            if end != parent_end.checked_add(ids.count()).ok_or_else(bad)? {
                return Err(bad().into());
            }
            let high = ids.last().unwrap_or(base.journal_high());
            let floor_ordinal = window.start.max(latest_start);
            let first = if window.count == 0 {
                None
            } else {
                if floor_ordinal >= end {
                    return Err(bad().into());
                }
                let row = read_gameplay_journal_window_position_v1(
                    window,
                    floor_ordinal,
                    read,
                    &mut work.originals,
                    common,
                )
                .map_err(|e| RootModuleExecutionErrorV1::ProductDelivery(e.into()))?;
                let (event, _) = read_gameplay_world_event_original_v1(
                    &row.location.original,
                    Some(row.encoded_bytes),
                    self.originals,
                    &mut work.originals,
                    common,
                    &mut self.ctx.cancelled,
                )
                .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
                Some(event.event().id)
            };
            let event_sequence = with_authorization_source_read_work_v1(
                refs.roots,
                self.ctx,
                read,
                &mut business.authorization,
                common,
                |ctx, _, _| super::metadata_parent::read_event_sequence(ctx),
            )
            .map_err(source)?;
            let physical_event_era = event_sequence.era;
            let physical_next_event_id = event_sequence.next;
            common.charge(1, 24).map_err(original)?;
            let bounds=oasis7_world_state::observation_projection::current_commitment_history::original_journal_bounds_from_actual_suffix_v1(usize::try_from(floor_ordinal.saturating_sub(window.start)).map_err(|_|bad())?,first,high,physical_event_era);
            let guard = self.ctx.transaction.guard();
            self.ctx.transaction.validate_guard(&guard, &live_owner())?;
            Ok(NormalTerminalReadMetadataV1 {
                binding_layout,
                governance_layout: graph,
                aliases,
                binding,
                authority,
                permission,
                governance_rows: rows,
                registry,
                governance: GovernancePartsV1 {
                    epoch,
                    candidates,
                    selected,
                    registry: effective,
                    digest,
                    _digest_fee: digest_fee,
                },
                clock,
                journal: bounds,
                latest_start,
                retained: window.count,
                physical_event_era,
                physical_next_event_id,
                manifest_hash,
                manifest_world: world,
                parent_record: refs.head.verified_record_id(),
                parent_fence: parent.fence(),
                guard,
                core: common.work().identity(),
                memory: common.memory().clone(),
                _phase: phase,
            })
        }));
        match attempt {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => {
                let _: Result<(), _> = self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
                Err(e)
            }
            Err(p) => {
                let _: Result<(), _> = self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
                std::panic::resume_unwind(p)
            }
        }
    }
}

pub(in crate::world) fn read_manifest_world<M, R, E, C, P, PE, PC>(
    ctx: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    q:&ReservedValue<oasis7_world_state::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1>,
    max: usize,
    live: &ExecutionFence,
) -> Result<(String, MemoryReservation), BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use oasis7_world_state::incremental_runtime_artifact_resources::*;
    let guard = ctx.transaction.guard();
    let child = open_current_chain_resource_manifest_v1(q, ctx, &guard, live)
        .map_err(current_scope::overlay)?;
    let mut read = WorldFieldReadContextV1 {
        root: child.root(),
        state: parent.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
    let first = read_runtime_artifact_member_v1(
        RuntimeArtifactRecordV1::ChainResourceManifest,
        &child,
        &"world_id",
        RuntimeArtifactMemberProfileV1::Text,
        &mut read,
        max as u64,
        &mut io::sink(),
    )?;
    let Some(RuntimeArtifactValueV1::Text { utf8_bytes }) = first else {
        return Err(bad().into());
    };
    let n = usize::try_from(utf8_bytes).map_err(|_| bad())?;
    let fee = read
        .values
        .memory
        .reserve(
            n.checked_add(size_of::<String>()).ok_or_else(bad)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(n).map_err(|_| bad())?;
    if raw.capacity() > n {
        return Err(bad().into());
    }
    let second = read_runtime_artifact_member_v1(
        RuntimeArtifactRecordV1::ChainResourceManifest,
        &child,
        &"world_id",
        RuntimeArtifactMemberProfileV1::Text,
        &mut read,
        utf8_bytes,
        &mut raw,
    )?;
    if !matches!(second,Some(RuntimeArtifactValueV1::Text{utf8_bytes:m})if m==utf8_bytes)
        || raw.len() != n
    {
        return Err(bad().into());
    }
    Ok((String::from_utf8(raw).map_err(original)?, fee))
}
