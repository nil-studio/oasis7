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
    stale_checkpoint_opens: Arc<std::sync::atomic::AtomicUsize>,
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
        let stale_checkpoint_opens = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let rejected_opens = Arc::clone(&stale_checkpoint_opens);
        let mut authority = authority;
        let mut instance = match role {
            EndpointRole::Primary => authority.definition().primary.instance.clone(),
            EndpointRole::Replica => authority.definition().replica.instance.clone(),
        };
        let join = std::thread::spawn(move || {
            // Independent registry pins role, activation and client credential;
            // values in an incoming request never register an identity.
            let mut ledger: Option<Ledger> = None;
            // This grant belongs to the same original protected ledger and
            // survives endpoint/client reconnects. It is not a new oracle.
            let mut repair: Option<(
                ProtectedRecoveryRepairRequestV1,
                ProtectedRecoveryRepairGrantV1,
            )> = None;
            for stream in listener.incoming() {
                if stopping.load(Ordering::Relaxed) {
                    break;
                }
                let mut stream = stream.unwrap();
                // Replies use the same length/body framing as the client.
                stream.set_nodelay(true).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let lease = h(222);
                let mut repair_lease = false;
                // Stream state never outlives this exclusive connection. The
                // verifier retains only the original digest state and hides
                // the internally computed receipt until its final hash matches.
                let mut receipt_replay: Option<ProtectedRecoveryReceiptReplayVerifierV1> = None;
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
                    let planned_credential = ledger.as_ref().and_then(|state| {
                        let native = state
                            .pending
                            .as_ref()
                            .map_or(state.native.as_slice(), |intent| {
                                intent.checkpoint_bytes.as_slice()
                            });
                        Store::verify_public(native, &instance.endpoint_public_key)
                            .ok()
                            .and_then(|body| {
                                body.recovery_plan.map(|plan| {
                                    plan.body.authorization.recovery_credential_public_key
                                })
                            })
                    });
                    let requested_repair_key = match &request.body.command {
                        MonotonicOracleCommandV1::RepairProtected(repair) => Some(
                            &repair
                                .plan
                                .body
                                .authorization
                                .recovery_credential_public_key,
                        ),
                        _ => None,
                    };
                    let permitted_credential = repair.as_ref().map_or_else(
                        || {
                            request.body.authentication_public_key == public(17)
                                || planned_credential.as_ref()
                                    == Some(&request.body.authentication_public_key)
                                || requested_repair_key
                                    == Some(&request.body.authentication_public_key)
                        },
                        |(_, grant)| {
                            request.body.authentication_public_key
                                == grant.recovery_credential_public_key
                                && (repair_lease
                                    || matches!(
                                        &request.body.command,
                                        MonotonicOracleCommandV1::RepairProtected(_)
                                    ))
                        },
                    );
                    if !instance_allowed
                        || !permitted_credential
                        || request.body.schema_version != 1
                        || request.body.lease.as_ref().is_some_and(|l| l != &lease)
                        || super::super::crypto::verify(
                            "oasis7.formal.monotonic_oracle.request.v1",
                            &request.body,
                            &request.signature,
                            &request.body.authentication_public_key,
                        )
                        .is_err()
                    {
                        break;
                    }
                    if let Some(state) = &ledger {
                        let current =
                            Store::verify_public(&state.native, &instance.endpoint_public_key)
                                .unwrap();
                        let mutation = match &request.body.command {
                            MonotonicOracleCommandV1::ProtectedSign(signing) => {
                                signing.purpose != ProtectedEndpointSignaturePurposeV1::RpcResponse
                            }
                            MonotonicOracleCommandV1::Begin(_)
                            | MonotonicOracleCommandV1::Finish
                            | MonotonicOracleCommandV1::ReplayRecoveryReceipt(_)
                            | MonotonicOracleCommandV1::BeginReplay(_)
                            | MonotonicOracleCommandV1::ReplayChunk { .. }
                            | MonotonicOracleCommandV1::FinishReplay
                            | MonotonicOracleCommandV1::InstallDurableSet(_)
                            | MonotonicOracleCommandV1::ActivateDurableSet(_) => true,
                            _ => false,
                        };
                        if oasis7_formal_protocol::controlled_authority::formal::validate_recovery_provider_credential(
                            &current, &request.body.authentication_public_key, mutation,
                        ).is_err() {
                            break;
                        }
                    }
                    if receipt_replay.is_some()
                        && !matches!(
                            &request.body.command,
                            MonotonicOracleCommandV1::ReplayChunk { .. }
                                | MonotonicOracleCommandV1::FinishReplay
                        )
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
                            MonotonicOracleCommandV1::RepairProtected(wanted) => {
                                let Some(state) = ledger.as_ref() else { break };
                                if request.body.lease.is_some()
                                    || hook.is_some()
                                    || wanted
                                        .plan
                                        .body
                                        .authorization
                                        .recovery_credential_public_key
                                        == public(17)
                                    || request.body.authentication_public_key
                                        != wanted
                                            .plan
                                            .body
                                            .authorization
                                            .recovery_credential_public_key
                                {
                                    break;
                                }
                                if let Some((original, grant)) = &repair {
                                    if original != wanted.as_ref()
                                        || grant.verify_request(&instance, wanted).is_err()
                                    {
                                        break;
                                    }
                                } else {
                                    if state.pending.is_some() || state.seal.is_some() {
                                        break;
                                    }
                                    let Ok(native) = Store::verify_public(
                                        &state.native,
                                        &instance.endpoint_public_key,
                                    ) else {
                                        break;
                                    };
                                    let Ok(grant) = authorize_protected_recovery_repair(
                                        &authority,
                                        &instance,
                                        &native,
                                        &state.native,
                                        &state.checkpoint,
                                        wanted,
                                    ) else {
                                        break;
                                    };
                                    repair = Some((*wanted.clone(), grant));
                                }
                                repair_lease = true;
                            }
                            MonotonicOracleCommandV1::BeginReplay(header) => {
                                let Some(state) = ledger.as_ref() else { break };
                                let Some((repair_request, grant)) = &repair else {
                                    break;
                                };
                                if !repair_lease
                                    || request.body.lease.as_deref() != Some(lease.as_str())
                                    || state.pending.is_some()
                                    || state.seal.is_some()
                                    || header.signing.checkpoint != state.checkpoint
                                {
                                    break;
                                }
                                let signature =
                                    hex::encode(role_key.sign(&header.signing.preimage).to_bytes());
                                let Ok(verifier) = ProtectedRecoveryReceiptReplayVerifierV1::new(
                                    &authority,
                                    &instance,
                                    repair_request,
                                    grant,
                                    *header.clone(),
                                    signature,
                                ) else {
                                    break;
                                };
                                receipt_replay = Some(verifier);
                            }
                            MonotonicOracleCommandV1::ReplayChunk { offset, bytes } => {
                                if request.body.lease.as_deref() != Some(lease.as_str()) {
                                    break;
                                }
                                let Some(verifier) = receipt_replay.as_mut() else {
                                    break;
                                };
                                if verifier.push(*offset, bytes).is_err() {
                                    break;
                                }
                            }
                            MonotonicOracleCommandV1::FinishReplay => {
                                if request.body.lease.as_deref() != Some(lease.as_str()) {
                                    break;
                                }
                                let Some(verifier) = receipt_replay.take() else {
                                    break;
                                };
                                let Ok((signing, signature)) = verifier.finish() else {
                                    break;
                                };
                                let Some(state) = ledger.as_ref() else { break };
                                if state.pending.is_some()
                                    || state.seal.is_some()
                                    || signing.checkpoint != state.checkpoint
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
                                    challenge: signing.challenge,
                                    endpoint_signature: signature,
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
                            MonotonicOracleCommandV1::ReplayRecoveryReceipt(replay) => {
                                let Some(state) = ledger.as_ref() else { break };
                                let Some((repair_request, grant)) = &repair else {
                                    break;
                                };
                                if !repair_lease
                                    || state.pending.is_some()
                                    || state.seal.is_some()
                                    || replay.signing.checkpoint != state.checkpoint
                                {
                                    break;
                                }
                                let signature =
                                    hex::encode(role_key.sign(&replay.signing.preimage).to_bytes());
                                if verify_protected_recovery_receipt_replay(
                                    &authority,
                                    &instance,
                                    repair_request,
                                    grant,
                                    replay,
                                    &signature,
                                )
                                .is_err()
                                {
                                    break;
                                }
                                let body = ProtectedEndpointSignatureBodyV1 {
                                    schema_version: 1,
                                    custody: ProtectedSignerCustodyV1::NonexportableLeaseBoundV1,
                                    instance_digest: instance.digest().unwrap(),
                                    resource_binding: resource.clone(),
                                    checkpoint: state.checkpoint.clone(),
                                    request_digest: replay.signing.digest().unwrap(),
                                    challenge: replay.signing.challenge.clone(),
                                    endpoint_signature: signature,
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
                                if let Some(bytes) = &reg.existing_checkpoint {
                                    let Ok(body) =
                                        Store::verify_public(bytes, &instance.endpoint_public_key)
                                    else {
                                        break;
                                    };
                                    let binding = Store::binding(&body, bytes);
                                    if binding != state.checkpoint
                                        && !state.pending.as_ref().is_some_and(|p| {
                                            binding == p.previous || binding == p.next
                                        })
                                    {
                                        rejected_opens.fetch_add(1, Ordering::Relaxed);
                                        break;
                                    }
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
                                let current = Store::verify_public(
                                    &state.native,
                                    &instance.endpoint_public_key,
                                )
                                .unwrap();
                                if signing.validate().is_err()
                                    || signing.instance_digest != instance.digest().unwrap()
                                    || signing.checkpoint != state.checkpoint
                                    || state.pending.is_some()
                                    || (state.seal.is_some() && signing.purpose.mutation())
                                    || repair.as_ref().is_some_and(|(request, _)| {
                                        validate_protected_recovery_repair_signing(&authority, &current, request, signing).is_err()
                                    })
                                    || oasis7_formal_protocol::controlled_authority::formal::validate_recovery_protected_signing_request(&current, &authority, signing).is_err()
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
                                    || repair.as_ref().is_some_and(|(request, _)| {
                                        validate_protected_recovery_repair_transition(&authority, &previous, &body, request).is_err()
                                    })
                                    || oasis7_formal_protocol::controlled_authority::formal::validate_recovery_checkpoint_transition(&previous, &body, &authority).is_err()
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
                        recovery_repair: repair.as_ref().map(|(_, grant)| grant.clone()),
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
            stale_checkpoint_opens,
            join: Some(join),
        }
    }
    #[allow(
        dead_code,
        reason = "only the DistFS endpoint-process harness inspects rejected checkpoint registrations"
    )]
    pub(super) fn stale_checkpoint_opens(&self) -> usize {
        self.stale_checkpoint_opens.load(Ordering::Relaxed)
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
