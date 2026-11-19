//! Fixed actual-Current scope reader, nested under the industrial owner.
//! No caller supplied observation, root, count or predicate participates.
use super::*;
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_module_execution_manifest_hash::{
        AcceptedWorldManifestHashV1, RootWorldManifestHashWorkV1,
        prepare_current_world_manifest_hash_v1,
    },
    incremental_module_read::ModuleReadContext,
    incremental_safe_alternative_record::PaidSignedSafeAlternativeRecordV1,
    incremental_safe_alternative_runtime_scope::runtime_scope_v1,
    incremental_safe_alternative_scope::{
        SafeAlternativeScopeStateV1, matching_candidate_v1, validate_matching_action_v1,
        validate_scope_state_v1,
    },
};
use oasis7_formal_protocol::controlled_authority::formal::{
    VerifiedFormalCommitV1,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use oasis7_world_state::{
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_runtime_artifact_config::{
        ArtifactConfigFamilyQualificationV1, read_current_registry_active_count_v1,
    },
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
    incremental_world_native::recover_gameplay_policy_overlay_v1,
};

pub(in crate::world) struct SafeAlternativeCurrentSourcesV1<'a, 's> {
    pub(in crate::world) industrial: &'a IndustrialEligibilitySourcesV1<'a, 's>,
    pub(in crate::world) revocation: &'a RuntimeRevocationCurrentV1,
    pub(in crate::world) history: &'a ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub(in crate::world) head: &'a VerifiedFormalCommitV1,
    pub(in crate::world) manifest: &'a ReservedValue<AcceptedWorldManifestHashV1>,
    pub(in crate::world) config: &'a ReservedValue<ArtifactConfigFamilyQualificationV1>,
}

/// A fixed-reader result, bound to the exact signed input and Current revision.
/// Only the parent matching module may turn a successful result into admission.
pub(in crate::world) struct CheckedSafeAlternativeCurrentScopeV1<'a, 'selected, 'source> {
    input: &'a PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    reason: Option<String>,
    _fee: ReservedValue<()>,
}
impl<'a, 'selected, 'source> CheckedSafeAlternativeCurrentScopeV1<'a, 'selected, 'source> {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        input: &PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Option<&str>, BodyMutationError> {
        input.validate_after_result(tx, common)?;
        tx.validate_guard(&self.guard, live)?;
        if !std::ptr::eq(self.input, input)
            || self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative Current scope binding",
            ));
        }
        Ok(self.reason.as_deref())
    }
    pub(in crate::world) fn into_result<M: MerkleNodeReader>(
        self,
        input: &PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(Option<String>, ReservedValue<()>), BodyMutationError> {
        self.validate(input, tx, common, live)?;
        Ok((self.reason, self._fee))
    }
}

fn overlay_error(
    e: oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1,
) -> BodyMutationError {
    match e {
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e) => {
            BodyMutationError::Execution(e)
        }
        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
            BodyMutationError::Value(e)
        }
        e => BodyMutationError::Original(Box::new(e)),
    }
}

fn industrial_error(e: IndustrialEligibilityErrorV1) -> BodyMutationError {
    use crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1 as D;
    match e {
        IndustrialEligibilityErrorV1::Execution(e) => BodyMutationError::Execution(e),
        IndustrialEligibilityErrorV1::Value(e) => BodyMutationError::Value(e),
        IndustrialEligibilityErrorV1::Overlay(e) => overlay_error(e),
        IndustrialEligibilityErrorV1::Directory(e) => match e {
            D::Execution(e) => BodyMutationError::Execution(e),
            D::Value(e) => BodyMutationError::Value(e),
            D::Overlay(e) => overlay_error(e),
            D::Body(e) => e,
        },
    }
}
fn manifest_error(
    e: crate::world::incremental_module_execution_manifest_hash::RootWorldManifestHashErrorV1,
) -> BodyMutationError {
    use crate::world::incremental_module_execution_manifest_hash::RootWorldManifestHashErrorV1 as H;
    match e {
        H::Execution(e) => BodyMutationError::Execution(e),
        H::Value(e) => BodyMutationError::Value(e),
        H::Overlay(e) => overlay_error(e),
        H::Formal(e) => BodyMutationError::Original(Box::new(e)),
    }
}

