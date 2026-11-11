//! Client adapter for an independently provisioned monotonic/exclusive oracle.
//! Cryptography authenticates the pinned oracle's contract; it cannot establish
//! its physical nonrollback storage or signer isolation. A connection is one
//! exclusive lease and remains alive across native publication and signing.
use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::Duration;
const REQUEST: &str = "oasis7.formal.monotonic_oracle.request.v1";
const RESPONSE: &str = "oasis7.formal.monotonic_oracle.response.v1";
const LIMIT: usize = 1024 * 1024;

fn connect_oracle(address: &SocketAddr, timeout: Duration) -> Result<TcpStream, FencingError> {
    let stream = TcpStream::connect_timeout(address, timeout)
        .map_err(|e| FencingError::Io(e.to_string()))?;
    stream
        .set_nonblocking(false)
        .map_err(|e| FencingError::Io(e.to_string()))?;
    // Length and body are separate writes on this persistent leased stream.
    // Send both promptly instead of waiting for the small header to be ACKed.
    stream
        .set_nodelay(true)
        .map_err(|e| FencingError::Io(e.to_string()))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| FencingError::Io(e.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| FencingError::Io(e.to_string()))?;
    Ok(stream)
}

#[path = "fencing_remote_bounded_acquire.rs"]
mod bounded_acquire;
pub use bounded_acquire::{
    BoundedAcquireErrorV1, BoundedMonotonicEndpointProvider, PaidRemoteLeaseV1,
    acquire_provider_lease_bounded_v1,
};
pub(in crate::controlled_authority) use bounded_acquire::{
    PaidNativeCheckpointBeginV1, PaidNativeCheckpointSignatureV1,
    begin_signed_native_checkpoint_bounded_v1, sign_native_checkpoint_bounded_v1,
};
#[path = "fencing_remote_durable_set.rs"]
mod durable_set_client;
#[path = "fencing_remote_protected.rs"]
mod protected_client;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonotonicOracleCommandV1 {
    Acquire(EndpointCheckpointBindingV1),
    InstallDurableSet(Box<DurableSetNativeInstallRequestV1>),
    QueryDurableSetInstall(String),
    ActivateDurableSet(Box<DurableSetNativeActivationRequestV1>),
    Begin(Box<CheckpointIntentV1>),
    Finish,
    InitializeProtected(Box<ProtectedEndpointRegistrationV1>),
    ProtectedSign(Box<ProtectedEndpointSigningRequestV1>),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonotonicOracleRequestBodyV1 {
    pub schema_version: u32,
    pub instance: EndpointInstanceBindingV1,
    pub authentication_public_key: String,
    pub lease: Option<String>,
    pub challenge: String,
    pub command: MonotonicOracleCommandV1,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedMonotonicOracleRequestV1 {
    pub body: MonotonicOracleRequestBodyV1,
    pub signature: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonotonicOracleReplyBodyV1 {
    pub schema_version: u32,
    pub capability: ProviderCapabilityV1,
    pub resource_binding: String,
    pub request_digest: String,
    pub challenge: String,
    pub lease: String,
    pub checkpoint: EndpointCheckpointBindingV1,
    pub pending: Option<CheckpointIntentV1>,
    pub seal: Option<EpochSealBodyV1>,
    pub evidence: Option<MonotonicProviderEvidenceV1>,
    pub native_checkpoint_bytes: Option<Vec<u8>>,
    pub protected_custody: Option<ProtectedSignerCustodyV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_set: Option<DurableSetProviderStateV1>,
    pub protected_signature: Option<ProtectedEndpointSignatureV1>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedMonotonicOracleReplyV1 {
    pub body: MonotonicOracleReplyBodyV1,
    pub signature: String,
}
pub fn monotonic_oracle_request_signing_bytes(
    body: &MonotonicOracleRequestBodyV1,
) -> Result<Vec<u8>, FencingError> {
    bytes(REQUEST, body)
}
pub fn monotonic_oracle_reply_signing_bytes(
    body: &MonotonicOracleReplyBodyV1,
) -> Result<Vec<u8>, FencingError> {
    bytes(RESPONSE, body)
}
pub struct RemoteMonotonicEndpointProvider {
    address: SocketAddr,
    timeout: Duration,
    key: SigningKey,
    policy: TrustedFencingPolicyV1,
}
impl RemoteMonotonicEndpointProvider {
    pub fn new(
        address: SocketAddr,
        timeout: Duration,
        key: SigningKey,
        policy: TrustedFencingPolicyV1,
    ) -> Result<Self, FencingError> {
        if !address.ip().is_loopback()
            || timeout.is_zero()
            || timeout > Duration::from_secs(60)
            || policy.authorized_resource_bindings.is_empty()
        {
            return Err(FencingError::FencingUnproven);
        }
        canonical_hash(&policy.provider_public_key)?;
        for resource in &policy.authorized_resource_bindings {
            canonical_hash(resource)?;
        }
        Ok(Self {
            address,
            timeout,
            key,
            policy,
        })
    }
}
struct RemoteGuard {
    stream: Mutex<TcpStream>,
    instance: EndpointInstanceBindingV1,
    key: SigningKey,
    policy: TrustedFencingPolicyV1,
    state: MonotonicOracleReplyBodyV1,
}
fn exchange(
    stream: &mut TcpStream,
    instance: &EndpointInstanceBindingV1,
    key: &SigningKey,
    policy: &TrustedFencingPolicyV1,
    lease: Option<&str>,
    command: MonotonicOracleCommandV1,
) -> Result<MonotonicOracleReplyBodyV1, FencingError> {
    let body = MonotonicOracleRequestBodyV1 {
        schema_version: 1,
        instance: instance.clone(),
        authentication_public_key: hex::encode(key.verifying_key().to_bytes()),
        lease: lease.map(str::to_string),
        challenge: fresh_challenge()?,
        command,
    };
    let request_digest = hash_value(&body)?;
    let request = SignedMonotonicOracleRequestV1 {
        signature: sign(REQUEST, &body, key)?,
        body,
    };
    let encoded = serde_cbor::to_vec(&request).map_err(|e| invalid(e.to_string()))?;
    if encoded.len() > LIMIT {
        return Err(invalid("monotonic oracle frame bound"));
    }
    stream
        .write_all(&(encoded.len() as u32).to_be_bytes())
        .and_then(|_| stream.write_all(&encoded))
        .map_err(|e| FencingError::Io(format!("monotonic oracle request write: {e}")))?;
    let mut length = [0; 4];
    stream
        .read_exact(&mut length)
        .map_err(|e| FencingError::Io(format!("monotonic oracle response header read: {e}")))?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > LIMIT {
        return Err(invalid("monotonic oracle frame bound"));
    }
    let mut bytes = vec![0; length];
    stream
        .read_exact(&mut bytes)
        .map_err(|e| FencingError::Io(format!("monotonic oracle response body read: {e}")))?;
    let reply: SignedMonotonicOracleReplyV1 =
        serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    validate_reply_with(
        &reply,
        &request_digest,
        &request.body.challenge,
        lease,
        instance,
        policy,
        &mut LegacyReplyValidation,
    )?;
    Ok(reply.body)
}
trait ReplyValidationOps {
    type Error: From<FencingError>;
    fn verify_reply(
        &mut self,
        reply: &SignedMonotonicOracleReplyV1,
        policy: &TrustedFencingPolicyV1,
    ) -> Result<(), Self::Error>;
    fn compare(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn resource_allowed(
        &mut self,
        resource: &str,
        policy: &TrustedFencingPolicyV1,
    ) -> Result<bool, Self::Error>;
    fn validate_checkpoint(
        &mut self,
        value: &EndpointCheckpointBindingV1,
    ) -> Result<(), Self::Error>;
    fn validate_seal(&mut self, value: &EpochSealBodyV1) -> Result<(), Self::Error>;
    fn verify_evidence(
        &mut self,
        value: &MonotonicProviderEvidenceV1,
        instance: &EndpointInstanceBindingV1,
        checkpoint: &EndpointCheckpointBindingV1,
        policy: &TrustedFencingPolicyV1,
    ) -> Result<(), Self::Error>;
    fn seal_digest(&mut self, value: &EpochSealBodyV1) -> Result<String, Self::Error>;
}
struct LegacyReplyValidation;
impl ReplyValidationOps for LegacyReplyValidation {
    type Error = FencingError;
    fn verify_reply(
        &mut self,
        reply: &SignedMonotonicOracleReplyV1,
        policy: &TrustedFencingPolicyV1,
    ) -> Result<(), FencingError> {
        verify(
            RESPONSE,
            &reply.body,
            &reply.signature,
            &policy.provider_public_key,
        )
    }
    fn compare(&mut self, _: usize) -> Result<(), FencingError> {
        Ok(())
    }
    fn resource_allowed(
        &mut self,
        resource: &str,
        policy: &TrustedFencingPolicyV1,
    ) -> Result<bool, FencingError> {
        Ok(policy
            .authorized_resource_bindings
            .iter()
            .any(|v| v == resource))
    }
    fn validate_checkpoint(
        &mut self,
        value: &EndpointCheckpointBindingV1,
    ) -> Result<(), FencingError> {
        value.validate()
    }
    fn validate_seal(&mut self, value: &EpochSealBodyV1) -> Result<(), FencingError> {
        value.validate()
    }
    fn verify_evidence(
        &mut self,
        value: &MonotonicProviderEvidenceV1,
        instance: &EndpointInstanceBindingV1,
        checkpoint: &EndpointCheckpointBindingV1,
        policy: &TrustedFencingPolicyV1,
    ) -> Result<(), FencingError> {
        verify_monotonic_checkpoint_evidence(value, instance, checkpoint, policy)
    }
    fn seal_digest(&mut self, value: &EpochSealBodyV1) -> Result<String, FencingError> {
        value.digest()
    }
}
fn reply_text_equal<O: ReplyValidationOps>(
    ops: &mut O,
    left: &str,
    right: &str,
) -> Result<bool, O::Error> {
    ops.compare(left.len().saturating_add(right.len()))?;
    Ok(left == right)
}
fn reply_instance_bytes(value: &EndpointInstanceBindingV1) -> usize {
    [
        &value.world_id,
        &value.chain_id,
        &value.genesis_digest,
        &value.endpoint_id,
        &value.endpoint_public_key,
        &value.instance_id,
        &value.storage_id,
        &value.storage_fingerprint,
    ]
    .into_iter()
    .fold(std::mem::size_of::<EndpointInstanceBindingV1>(), |n, v| {
        n.saturating_add(v.len())
    })
}
fn validate_reply_with<O: ReplyValidationOps>(
    reply: &SignedMonotonicOracleReplyV1,
    request_digest: &str,
    challenge: &str,
    old_lease: Option<&str>,
    instance: &EndpointInstanceBindingV1,
    policy: &TrustedFencingPolicyV1,
    ops: &mut O,
) -> Result<(), O::Error> {
    ops.verify_reply(reply, policy)?;
    let state = &reply.body;
    ops.compare(state.lease.len())?;
    canonical_hash(&state.lease).map_err(O::Error::from)?;
    ops.compare(std::mem::size_of::<u32>() + std::mem::size_of::<ProviderCapabilityV1>())?;
    if state.schema_version != 1
        || state.capability != ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1
        || !ops.resource_allowed(&state.resource_binding, policy)?
        || !reply_text_equal(ops, &state.request_digest, request_digest)?
        || !reply_text_equal(ops, &state.challenge, challenge)?
        || match old_lease {
            Some(old) => !reply_text_equal(ops, old, &state.lease)?,
            None => false,
        }
    {
        return Err(O::Error::from(FencingError::FencingUnproven));
    }
    ops.validate_checkpoint(&state.checkpoint)?;
    if let Some(seal) = &state.seal {
        ops.validate_seal(seal)?;
        ops.compare(
            reply_instance_bytes(&seal.instance).saturating_add(reply_instance_bytes(instance)),
        )?;
        if &seal.instance != instance {
            return Err(O::Error::from(FencingError::FencingUnproven));
        }
    }
    if state.pending.is_none() {
        let evidence = state
            .evidence
            .as_ref()
            .ok_or_else(|| O::Error::from(FencingError::FencingUnproven))?;
        ops.verify_evidence(evidence, instance, &state.checkpoint, policy)?;
        if !reply_text_equal(
            ops,
            &evidence.statement.resource_binding,
            &state.resource_binding,
        )? {
            return Err(O::Error::from(FencingError::FencingUnproven));
        }
        let seal_digest = state
            .seal
            .as_ref()
            .map(|seal| ops.seal_digest(seal))
            .transpose()?;
        let equal_digest = match (&evidence.statement.seal_digest, &seal_digest) {
            (Some(left), Some(right)) => reply_text_equal(ops, left, right)?,
            (None, None) => true,
            _ => false,
        };
        if !equal_digest {
            return Err(O::Error::from(FencingError::FencingUnproven));
        }
        ops.compare(2 * std::mem::size_of::<u64>())?;
        if evidence.statement.epoch_floor
            != state
                .seal
                .as_ref()
                .map(|s| s.next_epoch_floor)
                .unwrap_or(instance.authority_epoch)
        {
            return Err(O::Error::from(FencingError::FencingUnproven));
        }
    }
    Ok(())
}

// The original Begin predicates/order; callbacks meter equality only.
trait BeginReplyValidationOps {
    type Error: From<FencingError>;
    fn same<T: Serialize + PartialEq>(&mut self, a: &T, b: &T) -> Result<bool, Self::Error>;
}
struct LegacyBeginValidation;
impl BeginReplyValidationOps for LegacyBeginValidation {
    type Error = FencingError;
    fn same<T: Serialize + PartialEq>(&mut self, a: &T, b: &T) -> Result<bool, FencingError> {
        Ok(a == b)
    }
}
fn validate_begin_gate_with<E: From<FencingError>>(
    state: &MonotonicOracleReplyBodyV1,
    mut work: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), E> {
    work(std::mem::size_of_val(&state.durable_set))?;
    if state
        .durable_set
        .as_ref()
        .is_some_and(|s| s.activation_digest.is_none())
    {
        return Err(FencingError::FencingUnproven.into());
    }
    work(std::mem::size_of_val(&state.seal))?;
    if state.seal.is_some() {
        return Err(FencingError::EpochSealed.into());
    }
    Ok(())
}
fn validate_begin_reply_with<O: BeginReplyValidationOps>(
    state: &MonotonicOracleReplyBodyV1,
    previous_state: &MonotonicOracleReplyBodyV1,
    intent: &CheckpointIntentV1,
    ops: &mut O,
) -> Result<(), O::Error> {
    if !ops.same(&state.checkpoint, &intent.previous)?
        || !ops.same(&state.pending.as_ref(), &Some(intent))?
        || !ops.same(&state.resource_binding, &previous_state.resource_binding)?
        || !ops.same(&state.durable_set, &previous_state.durable_set)?
        || !ops.same(&state.protected_custody, &previous_state.protected_custody)?
        || !ops.same(&state.seal, &previous_state.seal)?
    {
        return Err(FencingError::FencingUnproven.into());
    }
    Ok(())
}

fn validate_finish_reply_with<O: BeginReplyValidationOps>(
    state: &MonotonicOracleReplyBodyV1,
    previous_state: &MonotonicOracleReplyBodyV1,
    intent: &CheckpointIntentV1,
    ops: &mut O,
) -> Result<(), O::Error> {
    if !ops.same(&state.checkpoint, &intent.next)?
        || !ops.same(&state.resource_binding, &previous_state.resource_binding)?
        || !ops.same(&state.durable_set, &previous_state.durable_set)?
        || !ops.same(&state.protected_custody, &previous_state.protected_custody)?
        || (previous_state.protected_custody.is_some()
            && !ops.same(
                &state.native_checkpoint_bytes.as_ref(),
                &Some(&intent.checkpoint_bytes),
            )?)
        || !ops.same(&state.pending.is_none(), &true)?
        || !ops.same(
            &state.seal.as_ref(),
            &intent.seal.as_ref().or(previous_state.seal.as_ref()),
        )?
    {
        return Err(FencingError::FencingUnproven.into());
    }
    Ok(())
}

impl MonotonicEndpointProvider for RemoteMonotonicEndpointProvider {
    fn install_durable_set(
        &self,
        instance: &EndpointInstanceBindingV1,
        sealed: &crate::controlled_authority::formal::VerifiedSealedDurableSetTransitionV1,
        request: DurableSetNativeInstallRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        durable_set_client::install(self, instance, sealed, request)
    }
    fn query_durable_set_install(
        &self,
        instance: &EndpointInstanceBindingV1,
        sealed: &crate::controlled_authority::formal::VerifiedSealedDurableSetTransitionV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        durable_set_client::query(self, instance, sealed)
    }
    fn activate_durable_set(
        &self,
        instance: &EndpointInstanceBindingV1,
        sealed: &crate::controlled_authority::formal::VerifiedSealedDurableSetTransitionV1,
        activation: &crate::controlled_authority::formal::VerifiedDurableSetActivationV1,
        request: DurableSetNativeActivationRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        durable_set_client::activate(self, instance, sealed, activation, request)
    }
    fn initialize_protected(
        &self,
        instance: &EndpointInstanceBindingV1,
        registration: ProtectedEndpointRegistrationV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        protected_client::initialize(self, instance, registration)
    }
    fn acquire(
        &self,
        instance: &EndpointInstanceBindingV1,
        initial: &EndpointCheckpointBindingV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        if instance.endpoint_public_key == hex::encode(self.key.verifying_key().to_bytes()) {
            return Err(FencingError::FencingUnproven);
        }
        let mut stream = connect_oracle(&self.address, self.timeout)?;
        let state = exchange(
            &mut stream,
            instance,
            &self.key,
            &self.policy,
            None,
            MonotonicOracleCommandV1::Acquire(initial.clone()),
        )?;
        Ok(Box::new(RemoteGuard {
            stream: Mutex::new(stream),
            instance: instance.clone(),
            key: self.key.clone(),
            policy: self.policy.clone(),
            state,
        }))
    }
}
impl MonotonicEndpointGuard for RemoteGuard {
    fn sign_endpoint(
        &mut self,
        request: ProtectedEndpointSigningRequestV1,
    ) -> Result<ProtectedEndpointSignatureV1, FencingError> {
        protected_client::protected_sign(self, request)
    }
    fn durable_set_state(&self) -> Option<&DurableSetProviderStateV1> {
        self.state.durable_set.as_ref()
    }
    fn protected_native_checkpoint(&self) -> Option<&[u8]> {
        self.state.native_checkpoint_bytes.as_deref()
    }
    fn checkpoint(&self) -> &EndpointCheckpointBindingV1 {
        &self.state.checkpoint
    }
    fn pending(&self) -> Option<&CheckpointIntentV1> {
        self.state.pending.as_ref()
    }
    fn seal(&self) -> Option<&EpochSealBodyV1> {
        self.state.seal.as_ref()
    }
    fn begin(&mut self, intent: CheckpointIntentV1) -> Result<(), FencingError> {
        validate_begin_gate_with(&self.state, |_| Ok::<(), FencingError>(()))?;
        let state = exchange(
            self.stream
                .get_mut()
                .map_err(|_| invalid("oracle stream poisoned"))?,
            &self.instance,
            &self.key,
            &self.policy,
            Some(&self.state.lease),
            MonotonicOracleCommandV1::Begin(Box::new(intent.clone())),
        )?;
        validate_begin_reply_with(&state, &self.state, &intent, &mut LegacyBeginValidation)?;
        self.state = state;
        Ok(())
    }
    fn finish(&mut self) -> Result<(), FencingError> {
        let intent = self
            .state
            .pending
            .clone()
            .ok_or_else(|| invalid("oracle finish without pending intent"))?;
        let state = exchange(
            self.stream
                .get_mut()
                .map_err(|_| invalid("oracle stream poisoned"))?,
            &self.instance,
            &self.key,
            &self.policy,
            Some(&self.state.lease),
            MonotonicOracleCommandV1::Finish,
        )?;
        validate_finish_reply_with(&state, &self.state, &intent, &mut LegacyBeginValidation)?;
        self.state = state;
        Ok(())
    }
    fn evidence(&self) -> Result<MonotonicProviderEvidenceV1, FencingError> {
        if self.state.pending.is_some() {
            return Err(FencingError::FencingUnproven);
        }
        self.state
            .evidence
            .clone()
            .ok_or(FencingError::FencingUnproven)
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn persistent_oracle_frames_keep_original_timeouts_without_nagle() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let timeout = Duration::from_secs(3);
        let mut client = connect_oracle(&listener.local_addr().unwrap(), timeout).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_nodelay(true).unwrap();
        peer.set_read_timeout(Some(timeout)).unwrap();
        peer.set_write_timeout(Some(timeout)).unwrap();
        assert!(client.nodelay().unwrap());
        assert_eq!(client.read_timeout().unwrap(), Some(timeout));
        assert_eq!(client.write_timeout().unwrap(), Some(timeout));
        for body in [b"first request".as_slice(), b"same leased connection"] {
            let header = (body.len() as u32).to_be_bytes();
            client.write_all(&header).unwrap();
            client.write_all(body).unwrap();
            let mut actual_header = [0; 4];
            peer.read_exact(&mut actual_header).unwrap();
            assert_eq!(actual_header, header);
            let mut actual_body = vec![0; body.len()];
            peer.read_exact(&mut actual_body).unwrap();
            assert_eq!(actual_body, body);
            peer.write_all(&header).unwrap();
            peer.write_all(&actual_body).unwrap();
            client.read_exact(&mut actual_header).unwrap();
            client.read_exact(&mut actual_body).unwrap();
            assert_eq!(actual_header, header);
            assert_eq!(actual_body, body);
        }
    }
}
