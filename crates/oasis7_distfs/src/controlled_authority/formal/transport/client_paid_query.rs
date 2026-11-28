//! Genuine remote signed-Query DATA owner with caller-owned prepaid inbound
//! frame and closed CBOR decoding. Never an accepted GEN/Root execution permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::{
    BoundedCborDataErrorV1, CborDataLayoutV1, PaidCborDataV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::borrow::Borrow;
use std::mem::size_of;

pub struct PaidRemoteFormalQueryV1 {
    response: PaidCborDataV1<Response>,
    _raw: ReservedValue<Vec<u8>>,
}
impl PaidRemoteFormalQueryV1 {
    /// Verified signed data, not qualified execution authority. Remote errors
    /// remain available by reference without losing their original identity.
    pub fn outcome(&self) -> Option<&FormalQueryOutcomeV1> {
        let response: &Response = self.response.borrow();
        match &response.body.result {
            Ok(Value::Query(value)) => Some(value.as_ref()),
            _ => None,
        }
    }
    pub fn remote_error(&self) -> Option<&FormalError> {
        let response: &Response = self.response.borrow();
        response.body.result.as_ref().err()
    }
    pub fn validate_owner(
        &self,
        memory: &MemoryBudget,
        work: &super::super::super::endpoint::FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let mut common = work.bounded_context(memory, cancelled);
        self.response
            .validate_context(&mut common)
            .map_err(|error| match error {
                BoundedCborDataErrorV1::Common(source) => source,
                other => invalid(other.to_string()),
            })?;
        if !self._raw.uses_budget(memory) {
            return Err(invalid("formal RPC paid response original pool"));
        }
        Ok(())
    }
}
impl RemoteFormalEndpoint {
    /// Opt-in real remote Query with one caller-provided Ordinary pool,
    /// continuing Work, cancellation, original signed provenance and paid
    /// response ownership through caller inspection. The legacy query trait
    /// remains separately callable and does not inherit this budget guarantee.
    pub fn query_data_bounded(
        &mut self,
        request: &str,
        memory: &MemoryBudget,
        work: &super::super::super::endpoint::FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<PaidRemoteFormalQueryV1, FormalError> {
        work.consume_steps(1, cancelled)?;
        let (signed, digest) = self.signed_request(Command::Query(request.to_owned()))?;
        let deadline = self
            .operation_deadline
            .unwrap_or_else(|| Instant::now() + self.timeout);
        let mut stream = wire::connect_and_write_before_deadline(&self.address, &signed, deadline)?;
        // The signed Query carries exactly one boxed FormalRequestStateV1;
        // that complete fixed-size root is included above. Its thousands of
        // CBOR array elements are repeated small headers, String/Vec payload
        // carriers and signed history head references, never thousands of
        // independent complete FormalRequestStateV1 allocations. Applying
        // size_of::<FormalRequestStateV1>() per sequence element multiplies
        // 5,583 elements by the entire state and a 15-phase scratch bound,
        // rejecting a genuine 22,397-byte Qualified signed response.
        // Leave the complete state charge in the root and preserve the
        // shared decoder's extra 4x per-element/serde Content/phase bounds.
        let root = size_of::<ResponseBody>()
            .saturating_add(size_of::<Response>())
            .saturating_add(size_of::<FormalQueryOutcomeV1>())
            .saturating_add(size_of::<FormalRequestStateV1>());
        let layout = CborDataLayoutV1 {
            root,
            sequence_element: size_of::<FormalHeadRefV1>()
                .max(size_of::<String>())
                .max(size_of::<Vec<u8>>()),
            map_pair: size_of::<(String, String)>().max(size_of::<(String, Vec<u8>)>()),
            set_element: size_of::<String>(),
            box_pointee: size_of::<FormalRequestStateV1>(),
            default_heap_per_value: size_of::<String>(),
            symbols_bytes: 0,
            visits: 64,
            missing: None,
        };
        let (response, raw): (PaidCborDataV1<Response>, ReservedValue<Vec<u8>>) =
            wire::read_paid_before_deadline(
                &mut stream,
                deadline,
                memory,
                work,
                layout,
                cancelled,
            )?;
        let verified: &Response = response.borrow();
        self.verify_authenticated_response(verified, &signed, &digest, deadline)?;
        if !matches!(&verified.body.result, Ok(Value::Query(_)) | Err(_)) {
            return Err(invalid("formal paid Query response type"));
        }
        Ok(PaidRemoteFormalQueryV1 {
            response,
            _raw: raw,
        })
    }
}