#[allow(clippy::too_many_arguments)]
fn read_scope_predicates<M, R, E, C, P, V, ER, CR, MC>(
    input: &PaidSignedSafeAlternativeRecordV1<'_, '_>,
    sources: &SafeAlternativeCurrentSourcesV1<'_, '_>,
    now: u64,
    state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    modules: &mut ModuleReadContext<'_, R, MC>,
    work: &mut IndustrialEligibilityWorkV1,
    hash_work: &mut RootWorldManifestHashWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<String>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    MC: FnMut() -> bool,
{
    overlay.transaction.validate_guard(guard, live)?;
    crate::world::incremental_clock::check_clock(
        now,
        overlay.transaction,
        &mut crate::world::incremental_body::BodyMutationContext {
            sink: &mut crate::world::incremental_body::Discard,
            memory: overlay.values.memory,
            io: &mut *overlay.values.io,
            cancelled: &mut overlay.cancelled,
        },
    )?;
    let record = input.record.record();
    let candidate = match matching_candidate_v1(record, now) {
        Ok(v) => v,
        Err(e) => return Ok(Some(e)),
    };
    let command: oasis7_client_api::safe_alternative::SafeAlternativeCommandV1 =
        match serde_json::from_value(record["binding"].clone()) {
            Ok(c) => c,
            Err(_) => return Ok(Some("outcome_unknown".into())),
        };
    let guard = &overlay.transaction.guard();
    let identity = super::safe_alternative_identity::read_current_safe_alternative_identity_v1(
        sources.industrial,
        &command.agent_id,
        state,
        overlay,
        read,
        work,
        guard,
        live,
    )
    .map_err(industrial_error)?;
    let Some(identity) = identity.as_ref() else {
        return Ok(Some("authority_changed".into()));
    };
    let Some(binding) = input.ledger.binding().filter(|v| !v.is_null()) else {
        return Ok(Some("authority_changed".into()));
    };
    let guard = &overlay.transaction.guard();
    let child =
        open_current_chain_resource_manifest_v1(sources.industrial.artifacts, overlay, guard, live)
            .map_err(overlay_error)?;
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
    )
    .map_err(industrial_error)?
    .ok_or(BodyMutationError::Invalid("SafeAlternative manifest world"))?;
    let hash = prepare_current_world_manifest_hash_v1(
        sources.history,
        sources.head,
        sources.industrial.roots,
        sources.manifest,
        None,
        sources.config,
        state,
        overlay,
        hash_work,
    )
    .map_err(manifest_error)?;
    let runtime = match runtime_scope_v1(
        binding,
        world.value.as_deref().ok_or(BodyMutationError::Invalid(
            "SafeAlternative manifest world text",
        ))?,
        hash.hash(),
    ) {
        Ok(v) => v,
        Err(_) => return Ok(Some("authority_changed".into())),
    };
    let guard = &overlay.transaction.guard();
    let owner = super::safe_alternative_owner::read_current_safe_alternative_owner_v1(
        sources.industrial,
        sources.revocation,
        &command.agent_id,
        state,
        overlay,
        read,
        work,
        guard,
        live,
    )
    .map_err(industrial_error)?;
    let Some(owner) = owner.as_ref() else {
        return Ok(Some("authority_changed".into()));
    };
    if modules.instance_index_total(overlay.transaction)? != 0 {
        return Ok(Some("evaluation_unavailable".into()));
    }
    let guard = &overlay.transaction.guard();
    if read_current_registry_active_count_v1(
        sources.config,
        overlay,
        &mut work.pending_entries_remaining,
        guard,
        live,
    )
    .map_err(overlay_error)?
        != 0
    {
        return Ok(Some("evaluation_unavailable".into()));
    }
    let mut coordinates = [None; 3];
    for (slot, field) in coordinates.iter_mut().zip([
        AgentFieldV1::PositionX,
        AgentFieldV1::PositionY,
        AgentFieldV1::PositionZ,
    ]) {
        let guard = &overlay.transaction.guard();
        *slot = match read_agent_subtree_scalar_overlay_v1(
            AgentSubtreeQualifiedRequestV1 {
                cell: AgentSubtreeScalarRequestV1 {
                    agent: &command.agent_id,
                    field,
                    ordinal: 0,
                },
                qualification: sources.industrial.agents,
            },
            overlay,
            64,
            &mut std::io::sink(),
            guard,
            live,
        )
        .map_err(overlay_error)?
        {
            Some(AgentSubtreeValueV1::I64(n)) => Some(n),
            None => None,
            _ => return Err(BodyMutationError::Invalid("SafeAlternative position type")),
        };
    }
    let position = match coordinates {
        [Some(x_cm), Some(y_cm), Some(z_cm)] => {
            Some(oasis7_agent_api::world_action::GeoPos { x_cm, y_cm, z_cm })
        }
        [None, None, None] => None,
        _ => {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative partial position",
            ));
        }
    };
    let policy = {
        let mut policy_read = WorldFieldReadContextV1 {
            root: sources
                .industrial
                .roots
                .state_root(StateNamespaceV1::Business),
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
        recover_gameplay_policy_overlay_v1(
            overlay.transaction,
            live,
            &mut policy_read,
            &mut work.scan,
            &mut work.typed,
        )
        .map_err(overlay_error)?
    };
    let public_key = (owner.player == identity.owner_binding)
        .then_some(owner.public_key.as_deref())
        .flatten();
    let state = SafeAlternativeScopeStateV1 {
        identity,
        runtime_world: runtime.world,
        runtime_branch: runtime.branch,
        runtime_reorg: runtime.reorg,
        owner_binding: &owner.player,
        owner_public_key: public_key,
        position: position.as_ref(),
        forbidden: &policy.value().value().forbidden_location_ids,
    };
    if let Err(reason) = validate_scope_state_v1(record, candidate, &command, &state, now) {
        return Ok(Some(reason));
    }
    Ok(validate_matching_action_v1(record, candidate, input.selected.envelope()).err())
}

