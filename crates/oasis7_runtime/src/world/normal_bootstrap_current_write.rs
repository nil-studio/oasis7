//! Fixed writes for the four original authorization event kinds. The only
//! input is a private plan produced by Bootstrap's Current business checks.
use super::publication::{
    ActualBootstrapGrantWriteV1, ActualBootstrapInvocationWriteV1,
    BootstrapAuthorizationMutationV1, VerifiedBootstrapCapabilityCheckV1,
};
use super::*;
use crate::world::{
    incremental_factory_phase::io::{FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1},
    incremental_module_invocation::*,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_authorization_family::{
        AuthorizationRecordChangeV1, change_authorization_record_overlay_v1,
    },
    incremental_runtime_authorization_grants_family::{
        AuthorizationGrantChangeV1, change_authorization_grant_overlay_v1,
    },
    incremental_runtime_revocation::*,
};

pub(super) enum BootstrapBeforeV1 {
    Revocation(ReservedValue<StateRootV1>),
    Grant {
        key: ReservedValue<String>,
        before: Option<ReservedValue<serde_json::Value>>,
        after: ReservedValue<serde_json::Value>,
    },
    Invocation {
        key: ReservedValue<String>,
        before: Option<ReservedValue<oasis7_world_state::CapabilityInvocationContext>>,
    },
}
pub(super) struct PreparedBootstrapAuthorizationStateV1 {
    pub(super) event: crate::CapabilityAuthorizationEvent,
    pub(super) event_graph: oasis7_proto::memory_budget::MemoryReservation,
    pub(super) before: BootstrapBeforeV1,
    pub(super) check: Option<VerifiedBootstrapCapabilityCheckV1>,
    pub(super) guard: PreparedGuard,
    pub(super) core: WorkIdentityV1,
    pub(super) now: u64,
    pub(super) fee: ReservedValue<()>,
}
/// Actual state-write evidence, before the authorization-root publication.
/// It cannot be used by the WorldEvent sink or as another business admission.
pub(in crate::world) struct WrittenCurrentBootstrapAuthorizationV1 {
    pub(super) event: crate::CapabilityAuthorizationEvent,
    pub(super) event_graph: oasis7_proto::memory_budget::MemoryReservation,
    pub(super) mutation: BootstrapAuthorizationMutationV1,
    pub(super) check: Option<VerifiedBootstrapCapabilityCheckV1>,
    pub(super) guard: PreparedGuard,
    pub(super) core: WorkIdentityV1,
    pub(super) now: u64,
    pub(super) fee: ReservedValue<()>,
}
impl WrittenCurrentBootstrapAuthorizationV1 {
    pub(in crate::world) fn event(&self) -> &crate::CapabilityAuthorizationEvent {
        &self.event
    }
    pub(in crate::world) fn storage_cut(&self) -> &PreparedGuard {
        &self.guard
    }
    pub(in crate::world) fn revocation_receipt(
        &self,
    ) -> Option<&ReservedValue<BootstrapRevocationWriteReceiptV1>> {
        match &self.mutation {
            BootstrapAuthorizationMutationV1::Revocation(v) => Some(v),
            BootstrapAuthorizationMutationV1::Grant(_)
            | BootstrapAuthorizationMutationV1::Invocation(_) => None,
        }
    }
    pub(in crate::world) fn validate_current<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self.event_graph.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "Bootstrap written storage source",
            ));
        }
        if let Some(proof) = &self.check {
            proof.validate(tx, common, live)?;
        }
        match (&self.event, &self.mutation) {
            (
                crate::CapabilityAuthorizationEvent::AgentIdentityInstalled { agent_id, .. },
                BootstrapAuthorizationMutationV1::Revocation(receipt),
            ) => {
                receipt.validate_transaction(tx).map_err(overlay_error)?;
                if !receipt.uses_budget(common.memory())
                    || receipt.key() != agent_id
                    || !matches!(
                        receipt.kind(),
                        BootstrapRevocationWriteKindV1::AgentIdentity
                    )
                {
                    return Err(BodyMutationError::Invalid(
                        "Bootstrap identity storage receipt",
                    ));
                }
            }
            (
                crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, .. },
                BootstrapAuthorizationMutationV1::Revocation(receipt),
            ) => {
                receipt.validate_transaction(tx).map_err(overlay_error)?;
                if self.check.is_none()
                    || !receipt.uses_budget(common.memory())
                    || receipt.key() != record.issuer_id
                    || !matches!(receipt.kind(), BootstrapRevocationWriteKindV1::Authority)
                {
                    return Err(BodyMutationError::Invalid(
                        "Bootstrap authority storage receipt",
                    ));
                }
            }
            (
                crate::CapabilityAuthorizationEvent::GrantRegistered { .. },
                BootstrapAuthorizationMutationV1::Grant(write),
            ) => {
                tx.validate_transaction_identity(&write.guard)?;
                if self.check.is_none() || !write.fee.uses_budget(common.memory()) {
                    return Err(BodyMutationError::Invalid(
                        "Bootstrap grant storage receipt",
                    ));
                }
            }
            (
                crate::CapabilityAuthorizationEvent::InvocationContextInstalled { .. },
                BootstrapAuthorizationMutationV1::Invocation(write),
            ) => {
                tx.validate_transaction_identity(&write.guard)?;
                if !write.fee.uses_budget(common.memory()) {
                    return Err(BodyMutationError::Invalid(
                        "Bootstrap invocation storage receipt",
                    ));
                }
            }
            _ => return Err(BodyMutationError::Invalid("Bootstrap storage event kind")),
        }
        Ok(())
    }
}
struct WriteLoan<'a, 'r, 'b, 'w, P, R, E, C> {
    read: &'a mut WorldFieldReadContextV1<'r, P, R, E, C>,
    work: &'a mut CurrentBootstrapWorkV1<'b, 'w>,
}
impl<P, R, E, C> FactoryPhaseMeterLanesV1 for WriteLoan<'_, '_, '_, '_, P, R, E, C> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        use FactoryPhaseMeterLaneV1::{Bytes as B, Steps as S};
        v(S(&mut self.read.values.io.nodes_remaining));
        v(B(&mut self.read.values.io.read_bytes_remaining));
        v(B(&mut self.read.values.io.write_bytes_remaining));
        v(B(&mut self.read.key_work.bytes_remaining));
        let read = &mut *self.work.module.capability_authorization;
        v(S(&mut read.scan.nodes_remaining));
        v(S(&mut read.scan.leaves_remaining));
        v(B(&mut read.scan.read_bytes_remaining));
        v(S(&mut read.typed.json.cells_remaining));
        v(S(&mut read.typed.json.json_nodes_remaining));
        v(S(&mut read.json.cells_remaining));
        v(S(&mut read.json.json_nodes_remaining));
        v(S(&mut self.work.revocation_write.cells_remaining));
        v(S(&mut self.work.revocation_write.json.cells_remaining));
        v(S(&mut self.work.revocation_write.json.json_nodes_remaining));
        v(S(&mut self.work.authorization_write.cells_remaining));
        v(S(&mut self.work.cognition.write.cells_remaining));
        v(S(&mut self.work.cognition.write.json_nodes_remaining));
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
    pub(super) fn write_current_bootstrap_authorization_v1<PE, PC>(
        &mut self,
        plan: PreparedBootstrapAuthorizationStateV1,
        sources: &BootstrapLifecycleSourcesV1<'_>,
        revocation: &mut RuntimeRevocationCurrentV1,
        parent: &mut WorldFieldReadContextV1<'_, M, R, PE, PC>,
        work: &mut CurrentBootstrapWorkV1<'_, '_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<WrittenCurrentBootstrapAuthorizationV1, RootModuleExecutionErrorV1<O::Error>>
    where
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        self.ctx
            .transaction
            .validate_guard(&plan.guard, &live_owner())?;
        if plan.core != common.work().identity()
            || !plan.fee.uses_budget(common.memory())
            || !plan.event_graph.uses_budget(common.memory())
            || !self.ctx.transaction.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid("Bootstrap fixed write plan context").into());
        }
        if let Some(check) = &plan.check {
            check.validate(self.ctx.transaction, common, &live_owner())?;
        }
        let core = common.work().identity();
        let header = std::mem::size_of::<WrittenCurrentBootstrapAuthorizationV1>();
        common.charge(1, (header * 2) as u64).map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        let written_fee = common
            .memory()
            .reserve(header, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let fee = common
            .memory()
            .reserve(header, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let mut external = WriteLoan { read: parent, work };
        let mutation = self.with_factory_prologue_external_meter_v1(
            common,
            live_owner,
            &mut external,
            |root, live, loan| {
                let guard = root.ctx.transaction.guard();
                match (&plan.before, &plan.event) {
                    (
                        BootstrapBeforeV1::Revocation(before),
                        event @ crate::CapabilityAuthorizationEvent::AgentIdentityInstalled {
                            ..
                        },
                    )
                    | (
                        BootstrapBeforeV1::Revocation(before),
                        event @ crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof {
                            ..
                        },
                    ) => {
                        let change = match event {
                            crate::CapabilityAuthorizationEvent::AgentIdentityInstalled {
                                agent_id,
                                identity,
                            } => BootstrapRevocationChangeV1::AgentIdentity { agent_id, identity },
                            crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof {
                                record,
                                proof,
                            } => BootstrapRevocationChangeV1::Authority { record, proof },
                            _ => {
                                return Err(BodyMutationError::Invalid(
                                    "Bootstrap revocation event",
                                )
                                .into());
                            }
                        };
                        let receipt = publish_current_bootstrap_revocation_v1(
                            before,
                            change,
                            sources.typed.revocation(),
                            revocation,
                            root.ctx,
                            loan.read,
                            &mut loan.work.module.capability_authorization.scan,
                            &mut loan.work.module.capability_authorization.typed,
                            root.members,
                            loan.work.revocation_write,
                            &guard,
                            &live(),
                        )
                        .map_err(overlay_error)?;
                        Ok(BootstrapAuthorizationMutationV1::Revocation(receipt))
                    }
                    (
                        BootstrapBeforeV1::Grant { key, before, after },
                        crate::CapabilityAuthorizationEvent::GrantRegistered { grant },
                    ) => {
                        if **key != grant.grant_id {
                            return Err(
                                BodyMutationError::Invalid("Bootstrap grant actual key").into()
                            );
                        }
                        let changed = change_authorization_grant_overlay_v1(
                            sources.typed.grants(),
                            sources.families.roots,
                            AuthorizationGrantChangeV1 {
                                key,
                                expected: before.as_deref(),
                                after: Some(after),
                            },
                            root.ctx.transaction,
                            loan.read,
                            root.members,
                            &guard,
                            &live(),
                            &mut loan.work.module.capability_authorization.json,
                            loan.work.module.max_selected_heap,
                            &mut loan.work.cognition.write,
                        )
                        .map_err(overlay_error)?;
                        if !changed {
                            return Err(BodyMutationError::Invalid(
                                "Bootstrap grant replay reached write publication",
                            )
                            .into());
                        }
                        Ok(BootstrapAuthorizationMutationV1::Grant(
                            ActualBootstrapGrantWriteV1 {
                                guard: root.ctx.transaction.guard(),
                                fee,
                            },
                        ))
                    }
                    (
                        BootstrapBeforeV1::Invocation { key, before },
                        crate::CapabilityAuthorizationEvent::InvocationContextInstalled {
                            key: event_key,
                            context,
                        },
                    ) => {
                        if **key != *event_key {
                            return Err(BodyMutationError::Invalid(
                                "Bootstrap invocation actual key",
                            )
                            .into());
                        }
                        let changed = change_authorization_record_overlay_v1(
                            sources.typed.authorization_records(),
                            sources.families.roots,
                            AuthorizationRecordChangeV1::Invocation {
                                key,
                                expected: before.as_deref(),
                                after: Some(context),
                            },
                            root.ctx.transaction,
                            loan.read,
                            root.members,
                            &guard,
                            &live(),
                            &mut loan.work.module.capability_authorization.scan,
                            &mut loan.work.module.capability_authorization.json,
                            loan.work.module.max_selected_heap,
                            loan.work.authorization_write,
                            &mut loan.work.cognition.write,
                        )
                        .map_err(overlay_error)?;
                        if !changed {
                            return Err(BodyMutationError::Invalid(
                                "Bootstrap context replay reached write publication",
                            )
                            .into());
                        }
                        Ok(BootstrapAuthorizationMutationV1::Invocation(
                            ActualBootstrapInvocationWriteV1 {
                                guard: root.ctx.transaction.guard(),
                                fee,
                            },
                        ))
                    }
                    _ => Err(BodyMutationError::Invalid("Bootstrap fixed event/write plan").into()),
                }
            },
        )?;
        // The event's own graph charge moves intact. The no-longer-used
        // before-row owners drop here; the written header has its separate fee.
        Ok(WrittenCurrentBootstrapAuthorizationV1 {
            event: plan.event,
            event_graph: plan.event_graph,
            mutation,
            check: plan.check,
            guard: self.ctx.transaction.guard(),
            core,
            now: plan.now,
            fee: written_fee,
        })
    }
}
