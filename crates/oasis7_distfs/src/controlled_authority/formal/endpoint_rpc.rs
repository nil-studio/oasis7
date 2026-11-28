use super::super::transport::{
    Command, RESPONSE_DIGEST_DOMAIN, RESPONSE_DOMAIN, Response, ResponseBody, Value,
    paid_response_digest,
};
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
#[path = "endpoint_rpc_minimum_paid.rs"]
mod minimum_paid;
use minimum_paid::MinimumSource;
pub(crate) use minimum_paid::RpcReadBudget;

pub(crate) struct RpcResponseOwner {
    // Drop the serialized fields before their source/copy reservations.
    response: Response,
    _checkpoint: Option<CheckpointRetention>,
    _ancestry: Option<minimum_paid::AncestryRetention>,
}
impl RpcResponseOwner {
    pub(crate) fn response(&self) -> &Response {
        &self.response
    }
}
struct CheckpointRetention {
    _original: ReservedValue<Vec<u8>>,
    _copy_fee: MemoryReservation,
}
fn paid_checkpoint_copy(
    store: &Store,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(Vec<u8>, CheckpointRetention), FormalError> {
    let original = store.checkpoint_bytes_bounded(ctx)?;
    let allocation = original
        .len()
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .ok_or_else(|| invalid("formal RPC checkpoint copy layout"))?;
    ctx.consume_steps(1)?;
    let copy_fee = ctx
        .memory()
        .reserve(allocation, MemoryClass::Ordinary)
        .map_err(|_| invalid("formal RPC checkpoint copy admission"))?;
    let mut bytes = Vec::with_capacity(original.len());
    if bytes.capacity() > original.len() {
        return Err(invalid("formal RPC checkpoint copy capacity"));
    }
    for chunk in original.chunks(8192) {
        ctx.charge(1, chunk.len() as u64)?;
        bytes.extend_from_slice(chunk);
    }
    Ok((
        bytes,
        CheckpointRetention {
            _original: original,
            _copy_fee: copy_fee,
        },
    ))
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
        mut budget: Option<RpcReadBudget<'_>>,
    ) -> Result<RpcResponseOwner, FormalError> {
        let mut guard = self.guard()?;
        let ancestry = self.minimum_witness_under_guard(&minimum, budget.as_mut())?;
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
            budget,
        )
    }

    pub(crate) fn sign_formal_rpc_response(
        &mut self,
        request_digest: String,
        challenge: String,
        minimum: FormalHeadRefV1,
        result: Result<Value, FormalError>,
        mut budget: Option<RpcReadBudget<'_>>,
    ) -> Result<RpcResponseOwner, FormalError> {
        let mut guard = self.guard()?;
        // The original second walk has no additional poison wrapper.
        let ancestry = MinimumSource::load(&self.store, &minimum, budget.as_mut())?;
        self.sign_formal_rpc_response_with_guard(
            (request_digest, challenge, minimum),
            result,
            ancestry,
            guard.as_mut(),
            budget,
        )
    }

    fn sign_formal_rpc_response_with_guard(
        &mut self,
        request_binding: (String, String, FormalHeadRefV1),
        result: Result<Value, FormalError>,
        ancestry: MinimumSource,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
        mut budget: Option<RpcReadBudget<'_>>,
    ) -> Result<RpcResponseOwner, FormalError> {
        let (request_digest, challenge, minimum) = request_binding;
        // Copy after the original walk/command, outside the guarded poison wrapper.
        let paid = ancestry.into_wire(budget.as_mut())?;
        // Declare the fees first: every early return drops wire DATA before them.
        let ancestry_retention = paid.retention;
        let ancestry = paid.wire;
        let mut context = budget.as_mut().map(RpcReadBudget::context);
        let mut checkpoint_retention = None;
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
            checkpoint_bytes: match context.as_mut() {
                Some(ctx) => {
                    let (bytes, retention) = paid_checkpoint_copy(&self.store, ctx)?;
                    checkpoint_retention = Some(retention);
                    bytes
                }
                None => self.store.checkpoint_bytes()?,
            },
            ancestry,
            provider: guard.evidence().map_err(fencing_error)?,
            result,
        };
        let legacy_digest;
        let paid_digest;
        let response_digest = match context.as_mut() {
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
        Ok(RpcResponseOwner {
            response: Response {
                body,
                signature,
                protected_signature,
            },
            _checkpoint: checkpoint_retention,
            _ancestry: ancestry_retention,
        })
    }

    fn minimum_witness_under_guard(
        &mut self,
        minimum: &FormalHeadRefV1,
        budget: Option<&mut RpcReadBudget<'_>>,
    ) -> Result<MinimumSource, FormalError> {
        if minimum.position == 0 && minimum != &self.authority.definition().world.genesis_head()? {
            return Err(FormalError::Rollback(
                "formal pinned genesis minimum fork".into(),
            ));
        }
        let result = MinimumSource::load(&self.store, minimum, budget);
        if matches!(&result, Err(FormalError::Invalid(_) | FormalError::Io(_))) {
            self.poisoned = true;
        }
        result
    }
}

#[cfg(test)]
#[path = "endpoint_rpc_response_digest_tests.rs"]
mod response_digest_tests;