/// The only scope producer: every observation is read in this actual tx under
/// one exclusive Common child. Attempted work is charged on returned errors;
/// abandoning the child poisons the parent balance rather than refunding it.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn check_current_safe_alternative_scope_v1<
    'a,
    'selected,
    'source,
    M,
    R,
    E,
    C,
    P,
    V,
    ER,
    CR,
    MC,
>(
    input: &'a PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    sources: &SafeAlternativeCurrentSourcesV1<'_, '_>,
    now: u64,
    state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    modules: &mut ModuleReadContext<'_, R, MC>,
    work: &mut IndustrialEligibilityWorkV1,
    hash_work: &mut RootWorldManifestHashWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<CheckedSafeAlternativeCurrentScopeV1<'a, 'selected, 'source>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
    MC: FnMut() -> bool,
{
    let result = (|| {
        input.validate_before_scope(overlay.transaction, common, live)?;
        if !overlay.values.memory.same_pool(common.memory())
            || !read.values.memory.same_pool(common.memory())
            || !modules.memory.same_pool(common.memory())
            || overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative scope source pool/class",
            ));
        }
        let bound =
            crate::world::incremental_safe_alternative_allocation::safe_alternative_owned_bound_v1(
                input.record.record(),
                common,
            )?;
        let fee = common
            .memory()
            .reserve(
                bound
                    .heap
                    .checked_add(std::mem::size_of::<
                        CheckedSafeAlternativeCurrentScopeV1<'a, 'selected, 'source>,
                    >())
                    .ok_or(ValueTreeError::ResourceLimited(
                        "SafeAlternative scope layout",
                    ))?,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        common
            .charge(
                bound
                    .visits
                    .checked_add(4096)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "SafeAlternative clock work",
                    ))?,
                bound
                    .bytes
                    .checked_add(4096)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "SafeAlternative clock bytes",
                    ))?,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let guard = overlay.transaction.guard();
        let (_, meter, stop) = common.parts();
        let child = meter
            .begin_child()
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let visits = child.remaining_steps() / 18;
        let bytes = child.remaining_read_bytes() / 10;
        work.read.cells_remaining = work.read.cells_remaining.min(visits);
        work.typed.json.cells_remaining = work.typed.json.cells_remaining.min(visits);
        work.typed.json.json_nodes_remaining = work.typed.json.json_nodes_remaining.min(visits);
        work.revocation.json.cells_remaining = work.revocation.json.cells_remaining.min(visits);
        work.revocation.json.json_nodes_remaining =
            work.revocation.json.json_nodes_remaining.min(visits);
        work.scan.nodes_remaining = work.scan.nodes_remaining.min(visits);
        work.scan.leaves_remaining = work.scan.leaves_remaining.min(visits);
        hash_work.body_scan.nodes_remaining = hash_work.body_scan.nodes_remaining.min(visits);
        hash_work.body_scan.leaves_remaining = hash_work.body_scan.leaves_remaining.min(visits);
        hash_work.fields.cells_remaining = hash_work.fields.cells_remaining.min(visits);
        hash_work.fields.json.cells_remaining = hash_work.fields.json.cells_remaining.min(visits);
        hash_work.fields.json.json_nodes_remaining =
            hash_work.fields.json.json_nodes_remaining.min(visits);
        hash_work.json.cells_remaining = hash_work.json.cells_remaining.min(visits);
        hash_work.json.json_nodes_remaining = hash_work.json.json_nodes_remaining.min(visits);
        overlay.values.io.nodes_remaining = overlay.values.io.nodes_remaining.min(visits);
        read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(visits);
        modules.io.nodes_remaining = modules.io.nodes_remaining.min(visits);
        work.read.comparison_bytes_remaining = work.read.comparison_bytes_remaining.min(bytes);
        work.scan.read_bytes_remaining = work.scan.read_bytes_remaining.min(bytes);
        hash_work.body_scan.read_bytes_remaining =
            hash_work.body_scan.read_bytes_remaining.min(bytes);
        hash_work.hash_bytes_remaining = hash_work.hash_bytes_remaining.min(bytes);
        overlay.values.io.read_bytes_remaining = overlay.values.io.read_bytes_remaining.min(bytes);
        overlay.values.io.write_bytes_remaining =
            overlay.values.io.write_bytes_remaining.min(bytes);
        read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(bytes);
        modules.io.read_bytes_remaining = modules.io.read_bytes_remaining.min(bytes);
        overlay.key_work.bytes_remaining = overlay.key_work.bytes_remaining.min(bytes);
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(bytes);
        work.pending_entries_remaining = work
            .pending_entries_remaining
            .min(usize::try_from(visits).unwrap_or(usize::MAX));
        let before_steps = [
            work.read.cells_remaining,
            work.typed.json.cells_remaining,
            work.typed.json.json_nodes_remaining,
            work.revocation.json.cells_remaining,
            work.revocation.json.json_nodes_remaining,
            work.scan.nodes_remaining,
            work.scan.leaves_remaining,
            hash_work.body_scan.nodes_remaining,
            hash_work.body_scan.leaves_remaining,
            hash_work.fields.cells_remaining,
            hash_work.fields.json.cells_remaining,
            hash_work.fields.json.json_nodes_remaining,
            hash_work.json.cells_remaining,
            hash_work.json.json_nodes_remaining,
            overlay.values.io.nodes_remaining,
            read.values.io.nodes_remaining,
            modules.io.nodes_remaining,
            work.pending_entries_remaining as u64,
        ];
        let before_bytes = [
            work.read.comparison_bytes_remaining,
            work.scan.read_bytes_remaining,
            hash_work.body_scan.read_bytes_remaining,
            hash_work.hash_bytes_remaining,
            overlay.values.io.read_bytes_remaining,
            overlay.values.io.write_bytes_remaining,
            read.values.io.read_bytes_remaining,
            modules.io.read_bytes_remaining,
            overlay.key_work.bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let reason = {
            let stop = std::cell::RefCell::new(stop);
            let overlay_stop = &mut overlay.cancelled;
            let read_stop = &mut read.cancelled;
            let module_stop = &mut modules.cancelled;
            let mut current = WorldFieldOverlayContextV1 {
                transaction: &mut *overlay.transaction,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: overlay.values.source,
                    memory: overlay.values.memory,
                    class: overlay.values.class,
                    io: &mut *overlay.values.io,
                },
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                take_state_error: &mut overlay.take_state_error,
                cancelled: || overlay_stop() || (stop.borrow_mut())(),
            };
            let mut parent = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || read_stop() || (stop.borrow_mut())(),
            };
            let mut module = ModuleReadContext {
                qualification: modules.qualification,
                values: modules.values,
                memory: modules.memory,
                io: &mut *modules.io,
                max_value_bytes: modules.max_value_bytes,
                cancelled: || module_stop() || (stop.borrow_mut())(),
            };
            read_scope_predicates(
                input,
                sources,
                now,
                state,
                &mut current,
                &mut parent,
                &mut module,
                work,
                hash_work,
                &guard,
                live,
            )
        };
        let after_steps = [
            work.read.cells_remaining,
            work.typed.json.cells_remaining,
            work.typed.json.json_nodes_remaining,
            work.revocation.json.cells_remaining,
            work.revocation.json.json_nodes_remaining,
            work.scan.nodes_remaining,
            work.scan.leaves_remaining,
            hash_work.body_scan.nodes_remaining,
            hash_work.body_scan.leaves_remaining,
            hash_work.fields.cells_remaining,
            hash_work.fields.json.cells_remaining,
            hash_work.fields.json.json_nodes_remaining,
            hash_work.json.cells_remaining,
            hash_work.json.json_nodes_remaining,
            overlay.values.io.nodes_remaining,
            read.values.io.nodes_remaining,
            modules.io.nodes_remaining,
            work.pending_entries_remaining as u64,
        ];
        let after_bytes = [
            work.read.comparison_bytes_remaining,
            work.scan.read_bytes_remaining,
            hash_work.body_scan.read_bytes_remaining,
            hash_work.hash_bytes_remaining,
            overlay.values.io.read_bytes_remaining,
            overlay.values.io.write_bytes_remaining,
            read.values.io.read_bytes_remaining,
            modules.io.read_bytes_remaining,
            overlay.key_work.bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let debit = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |sum, (b, a)| sum.checked_add(b.checked_sub(*a)?))
        };
        let Some(steps) = debit(&before_steps, &after_steps) else {
            return reason.and(Err(BodyMutationError::Invalid(
                "SafeAlternative scope visits accounting",
            )));
        };
        let Some(bytes) = debit(&before_bytes, &after_bytes) else {
            return reason.and(Err(BodyMutationError::Invalid(
                "SafeAlternative scope bytes accounting",
            )));
        };
        let paid = child.consume(steps, bytes);
        child.finish();
        let reason = reason?;
        paid.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        common
            .consume_steps(0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if (overlay.cancelled)() || (read.cancelled)() || (modules.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let guard = overlay.transaction.guard();
        overlay.transaction.validate_guard(&guard, live)?;
        Ok(CheckedSafeAlternativeCurrentScopeV1 {
            input,
            guard,
            core: common.work().identity(),
            reason,
            _fee: fee.retain(()),
        })
    })();
    poison_on_error(overlay.transaction, result.is_err());
    result
}
