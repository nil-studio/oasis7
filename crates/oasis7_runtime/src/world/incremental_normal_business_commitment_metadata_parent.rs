//! Explicit authenticated GEN/cold bootstrap. Ordinary ticks use only the
//! resident parent and actual changed authority paths, never this full reader.
use super::*;
use crate::world::incremental_normal_industry_due::delivery::{
    ProductValidationDeliveryWorkV1, read_gameplay_journal_position_v1,
    read_gameplay_world_event_original_v1,
};
use oasis7_world_state::incremental_event_fields::{EVENT_SEQUENCE_BYTES, EventSequenceV1};
use oasis7_world_state::incremental_runtime_allocators_codec::RuntimeAllocatorV1;
use oasis7_world_state::incremental_runtime_history_manifest::RuntimeHistoryLaneV1;
use oasis7_world_state::incremental_runtime_revocation::{
    ModuleAdmissionRevocationRequestV1 as Q, ModuleAdmissionRevocationValueV1 as V,
    read_current_module_admission_revocation_v1, recover_runtime_revocation_body_v1,
};
use std::mem::size_of;
pub struct NormalBusinessMetadataParentSourceV1 {
    record: [u8; 32],
    fence: ExecutionFence,
    authority: u64,
    permission: u64,
    first_latest_era: Option<u64>,
    high: u64,
    event_era: u64,
    retained: u64,
    latest_start: u64,
    _fee: MemoryReservation,
}
impl NormalBusinessMetadataParentSourceV1 {
    pub(in crate::world) fn validate_parent(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        parent.validate_context(common).map_err(original)?;
        common.charge(1, 64).map_err(original)?;
        if self.record != parent.refs().head.verified_record_id()
            || self.fence != parent.fence()
            || !self._fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn authority_generation(&self) -> u64 {
        self.authority
    }
    pub(in crate::world) fn permission_generation(&self) -> u64 {
        self.permission
    }
    pub(in crate::world) fn journal_first_latest_era(&self) -> Option<u64> {
        self.first_latest_era
    }
    pub(in crate::world) fn journal_high(&self) -> u64 {
        self.high
    }
    pub(in crate::world) fn journal_event_era(&self) -> u64 {
        self.event_era
    }
    pub(in crate::world) fn retained(&self) -> u64 {
        self.retained
    }
    pub(in crate::world) fn latest_start(&self) -> u64 {
        self.latest_start
    }
}
/// A true immutable World observation. It remains provisional data until the
/// real GEN parent is independently bound; it has no root/accepted constructor.
pub struct PreparedNormalBusinessMetadataGenesisV1 {
    aliases: super::metadata::RuntimeAuthorityAliasesV1,
    journal_start: usize,
    retained_events: u64,
    core: WorkIdentityV1,
    world_id: String,
    chain_id: String,
    genesis_digest: Option<String>,
    manifest_hash: String,
    raw_state_hash: String,
    clock: u64,
    result_count: usize,
    authority: u64,
    permission: u64,
    first_latest_era: Option<u64>,
    high: u64,
    event_era: u64,
    binding: serde_json::Value,
    _fee: MemoryReservation,
}
impl crate::World {
    pub fn prepare_normal_business_metadata_parent_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<PreparedNormalBusinessMetadataGenesisV1, BodyMutationError> {
        let binding = self.cognition().get("runtime_binding").ok_or_else(|| {
            world_error(
                crate::world::cognition_persistence_validation::cognition_validation(
                    "runtime_binding_missing",
                ),
            )
        })?;
        // Actual selected binding only; arbitrary unknown JSON fields are retained
        // under the same closed JSON graph admission used by normal metadata.
        let (n, mut fee) = super::metadata::admit_json_copy(binding, common)?;
        common.charge(n as u64, n as u64).map_err(original)?;
        let (_, mut manifest_fee) =
            super::metadata::admit_serializable_phase(&self.manifest, common)?;
        let mut hash_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<String>() + 64 + 256,
                MemoryClass::Ordinary,
            )
            .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
        let manifest_hash = oasis7_world_state::util::hash_json_with_work_v1(
            &self.manifest,
            &mut |steps, bytes| common.charge(steps, bytes),
        )
        .map_err(|error| match error {
            oasis7_world_state::util::JsonHashWorkErrorV1::Work(error) => original(error),
            oasis7_world_state::util::JsonHashWorkErrorV1::Codec(error) => original(error),
        })?;
        manifest_fee
            .merge(&mut hash_fee)
            .map_err(BodyMutationError::Invalid)?;
        // Alias strings are copied only after their own actual input-derived
        // payload/header bound has been admitted, independently of the JSON copy.
        let alias_bytes = n
            .checked_add(64)
            .and_then(|n| n.checked_add(size_of::<super::metadata::RuntimeAuthorityAliasesV1>()))
            .ok_or_else(bad)?;
        common.charge(8, alias_bytes as u64).map_err(original)?;
        let mut alias_fee = common
            .memory()
            .reserve(alias_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let aliases = crate::World::cognition_runtime_authority_aliases_from_current_fields_v1(
            binding,
            self.chain_resource_manifest().world_id.as_str(),
            &manifest_hash,
        )
        .map_err(world_error)?;
        let (
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
            runtime_manifest_hash,
        ) = aliases;
        let aliases = super::metadata::RuntimeAuthorityAliasesV1 {
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
            runtime_manifest_hash,
        };
        fee.merge(&mut alias_fee)
            .map_err(BodyMutationError::Invalid)?;
        fee.merge(&mut manifest_fee)
            .map_err(BodyMutationError::Invalid)?;
        let identity = self.chain_resource_manifest();
        let identity_bytes = identity
            .world_id
            .len()
            .checked_add(identity.chain_id.len())
            .and_then(|n| n.checked_add(identity.genesis_ref.as_ref().map_or(0, String::len)))
            .and_then(|n| {
                n.checked_add(
                    size_of::<PreparedNormalBusinessMetadataGenesisV1>() + 3 * 64 + 512 + 32768,
                )
            })
            .ok_or_else(bad)?;
        common.charge(1, identity_bytes as u64).map_err(original)?;
        let mut identity_fee = common
            .memory()
            .reserve(identity_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        fee.merge(&mut identity_fee)
            .map_err(BodyMutationError::Invalid)?;
        let policy_hash = oasis7_world_state::util::hash_json_with_work_v1(
            &self.policies,
            &mut |steps, bytes| common.charge(steps, bytes),
        )
        .map_err(|e| match e {
            oasis7_world_state::util::JsonHashWorkErrorV1::Work(e) => original(e),
            oasis7_world_state::util::JsonHashWorkErrorV1::Codec(e) => original(e),
        })?;
        // Same original semantic State projection as current_state_root_hash;
        // streamed from the immutable GEN borrow, with no World/State clone.
        let state = oasis7_world_state::WorldStateProjection::borrowed(&self.state);
        let projection = oasis7_world_state::codec::StateRootProjection {
            state: &state,
            manifest_hash: &manifest_hash,
            policy_hash: &policy_hash,
        };
        let raw_state_hash =
            oasis7_world_state::util::hash_json_with_work_v1(&projection, &mut |steps, bytes| {
                common.charge(steps, bytes)
            })
            .map_err(|e| match e {
                oasis7_world_state::util::JsonHashWorkErrorV1::Work(e) => original(e),
                oasis7_world_state::util::JsonHashWorkErrorV1::Codec(e) => original(e),
            })?;
        let binding = binding.clone();
        let mut authority = 0;
        for (key, row) in &self.capability_revocation_state().authority_records {
            common.charge(1, (key.len() + 8) as u64).map_err(original)?;
            authority = authority.max(row.governance_epoch)
        }
        let journal = &self.journal().events;
        let mut first = journal.first().map(|e| e.id);
        let mut journal_start = 0usize;
        for (index, pair) in journal.windows(2).enumerate() {
            common.charge(1, 16).map_err(original)?;
            if pair[0].id >= pair[1].id {
                first = Some(pair[1].id);
                journal_start = index.checked_add(1).ok_or_else(bad)?;
            }
        }
        Ok(PreparedNormalBusinessMetadataGenesisV1 {
            aliases,
            journal_start,
            retained_events: u64::try_from(journal.len()).map_err(|_| bad())?,
            core: common.work().identity(),
            world_id: identity.world_id.clone(),
            chain_id: identity.chain_id.clone(),
            genesis_digest: identity.genesis_ref.clone(),
            manifest_hash,
            raw_state_hash,
            clock: self.state.time,
            result_count: self
                .capability_revocation_state()
                .world_service_results
                .len(),
            authority,
            permission: self.capability_revocation_state().epoch,
            first_latest_era: first,
            high: journal
                .last()
                .map_or_else(|| self.next_event_id.saturating_sub(1), |e| e.id),
            event_era: self.next_event_id_era,
            binding,
            _fee: fee,
        })
    }
}
impl PreparedNormalBusinessMetadataGenesisV1 {
    pub(in crate::world) fn raw_state_hash_for_bootstrap_v1(&self) -> &str {
        &self.raw_state_hash
    }
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn identity(&self) -> (&str, &str, Option<&str>) {
        (
            &self.world_id,
            &self.chain_id,
            self.genesis_digest.as_deref(),
        )
    }
    pub(in crate::world) fn result_count(&self) -> usize {
        self.result_count
    }
    pub(in crate::world) fn authority(&self) -> u64 {
        self.authority
    }
    pub(in crate::world) fn permission(&self) -> u64 {
        self.permission
    }
    pub(in crate::world) fn journal_bounds(&self) -> (u64, u64, u64) {
        let b=oasis7_world_state::observation_projection::current_commitment_history::original_journal_bounds_from_actual_suffix_v1(0,self.first_latest_era,self.high,self.event_era);
        (b.floor, b.high, b.era)
    }
    pub(in crate::world) fn runtime_binding_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<ReservedValue<oasis7_agent_api::RuntimeBindingV1>, BodyMutationError> {
        self.validate_context(common)?;
        crate::World::cognition_runtime_binding_from_current_fields_bounded_v1(
            &self.binding,
            &self.world_id,
            &self.manifest_hash,
            &self.raw_state_hash,
            self.clock,
            common,
            canonical,
        )
    }
    pub fn runtime_binding_data(&self) -> &serde_json::Value {
        &self.binding
    }
    pub(in crate::world) fn check_actual_parent(
        &self,
        actual: &NormalBusinessMetadataParentSourceV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.charge(1, 40).map_err(original)?;
        if !self._fee.uses_budget(common.memory())
            || self.authority != actual.authority
            || self.permission != actual.permission
            || self.first_latest_era != actual.first_latest_era
            || self.high != actual.high
            || self.event_era != actual.event_era
        {
            return Err(bad());
        }
        Ok(())
    }
}

/// Closed readonly source selection. A GEN observation remains GEN data; it is
/// never converted into a synthetic terminal Current witness. The fixed cache
/// bootstrap separately binds it to the actual signed parent commitments.
#[derive(Clone, Copy)]
pub(in crate::world) enum OriginalProjectionReadMetadataV1<'a> {
    Current(&'a super::metadata::NormalTerminalReadMetadataV1),
    Genesis {
        source: &'a PreparedNormalBusinessMetadataGenesisV1,
        governance: &'a super::metadata::NormalTerminalReadMetadataV1,
    },
}
impl OriginalProjectionReadMetadataV1<'_> {
    pub(in crate::world) fn validate_provenance<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        match self {
            Self::Current(source) => source.validate_provenance(tx, common, live),
            Self::Genesis { source, governance } => {
                source.validate_context(common)?;
                if !tx.uses_budget(common.memory()) {
                    return Err(bad());
                }
                tx.validate_bootstrap_read_only_v1(live)?;
                governance.validate_provenance(tx, common, live)?;
                common.charge(16, 128).map_err(original)?;
                if source.clock != governance.current_clock()
                    || source.authority != governance.authority_generation()
                    || source.permission != governance.permission_generation()
                    || source.journal_start != governance.journal_current_era_start_v1()
                    || source.retained_events != governance.journal_retained_event_count_v1()
                    || source.journal_bounds()
                        != (
                            governance.journal_retention_floor(),
                            governance.journal_high_watermark(),
                            governance.event_id_era(),
                        )
                {
                    return Err(bad());
                }
                for (a, b) in [
                    (&source.aliases.world_id, governance.provider_world_id()),
                    (&source.aliases.branch_id, governance.branch_id()),
                    (
                        &source.aliases.finality_status,
                        governance.finality_status(),
                    ),
                    (&source.manifest_hash, governance.manifest_hash()),
                ] {
                    common
                        .charge(1, (a.len() + b.len()) as u64)
                        .map_err(original)?;
                    if a != b {
                        return Err(bad());
                    }
                }
                if source.aliases.finality_epoch != governance.finality_epoch()
                    || source.aliases.reorg_epoch != governance.reorg_generation()
                    || source.aliases.finality_block_hash.as_deref()
                        != governance.finality_block_hash()
                {
                    return Err(bad());
                }
                Ok(())
            }
        }
    }
    pub(in crate::world) fn current_clock(&self) -> u64 {
        match self {
            Self::Current(s) => s.current_clock(),
            Self::Genesis { source: s, .. } => s.clock,
        }
    }
    pub(in crate::world) fn authority_generation(&self) -> u64 {
        match self {
            Self::Current(s) => s.authority_generation(),
            Self::Genesis { source: s, .. } => s.authority,
        }
    }
    pub(in crate::world) fn permission_generation(&self) -> u64 {
        match self {
            Self::Current(s) => s.permission_generation(),
            Self::Genesis { source: s, .. } => s.permission,
        }
    }
    pub(in crate::world) fn journal_current_era_start_v1(&self) -> usize {
        match self {
            Self::Current(s) => s.journal_current_era_start_v1(),
            Self::Genesis { source: s, .. } => s.journal_start,
        }
    }
    pub(in crate::world) fn journal_retained_event_count_v1(&self) -> u64 {
        match self {
            Self::Current(s) => s.journal_retained_event_count_v1(),
            Self::Genesis { source: s, .. } => s.retained_events,
        }
    }
    pub(in crate::world) fn event_id_era(&self) -> u64 {
        match self {
            Self::Current(s) => s.event_id_era(),
            Self::Genesis { source: s, .. } => s.journal_bounds().2,
        }
    }
    pub(in crate::world) fn journal_retention_floor(&self) -> u64 {
        match self {
            Self::Current(s) => s.journal_retention_floor(),
            Self::Genesis { source: s, .. } => s.journal_bounds().0,
        }
    }
    pub(in crate::world) fn journal_high_watermark(&self) -> u64 {
        match self {
            Self::Current(s) => s.journal_high_watermark(),
            Self::Genesis { source: s, .. } => s.journal_bounds().1,
        }
    }
    pub(in crate::world) fn provider_world_id(&self) -> &str {
        match self {
            Self::Current(s) => s.provider_world_id(),
            Self::Genesis { source: s, .. } => &s.aliases.world_id,
        }
    }
    pub(in crate::world) fn branch_id(&self) -> &str {
        match self {
            Self::Current(s) => s.branch_id(),
            Self::Genesis { source: s, .. } => &s.aliases.branch_id,
        }
    }
    pub(in crate::world) fn reorg_generation(&self) -> u64 {
        match self {
            Self::Current(s) => s.reorg_generation(),
            Self::Genesis { source: s, .. } => s.aliases.reorg_epoch,
        }
    }
    pub(in crate::world) fn finality_epoch(&self) -> u64 {
        match self {
            Self::Current(s) => s.finality_epoch(),
            Self::Genesis { source: s, .. } => s.aliases.finality_epoch,
        }
    }
    pub(in crate::world) fn finality_status(&self) -> &str {
        match self {
            Self::Current(s) => s.finality_status(),
            Self::Genesis { source: s, .. } => &s.aliases.finality_status,
        }
    }
    pub(in crate::world) fn finality_block_hash(&self) -> Option<&str> {
        match self {
            Self::Current(s) => s.finality_block_hash(),
            Self::Genesis { source: s, .. } => s.aliases.finality_block_hash.as_deref(),
        }
    }
    pub(in crate::world) fn governance_membership_for_state_hash_bounded_v1(
        &self,
        actual_raw_state_hash: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        ReservedValue<oasis7_client_api::world_service::FormalGovernanceMembershipV1>,
        BodyMutationError,
    > {
        let (value, fee) = self
            .governance_membership_parts_for_state_hash_bounded_v1(actual_raw_state_hash, common)?;
        Ok(fee.retain(value))
    }
    pub(in crate::world) fn governance_membership_parts_for_state_hash_bounded_v1(
        &self,
        actual_raw_state_hash: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        (
            oasis7_client_api::world_service::FormalGovernanceMembershipV1,
            oasis7_proto::memory_budget::MemoryReservation,
        ),
        BodyMutationError,
    > {
        match self {
            Self::Current(source) => source.governance_membership_parts_for_state_hash_bounded_v1(
                actual_raw_state_hash,
                common,
            ),
            Self::Genesis { source, governance } => {
                common
                    .charge(
                        1,
                        (actual_raw_state_hash.len() + source.raw_state_hash.len()) as u64,
                    )
                    .map_err(original)?;
                if actual_raw_state_hash != source.raw_state_hash {
                    return Err(bad());
                }
                governance.governance_membership_parts_for_state_hash_bounded_v1(
                    actual_raw_state_hash,
                    common,
                )
            }
        }
    }
    pub(in crate::world) fn runtime_binding_for_state_hash_bounded_v1(
        &self,
        actual_raw_state_hash: &str,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<Option<ReservedValue<oasis7_agent_api::RuntimeBindingV1>>, BodyMutationError> {
        // Both concrete owners currently require a genuine enabled binding.
        // An actual Disabled source needs its own full-router producer; absence
        // is never fabricated by swallowing a codec/resource/signature error.
        let binding = match self {
            Self::Current(s) => s.runtime_binding_for_state_hash_bounded_v1(
                actual_raw_state_hash,
                common,
                canonical,
            )?,
            Self::Genesis { source: s, .. } => {
                common
                    .charge(
                        1,
                        (actual_raw_state_hash.len() + s.raw_state_hash.len()) as u64,
                    )
                    .map_err(original)?;
                if actual_raw_state_hash != s.raw_state_hash {
                    return Err(bad());
                }
                s.runtime_binding_bounded(common, canonical)?
            }
        };
        Ok(Some(binding))
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn bootstrap_normal_business_metadata_parent_v1<M, R, E, C, P, VV, PE, PC, O>(
    parent: &BoundNormalTickParentV1<'_>,
    ctx: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    read: &mut WorldFieldReadContextV1<'_, P, VV, PE, PC>,
    originals: &mut O,
    work: &mut AuthorizationRootSourceReadWorkV1,
    history_work: &mut ProductValidationDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<NormalBusinessMetadataParentSourceV1>, RootModuleExecutionErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    VV: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    let result = (|| {
        parent.validate_context(common).map_err(original)?;
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        if ctx.transaction.ordered_fact_count() != 0 {
            return Err(bad().into());
        }
        // Explicit startup only: no ordinary Current writes can be mistaken for the
        // accepted parent by recovering the qualified parent's immutable body.
        for ns in [
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Allocator,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Effects,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Artifacts,
        ] {
            ctx.transaction.visit_pending_writes::<BodyMutationError>(
                ns,
                &mut work.pending_entries,
                || common.consume_steps(1).map_err(original),
                |_, _, _| Err(bad()),
            )?;
        }
        let refs = parent.refs();
        let guard = ctx.transaction.guard();
        let (authority, permission, sequence) = with_authorization_source_read_work_v1(
            refs.roots,
            ctx,
            read,
            work,
            common,
            |ctx, read, w| {
                let body = read_current_module_admission_revocation_v1(
                    Q::Body,
                    refs.storage.current().runtime().revocation(),
                    ctx,
                    read,
                    &mut w.scan,
                    &mut w.typed,
                    &guard,
                    live,
                )?;
                let V::Body(body) = &*body else {
                    return Err(
                        ValueTreeError::Invalid("business bootstrap revocation body").into(),
                    );
                };
                if body != refs.storage.current().runtime().revocation().body() {
                    return Err(ValueTreeError::Invalid("business bootstrap changed parent").into());
                }
                let full =
                    recover_runtime_revocation_body_v1(body, read, &mut w.scan, &mut w.typed)?;
                let mut max = 0;
                for row in full.value().authority_records.values() {
                    w.typed.json.cells_remaining =
                        w.typed.json.cells_remaining.checked_sub(1).ok_or(
                            ValueTreeError::ResourceLimited("business bootstrap authorities"),
                        )?;
                    if (ctx.cancelled)() {
                        return Err(ValueTreeError::Cancelled.into());
                    }
                    max = max.max(row.governance_epoch)
                }
                let sequence = read_event_sequence(ctx)?;
                Ok((max, full.value().epoch, sequence))
            },
        )
        .map_err(source)?;
        let window = &refs.runtime.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()];
        let mut first = None;
        let mut high = sequence.next.saturating_sub(1);
        let mut latest_start = window.start;
        for ordinal in window.start..window.start.checked_add(window.count).ok_or_else(bad)? {
            let row = read_gameplay_journal_position_v1(
                refs.runtime,
                ordinal,
                read,
                history_work,
                common,
            )
            .map_err(|e| RootModuleExecutionErrorV1::ProductDelivery(e.into()))?;
            let (event, _) = read_gameplay_world_event_original_v1(
                &row.location.original,
                Some(row.encoded_bytes),
                originals,
                history_work,
                common,
                &mut ctx.cancelled,
            )
            .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
            common.charge(1, 16).map_err(original)?;
            let id = event.event().id;
            if first.is_none() || high >= id {
                first = Some(id);
                latest_start = ordinal
            }
            high = id;
        }
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<NormalBusinessMetadataParentSourceV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(NormalBusinessMetadataParentSourceV1 {
            record: refs.head.verified_record_id(),
            fence: parent.fence(),
            authority,
            permission,
            first_latest_era: first,
            high,
            event_era: sequence.era,
            retained: window.count,
            latest_start,
            _fee: common
                .memory()
                .reserve(
                    std::mem::size_of::<NormalBusinessMetadataParentSourceV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?,
        }))
    })();
    if result.is_err() {
        let _: Result<(), _> = ctx.transaction.contribute(|_| {
            Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
        });
    }
    result
}
pub(super) fn read_event_sequence<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
) -> Result<EventSequenceV1, SourceError> {
    let (key, n) = RuntimeAllocatorV1::Event.key();
    let id = ctx
        .transaction
        .read(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Allocator,
            &key[..n],
        )?
        .ok_or(ValueTreeError::Invalid("missing event control leaf"))?;
    let root = ctx.values.load_root(&id)?;
    if root.0.logical_len != EVENT_SEQUENCE_BYTES as u64 {
        return Err(ValueTreeError::Invalid("event control value length").into());
    }
    let mut bytes = [0; EVENT_SEQUENCE_BYTES];
    ctx.values.read_range(
        &root,
        0,
        EVENT_SEQUENCE_BYTES as u64,
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
        &mut ctx.cancelled,
    )?;
    EventSequenceV1::from_canonical_bytes(&bytes).map_err(|e| ValueTreeError::Invalid(e).into())
}

impl super::metadata::NormalTerminalReadMetadataV1 {
    pub(in crate::world) fn prepare_normal_candidate_source_v1(
        &self,
        terminal: &crate::world::incremental_normal_host_finish::SealedNormalResidentSourceV1,
        core: &ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,
        history: &ReservedValue<
            oasis7_world_state::incremental_runtime_history_source::CheckedRuntimeHistorySourceV1,
        >,
        head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<NormalBusinessMetadataParentSourceV1>, BodyMutationError> {
        self.validate_context(common)?;
        common.charge(16, 8192).map_err(original)?;
        terminal.validate_business_metadata_source_v1(self, common)?;
        let selected = history.occurrence();
        if !core.uses_budget(common.memory())
            || !history.uses_budget(common.memory())
            || core.candidate_record_id() != head.verified_record_id()
            || selected.candidate_record_id() != core.candidate_record_id()
            || selected.body_id() != core.catalog().entry.commit
            || selected.roots_id() != terminal.after_roots()
            || core.body().roots != terminal.after_roots()
            || history.manifest().roots != terminal.after_roots()
            || roots.digest()? != terminal.after_roots()
        {
            return Err(bad());
        }
        let old = hex::encode(self.parent_record());
        if head.record().parent_record_id != old {
            return Err(bad());
        }
        let fee = common
            .memory()
            .reserve(
                size_of::<NormalBusinessMetadataParentSourceV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let inner = common
            .memory()
            .reserve(
                size_of::<NormalBusinessMetadataParentSourceV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(NormalBusinessMetadataParentSourceV1 {
            record: head.verified_record_id(),
            fence: ExecutionFence {
                parent_commit: selected.body_id(),
                parent_roots: terminal.after_roots(),
                authority: core.body().authority,
            },
            authority: self.authority_generation(),
            permission: self.permission_generation(),
            first_latest_era: if self.retained() == 0 {
                None
            } else {
                Some(self.journal_retention_floor())
            },
            high: self.journal_high_watermark(),
            event_era: self.physical_event_era(),
            retained: self.retained(),
            latest_start: self.latest_start(),
            _fee: inner,
        }))
    }
}
impl NormalBusinessMetadataParentSourceV1 {
    pub(in crate::world) fn share_normal_ordinary_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Self>, BodyMutationError> {
        common
            .charge(1, size_of::<Self>() as u64)
            .map_err(original)?;
        if !self._fee.uses_budget(common.memory()) {
            return Err(bad());
        }
        let fee = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let inner = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(Self {
            record: self.record,
            fence: self.fence,
            authority: self.authority,
            permission: self.permission,
            first_latest_era: self.first_latest_era,
            high: self.high,
            event_era: self.event_era,
            retained: self.retained,
            latest_start: self.latest_start,
            _fee: inner,
        }))
    }
}
