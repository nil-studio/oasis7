//! Complete typed mark from the exact signed hot checkpoint before any sweep.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
const MAX_OBJECTS: usize = 262_144;
const MAX_METADATA_BYTES: u64 = 1024 * 1024 * 1024;
enum Edge {
    Index(String, Option<u16>),
    History(String, FormalHeadRefV1),
    Entry(String, Option<FormalHeadRefV1>),
}
impl Store {
    /// Caller holds the endpoint lease, which freezes publications. It acquires
    /// the exclusive physical retire fence only after this shared-pinned walk,
    /// then rechecks the exact checkpoint binding before any unlink. No archive
    /// or writer cache can supply missing registered-local objects during mark.
    pub(crate) fn verify_storage_roots_durable(
        &self,
        profile: u32,
        public_key: &str,
    ) -> Result<BTreeSet<String>, FormalError> {
        let bytes = self.checkpoint_bytes()?;
        let body = Self::verify_public(&bytes, public_key)?;
        if body != self.body || Self::binding(&body, &bytes) != self.checkpoint {
            return Err(invalid("storage GC checkpoint changed or uncommitted"));
        }
        let mut keep = BTreeSet::new();
        let mut work = Vec::new();
        if let Some(index) = &body.index {
            work.push(Edge::Index(index.clone(), None));
        }
        if let Some(pending) = &body.pending {
            work.push(Edge::Entry(pending.clone(), None));
        }
        for history in body.head_node.iter().chain(body.imported_head_node.iter()) {
            work.push(Edge::History(history.clone(), body.head.clone()));
        }
        if body.head.position > 0 && body.head_node.is_none() && body.imported_head_node.is_none() {
            return Err(invalid("storage GC qualified history root missing"));
        }
        let mut visited = BTreeSet::new();
        let mut metadata_bytes = 0u64;
        let mut entries = BTreeMap::new();
        while let Some(edge) = work.pop() {
            let (id, context) = match &edge {
                Edge::Index(id, previous) => (id, format!("index:{previous:?}")),
                Edge::History(id, head) => {
                    (id, format!("history:{}:{}", head.position, head.record_id))
                }
                Edge::Entry(id, head) => (id, format!("entry:{head:?}")),
            };
            if !visited.insert((id.clone(), context)) {
                continue;
            }
            if visited.len() > MAX_OBJECTS {
                return Err(invalid("storage GC typed edge budget"));
            }
            let bytes = self.published_local_read(id)?;
            metadata_bytes = metadata_bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| invalid("storage GC metadata overflow"))?;
            if metadata_bytes > MAX_METADATA_BYTES {
                return Err(invalid("storage GC metadata byte budget"));
            }
            keep.insert(id.clone());
            match edge {
                Edge::Index(_, previous) => match decode::<Index>(&bytes)? {
                    Index::Fork { bit, left, right } => {
                        if bit >= 256 || previous.is_some_and(|p| bit <= p) {
                            return Err(invalid("storage GC index bit order"));
                        }
                        work.push(Edge::Index(left, Some(bit)));
                        work.push(Edge::Index(right, Some(bit)));
                    }
                    Index::Leaf { request, entry } => {
                        hash_hex(&request)?;
                        let mut load = |id: &str| self.published_local_read(id);
                        if Self::lookup_from_root(body.index.as_deref(), &request, &mut load)?
                            != Some(entry.clone())
                        {
                            return Err(invalid("storage GC index leaf exact path"));
                        }
                        work.push(Edge::Entry(entry, None));
                    }
                },
                Edge::History(_, expected) => {
                    let node: HistoryNode = decode(&bytes)?;
                    if node.head != expected || node.ancestors.len() > 64 {
                        return Err(invalid("storage GC history identity/width"));
                    }
                    hash_hex(&node.request)?;
                    let mut load = |id: &str| self.published_local_read(id);
                    if Self::lookup_from_root(body.index.as_deref(), &node.request, &mut load)?
                        != Some(node.entry.clone())
                    {
                        return Err(invalid("storage GC history omitted from request index"));
                    }
                    work.push(Edge::Entry(node.entry, Some(node.head.clone())));
                    if node.head.position > 1 && node.ancestors.is_empty() {
                        return Err(invalid("storage GC history predecessor missing"));
                    }
                    for (level, link) in node.ancestors.into_iter().enumerate() {
                        let distance = 1u64
                            .checked_shl(level as u32)
                            .ok_or_else(|| invalid("storage GC history distance"))?;
                        if link.head.position.checked_add(distance) != Some(node.head.position) {
                            return Err(invalid("storage GC history skip edge"));
                        }
                        work.push(Edge::History(link.node, link.head));
                    }
                }
                Edge::Entry(id, expected) => {
                    let state: FormalRequestStateV1 = decode(&bytes)?;
                    if let Some(expected) = expected.as_ref() {
                        let FormalRequestStateV1::Qualified { candidate, proof } = &state else {
                            return Err(invalid("storage GC history contains unresolved state"));
                        };
                        let record = &candidate.candidate.record;
                        if record.position != expected.position
                            || record.record_id()? != expected.record_id
                            || proof.decision.record != *record
                        {
                            return Err(invalid("storage GC history exact Qualified record"));
                        }
                    }
                    if entries.insert(id, ()).is_some() {
                        continue;
                    }
                    match state {
                        FormalRequestStateV1::Frozen {
                            input, material, ..
                        } => {
                            verify_frozen_material(&input.body, &material)?;
                            keep.extend(self.verify_pinned_frozen_closure(&material, profile)?);
                        }
                        other => {
                            let candidate = other
                                .candidate()
                                .ok_or_else(|| invalid("storage GC unknown request state"))?;
                            keep.extend(self.verified_candidate_objects(candidate, profile)?);
                        }
                    }
                }
            }
            if keep.len() > MAX_OBJECTS {
                return Err(invalid("storage GC logical mark budget"));
            }
        }
        Ok(keep)
    }
}
