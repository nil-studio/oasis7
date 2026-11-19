//! Bootstrap-only changed-path projection and actual authorization Text CAS.
//! The input is the actual fixed state-write owner, never an OutcomeRecorded
//! substitute, raw event, caller root, or predicate callback.
use super::*;
use crate::world::incremental_body::BodyMutationError;
use crate::world::normal_bootstrap_current::WrittenCurrentBootstrapAuthorizationV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_revocation::{
        ModuleAdmissionRevocationRequestV1, ModuleAdmissionRevocationValueV1,
        RuntimeRevocationCurrentV1,
    },
    incremental_runtime_typed_complete::RuntimeTypedCompleteQualificationV1,
};

/// Historical receipt for a real root CAS. The stored original write cut
/// prevents binding a different bootstrap storage owner after publication.
pub(in crate::world) struct PublishedCurrentBootstrapRootV1 {
    storage_cut: PreparedGuard,
    guard: PreparedGuard,
    core: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    parent: [u8; 32],
    fee: ReservedValue<()>,
}
impl PublishedCurrentBootstrapRootV1 {
    pub(in crate::world) fn validate_evidence<M: MerkleNodeReader>(
        &self,
        storage_cut: &PreparedGuard,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.storage_cut.same_cut(storage_cut)
            || self.parent != tx.parent_fence().parent_roots
            || self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !tx.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "Bootstrap actual root receipt provenance",
            ));
        }
        Ok(())
    }
}
fn encoded_node<T: serde::Serialize + ?Sized>(
    value: &T,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ValueRef, SourceError> {
    let raw = wire::encode(value, limits, common, MemoryClass::Ordinary)?;
    Ok(cbor::parse(&raw, limits, common, MemoryClass::Ordinary)?)
}
impl CurrentAuthorizationProjectionV1<'_> {
    fn bootstrap_patch<T: serde::Serialize + ?Sized>(
        &mut self,
        path: &[AuthorizationProjectionPathV1<'_>],
        value: &T,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), SourceError> {
        let node = encoded_node(value, self.data.limits, common)?;
        self.data.root = patch(
            &self.data.root,
            path,
            Some(node),
            self.data.limits,
            common,
            MemoryClass::Ordinary,
        )?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn publish_bootstrap_root_v1<M, R, E, C, V, ER, CR, W, CW>(
        &mut self,
        written: &WrittenCurrentBootstrapAuthorizationV1,
        qualified: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
        current: &RuntimeRevocationCurrentV1,
        roots: &RootSetV1,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        parent: &mut WorldFieldReadContextV1<'_, M, V, ER, CR>,
        write: &mut MemberRecordWriteContext<'_, W, CW>,
        work: &mut AuthorizationRootSourceReadWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PublishedCurrentBootstrapRootV1, WarmError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        V: ValueNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        W: ValueNodeReader + ValueNodeWriter,
        CW: FnMut() -> bool,
    {
        let attempted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<PublishedCurrentBootstrapRootV1, WarmError> {
                self.validate(overlay.transaction, common)?;
                written
                    .validate_current(overlay.transaction, common, live)
                    .map_err(WarmError::Body)?;
                // Capture an owned observation of this exact current storage cut
                // before any subsequent reader changes the transaction read set.
                // Equality with the genuine writer cut is mandatory: this is not
                // a refreshed version of that writer's proof.
                let storage_cut = overlay.transaction.guard();
                if !storage_cut.same_cut(written.storage_cut()) {
                    return Err(invalid("Bootstrap writer exact storage cut").into());
                }
                read::metadata(roots, common)?;
                qualified
                    .validate_binding(roots)
                    .map_err(SourceError::from)?;
                if !qualified.uses_budget(common.memory())
                    || qualified.allocation_class() != MemoryClass::Ordinary
                {
                    return Err(invalid("Bootstrap projection qualified source").into());
                }
                // Reserve the retained receipt before any persistent graph change.
                let fee = common
                    .memory()
                    .reserve(
                        std::mem::size_of::<PublishedCurrentBootstrapRootV1>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(SourceError::from)?
                    .retain(());
                match written.event() {
                    crate::CapabilityAuthorizationEvent::AgentIdentityInstalled {
                        agent_id,
                        identity,
                    } => {
                        let receipt = written
                            .revocation_receipt()
                            .ok_or_else(|| invalid("Bootstrap identity actual receipt"))?;
                        if receipt.before_body() != &*self.revocation_body
                            || receipt.after_body() != current.body()
                        {
                            return Err(invalid("Bootstrap identity revocation chain").into());
                        }
                        self.bootstrap_patch(
                            &[
                                AuthorizationProjectionPathV1::Field("revocation"),
                                AuthorizationProjectionPathV1::Field("agent_identities"),
                                AuthorizationProjectionPathV1::Field(agent_id),
                            ],
                            identity,
                            common,
                        )?;
                        self.register_revocation(
                            overlay.transaction,
                            &mut work.pending_entries,
                            common,
                        )?;
                        self.revocation_body = paid_body(current.body(), common)?;
                    }
                    crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof {
                        record,
                        proof,
                    } => {
                        let receipt = written
                            .revocation_receipt()
                            .ok_or_else(|| invalid("Bootstrap authority actual receipt"))?;
                        if receipt.before_body() != &*self.revocation_body
                            || receipt.after_body() != current.body()
                        {
                            return Err(invalid("Bootstrap authority revocation chain").into());
                        }
                        // Both values come from the actual post-write body and the
                        // original selected column codecs, not guessed projections.
                        let guard = overlay.transaction.guard();
                        let epoch = read::paid(
                            roots,
                            overlay,
                            parent,
                            work,
                            common,
                            |overlay, parent, work| {
                                oasis7_world_state::incremental_runtime_revocation::read_current_module_admission_revocation_v1(ModuleAdmissionRevocationRequestV1::Epoch,qualified.revocation(),overlay,parent,&mut work.scan,&mut work.typed,&guard,live).map_err(SourceError::from)
                            },
                        )?;
                        let ModuleAdmissionRevocationValueV1::Epoch(epoch) = &*epoch else {
                            return Err(invalid("Bootstrap actual epoch selected codec").into());
                        };
                        let guard = overlay.transaction.guard();
                        let revoked = read::paid(
                            roots,
                            overlay,
                            parent,
                            work,
                            common,
                            |overlay, parent, work| {
                                oasis7_world_state::incremental_runtime_revocation::read_current_bootstrap_revoked_set_v1(qualified.revocation(),current,overlay,parent,&mut work.scan,&mut work.typed,&guard,live).map_err(SourceError::from)
                            },
                        )?;
                        self.bootstrap_patch(
                            &[
                                AuthorizationProjectionPathV1::Field("revocation"),
                                AuthorizationProjectionPathV1::Field("epoch"),
                            ],
                            epoch,
                            common,
                        )?;
                        self.bootstrap_patch(
                            &[
                                AuthorizationProjectionPathV1::Field("revocation"),
                                AuthorizationProjectionPathV1::Field("revoked_grant_ids"),
                            ],
                            revoked.value(),
                            common,
                        )?;
                        for (key, value) in &record.superseded_by {
                            common.charge(
                                1,
                                key.len().checked_add(value.len()).ok_or_else(limited)? as u64,
                            )?;
                            self.bootstrap_patch(
                                &[
                                    AuthorizationProjectionPathV1::Field("revocation"),
                                    AuthorizationProjectionPathV1::Field("superseded_by"),
                                    AuthorizationProjectionPathV1::Field(key),
                                ],
                                value,
                                common,
                            )?;
                        }
                        self.bootstrap_patch(
                            &[
                                AuthorizationProjectionPathV1::Field("revocation"),
                                AuthorizationProjectionPathV1::Field("finalized_receipt_id"),
                            ],
                            &Some(&record.finalized_receipt_id),
                            common,
                        )?;
                        self.bootstrap_patch(
                            &[
                                AuthorizationProjectionPathV1::Field("revocation"),
                                AuthorizationProjectionPathV1::Field("authority_records"),
                                AuthorizationProjectionPathV1::Field(&record.issuer_id),
                            ],
                            record,
                            common,
                        )?;
                        self.bootstrap_patch(
                            &[
                                AuthorizationProjectionPathV1::Field("revocation"),
                                AuthorizationProjectionPathV1::Field("authority_finality_proofs"),
                                AuthorizationProjectionPathV1::Field(&record.issuer_id),
                            ],
                            proof,
                            common,
                        )?;
                        self.register_revocation(
                            overlay.transaction,
                            &mut work.pending_entries,
                            common,
                        )?;
                        self.revocation_body = paid_body(current.body(), common)?;
                    }
                    crate::CapabilityAuthorizationEvent::GrantRegistered { grant } => {
                        let node = encoded_node(grant, self.data.limits, common)?;
                        sources::grant(
                            qualified.grants(),
                            roots,
                            &grant.grant_id,
                            &node,
                            self.data.limits,
                            overlay,
                            parent,
                            work,
                            common,
                            live,
                        )?;
                        self.data.root = patch(
                            &self.data.root,
                            &[
                                AuthorizationProjectionPathV1::Field("grants"),
                                AuthorizationProjectionPathV1::Field(&grant.grant_id),
                            ],
                            Some(node),
                            self.data.limits,
                            common,
                            MemoryClass::Ordinary,
                        )?;
                        self.register_record(
                            "grants",
                            &grant.grant_id,
                            overlay.transaction,
                            &mut work.pending_entries,
                            common,
                        )?;
                    }
                    crate::CapabilityAuthorizationEvent::InvocationContextInstalled {
                        key,
                        context,
                    } => {
                        let node = encoded_node(context, self.data.limits, common)?;
                        sources::invocation(
                            qualified.authorization_records(),
                            roots,
                            key,
                            &node,
                            self.data.limits,
                            overlay,
                            parent,
                            work,
                            common,
                            live,
                        )?;
                        self.data.root = patch(
                            &self.data.root,
                            &[
                                AuthorizationProjectionPathV1::Field("invocation_contexts"),
                                AuthorizationProjectionPathV1::Field(key),
                            ],
                            Some(node),
                            self.data.limits,
                            common,
                            MemoryClass::Ordinary,
                        )?;
                        self.register_record(
                            "invocation_contexts",
                            key,
                            overlay.transaction,
                            &mut work.pending_entries,
                            common,
                        )?;
                    }
                    _ => return Err(invalid("Bootstrap fixed publication kind").into()),
                }
                self.validate_registered_writes(
                    overlay.transaction,
                    &mut work.pending_entries,
                    common,
                )?;
                let hash = self.data.hash(common)?;
                let after: [u8; 64] = hash
                    .as_bytes()
                    .try_into()
                    .map_err(|_| invalid("Bootstrap root canonical SHA256 length"))?;
                publish::write_closed_root(
                    &self.expected,
                    &after,
                    qualified.authorization_root(),
                    overlay,
                    write,
                    common,
                    live,
                )?;
                self.register_authorization_root(
                    overlay.transaction,
                    &mut work.pending_entries,
                    common,
                )?;
                self.validate_registered_writes(
                    overlay.transaction,
                    &mut work.pending_entries,
                    common,
                )?;
                common.consume_steps(0)?;
                let guard = overlay.transaction.guard();
                overlay
                    .transaction
                    .validate_guard(&guard, live)
                    .map_err(SourceError::Execution)?;
                self.expected = after;
                Ok(PublishedCurrentBootstrapRootV1 {
                    storage_cut,
                    guard,
                    core: common.work().identity(),
                    parent: overlay.transaction.parent_fence().parent_roots,
                    fee,
                })
            },
        ));
        match attempted {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    overlay.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    overlay.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        }
    }
}
