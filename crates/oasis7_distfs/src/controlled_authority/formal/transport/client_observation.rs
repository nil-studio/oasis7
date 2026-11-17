//! Bounded facts from one fresh, authenticated guarded Head RPC.
use super::*;

/// An endpoint observation, NOT a lease or a write-admission capability.
///
/// The server holds one monotonic guard through checkpoint/minimum verification
/// and protected response signing. Construction follows the client's complete
/// challenge, authority, custody, provider and checkpoint verification. No
/// deserializer or public constructor can promote an arbitrary status payload.
/// Two independently acquired observations never establish atomic dual-endpoint
/// qualification. Mutation must still perform its original authority checks.
#[derive(Clone, Debug)]
pub struct VerifiedEndpointObservationV1 {
    authority_id: String,
    instance_digest: String,
    challenge: String,
    head: FormalHeadRefV1,
    checkpoint: fencing::EndpointCheckpointBindingV1,
    epoch_floor: u64,
    seal_digest: Option<String>,
    native_request_pending: bool,
    received_at: Instant,
}

impl VerifiedEndpointObservationV1 {
    pub fn authority_id(&self) -> &str {
        &self.authority_id
    }
    pub fn instance_digest(&self) -> &str {
        &self.instance_digest
    }
    pub fn challenge(&self) -> &str {
        &self.challenge
    }
    pub fn head(&self) -> &FormalHeadRefV1 {
        &self.head
    }
    pub fn checkpoint(&self) -> &fencing::EndpointCheckpointBindingV1 {
        &self.checkpoint
    }
    pub fn epoch_floor(&self) -> u64 {
        self.epoch_floor
    }
    pub fn seal_digest(&self) -> Option<&str> {
        self.seal_digest.as_deref()
    }
    /// Whether the verified native checkpoint retains a request slot. This is
    /// not the provider's temporary checkpoint intent and not a commit receipt.
    pub fn native_request_pending(&self) -> bool {
        self.native_request_pending
    }
    /// Local age since authenticated receipt, not provider wall-clock freshness.
    /// Cloning or reading this observation never refreshes this time.
    pub fn age(&self) -> Duration {
        self.received_at.elapsed()
    }
}

impl RemoteFormalEndpoint {
    /// Always performs a fresh Head RPC. Errors never return a cached success.
    /// Historical sealed heads may be observed; a successful observation does
    /// not imply that the endpoint is unsealed, available later, or writable.
    pub fn observe_authority(&mut self) -> Result<VerifiedEndpointObservationV1, FormalError> {
        let (body, native_request_pending) = self.call_verified(Command::Head)?;
        match body.result? {
            Value::Complete => Ok(VerifiedEndpointObservationV1 {
                authority_id: body.authority_id,
                instance_digest: body.instance_digest,
                challenge: body.challenge,
                head: body.head,
                checkpoint: body.checkpoint,
                epoch_floor: body.provider.statement.epoch_floor,
                seal_digest: body.provider.statement.seal_digest,
                native_request_pending,
                received_at: Instant::now(),
            }),
            _ => Err(invalid("formal observation response type")),
        }
    }
}
