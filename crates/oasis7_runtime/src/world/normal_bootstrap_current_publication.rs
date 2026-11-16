//! Bootstrap's four fixed authorization publications. Only the actual
//! bootstrap storage+projection publisher in this module's parent may mint
//! these owners; no raw-event or caller-verdict constructor is exposed.
use super::*;
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;

/// Historical evidence after the four-source CAS and the closed seven-source
/// authorization-root CAS. It is not fresh authorization for another mutation.
pub(in crate::world) struct CommittedCurrentBootstrapAuthorizationV1 {
    event: crate::WorldEvent,
    event_graph: oasis7_proto::memory_budget::MemoryReservation,
    root: crate::world::incremental_authorization_root_current::PublishedCurrentBootstrapRootV1,
    storage_cut: PreparedGuard,
    pub(super) mutation: BootstrapAuthorizationMutationV1,
    pub(super) check: Option<VerifiedBootstrapCapabilityCheckV1>,
    pub(super) guard: PreparedGuard,
    pub(super) core: WorkIdentityV1,
    pub(super) parent: [u8; 32],
    pub(super) now: u64,
    pub(super) fee: ReservedValue<()>,
}
pub(super) struct VerifiedBootstrapCapabilityCheckV1 {
    pub(super) guard: PreparedGuard,
    pub(super) core: WorkIdentityV1,
    pub(super) fee: ReservedValue<()>,
    pub(super) _sources: crate::world::ModuleCapabilityAdmissionFeesV1,
}
impl VerifiedBootstrapCapabilityCheckV1 {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "Bootstrap checked capability provenance",
            ));
        }
        Ok(())
    }
}
// Variants are exhaustive for the fixed producer. Mutation receipts are
// private and originate only after the corresponding actual storage kernel.
pub(super) enum BootstrapAuthorizationMutationV1 {
    Revocation(
        ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::BootstrapRevocationWriteReceiptV1,
        >,
    ),
    Grant(ActualBootstrapGrantWriteV1),
    Invocation(ActualBootstrapInvocationWriteV1),
}
pub(super) struct ActualBootstrapGrantWriteV1 {
    pub(super) guard: PreparedGuard,
    pub(super) fee: ReservedValue<()>,
}
pub(super) struct ActualBootstrapInvocationWriteV1 {
    pub(super) guard: PreparedGuard,
    pub(super) fee: ReservedValue<()>,
}
impl CommittedCurrentBootstrapAuthorizationV1 {
    pub(in crate::world) fn now(&self) -> u64 {
        self.now
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn authorization_event(&self) -> &crate::CapabilityAuthorizationEvent {
        match &self.event.body {
            crate::WorldEventBody::CapabilityAuthorization(event) => event,
            _ => unreachable!("private fixed Bootstrap event"),
        }
    }
    pub(in crate::world) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != common.work().identity()
            || self.now != now
            || self.parent != tx.parent_fence().parent_roots
            || !self.fee.uses_budget(common.memory())
            || !self.event_graph.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "Bootstrap authorization publication provenance",
            ));
        }
        if matches!(
            self.authorization_event(),
            crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof { .. }
                | crate::CapabilityAuthorizationEvent::GrantRegistered { .. }
        ) && self.check.is_none()
        {
            return Err(BodyMutationError::Invalid(
                "Bootstrap publication missing genuine capability check",
            ));
        }
        if let Some(check) = &self.check {
            check.validate(tx, common, live)?;
        }
        self.root
            .validate_evidence(&self.storage_cut, tx, common, live)?;
        match (&self.mutation, self.authorization_event()) {
            (
                BootstrapAuthorizationMutationV1::Revocation(receipt),
                crate::CapabilityAuthorizationEvent::AgentIdentityInstalled { agent_id, .. },
            ) => {
                receipt
                    .validate_transaction(tx)
                    .map_err(super::overlay_error)?;
                if !receipt.uses_budget(common.memory())||receipt.key()!=agent_id||!matches!(receipt.kind(),oasis7_world_state::incremental_runtime_revocation::BootstrapRevocationWriteKindV1::AgentIdentity){return Err(BodyMutationError::Invalid("Bootstrap identity publication receipt"))}
            }
            (
                BootstrapAuthorizationMutationV1::Revocation(receipt),
                crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof { record, .. },
            ) => {
                receipt
                    .validate_transaction(tx)
                    .map_err(super::overlay_error)?;
                if !receipt.uses_budget(common.memory())||receipt.key()!=record.issuer_id||!matches!(receipt.kind(),oasis7_world_state::incremental_runtime_revocation::BootstrapRevocationWriteKindV1::Authority){return Err(BodyMutationError::Invalid("Bootstrap authority publication receipt"))}
            }
            (
                BootstrapAuthorizationMutationV1::Grant(write),
                crate::CapabilityAuthorizationEvent::GrantRegistered { .. },
            ) => {
                tx.validate_transaction_identity(&write.guard)?;
                if !write.fee.uses_budget(common.memory()) {
                    return Err(BodyMutationError::Invalid(
                        "Bootstrap grant publication fee",
                    ));
                }
            }
            (
                BootstrapAuthorizationMutationV1::Invocation(write),
                crate::CapabilityAuthorizationEvent::InvocationContextInstalled { .. },
            ) => {
                tx.validate_transaction_identity(&write.guard)?;
                if !write.fee.uses_budget(common.memory()) {
                    return Err(BodyMutationError::Invalid(
                        "Bootstrap invocation publication fee",
                    ));
                }
            }
            _ => {
                return Err(BodyMutationError::Invalid(
                    "Bootstrap authorization publication kind",
                ));
            }
        }
        Ok(())
    }
}

impl super::write::WrittenCurrentBootstrapAuthorizationV1 {
    /// The only bridge into the fixed WorldEvent sink: both real state writes
    /// and their actual closed authorization-root CAS must already exist.
    pub(super) fn into_committed_v1<
        M: MerkleNodeReader,
        R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        self,
        root: crate::world::incremental_authorization_root_current::PublishedCurrentBootstrapRootV1,
        ctx: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
            '_,
            '_,
            '_,
            M,
            R,
            E,
            C,
        >,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<CommittedCurrentBootstrapAuthorizationV1, BodyMutationError> {
        root.validate_evidence(&self.guard, ctx.transaction, common, live)?;
        common
            .charge(
                1,
                std::mem::size_of::<CommittedCurrentBootstrapAuthorizationV1>() as u64,
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CommittedCurrentBootstrapAuthorizationV1>(),
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let id=crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(ctx,common)?;
        let event = crate::WorldEvent {
            id,
            time: self.now,
            caused_by: None,
            body: crate::WorldEventBody::CapabilityAuthorization(self.event),
        };
        common
            .consume_steps(0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        Ok(CommittedCurrentBootstrapAuthorizationV1 {
            event,
            event_graph: self.event_graph,
            root,
            storage_cut: self.guard,
            mutation: self.mutation,
            check: self.check,
            guard: ctx.transaction.guard(),
            core: self.core,
            parent: ctx.transaction.parent_fence().parent_roots,
            now: self.now,
            fee,
        })
    }
}
