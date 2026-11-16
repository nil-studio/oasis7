//! Fixed evidence keys and multiplicities. Actual source deltas, never caller
//! booleans, decide whether a marker participates in the receipt join.
use super::*;
#[path = "incremental_normal_cognition_service_evidence_tree.rs"]
pub(in crate::world) mod tree;
pub(super) struct Index {
    root: tree::Link,
}
impl Index {
    pub(super) fn from_rows(
        events: &[[u8; 32]],
        markers: &[WorldCommitRecordV1],
        receipts: &[CognitionReceiptViewV1],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        let mut value = Self { root: None };
        for event in events {
            value.add_event(event, common)?;
        }
        for (ordinal, marker) in markers.iter().enumerate() {
            value.replace_request_row(ordinal as u64, Some(marker_identity(marker)), common)?;
            if marker.status == "committed" {
                value.replace_row(
                    true,
                    ordinal as u64,
                    Some((&marker.receipt_id, &marker.receipt_digest)),
                    common,
                )?;
            }
        }
        for (ordinal, receipt) in receipts.iter().enumerate() {
            value.replace_row(
                false,
                ordinal as u64,
                Some((&receipt.receipt_id, &receipt.receipt_digest)),
                common,
            )?;
        }
        Ok(value)
    }
    fn count(
        &self,
        key: &[u8],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<u64, BodyMutationError> {
        match tree::get(&self.root, key, common)? {
            None => Ok(0),
            Some(tree::Data::Count(value)) => Ok(value),
            _ => Err(bad().into()),
        }
    }
    fn change_count(
        &mut self,
        key: &tree::Key,
        add: bool,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let old = self.count(key, common)?;
        let next = if add {
            old.checked_add(1)
        } else {
            old.checked_sub(1)
        }
        .ok_or_else(bad)?;
        self.root = if next == 0 {
            tree::remove(&self.root, key, common)?
        } else {
            tree::put(&self.root, key, tree::Data::Count(next), common)?
        };
        Ok(())
    }
    pub(super) fn add_event(
        &mut self,
        event: &[u8; 32],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let key = tree::key(&[&[0], event], common)?;
        self.change_count(&key, true, common)
    }
    pub(super) fn has_event(
        &self,
        event: &[u8; 32],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, BodyMutationError> {
        let key = tree::key(&[&[0], event], common)?;
        Ok(self.count(&key, common)? > 0)
    }
    pub(super) fn has_receipt(
        &self,
        id: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<bool, BodyMutationError> {
        let key = tree::key(&[&[3], id.as_bytes()], common)?;
        Ok(self.count(&key, common)? > 0)
    }
    fn change_pair(
        &mut self,
        pair: &tree::Key,
        marker: bool,
        add: bool,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let kind = if marker { 1 } else { 2 };
        let other = if marker { 2 } else { 1 };
        let own = tree::key(&[&[kind], pair.as_slice()], common)?;
        let opposite = tree::key(&[&[other], pair.as_slice()], common)?;
        let old = self.count(&own, common)?;
        let other = self.count(&opposite, common)?;
        let next = if add {
            old.checked_add(1)
        } else {
            old.checked_sub(1)
        }
        .ok_or_else(bad)?;
        let before = old > 0 && other > 0;
        let after = next > 0 && other > 0;
        self.change_count(&own, add, common)?;
        if before != after {
            let len = u64::from_be_bytes(
                pair.get(..8)
                    .ok_or_else(bad)?
                    .try_into()
                    .map_err(|_| bad())?,
            );
            let len = usize::try_from(len).map_err(|_| bad())?;
            let end = 8usize.checked_add(len).ok_or_else(bad)?;
            let id = pair.get(8..end).ok_or_else(bad)?;
            let key = tree::key(&[&[3], id], common)?;
            self.change_count(&key, after, common)?;
        }
        Ok(())
    }
    /// Only called after actual selected row/schema verification. The old row's
    /// pairing is held in this persistent tree, so deletes and changed digests
    /// remove exactly its previous contribution; duplicates retain multiplicity.
    pub(super) fn replace_row(
        &mut self,
        marker: bool,
        ordinal: u64,
        after: Option<(&str, &str)>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let kind = if marker { 4 } else { 5 };
        let row = tree::key(&[&[kind], &ordinal.to_be_bytes()], common)?;
        let old = tree::get(&self.root, &row, common)?;
        if let Some(old) = old {
            let tree::Data::Pair(pair) = old else {
                return Err(bad().into());
            };
            self.change_pair(&pair, marker, false, common)?;
            self.root = tree::remove(&self.root, &row, common)?;
        }
        if let Some((id, digest)) = after {
            let pair = tree::key(
                &[
                    &(id.len() as u64).to_be_bytes(),
                    id.as_bytes(),
                    &(digest.len() as u64).to_be_bytes(),
                    digest.as_bytes(),
                ],
                common,
            )?;
            self.change_pair(&pair, marker, true, common)?;
            self.root = tree::put(&self.root, &row, tree::Data::Pair(pair), common)?;
        }
        Ok(())
    }
    pub(super) fn share(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        common.charge(1, 0).map_err(formal)?;
        Ok(Self {
            root: self.root.as_ref().map(std::sync::Arc::clone),
        })
    }
}

/// Exactly the original outer-request identity. Status, current World head,
/// transport attempt and retry sequence deliberately do not enter this key.
pub(super) fn marker_identity(marker: &WorldCommitRecordV1) -> [&str; 5] {
    [
        &marker.agent_id,
        &marker.agent_session_id,
        &marker.agent_turn_id,
        &marker.decision_request_id,
        &marker.request_digest,
    ]
}
fn request_key(
    identity: [&str; 5],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<tree::Key, BodyMutationError> {
    let sizes = identity.map(|v| (v.len() as u64).to_be_bytes());
    tree::key(
        &[
            &[6],
            &sizes[0],
            identity[0].as_bytes(),
            &sizes[1],
            identity[1].as_bytes(),
            &sizes[2],
            identity[2].as_bytes(),
            &sizes[3],
            identity[3].as_bytes(),
            &sizes[4],
            identity[4].as_bytes(),
        ],
        common,
    )
}
impl Index {
    /// Retain every lifecycle status, including prepared/aborted and legacy empty
    /// identities. Original Vec::find selects the first ordinal, not latest status.
    pub(super) fn replace_request_row(
        &mut self,
        ordinal: u64,
        identity: Option<[&str; 5]>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        let row = tree::key(&[&[7], &ordinal.to_be_bytes()], common)?;
        if let Some(old) = tree::get(&self.root, &row, common)? {
            let tree::Data::Pair(old) = old else {
                return Err(bad().into());
            };
            self.root = tree::remove(&self.root, &old, common)?;
            self.root = tree::remove(&self.root, &row, common)?;
        }
        if let Some(identity) = identity {
            let prefix = request_key(identity, common)?;
            let entry = tree::key(&[prefix.as_slice(), &ordinal.to_be_bytes()], common)?;
            self.root = tree::put(&self.root, &entry, tree::Data::Count(ordinal), common)?;
            self.root = tree::put(&self.root, &row, tree::Data::Pair(entry), common)?;
        }
        Ok(())
    }
    pub(super) fn first_request_marker(
        &self,
        identity: [&str; 5],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<u64>, BodyMutationError> {
        let prefix = request_key(identity, common)?;
        let Some((key, data)) = tree::lower_bound(&self.root, &prefix, common)? else {
            return Ok(None);
        };
        common.charge(1, prefix.len() as u64).map_err(formal)?;
        if key.len() != prefix.len().checked_add(8).ok_or_else(bad)?
            || !key.starts_with(prefix.as_slice())
        {
            return Ok(None);
        }
        let tree::Data::Count(ordinal) = data else {
            return Err(bad().into());
        };
        if key[prefix.len()..] != ordinal.to_be_bytes() {
            return Err(bad().into());
        }
        Ok(Some(ordinal))
    }
}
