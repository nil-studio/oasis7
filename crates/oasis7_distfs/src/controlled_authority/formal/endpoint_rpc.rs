use super::super::transport::{Command, RESPONSE_DOMAIN, Response, ResponseBody, Value};
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_context::FormalBoundedContextV1, context_wire::encode_paid,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

const RESPONSE_DIGEST_DOMAIN: &str = "oasis7.formal.endpoint.rpc.response_body.v1";

fn paid_response_digest(
    body: &ResponseBody,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    let payload = encode_paid(
        body,
        64 << 20,
        ctx,
        "formal encoding exceeds byte bound",
        "formal RPC response body encoding",
    )?;
    ctx.consume_steps(1)?;
    // Admit the digest while its input payload is still live in the same pool.
    let fee = ctx
        .memory()
        .reserve(64 + std::mem::size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("formal RPC response digest admission"))?;
    let mut hash = blake3::Hasher::new();
    for part in [
        &(RESPONSE_DIGEST_DOMAIN.len() as u64).to_be_bytes()[..],
        RESPONSE_DIGEST_DOMAIN.as_bytes(),
        payload.as_slice(),
    ] {
        for chunk in part.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            hash.update(chunk);
        }
    }
    ctx.charge(1, 64)?;
    let digest = hash.finalize().to_hex().to_string();
    if digest.capacity() != 64 {
        return Err(invalid("formal RPC response digest capacity"));
    }
    Ok(fee.retain(digest))
}

pub(crate) fn verify_formal_rpc_checkpoint(
    response: &ResponseBody,
    authority: &VerifiedAuthorityV1,
    role: EndpointRole,
) -> Result<bool, FormalError> {
    let instance = match role {
        EndpointRole::Primary => &authority.definition().primary.instance,
        EndpointRole::Replica => &authority.definition().replica.instance,
    };
    if response.minimum.position == 0
        && response.minimum != authority.definition().world.genesis_head()?
    {
        return Err(invalid("formal RPC pinned genesis bridge"));
    }
    let body = Store::verify_remote_minimum(
        &response.checkpoint_bytes,
        &instance.endpoint_public_key,
        &response.minimum,
        &response.ancestry,
    )?;
    if Store::binding(&body, &response.checkpoint_bytes) != response.checkpoint
        || body.head != response.head
        || body.authority_id != authority.authority_id()?
        || body.instance_digest != instance.digest().map_err(fencing_error)?
        || body.activation_ref != authority.activation_ref()
        || body.epoch != authority.definition().epoch
    {
        return Err(invalid(
            "formal RPC signed checkpoint/authority/ancestry bridge",
        ));
    }
    // This is the authenticated native request slot, not the provider
    // checkpoint publication intent and not proof of new-write eligibility.
    Ok(body.pending.is_some())
}
impl FileFormalEndpoint {
    /// Serve the inexpensive immutable RPC reads under one authenticated
    /// monotonic guard. The guard spans minimum verification, object loading,
    /// and response signing so a read cannot cross an endpoint floor change.
    pub(crate) fn sign_formal_rpc_read_response(
        &mut self,
        request_digest: String,
        challenge: String,
        minimum: FormalHeadRefV1,
        command: &Command,
        context: Option<&mut FormalBoundedContextV1<'_>>,
    ) -> Result<Response, FormalError> {
        let mut guard = self.guard()?;
        let ancestry = self.minimum_witness_under_guard(&minimum)?;
        let result = match command {
            Command::Head => Ok(Value::Complete),
            Command::ArchiveChunk { id, offset, limit } => self
                .store
                .object_range(id, *offset, *limit)
                .map(Value::ArchiveChunk),
            Command::ArchiveObject(id) => match self.store.raw_object(id) {
                Ok(bytes) => {
                    if bytes.as_ref().is_some_and(|bytes| {
                        bytes.len() > FORMAL_ARCHIVE_OBJECT_LIMIT as usize
                            || blake3::hash(bytes).to_hex().as_str() != id
                    }) {
                        self.poisoned = true;
                        Err(invalid("formal archived object checksum"))
                    } else {
                        Ok(Value::ArchiveObject(bytes))
                    }
                }
                Err(error) => Err(error),
            },
            Command::BusinessIndexPath { root, key_digest } => root
                .path_nodes(key_digest, &self.store)
                .map(|path| Value::BusinessIndexPath(Box::new(path)))
                .map_err(invalid),
            _ => Err(invalid("formal RPC guarded read command")),
        };
        if self.poisoned {
            return Err(FormalError::Io(
                "formal endpoint outcome unknown; reopen required".into(),
            ));
        }
        self.sign_formal_rpc_response_with_guard(
            (request_digest, challenge, minimum),
            result,
            ancestry,
            guard.as_mut(),
            context,
        )
    }

