//! Normal history append and typed Qualified Store sources. Shared codecs,
//! physical roots and native atomic+Finish are the original bounded kernels.
use super::super::{
    FormalArchiveReadWorkV1,
    incremental::normal_qualified::{NormalInstallPermit, NormalValidatedInputs},
};
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, EndpointInstanceBindingV1, RemoteMonotonicEndpointProvider,
    TrustedFencingPolicyV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub(super) struct PreparedNormalQualifiedUpdate {
    pub update: PreparedPendingRequestUpdate,
    pub inputs: NormalValidatedInputs,
    pub history: ReservedValue<String>,
    pub record: ReservedValue<String>,
}
impl PreparedNormalQualifiedUpdate {
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
            .expect("private normal complete Qualified closure")
    }
}
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("normal history layout"))
}
fn push_link(
    link: &HistoryLink,
    ancestors: &mut Vec<HistoryLink>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, FormalError> {
    let n = add(link.head.record_id.len(), link.node.len())?;
    let fee = memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("normal history link admission"))?;
    work.charge(1, n as u64, stop)?;
    let value = link.clone();
    if value.head.record_id.capacity() > value.head.record_id.len()
        || value.node.capacity() > value.node.len()
    {
        return Err(invalid("normal history link capacity"));
    }
    ancestors.push(value);
    Ok(fee)
}
fn append_history(
    store: &Store,
    update: &PreparedPendingRequestUpdate,
    record: &str,
    position: u64,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<String>, FormalError> {
    if Some(position) != store.body.head.position.checked_add(1) {
        return Err(invalid("formal history position"));
    }
    let payload = add(
        add(record.len(), update.request_digest().len())?,
        update.entry_id().len(),
    )?;
    let slots = 64usize
        .checked_mul(
            std::mem::size_of::<HistoryLink>()
                + std::mem::size_of::<oasis7_proto::memory_budget::MemoryReservation>(),
        )
        .ok_or_else(|| invalid("normal history slots"))?;
    let _base = memory
        .reserve(
            add(
                add(payload, slots)?,
                std::mem::size_of::<HistoryNode>()
                    + std::mem::size_of::<Vec<oasis7_proto::memory_budget::MemoryReservation>>(),
            )?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("normal history admission"))?;
    work.charge(1, (payload + slots) as u64, stop)?;
    let mut ancestors = Vec::new();
    ancestors
        .try_reserve_exact(64)
        .map_err(|_| invalid("normal history ancestor slots"))?;
    let mut charges = Vec::new();
    charges
        .try_reserve_exact(64)
        .map_err(|_| invalid("normal history charge slots"))?;
    if ancestors.capacity() > 64 || charges.capacity() > 64 {
        return Err(invalid("normal history slot capacity"));
    }
    if let Some(id) = store
        .body
        .head_node
        .as_ref()
        .or(store.body.imported_head_node.as_ref())
    {
        // The first link is the original Store head; reserve both Strings before
        // constructing it (the temporary source stays paid through its real clone).
        let n = add(store.body.head.record_id.len(), id.len())?;
        let _temp = memory
            .reserve(
                n + std::mem::size_of::<HistoryLink>(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("normal first history link"))?;
        work.charge(1, n as u64, stop)?;
        let first = HistoryLink {
            head: store.body.head.clone(),
            node: id.clone(),
        };
        charges.push(push_link(&first, &mut ancestors, memory, work, stop)?);
        for level in 1..64 {
            let node = super::bounded_history_read::read_history_object_bounded_v1(
                store,
                &ancestors[level - 1].node,
                memory,
                work,
                stop,
            )?;
            let Some(link) = node.node().ancestors.get(level - 1) else {
                break;
            };
            if position.checked_sub(1u64 << level) != Some(link.head.position) {
                return Err(invalid("formal history ancestor position"));
            }
            charges.push(push_link(link, &mut ancestors, memory, work, stop)?);
        }
    }
    let node = HistoryNode {
        head: FormalHeadRefV1 {
            position,
            record_id: record.to_owned(),
        },
        request: update.request_digest().to_owned(),
        entry: update.entry_id().to_owned(),
        ancestors,
    };
    if node.head.record_id.capacity() > node.head.record_id.len()
        || node.request.capacity() > node.request.len()
        || node.entry.capacity() > node.entry.len()
    {
        return Err(invalid("normal history node capacity"));
    }
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
    let id = super::bounded_index::copy_paid(staged.id(), memory, work, stop)?;
    drop(retained);
    drop(staged);
    drop(node);
    drop(charges);
    Ok(id)
}
pub(super) fn prepare_normal_qualified_update(
    store: &Store,
    inputs: NormalValidatedInputs,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PreparedNormalQualifiedUpdate, FormalError> {
    inputs.validate_context(memory, work, stop)?;
    let commit = inputs.verified();
    work.charge(
        1,
        (store.body.head.record_id.len() + commit.record().parent_record_id.len()) as u64,
        stop,
    )?;
    if store.body.head.record_id != commit.record().parent_record_id
        || store.body.head.position.checked_add(1) != Some(commit.record().position)
        || !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(
            commit.record().schema_version,
        )
        || commit.record().kind != FormalRecordKindV1::Execution
    {
        return Err(invalid("normal Qualified actual Store predecessor"));
    }
    let update = store.prepare_request_update_bounded(inputs.state(), memory, work, stop)?;
    let mut fixed = [0u8; 64];
    work.charge(1, 64, stop)?;
    hex::encode_to_slice(commit.verified_record_id(), &mut fixed)
        .map_err(|_| invalid("normal record ID"))?;
    let id = std::str::from_utf8(&fixed).map_err(|_| invalid("normal record ID"))?;
    let record = super::bounded_index::copy_paid(id, memory, work, stop)?;
    let history = append_history(
        store,
        &update,
        &record,
        commit.record().position,
        memory,
        work,
        stop,
    )?;
    Ok(PreparedNormalQualifiedUpdate {
        update,
        inputs,
        history,
        record,
    })
}
pub(in crate::controlled_authority::formal::endpoint) struct DurableNormalPublication<'a> {
    quorum: oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedNormalCommitV1,
    job: super::super::incremental::jobs::NormalJobLease<'a>,
}
impl<'a> DurableNormalPublication<'a> {
    pub(in crate::controlled_authority::formal::endpoint) fn into_parts(
        self,
    ) -> (
        oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedNormalCommitV1,
        super::super::incremental::jobs::NormalJobLease<'a>,
    ) {
        (self.quorum, self.job)
    }
}
impl Store {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::controlled_authority::formal::endpoint) fn publish_normal_install_bounded<'j>(
        &mut self,
        permit: NormalInstallPermit<'j>,
        provider: &RemoteMonotonicEndpointProvider,
        instance: &EndpointInstanceBindingV1,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<DurableNormalPublication<'j>, BoundedAcquireErrorV1> {
        let mut lease = None;
        self.publish_normal_install_with_lease_bounded(
            permit, &mut lease, provider, instance, policy, memory, work, stop,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::controlled_authority::formal::endpoint) fn publish_normal_install_with_lease_bounded<
        'j,
    >(
        &mut self,
        permit: NormalInstallPermit<'j>,
        lease_slot: &mut Option<crate::controlled_authority::fencing::PaidRemoteLeaseV1>,
        provider: &RemoteMonotonicEndpointProvider,
        instance: &EndpointInstanceBindingV1,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<DurableNormalPublication<'j>, BoundedAcquireErrorV1> {
        let (job, inputs) = permit.split();
        inputs.validate_context(memory, work, stop)?;
        let mut fixed = [0u8; 64];
        work.charge(1, 64, stop)?;
        hex::encode_to_slice(inputs.verified().verified_record_id(), &mut fixed)
            .map_err(|_| invalid("normal accepted record ID"))?;
        let expected =
            std::str::from_utf8(&fixed).map_err(|_| invalid("normal accepted record ID"))?;
        if self.body.head.position == inputs.verified().record().position {
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
                return Err(invalid("normal installed head/closure mismatch").into());
            }
            let actual = self
                .read_request_bounded(
                    &inputs.verified().record().request_digest,
                    memory,
                    work,
                    stop,
                )?
                .ok_or_else(|| invalid("normal installed Qualified missing"))?;
            if !super::bounded_transaction::same(
                actual.state(),
                inputs.state().state(),
                memory,
                work,
                stop,
            )? {
                return Err(invalid("normal installed original Qualified mismatch").into());
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
            let lease = match lease_slot.take() {
                Some(lease) => lease,
                None => crate::controlled_authority::fencing::acquire_provider_lease_bounded_v1(
                    provider,
                    instance,
                    &self.checkpoint,
                    policy,
                    &mut work.bounded_context(memory, stop),
                )?,
            };
            lease.validate_current_binding(
                instance,
                &self.checkpoint,
                policy,
                &mut work.bounded_context(memory, stop),
            )?;
            work.consume_steps(1, stop)?;
            if lease.pending().is_some()
                || lease.protected_custody()
                    != Some(&fencing::ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
                || lease.native_checkpoint_bytes().is_none()
                || !super::bounded_transaction::same(
                    &lease.native_checkpoint_bytes(),
                    &Some(disk.bytes()),
                    memory,
                    work,
                    stop,
                )?
                || !super::bounded_transaction::same(
                    lease.checkpoint(),
                    &self.checkpoint,
                    memory,
                    work,
                    stop,
                )?
            {
                return Err(FormalError::FencingUnproven.into());
            }
            *lease_slot = Some(lease);
            return Ok(DurableNormalPublication {
                quorum: inputs.into_quorum(),
                job,
            });
        }
        let source = prepare_normal_qualified_update(self, inputs, memory, work, stop)?;
        let disk = super::bounded_fresh_draft::disk_source(
            self,
            &instance.endpoint_public_key,
            memory,
            work,
            stop,
        )?;
        let draft = super::bounded_native_draft::prepare_normal_draft_from_sources_v1(
            self, source, memory, work, stop,
        )?
        .retain_disk_source(disk);
        let draft = super::bounded_native_refs::validate_native_checkpoint_refs_bounded_v1(
            draft, memory, work, stop,
        )?
        .into_draft();
        let lease = match lease_slot.take() {
            Some(lease) => lease,
            None => crate::controlled_authority::fencing::acquire_provider_lease_bounded_v1(
                provider,
                instance,
                &self.checkpoint,
                policy,
                &mut work.bounded_context(memory, stop),
            )?,
        };
        let installed =
            super::bounded_qualified_install::install_signed_qualified_draft_with_lease_bounded(
                self,
                draft,
                lease,
                provider,
                instance,
                policy,
                memory,
                work,
                stop,
                |draft| draft.into_normal_sources(),
            )?;
        let quorum = installed.value.inputs.into_quorum();
        *lease_slot = Some(installed.lease);
        self.body = installed.body;
        self.checkpoint = installed.checkpoint;
        self._bounded_cache_fee = Some(installed.body_fee);
        self._bounded_checkpoint_fee = Some(installed.checkpoint_fee);
        Ok(DurableNormalPublication { quorum, job })
    }
}
