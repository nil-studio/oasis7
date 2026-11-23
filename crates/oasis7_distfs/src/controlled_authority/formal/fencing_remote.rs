//! Client adapter for an independently provisioned monotonic/exclusive oracle.
//! Cryptography authenticates the pinned oracle's contract; it cannot establish
//! its physical nonrollback storage or signer isolation. A connection is one
//! exclusive lease and remains alive across native publication and signing.
use super::*;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};
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

#[path = "fencing_remote_durable_set.rs"]
mod durable_set_client;
#[path = "fencing_remote_lease_failure.rs"]
mod lease_failure;
#[cfg(test)]
#[path = "fencing_remote_lease_failure_tests.rs"]
mod lease_failure_tests;
#[path = "fencing_remote_protected.rs"]
mod protected_client;
#[path = "fencing_remote_recovery_repair.rs"]
mod recovery_repair_client;
pub use recovery_repair_client::RecoveryRepairEndpointProviderV1;
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
    RepairProtected(Box<ProtectedRecoveryRepairRequestV1>),
    ReplayRecoveryReceipt(Box<ProtectedRecoveryReceiptReplayV1>),
    BeginReplay(Box<ProtectedRecoveryReceiptReplayHeaderV1>),
    ReplayChunk { offset: u64, bytes: Vec<u8> },
    FinishReplay,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_repair: Option<ProtectedRecoveryRepairGrantV1>,
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
    failed: bool,
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
    exchange_before(stream, instance, key, policy, lease, command, None)
}

fn remaining(deadline: Instant) -> Result<Duration, FencingError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            FencingError::Io("monotonic oracle original operation deadline expired".into())
        })
}

fn write_before(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    deadline: Option<Instant>,
) -> Result<(), FencingError> {
    let Some(deadline) = deadline else {
        return stream
            .write_all(bytes)
            .map_err(|e| FencingError::Io(format!("monotonic oracle request write: {e}")));
    };
    while !bytes.is_empty() {
        stream
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(|e| FencingError::Io(e.to_string()))?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(FencingError::Io(
                    "monotonic oracle request write zero".into(),
                ));
            }
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(FencingError::Io(format!(
                    "monotonic oracle request write: {error}"
                )));
            }
        }
        remaining(deadline)?;
    }
    Ok(())
}

fn read_before(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: Option<Instant>,
    phase: &str,
) -> Result<(), FencingError> {
    let Some(deadline) = deadline else {
        return stream
            .read_exact(bytes)
            .map_err(|e| FencingError::Io(format!("monotonic oracle {phase} read: {e}")));
    };
    while !bytes.is_empty() {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(|e| FencingError::Io(e.to_string()))?;
        match stream.read(bytes) {
            Ok(0) => return Err(FencingError::Io(format!("monotonic oracle {phase} EOF"))),
            Ok(read) => bytes = &mut bytes[read..],
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(FencingError::Io(format!(
                    "monotonic oracle {phase} read: {error}"
                )));
            }
        }
        remaining(deadline)?;
    }
    Ok(())
}