    pub(crate) fn sign_formal_rpc_response(
        &mut self,
        request_digest: String,
        challenge: String,
        minimum: FormalHeadRefV1,
        result: Result<Value, FormalError>,
        context: Option<&mut FormalBoundedContextV1<'_>>,
    ) -> Result<Response, FormalError> {
        let mut guard = self.guard()?;
        let ancestry = self.store.minimum_witness(&minimum)?;
        self.sign_formal_rpc_response_with_guard(
            (request_digest, challenge, minimum),
            result,
            ancestry,
            guard.as_mut(),
            context,
        )
    }

    fn sign_formal_rpc_response_with_guard(
        &mut self,
        request_binding: (String, String, FormalHeadRefV1),
        result: Result<Value, FormalError>,
        ancestry: Vec<Vec<u8>>,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
        context: Option<&mut FormalBoundedContextV1<'_>>,
    ) -> Result<Response, FormalError> {
        let (request_digest, challenge, minimum) = request_binding;
        let body = ResponseBody {
            schema_version: 1,
            request_digest,
            challenge,
            world: self.authority.definition().world.clone(),
            authority_id: self.authority.authority_id()?,
            instance_digest: self.instance().digest().map_err(fencing_error)?,
            minimum,
            head: self.store.body.head.clone(),
            checkpoint: self.store.checkpoint.clone(),
            checkpoint_bytes: self.store.checkpoint_bytes()?,
            ancestry,
            provider: guard.evidence().map_err(fencing_error)?,
            result,
        };
        let legacy_digest;
        let paid_digest;
        let response_digest = match context {
            Some(ctx) => {
                paid_digest = paid_response_digest(&body, ctx)?;
                &*paid_digest
            }
            None => {
                legacy_digest = hash(RESPONSE_DIGEST_DOMAIN, &body)?;
                &legacy_digest
            }
        };
        // The digest owner spans the original preimage/signing operations.
        // Response DTO/source reads and signer workspace are separate phases.
        let preimage = bytes(RESPONSE_DOMAIN, response_digest)?;
        let (signature, protected_signature) = if self.key.is_some() {
            (
                self.sign_at_checkpoint(
                    guard,
                    fencing::ProtectedEndpointSignaturePurposeV1::RpcResponse,
                    preimage,
                )?,
                None,
            )
        } else {
            let request = fencing::ProtectedEndpointSigningRequestV1 {
                schema_version: 1,
                instance_digest: self.instance().digest().map_err(fencing_error)?,
                checkpoint: guard.checkpoint().clone(),
                challenge: fencing::fresh_challenge().map_err(fencing_error)?,
                purpose: fencing::ProtectedEndpointSignaturePurposeV1::RpcResponse,
                preimage,
            };
            let signed = guard
                .sign_endpoint(request.clone())
                .map_err(fencing_error)?;
            let signature = fencing::verify_protected_endpoint_signature(
                &signed,
                &request,
                self.instance(),
                &self.signing_policy(),
            )
            .map_err(fencing_error)?;
            (signature, Some(signed))
        };
        Ok(Response {
            body,
            signature,
            protected_signature,
        })
    }

    fn minimum_witness_under_guard(
        &mut self,
        minimum: &FormalHeadRefV1,
    ) -> Result<Vec<Vec<u8>>, FormalError> {
        if minimum.position == 0 && minimum != &self.authority.definition().world.genesis_head()? {
            return Err(FormalError::Rollback(
                "formal pinned genesis minimum fork".into(),
            ));
        }
        let result = self.store.minimum_witness(minimum);
        if matches!(&result, Err(FormalError::Invalid(_) | FormalError::Io(_))) {
            self.poisoned = true;
        }
        result
    }
}

#[cfg(test)]
#[path = "endpoint_rpc_response_digest_tests.rs"]
mod response_digest_tests;
