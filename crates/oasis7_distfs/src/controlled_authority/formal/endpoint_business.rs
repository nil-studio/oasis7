//! Profile 2 deltas are checked before any Prepare receipt and raw CAS writes
//! are durable before the signed reservation checkpoint can reference them.
use super::*;
use crate::controlled_authority::formal::business_index::BusinessIndexNodeReader;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

const CANDIDATE_NODE_MEMO_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Avoid reopening the same immutable sparse-tree path node several times
/// during one candidate's retained-history and business-delta verification.
/// The memo is deliberately short-lived and positive-only: a new candidate
/// always reads its first copy from the durable Store again.
struct CandidateNodeMemo<'a, R> {
    source: &'a R,
    nodes: RefCell<BTreeMap<String, Vec<u8>>>,
    cached_bytes: Cell<usize>,
}

impl<'a, R> CandidateNodeMemo<'a, R> {
    fn new(source: &'a R) -> Self {
        Self {
            source,
            nodes: RefCell::new(BTreeMap::new()),
            cached_bytes: Cell::new(0),
        }
    }
}

impl<R: BusinessIndexNodeReader> BusinessIndexNodeReader for CandidateNodeMemo<'_, R> {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.nodes.borrow().get(digest).cloned() {
            if blake3::hash(&bytes).to_hex().as_str() != digest {
                return Err("business candidate memo content hash mismatch".into());
            }
            return Ok(Some(bytes));
        }

        let Some(bytes) = self.source.read_node(digest)? else {
            return Ok(None);
        };
        if blake3::hash(&bytes).to_hex().as_str() != digest {
            return Err("business candidate source content hash mismatch".into());
        }
        let current = self.cached_bytes.get();
        if bytes.len() <= CANDIDATE_NODE_MEMO_MAX_BYTES.saturating_sub(current) {
            self.nodes
                .borrow_mut()
                .insert(digest.to_owned(), bytes.clone());
            self.cached_bytes.set(current + bytes.len());
        }
        Ok(Some(bytes))
    }
}

impl BusinessIndexNodeReader for Store {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.raw_object(digest).map_err(|error| error.to_string())
    }
}
impl FileFormalEndpoint {
    pub(super) fn persist_business_candidate(
        &self,
        candidate: &FormalCandidateMaterialV1,
        closure: &super::business_material::VerifiedCandidateMaterialClosureV1<'_>,
    ) -> Result<(), FormalError> {
        if !closure.matches_candidate(candidate) {
            return Err(invalid("formal candidate closure token identity mismatch"));
        }
        let authority = if candidate.branch == FormalCompletionBranchV1::Normal {
            &self.authority
        } else {
            &self.original
        };
        let parent = VerifiedFormalBusinessParentV1::from_endpoint(self)?;
        let node_memo = CandidateNodeMemo::new(&self.store);
        if let Some(verified) =
            super::super::business_material::verify_formal_business_material_with_closure(
                authority,
                candidate,
                parent.as_ref(),
                &node_memo,
                closure,
            )?
        {
            self.store.persist_business_nodes(verified.node_writes())?;
            self.store
                .persist_business_nodes(verified.retained_node_writes())?;
        }
        let mut artifacts = std::collections::BTreeMap::new();
        for object in candidate.material.objects.iter().chain(
            candidate
                .frozen_material
                .iter()
                .flat_map(|material| material.objects.iter()),
        ) {
            if let Some(previous) =
                artifacts.insert(object.content_hash.clone(), object.bytes.clone())
                && previous != object.bytes
            {
                return Err(invalid("closed artifact digest collision"));
            }
        }
        let writes: Vec<_> = artifacts
            .into_iter()
            .map(
                |(digest, bytes)| super::super::business_index::BusinessIndexNodeWriteV1 {
                    digest,
                    bytes,
                },
            )
            .collect();
        self.store.persist_closed_artifacts(&writes)?;
        Ok(())
    }
    pub(super) fn verify_business_checkpoint(&self) -> Result<(), FormalError> {
        if self.authority.definition().profile_version != 2 {
            return Ok(());
        }
        let id = self
            .store
            .body
            .head_node
            .as_ref()
            .or(self.store.body.imported_head_node.as_ref());
        let Some(id) = id else {
            return Ok(());
        };
        let node: HistoryNode = self.store.get(id)?;
        let FormalRequestStateV1::Qualified {
            candidate, proof, ..
        } = self.store.get(&node.entry)?
        else {
            return Err(invalid("profile2 checkpoint head must be Qualified"));
        };
        let base = if let Some(link) = node.ancestors.first() {
            let parent_node: HistoryNode = self.store.get(&link.node)?;
            if parent_node.head != link.head
                || link.head.position.checked_add(1) != Some(node.head.position)
            {
                return Err(invalid("profile2 business immediate parent"));
            }
            let FormalRequestStateV1::Qualified { proof: parent, .. } =
                self.store.get(&parent_node.entry)?
            else {
                return Err(invalid("profile2 business parent not qualified"));
            };
            if parent.decision.record.record_id() != Ok(link.head.record_id.clone()) {
                return Err(invalid("profile2 business parent canonical identity"));
            }
            Some(VerifiedFormalBusinessParentV1::from_authenticated_parent(
                &CheckpointParent {
                    record: parent.decision.record.clone(),
                    finalizing_authority_id: parent
                        .decision
                        .primary
                        .body
                        .finalizing_authority_id
                        .clone(),
                },
                parent,
            )?)
        } else {
            None
        };
        if let Some(verified) = super::super::business_material::verify_formal_business_material(
            &self.authority,
            &proof.decision.record,
            &candidate.material,
            base.as_ref(),
            candidate.frozen_material.as_ref(),
            &self.store,
        )? {
            for write in verified
                .node_writes()
                .iter()
                .chain(verified.retained_node_writes())
            {
                if self.store.raw_object(&write.digest)?.as_deref() != Some(write.bytes.as_slice())
                {
                    return Err(invalid("profile2 checkpoint business node missing/corrupt"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "endpoint_business_memo_tests.rs"]
mod memo_tests;
