//! A complete GEN-only Qualified entry, original history node and retained Index.
//! Inputs are privately issued from full GEN bytes plus the SAME verified quorum.
use super::super::FormalArchiveReadWorkV1;
use super::super::incremental::genesis::GenesisValidatedInputs;
use super::bounded_index::copy_paid;
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub(super) struct PreparedGenesisQualifiedUpdate {
    pub(super) update: PreparedPendingRequestUpdate,
    pub(super) inputs: GenesisValidatedInputs,
    pub(super) history: ReservedValue<String>,
    pub(super) record: ReservedValue<String>,
}
impl PreparedGenesisQualifiedUpdate {
    pub(super) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.update.uses_budget(memory)
            && self.inputs.uses_budget(memory)
            && self.history.uses_budget(memory)
            && self.record.uses_budget(memory)
    }
    pub(super) fn reserved_bytes(&self) -> usize {
        self.update.reserved_bytes()
            + self.inputs.reserved_bytes()
            + self.history.reserved_bytes()
            + self.record.reserved_bytes()
    }
    pub(super) fn closure(&self) -> &str {
        self.inputs
            .state()
            .state()
            .local_data_closure_reference()
            .expect("private GEN Qualified closure")
    }
}
pub(super) fn prepare_genesis_qualified_update(
    store: &Store,
    inputs: GenesisValidatedInputs,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PreparedGenesisQualifiedUpdate, FormalError> {
    inputs.validate_context(memory, work, stop)?;
    let commit = inputs.verified();
    work.charge(
        1,
        (store.body.head.record_id.len() + commit.record().parent_record_id.len()) as u64,
        stop,
    )?;
    if store.body.head.position != 0
        || store.body.head_node.is_some()
        || store.body.imported_head_node.is_some()
        || store.body.import_digest.is_some()
        || store.body.base_parent.is_some()
        || store.body.head.record_id != commit.record().parent_record_id
        || commit.record().position != 1
    {
        return Err(invalid("GEN Qualified exact initial Store parent"));
    }
    let update = store.prepare_request_update_bounded(inputs.state(), memory, work, stop)?;
    let mut fixed = [0u8; 64];
    hex::encode_to_slice(commit.verified_record_id(), &mut fixed)
        .map_err(|_| invalid("GEN verified record ID"))?;
    let id = std::str::from_utf8(&fixed).map_err(|_| invalid("GEN verified record ID"))?;
    let record = copy_paid(id, memory, work, stop)?;
    let payload = record
        .len()
        .checked_add(update.request_digest().len())
        .and_then(|n| n.checked_add(update.entry_id().len()))
        .ok_or_else(|| invalid("GEN history layout"))?;
    let fee = memory
        .reserve(
            payload
                .checked_add(std::mem::size_of::<HistoryNode>())
                .ok_or_else(|| invalid("GEN history layout"))?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("GEN history admission"))?;
    work.charge(1, payload as u64, stop)?;
    // This is the original first-node append_node result: GEN has no older
    // permanent history node, therefore the original ancestors vector is empty.
    let node = HistoryNode {
        head: FormalHeadRefV1 {
            position: 1,
            record_id: (*record).clone(),
        },
        request: update.request_digest().to_owned(),
        entry: update.entry_id().to_owned(),
        ancestors: Vec::new(),
    };
    if node.head.record_id.capacity() > node.head.record_id.len()
        || node.request.capacity() > node.request.len()
        || node.entry.capacity() > node.entry.len()
    {
        return Err(invalid("GEN history capacity"));
    }
    let node = fee.retain(node);
    let history = {
        update.validate_source(store, memory, work, stop)?;
        let staged = super::stage_io::stage_history_node_bounded_v1(
            update.publication_guard(),
            &node,
            memory,
            work,
            stop,
        )?;
        let retained =
            crate::controlled_authority::formal::physical::roots::append_staged_history_bounded_v1(
                &staged, memory, work, stop,
            )?;
        let id = copy_paid(staged.id(), memory, work, stop)?;
        drop(retained);
        drop(staged);
        id
    };
    Ok(PreparedGenesisQualifiedUpdate {
        update,
        inputs,
        history,
        record,
    })
}

/// Only this successful Store transaction can construct durable publication.
/// The real job lease continues to pin CompleteData until cache promotion.
pub(in crate::controlled_authority::formal::endpoint) struct DurableGenesisPublication<'a> {
    quorum: oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedGenesisCommitV1,
    job: super::super::incremental::jobs::GenesisJobLease<'a>,
}
impl<'a> DurableGenesisPublication<'a> {
    pub(in crate::controlled_authority::formal::endpoint) fn into_parts(
        self,
    ) -> (
        oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedGenesisCommitV1,
        super::super::incremental::jobs::GenesisJobLease<'a>,
    ) {
        (self.quorum, self.job)
    }
}
impl Store {
    pub(in crate::controlled_authority::formal::endpoint) fn publish_genesis_install_bounded<'j>(
        &mut self,
        permit: super::super::incremental::genesis::GenesisInstallPermit<'j>,
        provider: &crate::controlled_authority::fencing::RemoteMonotonicEndpointProvider,
        instance: &fencing::EndpointInstanceBindingV1,
        policy: &fencing::TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<
        DurableGenesisPublication<'j>,
        crate::controlled_authority::fencing::BoundedAcquireErrorV1,
    > {
        use crate::controlled_authority::fencing::{
            acquire_provider_lease_bounded_v1, begin_signed_native_checkpoint_bounded_v1,
            sign_native_checkpoint_bounded_v1,
        };
        let (job, inputs) = permit.split();
        if self.body.head.position == 1 {
            inputs.validate_context(memory, work, stop)?;
            let mut fixed = [0u8; 64];
            work.charge(1, 64, stop)?;
            hex::encode_to_slice(inputs.verified().verified_record_id(), &mut fixed)
                .map_err(|_| invalid("GEN accepted record ID"))?;
            let expected =
                std::str::from_utf8(&fixed).map_err(|_| invalid("GEN accepted record ID"))?;
            if !super::bounded_transaction::same(
                &self.body.head.record_id.as_str(),
                &expected,
                memory,
                work,
                stop,
            )? || !super::bounded_transaction::same(
                &self.body.data_closure.as_deref(),
                &inputs.state().state().local_data_closure_reference(),
                memory,
                work,
                stop,
            )? {
                return Err(invalid("GEN accepted native head/closure mismatch").into());
            }
            let actual = self
                .read_request_bounded(
                    &inputs.verified().record().request_digest,
                    memory,
                    work,
                    stop,
                )?
                .ok_or_else(|| invalid("GEN accepted Qualified entry missing"))?;
            if !super::bounded_transaction::same(
                actual.state(),
                inputs.state().state(),
                memory,
                work,
                stop,
            )? {
                return Err(invalid("GEN accepted exact Qualified entry").into());
            }
            let disk = super::bounded_fresh_draft::disk_source(
                self,
                &instance.endpoint_public_key,
                memory,
                work,
                stop,
            )?;
            super::bounded_native_refs::validate_recovery_refs(
                self,
                disk.body(),
                memory,
                work,
                stop,
            )?;
            let mut ctx = work.bounded_context(memory, stop);
            let lease = acquire_provider_lease_bounded_v1(
                provider,
                instance,
                &self.checkpoint,
                policy,
                &mut ctx,
            )?;
            drop(ctx);
            work.consume_steps(1, stop)?;
            if lease.pending().is_some()
                || lease.protected_custody()
                    != Some(&fencing::ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
                || lease.native_checkpoint_bytes().is_none()
            {
                return Err(FormalError::FencingUnproven.into());
            }
            if !super::bounded_transaction::same(
                &lease.native_checkpoint_bytes(),
                &Some(disk.bytes()),
                memory,
                work,
                stop,
            )? || !super::bounded_transaction::same(
                lease.checkpoint(),
                &self.checkpoint,
                memory,
                work,
                stop,
            )? {
                return Err(FormalError::FencingUnproven.into());
            }
            return Ok(DurableGenesisPublication {
                quorum: inputs.into_quorum(),
                job,
            });
        }
        let source = prepare_genesis_qualified_update(self, inputs, memory, work, stop)?;
        let disk = super::bounded_fresh_draft::disk_source(
            self,
            &instance.endpoint_public_key,
            memory,
            work,
            stop,
        )?;
        let draft = super::bounded_native_draft::prepare_genesis_draft_from_sources_v1(
            self, source, memory, work, stop,
        )?
        .retain_disk_source(disk);
        let draft = super::bounded_native_refs::validate_native_checkpoint_refs_bounded_v1(
            draft, memory, work, stop,
        )?
        .into_draft();
        let mut ctx = work.bounded_context(memory, stop);
        let lease = acquire_provider_lease_bounded_v1(
            provider,
            instance,
            &self.checkpoint,
            policy,
            &mut ctx,
        )?;
        let signed = sign_native_checkpoint_bounded_v1(provider, lease, draft, &mut ctx)?;
        let checkpoint =
            super::bounded_signed_checkpoint::prepare_signed_native_checkpoint_bounded_v1(
                signed, &mut ctx,
            )?;
        let begun = begin_signed_native_checkpoint_bounded_v1(provider, checkpoint, &mut ctx)?;
        let (body, body_fee) = begun.draft().paid_cache_copy(&mut ctx)?;
        drop(ctx);
        let (binding, binding_fee) = super::bounded_transaction::copy_checkpoint(
            begun.next_checkpoint(),
            memory,
            work,
            stop,
        )?;
        #[cfg(test)]
        if self.fault == Some(1) {
            return Err(FormalError::Io("injected after provider intent".into()).into());
        }
        self.atomic_hot_bounded(begun.native_bytes(), memory, work, stop)?;
        #[cfg(test)]
        if self.fault == Some(2) {
            return Err(FormalError::Io("injected after native checkpoint".into()).into());
        }
        let mut ctx = work.bounded_context(memory, stop);
        let finished = begun.finish_bounded(provider, &mut ctx)?;
        let source = finished.consume_draft(|draft| draft.into_genesis_sources())?;
        let quorum = source.inputs.into_quorum();
        self.body = body;
        self.checkpoint = binding;
        self._bounded_cache_fee = Some(body_fee);
        self._bounded_checkpoint_fee = Some(binding_fee);
        Ok(DurableGenesisPublication { quorum, job })
    }
}

#[cfg(test)]
#[path = "endpoint_store_bounded_genesis_qualified_tests.rs"]
mod tests;
