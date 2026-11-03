use super::*;
impl Server {
    pub(super) fn handle(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        raw: &[u8],
    ) -> Result<Vec<u8>, String> {
        self.common_guards(transport)?;
        transport
            .request_context()
            .check_cancelled(|| "admission cancelled".to_owned())?;
        if let Ok(hint) = decode_frame::<super::super::batch_hint::HintRequestV2>(raw) {
            return self.hint(transport, hint);
        }
        let request = BatchRequestV2::decode(raw)?;
        if request.read.requester_peer_id != transport.peer_id().to_string()
            || request.read.serving_peer_id != self.local_peer
            || request.read.reference.source.binding_digest
                != self
                    .binding
                    .digest()
                    .map_err(|_| "admission binding digest")?
        {
            return Err("admission v2 source/Noise endpoints differ".into());
        }
        let subject = request.verify_subject()?;
        self.gate.validate_identity(&request.authorization)?;
        match &self.view {
            ViewOwner::Node(state) => {
                let view = self.node_view(transport, state)?;
                self.authorized(
                    transport,
                    &request,
                    &subject,
                    NodeBftAdmissionInputReadViewV1::Node(&view),
                )
            }
            ViewOwner::Provider(service) => {
                let started = Instant::now();
                debug_phase("handle.provider.observe.begin", started);
                let result = service
                    .with_observed_original_input_view(
                        transport.request_context().deadline(),
                        |view| {
                            debug_phase("handle.provider.observe.callback", started);
                            self.provider_guard(transport, view)
                                .map_err(BftSignerErrorV3)?;
                            self.authorized(
                                transport,
                                &request,
                                &subject,
                                NodeBftAdmissionInputReadViewV1::Provider(view),
                            )
                            .map_err(BftSignerErrorV3)
                        },
                    )
                    .map_err(|e| e.to_string());
                debug_phase(
                    if result.is_ok() {
                        "handle.provider.observe.ok"
                    } else {
                        "handle.provider.observe.error"
                    },
                    started,
                );
                result
            }
        }
    }
    fn hint(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        request: super::super::batch_hint::HintRequestV2,
    ) -> Result<Vec<u8>, String> {
        request.validate()?;
        if request.requester_peer_id != transport.peer_id().to_string()
            || request.serving_peer_id != self.local_peer
        {
            return Err("admission hint Noise endpoints differ".into());
        }
        let deadline = self
            .private
            .original_deadline(
                request.requester_peer_id.clone(),
                request.nonce.clone(),
                request.digest()?,
                digest(
                    b"oasis7.bft.admission-input.hint-identity.v1",
                    &encode(&(&request.requester_peer_id, &request.serving_peer_id))?,
                ),
                true,
            )?
            .min(transport.request_context().deadline());
        transport
            .restrict_reply_until(deadline)
            .map_err(|_| "admission hint deadline")?;
        let encode_hint = |view: NodeBftAdmissionInputReadViewV1<'_>| {
            encode_frame(&super::super::batch_hint::HintResponseV2 {
                domain: super::super::batch_hint::HINT_V2.into(),
                request_digest: request.digest()?,
                view: super::super::input_hint::NodeBftAdmissionInputCurrentViewV1 {
                    binding_digest: self
                        .binding
                        .digest()
                        .map_err(|_| "admission hint binding")?,
                    scope: view.schedule().scope().clone(),
                    schedule_digest: view.schedule().digest()?,
                    generation: view.generation(),
                },
            })
        };
        match &self.view {
            ViewOwner::Node(state) => {
                let view = self.node_view(transport, state)?;
                encode_hint(NodeBftAdmissionInputReadViewV1::Node(&view))
            }
            ViewOwner::Provider(service) => service
                .with_charged_original_input_response(deadline, |view| {
                    self.provider_guard(transport, view)
                        .map_err(BftSignerErrorV3)?;
                    encode_hint(NodeBftAdmissionInputReadViewV1::Provider(view))
                        .map_err(BftSignerErrorV3)
                })
                .map_err(|e| e.to_string()),
        }
    }
    /// Each invocation obtains a new actual local current view. No service or
    /// backend mutex spans issuer checks, immutable source reads, or networking.
    pub(super) fn fresh<T>(
        &self,
        base: NodeBftAdmissionInputReadViewV1<'_>,
        deadline: Instant,
        inspect: impl FnOnce(NodeBftAdmissionInputReadViewV1<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        match (&self.view, base) {
            (ViewOwner::Node(state), NodeBftAdmissionInputReadViewV1::Node(old)) => {
                if !state.is_serving() {
                    return Err("admission Node stopped".into());
                }
                let current = state.view()?.ok_or("admission Node view missing")?;
                if current.generation != old.generation || current.schedule != old.schedule {
                    return Err("admission Node base changed".into());
                }
                let result = inspect(NodeBftAdmissionInputReadViewV1::Node(&current))?;
                let after = state.view()?.ok_or("admission Node view missing")?;
                if !state.is_serving()
                    || after.generation != old.generation
                    || after.schedule != old.schedule
                {
                    return Err("admission Node base changed".into());
                }
                Ok(result)
            }
            (ViewOwner::Provider(service), NodeBftAdmissionInputReadViewV1::Provider(old)) => {
                let started = Instant::now();
                debug_phase("fresh.provider.observe.begin", started);
                let result = service
                    .with_observed_original_input_view(deadline, |current| {
                        debug_phase("fresh.provider.observe.callback", started);
                        if !same_provider(old, current) {
                            debug_phase("fresh.provider.base.changed", started);
                            return Err(BftSignerErrorV3("admission provider base changed".into()));
                        }
                        let result = inspect(NodeBftAdmissionInputReadViewV1::Provider(current))
                            .map_err(BftSignerErrorV3)?;
                        if !old.is_current() {
                            return Err(BftSignerErrorV3("admission original base revoked".into()));
                        }
                        Ok(result)
                    })
                    .map_err(|e| e.to_string());
                debug_phase(
                    if result.is_ok() {
                        "fresh.provider.observe.ok"
                    } else {
                        "fresh.provider.observe.error"
                    },
                    started,
                );
                result
            }
            _ => Err("admission view owner differs".into()),
        }
    }
    fn authorized(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        request: &BatchRequestV2,
        subject: &VerifiedBatchSubjectV2,
        base: NodeBftAdmissionInputReadViewV1<'_>,
    ) -> Result<Vec<u8>, String> {
        let body = &request.authorization.grant.body;
        let started = Instant::now();
        let mismatch = if body.scope != *base.schedule().scope() {
            Some("authorize.scope.mismatch")
        } else if body.schedule_digest != base.schedule().digest()? {
            Some("authorize.schedule.mismatch")
        } else if body.view_generation != base.generation() {
            Some("authorize.generation.mismatch")
        } else {
            None
        };
        if let Some(phase) = mismatch {
            debug_phase(phase, started);
            return Err("admission v2 grant current view differs".into());
        }
        debug_phase("authorize.scope.accepted", started);
        let mut calls = 0usize;
        let mut produced = None;
        let mut response = |until: Instant| {
            calls += 1;
            if calls != 1 {
                return Err("admission batch callback repeated".into());
            }
            debug_phase("authorize.original.deadline.begin", started);
            let deadline = self
                .private
                .input_deadline(
                    request.read.requester_peer_id.clone(),
                    request.read.nonce.clone(),
                    request.base()?,
                    digest(
                        b"oasis7.bft.admission-input.identity.v2",
                        &encode(&request.authorization.grant)?,
                    ),
                    matches!(request.read.operation, BatchOperationV2::Manifest),
                )?
                .min(until)
                .min(transport.request_context().deadline());
            debug_phase("authorize.original.deadline.accepted", started);
            transport
                .restrict_reply_until(deadline)
                .map_err(|_| "admission original deadline")?;
            let bytes = self.material(transport, request, subject, base, deadline)?;
            produced = Some((bytes.len(), blake3::hash(&bytes)));
            Ok(bytes)
        };
        let bytes = self.fresh(base, transport.request_context().deadline(), |current| {
            debug_phase("authorize.batch.gate.begin", started);
            self.gate.authorize_batch(
                transport,
                &NodeBftAdmissionInputBatchScopeV2 {
                    view: current,
                    subject,
                },
                &mut response,
            )
        })?;
        drop(response);
        if calls != 1 || produced != Some((bytes.len(), blake3::hash(&bytes))) {
            return Err("admission gate altered/skipped response".into());
        }
        Ok(bytes)
    }
}
pub(super) fn same_provider(
    a: &oasis7_distfs::protected_bft_v3::ObservedBftOriginalInputServingViewV1,
    b: &oasis7_distfs::protected_bft_v3::ObservedBftOriginalInputServingViewV1,
) -> bool {
    a.is_current()
        && b.is_current()
        && a.binding_digest() == b.binding_digest()
        && a.provider_public_key() == b.provider_public_key()
        && a.lease_generation() == b.lease_generation()
        && a.revision() == b.revision()
        && a.state_digest() == b.state_digest()
        && a.generation() == b.generation()
        && a.schedule() == b.schedule()
}
