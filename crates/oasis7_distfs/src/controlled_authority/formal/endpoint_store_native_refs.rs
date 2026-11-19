//! One reference-validation decision core for legacy and metered consumers.
use super::super::incremental::local_closure::LocalDataClosureRecordV1;
use super::*;
pub(super) trait NativeReferenceValidationOps {
    type Error: From<FormalError>;
    type History;
    type Closure;
    type Request;
    type RecordId;
    fn history(&mut self, id: &str) -> Result<Self::History, Self::Error>;
    fn history_node(value: &Self::History) -> &HistoryNode;
    fn closure(&mut self, id: &str) -> Result<Self::Closure, Self::Error>;
    fn closure_record(value: &Self::Closure) -> &LocalDataClosureRecordV1;
    fn request(&mut self, id: &str) -> Result<Self::Request, Self::Error>;
    fn request_state(value: &Self::Request) -> &FormalRequestStateV1;
    fn index(&mut self, id: &str) -> Result<(), Self::Error>;
    fn record_id(&mut self, record: &CanonicalRecordV1) -> Result<Self::RecordId, Self::Error>;
    fn id(value: &Self::RecordId) -> &str;
    fn before_hash_reference(&mut self, id: &str) -> Result<(), Self::Error>;
    fn before_scope(
        &mut self,
        record: &LocalDataClosureRecordV1,
        body: &HotBody,
    ) -> Result<(), Self::Error>;
    /// Charge before comparisons; the original predicates below remain sole authority.
    fn compare(&mut self, bytes: usize) -> Result<(), Self::Error>;
}
pub(super) fn validate_refs_with<O: NativeReferenceValidationOps>(
    body: &HotBody,
    ops: &mut O,
) -> Result<(), O::Error> {
    if let Some(reference) = &body.data_closure {
        ops.before_hash_reference(reference)?;
        hash_hex(reference)?;
        if body.head_node.is_none() {
            return Err(invalid("local data closure requires local Qualified head").into());
        }
        let record = ops.closure(reference)?;
        ops.before_scope(O::closure_record(&record), body)?;
        O::closure_record(&record).validate_scope(body)?;
    }
    if body.head.position > 0
        && (body.index.is_none() || (body.head_node.is_none() && body.imported_head_node.is_none()))
    {
        return Err(invalid("formal nonzero head requires permanent request/history roots").into());
    }
    if let Some(id) = &body.imported_head_node {
        let node_owner = ops.history(id)?;
        let node = O::history_node(&node_owner);
        let parent = body
            .base_parent
            .as_ref()
            .ok_or_else(|| invalid("formal imported archive missing verified cut"))?;
        ops.compare(std::mem::size_of::<u64>())?;
        if node.head.position != parent.record.position {
            return Err(invalid("formal imported archive canonical cut").into());
        }
        let record_id = ops.record_id(&parent.record)?;
        ops.compare(
            node.head
                .record_id
                .len()
                .saturating_add(O::id(&record_id).len()),
        )?;
        if node.head.record_id != O::id(&record_id) {
            return Err(invalid("formal imported archive canonical cut").into());
        }
    }
    if let Some(root) = &body.index {
        ops.index(root)?;
    }
    if let Some(id) = &body.pending {
        let _entry = ops.request(id)?;
    }
    if let Some(id) = &body.head_node {
        let node_owner = ops.history(id)?;
        let node = O::history_node(&node_owner);
        ops.compare(
            node.head
                .record_id
                .len()
                .saturating_add(body.head.record_id.len())
                .saturating_add(16),
        )?;
        if node.head != body.head || node.ancestors.len() > 64 {
            return Err(invalid("formal head/node").into());
        }
        // Keep node_owner live while decoding and comparing its referenced entry.
        let entry_owner = ops.request(&node.entry)?;
        let entry = O::request_state(&entry_owner);
        ops.compare(
            entry
                .local_data_closure_reference()
                .map_or(0, str::len)
                .saturating_add(body.data_closure.as_ref().map_or(0, String::len)),
        )?;
        if entry.local_data_closure_reference() != body.data_closure.as_deref() {
            return Err(invalid("local data closure checkpoint/Qualified mismatch").into());
        }
        ops.compare(
            entry
                .request_digest()
                .len()
                .saturating_add(node.request.len())
                .saturating_add(16),
        )?;
        if !matches!(entry, FormalRequestStateV1::Qualified { .. })
            || entry.position() != node.head.position
            || entry.request_digest() != node.request
        {
            return Err(invalid("formal qualified head entry").into());
        }
    }
    Ok(())
}
pub(super) struct LegacyReferenceOps<'a>(pub(super) &'a Store);
impl NativeReferenceValidationOps for LegacyReferenceOps<'_> {
    type Error = FormalError;
    type History = HistoryNode;
    type Closure = LocalDataClosureRecordV1;
    type Request = FormalRequestStateV1;
    type RecordId = String;
    fn history(&mut self, id: &str) -> Result<Self::History, FormalError> {
        self.0.get(id)
    }
    fn history_node(value: &Self::History) -> &HistoryNode {
        value
    }
    fn closure(&mut self, id: &str) -> Result<Self::Closure, FormalError> {
        self.0.get(id)
    }
    fn closure_record(value: &Self::Closure) -> &LocalDataClosureRecordV1 {
        value
    }
    fn request(&mut self, id: &str) -> Result<Self::Request, FormalError> {
        self.0.get(id)
    }
    fn request_state(value: &Self::Request) -> &FormalRequestStateV1 {
        value
    }
    fn index(&mut self, id: &str) -> Result<(), FormalError> {
        let _: Index = self.0.get(id)?;
        Ok(())
    }
    fn record_id(&mut self, record: &CanonicalRecordV1) -> Result<String, FormalError> {
        record.record_id()
    }
    fn id(value: &String) -> &str {
        value
    }
    fn before_hash_reference(&mut self, _: &str) -> Result<(), FormalError> {
        Ok(())
    }
    fn before_scope(
        &mut self,
        record: &LocalDataClosureRecordV1,
        body: &HotBody,
    ) -> Result<(), FormalError> {
        let _ = (record, body);
        Ok(())
    }
    fn compare(&mut self, _: usize) -> Result<(), FormalError> {
        Ok(())
    }
}
