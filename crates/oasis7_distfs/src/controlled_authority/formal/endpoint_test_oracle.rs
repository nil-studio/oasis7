//! Test-only simulated external registry, lease, custody and floor; not physical evidence.
use super::*;
/// Optional test-only upgrade registry. The default oracle supports no healthy
/// transition commands. A hook is explicitly provisioned with opaque approval.
pub(super) trait DurableSetOracleHook: Send {
    fn accepts_instance(
        &self,
        requested: &EndpointInstanceBindingV1,
        current: &EndpointInstanceBindingV1,
        command: &MonotonicOracleCommandV1,
    ) -> bool;
    #[expect(
        clippy::too_many_arguments,
        reason = "test oracle CAS binds command, requested/current scope, authority, floor, protected key and resource independently"
    )]
    fn dispatch(
        &mut self,
        command: &MonotonicOracleCommandV1,
        requested: &EndpointInstanceBindingV1,
        current: &mut EndpointInstanceBindingV1,
        authority: &mut VerifiedAuthorityV1,
        ledger: &mut Option<Ledger>,
        role_key: &SigningKey,
        resource: &str,
    ) -> Result<bool, ()>;
    fn normal_allowed(&self, command: &MonotonicOracleCommandV1) -> bool;
    fn installed_state(&self) -> Option<DurableSetProviderStateV1>;
}
pub(super) struct Oracle {
    pub(super) address: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Oracle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        self.join.take().unwrap().join().unwrap();
    }
}
impl Oracle {
    pub(super) fn start(fixture: &Fixture, role: EndpointRole) -> Self {
        Self::start_with_custody(fixture, role, true)
    }
    pub(super) fn start_with_custody(fixture: &Fixture, role: EndpointRole, custody: bool) -> Self {
        Self::start_impl(
            fixture.authority.clone(),
            role,
            key(if role == EndpointRole::Primary { 5 } else { 6 }),
            fixture
                .authority
                .policy()
                .authorized_fencing_resource_bindings[0]
                .clone(),
            None,
            custody,
            None,
        )
    }
    pub(super) fn start_registered(
        authority: VerifiedAuthorityV1,
        role: EndpointRole,
        role_key: SigningKey,
        resource: String,
        expected: Option<HotBody>,
    ) -> Self {
        Self::start_impl(authority, role, role_key, resource, expected, true, None)
    }
    #[allow(
        dead_code,
        reason = "the actual endpoint-process harness opts into this shared test-only registry hook"
    )]
    pub(super) fn start_with_durable_set_hook(
        authority: VerifiedAuthorityV1,
        role: EndpointRole,
        role_key: SigningKey,
        resource: String,
        hook: Box<dyn DurableSetOracleHook>,
    ) -> Self {
        Self::start_impl(authority, role, role_key, resource, None, true, Some(hook))
    }
    fn start_impl(
        authority: VerifiedAuthorityV1,
        role: EndpointRole,
        role_key: SigningKey,
        resource: String,
        expected: Option<HotBody>,
        custody: bool,
        mut hook: Option<Box<dyn DurableSetOracleHook>>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let mut authority = authority;
        let mut instance = match role {
            EndpointRole::Primary => authority.definition().primary.instance.clone(),
            EndpointRole::Replica => authority.definition().replica.instance.clone(),
        };
        let join = std::thread::spawn(move || {
            // Independent registry pins role, activation and client credential;
            // values in an incoming request never register an identity.
            let mut ledger: Option<Ledger> = None;
            for stream in listener.incoming() {
                if stopping.load(Ordering::Relaxed) {
                    break;
                }
                let mut stream = stream.unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let lease = h(222);
                while !stopping.load(Ordering::Relaxed) {
                    let request = match recv::<SignedMonotonicOracleRequestV1>(&mut stream) {
                        Ok(Some(request)) => request,
                        Ok(None) => continue,
                        Err(()) => break,
                    };
                    let instance_allowed =
                        hook.as_ref()
                            .map_or(request.body.instance == instance, |hook| {
                                hook.accepts_instance(
                                    &request.body.instance,
                                    &instance,
                                    &request.body.command,
                                )
                            });
                    if !instance_allowed
                        || request.body.authentication_public_key != public(17)
                        || request.body.schema_version != 1
                        || request.body.lease.as_ref().is_some_and(|l| l != &lease)
                        || super::super::crypto::verify(
                            "oasis7.formal.monotonic_oracle.request.v1",
                            &request.body,
                            &request.signature,
                            &public(17),
                        )
                        .is_err()
                    {
                        break;
                    }
                    let mut protected_signature = None;
                    let handled = if let Some(hook) = hook.as_mut() {
                        match hook.dispatch(
                            &request.body.command,
                            &request.body.instance,
                            &mut instance,
                            &mut authority,
                            &mut ledger,
                            &role_key,
                            &resource,
                        ) {
                            Ok(handled) => handled,
                            Err(()) => break,
                        }
                    } else {
                        false
                    };
                    if !handled
                        && hook
                            .as_ref()
                            .is_some_and(|hook| !hook.normal_allowed(&request.body.command))
                    {
                        break;
                    }
                    if !handled {
                        match &request.body.command {
                            MonotonicOracleCommandV1::InitializeProtected(reg) => {
                                if reg.authority_id != authority.authority_id().unwrap()
                                    || reg.activation_ref != authority.activation_ref()
                                {
                                    break;
                                }
                                if ledger.is_none() {
                                    let prefix =
                                        8 + "oasis7.formal.endpoint_checkpoint.sign.v1".len();
                                    let body: HotBody = serde_cbor::from_slice(
                                        &reg.unsigned_checkpoint_preimage[prefix..],
                                    )
                                    .unwrap();
                                    if Store::checkpoint_signing_bytes(&body).unwrap()
                                        != reg.unsigned_checkpoint_preimage
                                        || body.version != 1
                                        || body.revision != 0
                                        || body.instance_digest != instance.digest().unwrap()
                                        || body.authority_id != reg.authority_id
                                        || body.activation_ref != reg.activation_ref
                                        || body.epoch != instance.authority_epoch
                                        || body.seal.is_some()
                                        || expected.as_ref().map_or(
                                            body.head
                                                != authority
                                                    .definition()
                                                    .world
                                                    .genesis_head()
                                                    .unwrap()
                                                || body.index.is_some()
                                                || body.pending.is_some()
                                                || body.decision_head.is_some()
                                                || body.head_node.is_some()
                                                || body.imported_head_node.is_some()
                                                || body.base_parent.is_some(),
                                            |registered| registered != &body,
                                        )
                                    {
                                        break;
                                    }
                                    #[derive(serde::Serialize)]
                                    struct Envelope<'a> {
                                        body: &'a HotBody,
                                        signature: String,
                                    }
                                    let native = serde_cbor::to_vec(&Envelope {
                                        body: &body,
                                        signature: hex::encode(
                                            role_key
                                                .sign(&reg.unsigned_checkpoint_preimage)
                                                .to_bytes(),
                                        ),
                                    })
                                    .unwrap();
                                    ledger = Some(Ledger {
                                        checkpoint: Store::binding(&body, &native),
                                        native,
                                        pending: None,
                                        seal: None,
                                    });
                                }
                                let state = ledger.as_ref().unwrap();
                                if reg.existing_checkpoint.as_ref().is_some_and(|bytes| {
                                    Store::verify_public(bytes, &instance.endpoint_public_key)
                                        .map(|body| {
                                            let binding = Store::binding(&body, bytes);
                                            binding != state.checkpoint
                                                && !state.pending.as_ref().is_some_and(|p| {
                                                    binding == p.previous || binding == p.next
                                                })
                                        })
                                        .unwrap_or(true)
                                }) {
                                    break;
                                }
                            }
                            MonotonicOracleCommandV1::Acquire(checkpoint) => {
                                let Some(state) = &ledger else { break };
                                if checkpoint != &state.checkpoint
                                    && !state.pending.as_ref().is_some_and(|p| {
                                        checkpoint == &p.previous || checkpoint == &p.next
                                    })
                                {
                                    break;
                                }
                            }
                            MonotonicOracleCommandV1::ProtectedSign(signing) => {
                                let Some(state) = &ledger else { break };
                                if signing.validate().is_err()
                                    || signing.instance_digest != instance.digest().unwrap()
                                    || signing.checkpoint != state.checkpoint
                                    || state.pending.is_some()
                                    || (state.seal.is_some() && signing.purpose.mutation())
                                {
                                    break;
                                }
                                let body = ProtectedEndpointSignatureBodyV1 {
                                    schema_version: 1,
                                    custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
                                    instance_digest: instance.digest().unwrap(),
                                    resource_binding: resource.clone(),
                                    checkpoint: state.checkpoint.clone(),
                                    request_digest: signing.digest().unwrap(),
                                    challenge: signing.challenge.clone(),
                                    endpoint_signature: hex::encode(
                                        role_key.sign(&signing.preimage).to_bytes(),
                                    ),
                                };
                                protected_signature = Some(ProtectedEndpointSignatureV1 {
                                    provider_signature: hex::encode(
                                        key(3)
                                            .sign(
                                                &protected_endpoint_signature_signing_bytes(&body)
                                                    .unwrap(),
                                            )
                                            .to_bytes(),
                                    ),
                                    body,
                                });
                            }
                            MonotonicOracleCommandV1::Begin(intent) => {
                                let Some(state) = &mut ledger else { break };
                                if state.seal.is_some()
                                    || state.pending.is_some()
                                    || intent.previous != state.checkpoint
                                    || state.checkpoint.revision.checked_add(1)
                                        != Some(intent.next.revision)
                                {
                                    break;
                                }
                                let body = Store::verify_public(
                                    &intent.checkpoint_bytes,
                                    &instance.endpoint_public_key,
                                )
                                .unwrap();
                                let previous = Store::verify_public(
                                    &state.native,
                                    &instance.endpoint_public_key,
                                )
                                .unwrap();
                                if Store::binding(&body, &intent.checkpoint_bytes) != intent.next
                                    || body.seal != intent.seal
                                    || body.instance_digest != previous.instance_digest
                                    || body.authority_id != authority.authority_id().unwrap()
                                    || body.epoch != instance.authority_epoch
                                    || body.activation_ref != authority.activation_ref()
                                    || body.durable_set != previous.durable_set
                                {
                                    break;
                                }
                                state.pending = Some(*intent.clone());
                            }
                            MonotonicOracleCommandV1::InstallDurableSet(_)
                            | MonotonicOracleCommandV1::QueryDurableSetInstall(_)
                            | MonotonicOracleCommandV1::ActivateDurableSet(_) => break,
                            MonotonicOracleCommandV1::Finish => {
                                let Some(state) = &mut ledger else { break };
                                let Some(intent) = state.pending.take() else {
                                    break;
                                };
                                state.checkpoint = intent.next;
                                state.native = intent.checkpoint_bytes;
                                state.seal = intent.seal;
                            }
                        }
                    }
                    let state = ledger.as_ref().unwrap();
                    let evidence = if state.pending.is_none() {
                        let statement = MonotonicProviderStatementV1 {
                            schema_version: 1,
                            capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
                            resource_binding: resource.clone(),
                            instance_digest: instance.digest().unwrap(),
                            checkpoint: state.checkpoint.clone(),
                            epoch_floor: state
                                .seal
                                .as_ref()
                                .map(|s| s.next_epoch_floor)
                                .unwrap_or(instance.authority_epoch),
                            seal_digest: state.seal.as_ref().map(|s| s.digest().unwrap()),
                        };
                        Some(MonotonicProviderEvidenceV1 {
                            signature: signed(
                                super::super::crypto::bytes(
                                    "oasis7.endpoint.monotonic-provider.v1",
                                    &statement,
                                )
                                .unwrap(),
                                3,
                            ),
                            statement,
                        })
                    } else {
                        None
                    };
                    let body = MonotonicOracleReplyBodyV1 {
                        schema_version: 1,
                        capability: ProviderCapabilityV1::MonotonicExclusiveSignerStorageV1,
                        resource_binding: resource.clone(),
                        request_digest: fencing::hash_value(&request.body).unwrap(),
                        challenge: request.body.challenge,
                        lease: lease.clone(),
                        checkpoint: state.checkpoint.clone(),
                        pending: state.pending.clone(),
                        seal: state.seal.clone(),
                        evidence,
                        native_checkpoint_bytes: Some(state.native.clone()),
                        protected_custody: custody
                            .then_some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1),
                        protected_signature,
                        durable_set: hook.as_ref().and_then(|hook| hook.installed_state()),
                    };
                    let reply = SignedMonotonicOracleReplyV1 {
                        signature: signed(monotonic_oracle_reply_signing_bytes(&body).unwrap(), 3),
                        body,
                    };
                    if send(&mut stream, &reply).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            address,
            stop,
            join: Some(join),
        }
    }
    pub(super) fn provider(&self, fixture: &Fixture) -> Arc<dyn MonotonicEndpointProvider> {
        Arc::new(
            RemoteMonotonicEndpointProvider::new(
                self.address,
                Duration::from_secs(3),
                key(17),
                TrustedFencingPolicyV1 {
                    provider_public_key: public(3),
                    authorized_resource_bindings: fixture
                        .authority
                        .policy()
                        .authorized_fencing_resource_bindings
                        .clone(),
                },
            )
            .unwrap(),
        )
    }
}
