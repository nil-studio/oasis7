//! Accepted Core reconstruction from genuine accepted head and predecessor.
use super::*;
use oasis7_formal_protocol::{
    incremental_material::{AcceptedCoreRequest, CheckedCommitCore, verify_accepted_commit_core},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueIoBudget,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;

// This adapter only charges the original node returned by the real page
// session; it never selects a different digest or copies/decodes its bytes.
pub(super) struct Pages<'a, M> {
    pub(super) source: &'a M,
    pub(super) io: RefCell<&'a mut ValueIoBudget>,
}
impl<M: MerkleNodeReader> MerkleNodeReader for Pages<'_, M> {
    type Node = M::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String> {
        let bound = u64::try_from(max_bytes).map_err(|_| "cold Core node bound".to_owned())?;
        {
            let mut io = self.io.borrow_mut();
            if io.nodes_remaining == 0 || io.read_bytes_remaining < bound {
                return Err("cold Core continuous node/byte allowance".to_owned());
            }
            io.nodes_remaining -= 1;
            io.read_bytes_remaining -= bound;
        }
        let result = self.source.read_node(digest, max_bytes);
        // Err/panic retains the full admitted bound. Success refunds only the
        // part of this scalar admission for which no bytes were returned.
        if let Ok(node) = &result {
            let actual = node.as_ref().map_or(0, |n| n.as_ref().len());
            if actual > max_bytes {
                return Err("cold Core returned node exceeds bound".to_owned());
            }
            self.io.borrow_mut().read_bytes_remaining += bound - actual as u64;
        }
        result
    }
}
pub(super) fn recover_ordinary_core_v1(
    endpoint: &FileFormalEndpoint,
    checkpoint: &super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    work: &mut IwsColdParentWorkV1,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<CheckedCommitCore>, FormalError> {
    checkpoint.validate_context(common)?;
    let capability = endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("cold actual Core page capability"))?;
    let pages = &capability.pages;
    let memory = pages.memory_budget();
    if !memory.same_pool(common.memory()) {
        return Err(invalid("cold actual Core source pool"));
    }
    let head = checkpoint.head();
    let original =
        if checkpoint.native().proof().decision.branch == FormalCompletionBranchV1::Normal {
            &endpoint.authority
        } else {
            &endpoint.original
        };
    super::cold_io::run(work, common, |work, cancelled| {
        let physical = std::sync::atomic::AtomicBool::new(false);
        let mut poll = || {
            let stop = cancelled();
            if stop {
                physical.store(true, std::sync::atomic::Ordering::Release);
            }
            stop
        };
        let objects = crate::incremental_page_store::objects::IncrementalObjectStore::open(
            pages,
            MemoryClass::Ordinary,
            &mut work.objects_physical,
            &physical,
        )
        .map_err(page_store_formal_error)?;
        let mut io = crate::incremental_page_store::objects::ObjectIo {
            physical: &mut work.objects_physical,
            logical: &mut work.objects_values,
            class: MemoryClass::Ordinary,
            cancelled: &physical,
        };
        let mut reader = objects.original_reader(&mut io);
        let state_loan = super::cold_page_loans::StateLoan::new(
            pages,
            &mut work.state_ordinary,
            MemoryClass::Ordinary,
            &physical,
            None,
        )
        .map_err(page_store_formal_error)?;
        let counted = Pages {
            source: state_loan.source(),
            io: RefCell::new(&mut work.read_ordinary),
        };
        // The one original accepted-record kernel validates catalog/body/roots,
        // parent continuity, original bytes and typed IWS commit bindings.
        let result = verify_accepted_commit_core(
            &AcceptedCoreRequest {
                head,
                original_authority: original,
                parent: checkpoint.predecessor(),
                max_pending_node_bytes: capability.budgets.max_pending_node_bytes,
            },
            &mut reader,
            &counted,
            memory,
            &mut work.core,
            &mut poll,
        );
        match result {
            Ok(core) => Ok(core),
            Err(error) => {
                if let Some(first) = state_loan.source().take_read_error() {
                    return Err(page_store_formal_error(first));
                }
                Err(invalid(format!("cold original accepted Core: {error:?}")))
            }
        }
    })
}