fn exchange_before(
    stream: &mut TcpStream,
    instance: &EndpointInstanceBindingV1,
    key: &SigningKey,
    policy: &TrustedFencingPolicyV1,
    lease: Option<&str>,
    command: MonotonicOracleCommandV1,
    deadline: Option<Instant>,
) -> Result<MonotonicOracleReplyBodyV1, FencingError> {
    if let Some(deadline) = deadline {
        remaining(deadline)?;
    }
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
    write_before(stream, &(encoded.len() as u32).to_be_bytes(), deadline)?;
    write_before(stream, &encoded, deadline)?;
    let mut length = [0; 4];
    read_before(stream, &mut length, deadline, "response header")?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > LIMIT {
        return Err(invalid("monotonic oracle frame bound"));
    }
    let mut bytes = vec![0; length];
    read_before(stream, &mut bytes, deadline, "response body")?;
    let reply: SignedMonotonicOracleReplyV1 =
        serde_cbor::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    verify(
        RESPONSE,
        &reply.body,
        &reply.signature,
        &policy.provider_public_key,
    )?;
    let state = reply.body;
    canonical_hash(&state.lease)?;
    if state.schema_version != 1
        || state.capability != ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1
        || !policy
            .authorized_resource_bindings
            .contains(&state.resource_binding)
        || state.request_digest != request_digest
        || state.challenge != request.body.challenge
        || lease.is_some_and(|old| old != state.lease)
    {
        return Err(FencingError::FencingUnproven);
    }
    state.checkpoint.validate()?;
    if let Some(seal) = &state.seal {
        seal.validate()?;
        if &seal.instance != instance {
            return Err(FencingError::FencingUnproven);
        }
    }
    if state.pending.is_none() {
        let evidence = state
            .evidence
            .as_ref()
            .ok_or(FencingError::FencingUnproven)?;
        verify_monotonic_checkpoint_evidence(evidence, instance, &state.checkpoint, policy)?;
        if evidence.statement.resource_binding != state.resource_binding {
            return Err(FencingError::FencingUnproven);
        }
        if evidence.statement.seal_digest
            != state
                .seal
                .as_ref()
                .map(EpochSealBodyV1::digest)
                .transpose()?
            || evidence.statement.epoch_floor
                != state
                    .seal
                    .as_ref()
                    .map(|s| s.next_epoch_floor)
                    .unwrap_or(instance.authority_epoch)
        {
            return Err(FencingError::FencingUnproven);
        }
    }
    if let Some(deadline) = deadline {
        remaining(deadline)?;
    }
    Ok(state)
}
impl MonotonicEndpointProvider for RemoteMonotonicEndpointProvider {
    fn repair_protected(
        &self,
        instance: &EndpointInstanceBindingV1,
        request: ProtectedRecoveryRepairRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        recovery_repair_client::open(self, instance, request)
    }
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
        if state.recovery_repair.is_some() {
            return Err(FencingError::FencingUnproven);
        }
        Ok(Box::new(RemoteGuard {
            failed: false,
            stream: Mutex::new(stream),
            instance: instance.clone(),
            key: self.key.clone(),
            policy: self.policy.clone(),
            state,
        }))
    }
}
impl MonotonicEndpointGuard for RemoteGuard {
    fn recovery_repair_grant(&self) -> Option<&ProtectedRecoveryRepairGrantV1> {
        self.state.recovery_repair.as_ref()
    }
    fn replay_recovery_receipt(
        &mut self,
        request: ProtectedRecoveryReceiptReplayV1,
    ) -> Result<ProtectedEndpointSignatureV1, FencingError> {
        self.with_live_lease(|guard| recovery_repair_client::replay(guard, request))
    }
    fn sign_endpoint(
        &mut self,
        request: ProtectedEndpointSigningRequestV1,
    ) -> Result<ProtectedEndpointSignatureV1, FencingError> {
        self.with_live_lease(|guard| protected_client::protected_sign(guard, request))
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
        self.with_live_lease(|guard| guard.begin_live(intent))
    }
    fn finish(&mut self) -> Result<(), FencingError> {
        self.with_live_lease(Self::finish_live)
    }
    fn evidence(&self) -> Result<MonotonicProviderEvidenceV1, FencingError> {
        if self.failed || self.state.pending.is_some() {
            return Err(FencingError::FencingUnproven);
        }
        self.state
            .evidence
            .clone()
            .ok_or(FencingError::FencingUnproven)
    }
}

impl RemoteGuard {
    fn begin_live(&mut self, intent: CheckpointIntentV1) -> Result<(), FencingError> {
        if self
            .state
            .durable_set
            .as_ref()
            .is_some_and(|s| s.activation_digest.is_none())
        {
            return Err(FencingError::FencingUnproven);
        }
        if self.state.seal.is_some() {
            return Err(FencingError::EpochSealed);
        }
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
        if state.checkpoint != intent.previous
            || state.pending.as_ref() != Some(&intent)
            || state.resource_binding != self.state.resource_binding
            || state.durable_set != self.state.durable_set
            || state.recovery_repair != self.state.recovery_repair
            || state.protected_custody != self.state.protected_custody
            || state.seal != self.state.seal
        {
            return Err(FencingError::FencingUnproven);
        }
        self.state = state;
        Ok(())
    }
    fn finish_live(&mut self) -> Result<(), FencingError> {
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
        if state.checkpoint != intent.next
            || state.resource_binding != self.state.resource_binding
            || state.durable_set != self.state.durable_set
            || state.recovery_repair != self.state.recovery_repair
            || state.protected_custody != self.state.protected_custody
            || (self.state.protected_custody.is_some()
                && state.native_checkpoint_bytes.as_ref() != Some(&intent.checkpoint_bytes))
            || state.pending.is_some()
            || state.seal != intent.seal.or(self.state.seal.clone())
        {
            return Err(FencingError::FencingUnproven);
        }
        self.state = state;
        Ok(())
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
