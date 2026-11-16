//! Actual signed-head Store rows for authenticated cold reconstruction.
use super::super::{FormalArchiveReadOwnerV1, FormalArchiveReadWorkV1};
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;

pub(in crate::controlled_authority::formal::endpoint) struct PaidColdStoreHeadV1 {
    node: super::bounded_history_read::PaidHistoryNodeV1,
    request: super::bounded_request::PaidRequestStateV1,
    parent_node: Option<super::bounded_history_read::PaidHistoryNodeV1>,
    parent: Option<super::bounded_request::PaidRequestStateV1>,
}
impl PaidColdStoreHeadV1 {
    pub(in crate::controlled_authority::formal::endpoint) fn state(&self) -> &FormalRequestStateV1 {
        self.request.state()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn parent_state(
        &self,
    ) -> Option<&FormalRequestStateV1> {
        self.parent.as_ref().map(|p| p.state())
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.node.uses_budget(memory)
            && self.request.uses_budget(memory)
            && self
                .parent_node
                .as_ref()
                .is_none_or(|p| p.uses_budget(memory))
            && self.parent.as_ref().is_none_or(|p| p.uses_budget(memory))
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.node.reserved_bytes()
            + self.request.reserved_bytes()
            + self.parent_node.as_ref().map_or(0, |p| p.reserved_bytes())
            + self.parent.as_ref().map_or(0, |p| p.reserved_bytes())
    }
}
impl Store {
    pub(in crate::controlled_authority::formal::endpoint) fn reconcile_authenticated_cold_scope_bounded_v1(
        &mut self,
        authority: &VerifiedAuthorityV1,
        role: crate::controlled_authority::replicated_protocol::EndpointRole,
        provider: &crate::controlled_authority::fencing::RemoteMonotonicEndpointProvider,
        pinned: &fencing::TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), crate::controlled_authority::fencing::BoundedAcquireErrorV1> {
        let instance = match role {
            crate::controlled_authority::replicated_protocol::EndpointRole::Primary => {
                &authority.definition().primary.instance
            }
            crate::controlled_authority::replicated_protocol::EndpointRole::Replica => {
                &authority.definition().replica.instance
            }
        };
        self.reconcile_pending_update_with_scope(
            provider,
            instance,
            pinned,
            memory,
            work,
            stop,
            &super::bounded_transaction::RecoveryScope::Cold { authority, role },
        )
    }
    pub(in crate::controlled_authority::formal::endpoint) fn read_cold_local_original_bounded_v1(
        &self,
        reference: &str,
        max: usize,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<FormalArchiveReadOwnerV1, FormalError> {
        let bytes = self
            .path
            .as_os_str()
            .as_encoded_bytes()
            .len()
            .checked_add("formal-objects-v1".len() + 1)
            .ok_or_else(|| invalid("cold local path layout"))?;
        work.charge(1, bytes as u64, stop)?;
        let _fee = memory
            .reserve(
                bytes + std::mem::size_of::<std::path::PathBuf>(),
                oasis7_proto::memory_budget::MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("cold local path admission"))?;
        let mut directory = std::path::PathBuf::with_capacity(bytes);
        if directory.capacity() > bytes {
            return Err(invalid("cold local path capacity"));
        }
        directory.push(&self.path);
        directory.push("formal-objects-v1");
        // Local only: an archive hint cannot stand in for this endpoint's
        // durable namespace attestation original.
        super::super::archive_bounded_read::read_local_archive_bounded_v1(
            &directory, reference, max, memory, work, stop,
        )?
        .ok_or_else(|| invalid("cold local record missing"))
    }
    pub(in crate::controlled_authority::formal::endpoint) fn read_cold_qualified_head_bounded_v1(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidColdStoreHeadV1, FormalError> {
        work.consume_steps(1, stop)?;
        let id = self
            .body
            .head_node
            .as_ref()
            .or(self.body.imported_head_node.as_ref())
            .ok_or_else(|| invalid("native provenance history root"))?;
        let node = super::bounded_history_read::read_history_object_bounded_v1(
            self, id, memory, work, stop,
        )?;
        work.charge(
            1,
            (node.node().head.record_id.len() + self.body.head.record_id.len()) as u64,
            stop,
        )?;
        if node.node().head != self.body.head {
            return Err(invalid("formal signed checkpoint head identity"));
        }
        let read = |id: &str,
                    stop: &mut dyn FnMut() -> bool|
         -> Result<FormalArchiveReadOwnerV1, FormalError> {
            self.raw_object_bounded(id, LIMIT, memory, work, stop)?
                .ok_or_else(|| invalid("native checkpoint object missing"))
        };
        let raw = read(&node.node().entry, stop)?;
        let request = super::bounded_request::decode_request_bounded_v1(&raw, memory, work, stop)?;
        if !matches!(request.state(), FormalRequestStateV1::Qualified { .. }) {
            return Err(invalid("formal checkpoint qualified entry"));
        }
        let (parent_node, parent) = if let Some(link) = node.node().ancestors.first() {
            if link.head.position.checked_add(1) != Some(node.node().head.position) {
                return Err(invalid("formal checkpoint immediate ancestor position"));
            }
            let parent_node = super::bounded_history_read::read_history_object_bounded_v1(
                self, &link.node, memory, work, stop,
            )?;
            work.charge(
                1,
                (parent_node.node().head.record_id.len() + link.head.record_id.len()) as u64,
                stop,
            )?;
            if parent_node.node().head != link.head {
                return Err(invalid("formal checkpoint immediate ancestor binding"));
            }
            let raw = read(&parent_node.node().entry, stop)?;
            let parent =
                super::bounded_request::decode_request_bounded_v1(&raw, memory, work, stop)?;
            if !matches!(parent.state(), FormalRequestStateV1::Qualified { .. }) {
                return Err(invalid("formal checkpoint ancestor qualified entry"));
            }
            (Some(parent_node), Some(parent))
        } else {
            (None, None)
        };
        Ok(PaidColdStoreHeadV1 {
            node,
            request,
            parent_node,
            parent,
        })
    }
}
