//! Candidate management never implements the ordinary durability interface.
//! Replies carry immutable signed receipts, not an assertion of Ready. After
//! activation, callers must probe the separately pinned ordinary RPC listener.
use super::*;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;
const DOMAIN: &str = "oasis7.formal.durable_set_candidate.request.v1";
#[derive(Clone, Serialize, Deserialize)]
enum Operation {
    Install,
    QueryInstall,
    Activate(
        Box<DurableSetInstallReceiptV1>,
        Box<DurableSetInstallReceiptV1>,
    ),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    schema_version: u32,
    transition_digest: String,
    instance_digest: String,
    challenge: String,
    operation: Operation,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    body: Body,
    signature: String,
}
#[derive(Serialize, Deserialize)]
enum Reply {
    Install(Box<DurableSetInstallReceiptV1>),
    Activated(Box<DurableSetInstallReceiptV1>),
    Unknown,
}
fn instance_digest(
    sealed: &VerifiedSealedDurableSetTransitionV1,
    role: EndpointRole,
) -> Result<String, FormalError> {
    let successor = &sealed.transition().body().successor;
    match role {
        EndpointRole::Primary => &successor.primary.instance,
        EndpointRole::Replica => &successor.replica.instance,
    }
    .digest()
    .map_err(super::super::endpoint::fencing_error)
}
type RecoveryInputs<'a> = (
    &'a std::path::Path,
    Arc<dyn super::super::fencing::MonotonicEndpointProvider>,
    u64,
);
pub struct DurableSetCandidateServerV1 {
    candidate: Option<DurableSetCandidateV1>,
    sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    role: EndpointRole,
    timeout: Duration,
    installed: Option<DurableSetInstallReceiptV1>,
    activated: bool,
    activation_receipts: Option<[DurableSetInstallReceiptV1; 2]>,
}
impl DurableSetCandidateServerV1 {
    /// Cached management reconciliation for an independently reopened active
    /// owner. This factory grants no ordinary endpoint or Ready permission.
    pub fn from_activated(
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        role: EndpointRole,
        timeout: Duration,
        activation: &VerifiedDurableSetActivationV1,
        primary: DurableSetInstallReceiptV1,
        replica: DurableSetInstallReceiptV1,
    ) -> Result<Self, FormalError> {
        let checked = verify_durable_set_activation(
            sealed.transition(),
            [
                &sealed.evidence().primary_seal,
                &sealed.evidence().replica_seal,
            ],
            [&primary, &replica],
        )?;
        if timeout.is_zero()
            || timeout > Duration::from_secs(60)
            || checked.activation_digest() != activation.activation_digest()
            || checked.successor().context_digest()? != activation.successor().context_digest()?
        {
            return Err(invalid("cached candidate management exact activation"));
        }
        Ok(Self {
            candidate: None,
            sealed,
            role,
            timeout,
            installed: Some(match role {
                EndpointRole::Primary => primary.clone(),
                EndpointRole::Replica => replica.clone(),
            }),
            activated: true,
            activation_receipts: Some([primary, replica]),
        })
    }
    pub fn new(
        candidate: DurableSetCandidateV1,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        role: EndpointRole,
        timeout: Duration,
    ) -> Result<Self, FormalError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(invalid("candidate RPC timeout in (0,60s]"));
        }
        Ok(Self {
            candidate: Some(candidate),
            sealed,
            role,
            timeout,
            installed: None,
            activated: false,
            activation_receipts: None,
        })
    }
    /// Recover the same durable candidate or a freshly proven active owner.
    /// Retained receipts alone never establish activation; all other query or
    /// readback failures remain errors, including authentication/rollback errors.
    pub fn reopen(
        path: &std::path::Path,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        role: EndpointRole,
        timeout: Duration,
        provider: Arc<dyn super::super::fencing::MonotonicEndpointProvider>,
        operation_byte_budget: u64,
    ) -> Result<(Self, Option<FileFormalEndpoint>), FormalError> {
        let retained = DurableSetCandidateV1::retained_activation_with_receipts(path, &sealed)?;
        if let Some((activation, receipts)) = &retained {
            match FileFormalEndpoint::open_durable_set_active(
                path,
                sealed.clone(),
                activation,
                role,
                provider.clone(),
                operation_byte_budget,
            ) {
                Ok(endpoint) => {
                    let server = Self::from_activated(
                        sealed,
                        role,
                        timeout,
                        activation,
                        receipts[0].clone(),
                        receipts[1].clone(),
                    )?;
                    return Ok((server, Some(endpoint)));
                }
                Err(FormalError::FencingUnproven) => {}
                Err(error) => return Err(error),
            }
        }
        let candidate = DurableSetCandidateV1::reopen(
            path,
            sealed.clone(),
            role,
            provider,
            operation_byte_budget,
        )?;
        let mut server = Self::new(candidate, sealed, role, timeout)?;
        server.activation_receipts = retained.map(|(_, receipts)| receipts);
        Ok((server, None))
    }

    /// The ordinary API cannot reopen a consumed owner without its independently
    /// configured provider/path. Healthy owners supply those exact inputs here.
    pub fn serve_one_recovering(
        &mut self,
        listener: &TcpListener,
        path: &std::path::Path,
        provider: Arc<dyn super::super::fencing::MonotonicEndpointProvider>,
        operation_byte_budget: u64,
    ) -> Result<Option<FileFormalEndpoint>, FormalError> {
        self.serve_one_inner(listener, Some((path, provider, operation_byte_budget)))
    }

    /// Returns the newly active owner only after dual receipt verification and
    /// protected activation. The caller publishes its ordinary port afterward.
    pub fn serve_one(
        &mut self,
        listener: &TcpListener,
    ) -> Result<Option<FileFormalEndpoint>, FormalError> {
        self.serve_one_inner(listener, None)
    }
    fn serve_one_inner(
        &mut self,
        listener: &TcpListener,
        recovery: Option<RecoveryInputs<'_>>,
    ) -> Result<Option<FileFormalEndpoint>, FormalError> {
        wire::address(listener.local_addr().map_err(wire::io)?)?;
        let (mut stream, peer) = listener.accept().map_err(wire::io)?;
        wire::address(peer)?;
        wire::configure(&stream, self.timeout)?;
        let request: Envelope = wire::read(&mut stream)?;
        let body = request.body;
        hash_hex(&body.challenge)?;
        if body.schema_version != 1
            || body.transition_digest != self.sealed.transition().digest()
            || body.instance_digest != instance_digest(&self.sealed, self.role)?
        {
            return Err(invalid("candidate RPC exact transition/instance"));
        }
        verify(
            DOMAIN,
            &body,
            &request.signature,
            &self.sealed.transition().body().successor.writer_public_key,
        )?;
        // Bind BOTH original receipts before consuming the owner. A failed
        // activation must not let a later valid-but-different pair replace it.
        if let Operation::Activate(primary, replica) = &body.operation {
            verify_durable_set_activation(
                self.sealed.transition(),
                [
                    &self.sealed.evidence().primary_seal,
                    &self.sealed.evidence().replica_seal,
                ],
                [primary, replica],
            )?;
            let receipts = [*primary.clone(), *replica.clone()];
            if self
                .activation_receipts
                .as_ref()
                .is_some_and(|original| original != &receipts)
            {
                return Err(invalid(
                    "candidate activation original dual receipt identity",
                ));
            }
            self.activation_receipts = Some(receipts);
        }
        let mut active = None;
        if self.candidate.is_none()
            && !self.activated
            && let Some((path, provider, budget)) = recovery
        {
            // activate(self) has already dropped the old owner and its lease.
            // Recover only on a new authenticated request, never in a busy loop.
            let (mut reopened, endpoint) = Self::reopen(
                path,
                self.sealed.clone(),
                self.role,
                self.timeout,
                provider,
                budget,
            )?;
            if reopened
                .activation_receipts
                .as_ref()
                .is_some_and(|retained| Some(retained) != self.activation_receipts.as_ref())
            {
                return Err(invalid("candidate recovery original dual receipt identity"));
            }
            // Persistence may itself have failed before the provider call. Keep
            // the authenticated pair fixed, without resurrecting the old owner
            // or treating the absent retained certificate as Active evidence.
            reopened.activation_receipts = self.activation_receipts.clone();
            *self = reopened;
            active = endpoint;
        }
        let installing = matches!(body.operation, Operation::Install);
        let result = match body.operation {
            Operation::Install | Operation::QueryInstall => {
                let install = if let Some(candidate) = &mut self.candidate {
                    if installing {
                        Some(candidate.install()?)
                    } else {
                        candidate.query_install()?
                    }
                } else {
                    self.installed.clone()
                };
                if let Some(install) = install {
                    super::super::durable_set_install::verify_one_durable_set_install(
                        &self.sealed,
                        self.role,
                        &install,
                    )?;
                    self.installed = Some(install.clone());
                    Reply::Install(Box::new(install))
                } else {
                    Reply::Unknown
                }
            }
            Operation::Activate(primary, replica) => {
                let activation = verify_durable_set_activation(
                    self.sealed.transition(),
                    [
                        &self.sealed.evidence().primary_seal,
                        &self.sealed.evidence().replica_seal,
                    ],
                    [&primary, &replica],
                )?;
                let own = match self.role {
                    EndpointRole::Primary => *primary.clone(),
                    EndpointRole::Replica => *replica.clone(),
                };
                if let Some(candidate) = self.candidate.take() {
                    // On uncertain failure this server cannot retry from memory;
                    // reopen from the exact durable candidate/provider instead.
                    active = Some(candidate.activate(&activation, *primary, *replica)?);
                    self.installed = Some(own.clone());
                    self.activated = true;
                } else if !self.activated || self.installed.as_ref() != Some(&own) {
                    return Err(invalid("candidate activation original receipt identity"));
                }
                Reply::Activated(Box::new(own))
            }
        };
        // Preserve the activated owner even when the management reply is lost.
        // The outer owner loop must publish it before handling reply I/O errors.
        if let Err(error) = wire::write(&mut stream, &result)
            && active.is_none()
        {
            return Err(error);
        }
        Ok(active)
    }
}
pub struct RemoteDurableSetCandidateV1 {
    address: SocketAddr,
    timeout: Duration,
    sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
    role: EndpointRole,
    key: SigningKey,
}
impl RemoteDurableSetCandidateV1 {
    pub fn new(
        address: SocketAddr,
        timeout: Duration,
        sealed: Arc<VerifiedSealedDurableSetTransitionV1>,
        role: EndpointRole,
        key: SigningKey,
    ) -> Result<Self, FormalError> {
        wire::address(address)?;
        if timeout.is_zero()
            || timeout > Duration::from_secs(60)
            || hex::encode(key.verifying_key().to_bytes())
                != sealed.transition().body().successor.writer_public_key
        {
            return Err(invalid("candidate RPC writer/timeout"));
        }
        Ok(Self {
            address,
            timeout,
            sealed,
            role,
            key,
        })
    }
    fn call(&mut self, operation: Operation) -> Result<Reply, FormalError> {
        let body = Body {
            schema_version: 1,
            transition_digest: self.sealed.transition().digest().into(),
            instance_digest: instance_digest(&self.sealed, self.role)?,
            challenge: fencing::fresh_challenge().map_err(wire::io)?,
            operation,
        };
        let envelope = Envelope {
            signature: hex::encode(self.key.sign(&bytes(DOMAIN, &body)?).to_bytes()),
            body,
        };
        let mut stream =
            TcpStream::connect_timeout(&self.address, self.timeout).map_err(wire::io)?;
        wire::configure(&stream, self.timeout)?;
        wire::write(&mut stream, &envelope)?;
        let reply: Reply = wire::read(&mut stream)?;
        match &reply {
            Reply::Install(install) | Reply::Activated(install) => {
                super::super::durable_set_install::verify_one_durable_set_install(
                    &self.sealed,
                    self.role,
                    install,
                )?;
            }
            Reply::Unknown => {
                return Err(FormalError::Io(
                    "candidate outcome unknown; reconcile original operation".into(),
                ));
            }
        }
        Ok(reply)
    }
    pub fn install(&mut self) -> Result<DurableSetInstallReceiptV1, FormalError> {
        match self.call(Operation::Install)? {
            Reply::Install(receipt) => Ok(*receipt),
            _ => Err(invalid("candidate install reply type")),
        }
    }
    pub fn query_install(&mut self) -> Result<DurableSetInstallReceiptV1, FormalError> {
        match self.call(Operation::QueryInstall)? {
            Reply::Install(receipt) => Ok(*receipt),
            _ => Err(invalid("candidate query reply type")),
        }
    }
    /// This result is an immutable install receipt. Ready requires a fresh
    /// ordinary protected RPC probe using the caller's verified activation.
    pub fn activate(
        &mut self,
        primary: DurableSetInstallReceiptV1,
        replica: DurableSetInstallReceiptV1,
    ) -> Result<DurableSetInstallReceiptV1, FormalError> {
        verify_durable_set_activation(
            self.sealed.transition(),
            [
                &self.sealed.evidence().primary_seal,
                &self.sealed.evidence().replica_seal,
            ],
            [&primary, &replica],
        )?;
        match self.call(Operation::Activate(Box::new(primary), Box::new(replica)))? {
            Reply::Activated(receipt) => Ok(*receipt),
            _ => Err(invalid("candidate activate reply type")),
        }
    }
}
