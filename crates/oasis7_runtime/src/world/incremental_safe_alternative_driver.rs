//! Actual matching lane in the sole Root transaction. No raw record/bool is
//! accepted: common admission selects the genuine Current record itself.
use super::*;
use crate::world::{
    incremental_action_admission::{
        CoreActionAdmissionV1, SelectedActionAdmissionV1, admit_selected_action_v1,
    },
    incremental_action_execution::SelectedPendingActionV1,
    incremental_action_execution_core::CompletedCoreActionV1,
    incremental_safe_alternative_matching::{
        CheckedMatchingSafeAlternativeV1, prepare_matching_safe_alternative_v1,
    },
    incremental_safe_alternative_proof_ledger::open_safe_alternative_proof_ledger_v1,
    incremental_safe_alternative_record::prepare_signed_safe_alternative_record_v1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_world_state::{
    incremental_cognition_proof_ledger::{CurrentCognitionProofLedgerV1, ProofLedgerReadWorkV1},
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::{
        RevocationReadWorkV1, RuntimeRevocationCurrentV1, RuntimeRevocationQualificationV1,
    },
};
#[derive(Debug)]
pub(in crate::world) enum MatchingDriverErrorV1<OE> {
    Body(BodyMutationError),
    Admission(crate::world::incremental_action_admission::ActionAdmissionErrorV1),
    Module(RootModuleExecutionErrorV1<OE>),
    Core(super::core_driver::CoreDriverErrorV1<OE>),
}
impl<OE> From<BodyMutationError> for MatchingDriverErrorV1<OE> {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl<OE> From<RootModuleExecutionErrorV1<OE>> for MatchingDriverErrorV1<OE> {
    fn from(e: RootModuleExecutionErrorV1<OE>) -> Self {
        Self::Module(e)
    }
}

/// The next fixed publisher consumes publication, updates original auth-root,
/// appends counted OutcomeRecorded/consensus/side ledger, then the successful
/// branch calls finish_core_post_action_v1 with this exact completed Core event.
/// Rejected skips Rule/Core/settlement/PostAction exactly as the old World loop.
pub(in crate::world) enum MatchingBeforePublicationV1 {
    NoMatch,
    Applied {
        outcome:ReservedValue<crate::world::incremental_module_invocation::RootRulePhaseOutcomeV1>,
        completed:CompletedCoreActionV1,
        publication:crate::world::incremental_safe_alternative_matching::PreparedSafeAlternativeOutcomeRecordedV1,
    },
    Rejected {
        publication:crate::world::incremental_safe_alternative_matching::PreparedSafeAlternativeOutcomeRecordedV1,
    },
}

#[allow(clippy::too_many_arguments)]
fn admit_with_common<'selected, M, R, E, C, P, V, ER, CR, OE>(
    selected: &'selected SelectedPendingActionV1,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<SelectedActionAdmissionV1<'selected>, MatchingDriverErrorV1<OE>>
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
        if !overlay.values.memory.same_pool(common.memory())
            || !read.values.memory.same_pool(common.memory())
        {
            return Err(BodyMutationError::Invalid("matching admission Common source").into());
        }
        common
            .consume_steps(1)
            .map_err(|e| MatchingDriverErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
        let guard = overlay.transaction.guard();
        let (_, meter, stop) = common.parts();
        let child = meter
            .begin_child()
            .map_err(|e| MatchingDriverErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
        let visits = child.remaining_steps() / 6;
        let bytes = child.remaining_read_bytes() / 5;
        overlay.values.io.nodes_remaining = overlay.values.io.nodes_remaining.min(visits);
        read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(visits);
        scan.nodes_remaining = scan.nodes_remaining.min(visits);
        scan.leaves_remaining = scan.leaves_remaining.min(visits);
        work.json.cells_remaining = work.json.cells_remaining.min(visits);
        work.json.json_nodes_remaining = work.json.json_nodes_remaining.min(visits);
        overlay.values.io.read_bytes_remaining = overlay.values.io.read_bytes_remaining.min(bytes);
        read.values.io.read_bytes_remaining = read.values.io.read_bytes_remaining.min(bytes);
        scan.read_bytes_remaining = scan.read_bytes_remaining.min(bytes);
        overlay.key_work.bytes_remaining = overlay.key_work.bytes_remaining.min(bytes);
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(bytes);
        let before_steps = [
            overlay.values.io.nodes_remaining,
            read.values.io.nodes_remaining,
            scan.nodes_remaining,
            scan.leaves_remaining,
            work.json.cells_remaining,
            work.json.json_nodes_remaining,
        ];
        let before_bytes = [
            overlay.values.io.read_bytes_remaining,
            read.values.io.read_bytes_remaining,
            scan.read_bytes_remaining,
            overlay.key_work.bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let admitted = {
            let stop = std::cell::RefCell::new(stop);
            let mut current_context = WorldFieldOverlayContextV1 {
                transaction: &mut *overlay.transaction,
                values: ValueReadContext {
                    source: overlay.values.source,
                    memory: overlay.values.memory,
                    class: overlay.values.class,
                    io: &mut *overlay.values.io,
                },
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                take_state_error: &mut overlay.take_state_error,
                cancelled: || (overlay.cancelled)() || (stop.borrow_mut())(),
            };
            let mut parent_context = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: || (read.cancelled)() || (stop.borrow_mut())(),
            };
            admit_selected_action_v1(
                selected,
                qualification,
                current,
                &mut current_context,
                &mut parent_context,
                scan,
                work,
                &guard,
                live,
            )
            .map_err(MatchingDriverErrorV1::Admission)
        };
        let after_steps = [
            overlay.values.io.nodes_remaining,
            read.values.io.nodes_remaining,
            scan.nodes_remaining,
            scan.leaves_remaining,
            work.json.cells_remaining,
            work.json.json_nodes_remaining,
        ];
        let after_bytes = [
            overlay.values.io.read_bytes_remaining,
            read.values.io.read_bytes_remaining,
            scan.read_bytes_remaining,
            overlay.key_work.bytes_remaining,
            read.key_work.bytes_remaining,
        ];
        let debit = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
        };
        let Some(steps) = debit(&before_steps, &after_steps) else {
            return admitted.and(Err(BodyMutationError::Invalid(
                "matching admission visits accounting",
            )
            .into()));
        };
        let Some(bytes) = debit(&before_bytes, &after_bytes) else {
            return admitted.and(Err(BodyMutationError::Invalid(
                "matching admission bytes accounting",
            )
            .into()));
        };
        let paid = child.consume(steps, bytes);
        child.finish();
        let admitted = admitted?;
        paid.map_err(|e| MatchingDriverErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
        common
            .consume_steps(0)
            .map_err(|e| MatchingDriverErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
        Ok(admitted)
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            overlay.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// Execute the true matched/rejected business lane through exact record
    /// consumption. The sole normal driver must immediately consume the returned
    /// private publication owner in its fixed full authorization publisher.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_selected_matching_core_before_publication_v1<
        P,
        ER,
        CR,
        PE,
        PC,
    >(
        &mut self,
        selected: &SelectedPendingActionV1,
        ledger: &CurrentCognitionProofLedgerV1,
        qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
        current: &mut RuntimeRevocationCurrentV1,
        industrial:&crate::world::incremental_industrial_eligibility::IndustrialEligibilitySourcesV1<'_,'_>,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,impl FnMut()->Option<ValueTreeError>>,
        read: &mut WorldFieldReadContextV1<'_, P, R, ER, CR>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        accepted_entries: &mut usize,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut RevocationReadWorkV1,
        ledger_scan: &mut StateScanBudget,
        ledger_work: &mut ProofLedgerReadWorkV1,
        scope_work:&mut crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
        scope_module_io: &mut ValueIoBudget,
        write_work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationWriteWorkV1,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<MatchingBeforePublicationV1, MatchingDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        use crate::world::incremental_safe_alternative_matching::{
            SafeAlternativeResultOwnerV1, apply_safe_alternative_outcome_v1,
            publish_safe_alternative_rejection_v1,
        };
        let result = (|| {
            let live = live_owner();
            let record = match admit_with_common(
                selected,
                qualification,
                current,
                self.ctx,
                read,
                policy_scan,
                policy_work,
                common,
                &live,
            )? {
                SelectedActionAdmissionV1::Unbound(admission) => {
                    drop(admission);
                    return Ok(MatchingBeforePublicationV1::NoMatch);
                }
                SelectedActionAdmissionV1::RequiresSafeAlternativeScope(record) => record,
            };
            let proof = open_safe_alternative_proof_ledger_v1(
                ledger,
                selected,
                self.ctx.transaction,
                read,
                ledger_scan,
                ledger_work,
                common,
                &live,
            )?;
            // The ledger read may extend the transaction read set. Reacquire
            // the real Current record instead of widening its original guard.
            drop(record);
            let record = match admit_with_common(
                selected,
                qualification,
                current,
                self.ctx,
                read,
                policy_scan,
                policy_work,
                common,
                &live_owner(),
            )? {
                SelectedActionAdmissionV1::RequiresSafeAlternativeScope(record) => record,
                SelectedActionAdmissionV1::Unbound(_) => {
                    return Err(
                        BodyMutationError::Invalid("matching Current source disappeared").into(),
                    );
                }
            };
            let signed = prepare_signed_safe_alternative_record_v1(
                record,
                selected,
                &proof,
                self.ctx.transaction,
                common,
                &live,
            )?;
            let admission = {
                let sources=crate::world::incremental_industrial_eligibility::SafeAlternativeCurrentSourcesV1 {
                    industrial,revocation:&*current,history:self.families.parent,head:self.families.head,
                    manifest:self.manifest_cache,config:self.families.config,
                };
                let mut modules = crate::world::incremental_module_read::ModuleReadContext {
                    qualification: self.families.instances,
                    values: self.ctx.values.source,
                    memory: self.ctx.values.memory,
                    io: &mut *scope_module_io,
                    max_value_bytes: self.ctx.record_bytes,
                    cancelled: || false,
                };
                prepare_matching_safe_alternative_v1(
                    signed,
                    &sources,
                    self.now,
                    self.state,
                    self.ctx,
                    read,
                    &mut modules,
                    scope_work,
                    self.manifest_work,
                    common,
                    &live_owner(),
                )?
            };
            match admission {
                CheckedMatchingSafeAlternativeV1::Matched(admission) => {
                    // Scope proved actual instances and RegistryActive empty.
                    // Still execute the unique original Rule/Modify algorithm;
                    // a fabricated Allow/Modify result is never constructed.
                    let outcome = self.execute_pre_action_rule_phases_v1(
                        selected, directory, rule_work, live_owner,
                    )?;
                    self.publish_action_override_v1(selected, &outcome, audit_work, live_owner)?;
                    drop(admission);
                    let record = match admit_with_common(
                        selected,
                        qualification,
                        current,
                        self.ctx,
                        read,
                        policy_scan,
                        policy_work,
                        common,
                        &live_owner(),
                    )? {
                        SelectedActionAdmissionV1::RequiresSafeAlternativeScope(record) => record,
                        SelectedActionAdmissionV1::Unbound(_) => {
                            return Err(BodyMutationError::Invalid(
                                "matching Current source disappeared",
                            )
                            .into());
                        }
                    };
                    let signed = prepare_signed_safe_alternative_record_v1(
                        record,
                        selected,
                        &proof,
                        self.ctx.transaction,
                        common,
                        &live_owner(),
                    )?;
                    let admission = {
                        let sources=crate::world::incremental_industrial_eligibility::SafeAlternativeCurrentSourcesV1 {
                    industrial,revocation:&*current,history:self.families.parent,head:self.families.head,
                    manifest:self.manifest_cache,config:self.families.config,
                };
                        let mut modules =
                            crate::world::incremental_module_read::ModuleReadContext {
                                qualification: self.families.instances,
                                values: self.ctx.values.source,
                                memory: self.ctx.values.memory,
                                io: &mut *scope_module_io,
                                max_value_bytes: self.ctx.record_bytes,
                                cancelled: || false,
                            };
                        prepare_matching_safe_alternative_v1(
                            signed,
                            &sources,
                            self.now,
                            self.state,
                            self.ctx,
                            read,
                            &mut modules,
                            scope_work,
                            self.manifest_work,
                            common,
                            &live,
                        )?
                    };
                    let admission = match admission {
                        CheckedMatchingSafeAlternativeV1::Matched(admission) => admission,
                        CheckedMatchingSafeAlternativeV1::Rejected(_) => {
                            return Err(BodyMutationError::Invalid(
                                "matching scope changed during empty Rule phase",
                            )
                            .into());
                        }
                    };
                    let completed = self
                        .execute_scoped_core_before_post_v1(
                            selected,
                            CoreActionAdmissionV1::Matched(&admission),
                            Some(&mut *common),
                            &outcome,
                            materials,
                            read,
                            Some(&mut *parent_read),
                            Some(&mut *accepted_entries),
                            policy_scan,
                            policy_work,
                            audit_work,
                            directory,
                            enumeration,
                            live_owner,
                        )
                        .map_err(MatchingDriverErrorV1::Core)?;
                    let event=crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_core_action(&completed);
                    let applied = apply_safe_alternative_outcome_v1(
                        SafeAlternativeResultOwnerV1::Matched(admission),
                        selected,
                        Some(&event),
                        qualification,
                        current,
                        self.ctx,
                        read,
                        policy_scan,
                        policy_work,
                        self.members,
                        write_work,
                        common,
                        &live_owner(),
                    )?;
                    let publication =
                        applied.prepare_publication(self.ctx.transaction, common, &live_owner())?;
                    drop(proof);
                    Ok(MatchingBeforePublicationV1::Applied {
                        outcome,
                        completed,
                        publication,
                    })
                }
                CheckedMatchingSafeAlternativeV1::Rejected(rejected) => {
                    let prepared = rejected.prepare_rejection(
                        selected,
                        self.ctx.transaction,
                        common,
                        &live_owner(),
                    )?;
                    self.append_selected_accepted_disposition_v1(
                        selected,
                        materials,
                        self.families.agents,
                        read,
                        parent_read,
                        accepted_entries,
                        audit_work,
                        live_owner,
                    )?;
                    let live = live_owner();
                    let guard = self.ctx.transaction.guard();
                    let completed = {
                        let mut step =
                            crate::world::incremental_step::IwsStepContext::borrow_counted_at(
                                self.ctx.transaction,
                                self.now,
                                &guard,
                                &live,
                                self.events.values,
                                &mut self.events.mutation,
                                &mut self.events.original_work,
                                self.events.world_events,
                            )?;
                        publish_safe_alternative_rejection_v1(
                            prepared,
                            &mut step,
                            selected,
                            materials,
                            common,
                            &live,
                            self.events.max_event_bytes,
                            &mut self.events.persist_original,
                        )?
                    };
                    let applied = apply_safe_alternative_outcome_v1(
                        SafeAlternativeResultOwnerV1::Rejected(completed),
                        selected,
                        None,
                        qualification,
                        current,
                        self.ctx,
                        read,
                        policy_scan,
                        policy_work,
                        self.members,
                        write_work,
                        common,
                        &live_owner(),
                    )?;
                    let publication =
                        applied.prepare_publication(self.ctx.transaction, common, &live_owner())?;
                    drop(proof);
                    Ok(MatchingBeforePublicationV1::Rejected { publication })
                }
            }
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}

use oasis7_formal_protocol::incremental_value::ValueIoBudget;
