//! Genuine remote signed-Query DATA owner with caller-owned prepaid inbound
//! frame and closed CBOR decoding. Never an accepted GEN/Root execution permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::{
    BoundedCborDataErrorV1, CborDataLayoutV1, PaidCborDataV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::io::Write;
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
    // Admission for the signed Query DTO before any RequestBody clone, digest
    // CBOR, signing preimage, or TCP connection. Count the immutable source
    // fields by streaming into a non-allocating writer; keep the admitted
    // Ordinary fee until the original response has been authenticated.
    // This is conservative heap/phase admission, not authority or GEN proof.
    fn prepay_signed_query_request(
        &self,
        request: &str,
        memory: &MemoryBudget,
        work: &super::super::super::endpoint::FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<MemoryReservation, FormalError> {
        struct Count(usize);
        impl Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .ok_or_else(|| std::io::Error::other("signed Query source too large"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        fn encoded_len<T: serde::Serialize>(value: &T) -> Result<usize, FormalError> {
            let mut count = Count(0);
            serde_cbor::to_writer(&mut count, value)
                .map_err(|error| invalid(error.to_string()))?;
            Ok(count.0)
        }
        work.consume_steps(1, cancelled)?;
        let grant_bytes = match &self.archive_read_grant {
            Some(grant) => encoded_len(grant)?,
            None => 0,
        };
        let sources = [
            encoded_len(&self.authority.definition().world)?,
            encoded_len(&self.minimum)?,
            grant_bytes,
            request.len(),
            // Wire tags, authority/instance/challenge, digest, signature, and
            // fixed Request/RequestBody/Command allocations.
            size_of::<Request>() + size_of::<RequestBody>() + size_of::<Command>() + 4096,
        ];
        let base = sources
            .into_iter()
            .try_fold(0usize, |sum, n| sum.checked_add(n))
            .ok_or_else(|| invalid("formal paid signed Query size overflow"))?;
        if base > 64 * 1024 * 1024 {
            return Err(invalid("formal paid signed Query source frame bound"));
        }
        // Includes cloned source graphs, transient hash/signature CBOR and
        // signed preimages, and their possible Vec capacity growth. Distinct
        // outbound TCP encoding has its own independently held fee.
        let signing_capacity = base
            .checked_mul(24)
            .ok_or_else(|| invalid("formal paid signed Query capacity overflow"))?;
        let work_bytes = base
            .checked_mul(3)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| invalid("formal paid signed Query work overflow"))?;
        work.charge(6, work_bytes, cancelled)?;
        memory
            .reserve(signing_capacity, MemoryClass::Ordinary)
            .map_err(|_| invalid("formal paid signed Query pre-admission"))
    }

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
        let deadline = self
            .operation_deadline
            .unwrap_or_else(|| Instant::now() + self.timeout);
        let _signed_request_owner =
            self.prepay_signed_query_request(request, memory, work, cancelled)?;
        let (signed, digest) = self.signed_request(Command::Query(request.to_owned()))?;
        let mut stream = wire::connect_and_write_paid_before_deadline(
            &self.address,
            &signed,
            deadline,
            memory,
            work,
            cancelled,
        )?;
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
        // Verification advances the client's anti-rollback minimum even
        // though this is data-only. Charge the persistent minimum head clone
        // to the original caller pool/Work, not only the transient response.
        // The fee moves into RemoteFormalEndpoint and is released when the
        // minimum is replaced by a later authenticated RPC or on drop.
        let minimum_bytes = size_of::<FormalHeadRefV1>()
            .checked_add(verified.body.head.record_id.len())
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(|| invalid("formal paid Query minimum owner layout"))?;
        work.charge(1, minimum_bytes as u64, cancelled)?;
        let minimum_fee = memory
            .reserve(minimum_bytes, MemoryClass::Ordinary)
            .map_err(|_| invalid("formal paid Query minimum owner admission"))?;
        self.verify_authenticated_response(
            verified,
            &signed,
            &digest,
            deadline,
            Some(minimum_fee),
        )?;
        if !matches!(&verified.body.result, Ok(Value::Query(_)) | Err(_)) {
            return Err(invalid("formal paid Query response type"));
        }
        Ok(PaidRemoteFormalQueryV1 {
            response,
            _raw: raw,
        })
    }
}
