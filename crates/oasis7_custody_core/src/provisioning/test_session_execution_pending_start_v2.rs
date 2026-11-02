//! The first actual ingress5 commits before the remaining134 reservation.
//! Accepted metadata is not Current, BookSome, Source payment or send authority.
use super::*;
#[path = "test_session_execution_pending_inventory_v2.rs"]
mod inventory;
pub(in crate::provisioning) use inventory::ExecutionPendingLeafV2;
use inventory::StartIngressPortV2;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct ExecutionPendingIngressV2 {
    pub(super) request: SessionActionRequestV2,
    pub(super) approved_input_digest: String,
    pub(super) signed_attempt_digest: String,
    pub(super) credential_generation: DecimalU64,
    pub(super) challenge_request_id: String,
    pub(super) signed_until: UnixMillis,
    pub(super) charged_at: UnixMillis,
    pub(super) ingress_ports: [String; 4],
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct ExecutionPendingStartV2 {
    pub(super) ingress_attempt_id: String,
    pub(super) source_nonce_salt: String,
    pub(super) source_operation_id: String,
    pub(super) source_verify_nonce: String,
    pub(super) accepted_at: UnixMillis,
    pub(super) absolute_until: UnixMillis,
    pub(super) accepted_lease_until: UnixMillis,
    pub(super) accepted_ordinal: u64,
    physical_ports: Vec<ExecutionPendingPhysicalPortV2>,
    #[serde(deserialize_with = "required_nullable")]
    directory_signature: Option<PendingDirectorySignatureV2>,
    #[serde(deserialize_with = "required_nullable")]
    fence_dispatch: Option<fence::FenceDispatchRecord>,
    #[serde(deserialize_with = "required_nullable")]
    fence_read: Option<fence::FenceReadRecord>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionPendingPhysicalPortV2 {
    leaf: ExecutionPendingLeafV2,
    physical_id: String,
    #[serde(deserialize_with = "required_nullable")]
    claimed_ordinal: Option<u64>,
    #[serde(deserialize_with = "required_nullable")]
    claimed_at: Option<UnixMillis>,
}
fn required_nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> core::result::Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
/// Only the first successful Account SQL returns this non-wire handle. It is
/// consumed by the next fixed leaf factory, never by a generic transport.
pub struct AcceptedExecutionPendingStartV2 {
    session_id: String,
    ingress_attempt_id: String,
    accepted_ordinal: u64,
}
impl AcceptedExecutionPendingStartV2 {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}
fn port_id(
    s: &TestSessionAuthorityV2,
    ingress: &str,
    original: &ExecutionPendingIngressV2,
    leaf: &ExecutionPendingLeafV2,
) -> Result<String> {
    Ok(format!(
        "execution-start-v2-{}",
        digest(&(
            "oasis7.custody.execution-pending-port.v2",
            s.input_digest()?,
            &s.input.operation_id,
            ingress,
            &original.request,
            &original.signed_attempt_digest,
            leaf
        ))?
    ))
}
fn original_input(
    state: &AccountProvisionApprovalState,
    proof: &SessionProofRequest<'_>,
    now: UnixMillis,
) -> Result<ExecutionPendingIngressV2> {
    let q = super::v2_action_http::action_original_v2(proof)?;
    if q.purpose() != SessionPurposeV1::Execution {
        return Err(invalid());
    }
    let s = state.test_session_v2_local(&q.session_id, SessionPurposeV1::Execution, now)?;
    if s.execution_book.is_some()
        || !s
            .native_machine_registration
            .as_ref()
            .is_some_and(|m| m.ready())
    {
        return Err(invalid());
    }
    super::authenticated_receipt_v2(s, SessionPurposeV1::Execution, proof.possession, now)?;
    super::v2_purpose_http::execution_target_v2(
        state,
        s,
        SessionPurposeV1::Execution,
        proof.possession,
    )?;
    let root = s
        .roots
        .get(&SessionPurposeV1::Execution)
        .ok_or_else(invalid)?;
    let (plan, _) = s.header()?;
    if !plan.selection.is_exact_move0()
        || plan.business_limit.0 != 0
        || !plan.allowed_resource_refs.is_empty()
        || !matches!((&q.action,plan.actions.as_slice()),(SessionToolActionV1::Execute(a),[b]) if a==b)
    {
        return Err(invalid());
    }
    let nonce = proof.possession.nonce().ok_or_else(invalid)?;
    let (challenge_id, c) = s
        .challenges
        .iter()
        .find(|(_, c)| c.nonce == nonce)
        .ok_or_else(invalid)?;
    let link = s.tool_nonce_links.get(challenge_id).ok_or_else(invalid)?;
    let metadata = s
        .purpose_http_admissions
        .get(&link.first_ingress)
        .ok_or_else(invalid)?;
    if !matches!(
        metadata.class,
        SessionPurposeHttpAdmissionClassV1::ExecutionNonceFiveV2 { .. }
    ) || c.kind != SessionChallengeKindV1::ToolMetadata
        || c.purpose != SessionPurposeV1::Execution
        || c.logical_request_id != q.request_id
        || c.body_digest != q.digest()?
        || c.credential_generation != root.credential.as_ref().ok_or_else(invalid)?.generation
        || now.get() >= c.expires_at.get()
    {
        return Err(invalid());
    }
    let until = proof
        .possession
        .expires()
        .checked_millis()
        .map_err(|_| invalid())?;
    let original = ExecutionPendingIngressV2 {
        request: q,
        approved_input_digest: s.input_digest()?,
        signed_attempt_digest: proof.possession.transport_attempt_digest(),
        credential_generation: c.credential_generation,
        challenge_request_id: challenge_id.clone(),
        signed_until: until,
        charged_at: now,
        ingress_ports: std::array::from_fn(|_| String::new()),
    };
    let ports = [
        StartIngressPortV2::AbuseProviderActor,
        StartIngressPortV2::AbuseRealmActor,
        StartIngressPortV2::OwnerService,
        StartIngressPortV2::AccountActor,
    ]
    .map(|p| {
        port_id(
            s,
            proof.ingress_attempt_id,
            &original,
            &ExecutionPendingLeafV2::Ingress(p),
        )
    })
    .into_iter()
    .collect::<Result<Vec<_>>>()?;
    Ok(ExecutionPendingIngressV2 {
        ingress_ports: ports.try_into().map_err(|_| invalid())?,
        ..original
    })
}
fn fee_class(original: &ExecutionPendingIngressV2) -> SessionPurposeHttpAdmissionClassV1 {
    let [
        provider_abuse_attempt,
        realm_abuse_attempt,
        owner_attempt,
        account_attempt,
    ] = original.ingress_ports.clone();
    SessionPurposeHttpAdmissionClassV1::ExecutionStartFiveV2 {
        provider_abuse_attempt,
        realm_abuse_attempt,
        owner_attempt,
        account_attempt,
    }
}
impl AccountProvisionApprovalState {
    /// SQL1, after actual H PoP and the two real limiter leaves. No RNG/Source IO.
    pub fn charge_test_session_v2_execution_pending_ingress(
        &mut self,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let mut original = original_input(self, &proof, now)?;
        let id = &original.request.session_id;
        let s = self.test_session_v2_record(id)?;
        if let Some(old) = s.execution_pending_ingresses.get(proof.ingress_attempt_id) {
            original.charged_at = old.charged_at;
            return if old == &original
                && s.purpose_http_admissions
                    .get(proof.ingress_attempt_id)
                    .is_some_and(|p| {
                        p.class == fee_class(old)
                            && p.signed_attempt_digest == old.signed_attempt_digest
                    })
            {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if s.purpose_http_admissions.len() >= 700
            || s.challenges
                .get(&original.challenge_request_id)
                .ok_or_else(invalid)?
                .consumed_by
                .is_some()
        {
            return Err(invalid());
        }
        for key in std::iter::once(proof.ingress_attempt_id)
            .chain(original.ingress_ports.iter().map(String::as_str))
        {
            self.test_session_v2_transport_id_available(id, key)?;
        }
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(id)?;
        saved
            .roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(5)
            .map_err(|_| invalid())?;
        saved.purpose_http_admissions.insert(
            proof.ingress_attempt_id.into(),
            SessionPurposeHttpAdmissionV1 {
                class: fee_class(&original),
                signed_attempt_digest: original.signed_attempt_digest.clone(),
                purpose: SessionPurposeV1::Execution,
                hop: SessionControlHopV1::Host,
                charged_at: now,
            },
        );
        for key in &original.ingress_ports {
            saved
                .purpose_transports
                .insert(key.clone(), SessionPurposeV1::Execution.into());
        }
        saved
            .execution_pending_ingresses
            .insert(proof.ingress_attempt_id.into(), original);
        append_session_event_v2(
            saved,
            SessionEventKindV2::PurposeHttpCharged {
                ingress_attempt_id: proof.ingress_attempt_id.into(),
            },
            now,
        )?;
        if saved
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && saved.closing.is_none()
        {
            saved.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
    /// SQL2. All134 leaves are held atomically, but none is claimed for sending.
    /// Failing this transition preserves SQL1's five already-observed charges.
    pub fn reserve_test_session_v2_execution_pending(
        &mut self,
        proof: SessionProofRequest<'_>,
        source_nonce_salt: &str,
        native_scope: &NativeStageScope<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<Option<AcceptedExecutionPendingStartV2>> {
        let mut original = original_input(self, &proof, now)?;
        let id = original.request.session_id.clone();
        // Native history remains anchored to the actual configured external
        // Root. Ready metadata/self-reported stored pins do not authorize the
        // new master. This local strong verification performs no HTTP.
        for job in self.test_session_v2_native_restore_checks(&id, native_scope)? {
            verifier.verify(&job)?;
        }
        let s = self.test_session_v2_record(&id)?;
        let old = s
            .execution_pending_ingresses
            .get(proof.ingress_attempt_id)
            .ok_or_else(invalid)?;
        original.charged_at = old.charged_at;
        if old != &original || now.get() < old.charged_at.get() {
            return Err(invalid());
        }
        if let Some(p) = &s.execution_pending_start {
            return if p.ingress_attempt_id == proof.ingress_attempt_id {
                Ok(None)
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if !hex64(source_nonce_salt)
            || s.challenges
                .get(&original.challenge_request_id)
                .ok_or_else(invalid)?
                .consumed_by
                .is_some()
        {
            return Err(invalid());
        }
        let until = UnixMillis::new(
            now.get()
                .checked_add(5000)
                .ok_or_else(invalid)?
                .min(original.signed_until.get())
                .min(
                    s.challenges
                        .get(&original.challenge_request_id)
                        .ok_or_else(invalid)?
                        .expires_at
                        .get(),
                )
                .min(s.lease_until.ok_or_else(invalid)?.get())
                .min(
                    s.roots
                        .get(&SessionPurposeV1::Execution)
                        .ok_or_else(invalid)?
                        .deadline
                        .get(),
                ),
        );
        if now.get() >= until.get() {
            return Err(invalid());
        }
        let mut ports = Vec::new();
        for leaf in inventory::fixed_inventory()?.into_iter().skip(5) {
            let physical_id = port_id(s, proof.ingress_attempt_id, &original, &leaf)?;
            self.test_session_v2_transport_id_available(&id, &physical_id)?;
            ports.push(ExecutionPendingPhysicalPortV2 {
                leaf,
                physical_id,
                claimed_ordinal: None,
                claimed_at: None,
            });
        }
        if ports.len() != 134 {
            return Err(invalid());
        }
        let source_operation_id = format!(
            "execution-start-source-{}",
            digest(&(
                "oasis7.custody.execution-pending-source-id.v2",
                &original.approved_input_digest,
                &id,
                proof.ingress_attempt_id,
                &original.signed_attempt_digest,
                &original.request,
                source_nonce_salt
            ))?
        );
        let source_verify_nonce = digest(&(
            "oasis7.custody.execution-pending-source-nonce.v2",
            &original.approved_input_digest,
            &id,
            &source_operation_id,
            proof.ingress_attempt_id,
            &original.signed_attempt_digest,
            original.request.digest()?,
            source_nonce_salt,
        ))?;
        self.test_session_v2_transport_id_available(&id, &source_operation_id)?;
        self.test_session_v2_transport_id_available(&id, &source_verify_nonce)?;
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(&id)?;
        saved
            .roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(134)
            .map_err(|_| invalid())?;
        let ordinal = next_session_ordinal_v2(saved)?;
        let pending = ExecutionPendingStartV2 {
            ingress_attempt_id: proof.ingress_attempt_id.into(),
            source_nonce_salt: source_nonce_salt.into(),
            source_operation_id,
            source_verify_nonce,
            accepted_at: now,
            absolute_until: until,
            accepted_lease_until: s.lease_until.ok_or_else(invalid)?,
            accepted_ordinal: ordinal,
            physical_ports: ports,
            directory_signature: None,
            fence_dispatch: None,
            fence_read: None,
        };
        let master_digest = pending.master_digest(&original)?;
        let source_id = pending.source_operation_id.clone();
        saved.sources.insert(
            source_id.clone(),
            TestSessionSourceV2 {
                api_version: 2,
                approved_input_digest: original.approved_input_digest.clone(),
                source: ProvisionOriginInspectionV1 {
                    operation_id: source_id.clone(),
                    verify_nonce: pending.source_verify_nonce.clone(),
                    created_at: DecimalU64(now.get()),
                    expires_at: DecimalU64(until.get()),
                    claimed_at: None,
                    request_bytes: None,
                    verification: None,
                    verified_at: None,
                },
            },
        );
        saved.source_funding.insert(
            source_id.clone(),
            SessionSourceFundingV1::ExecutionPendingStartV2 { master_digest },
        );
        saved.source_oracle_reads.insert(source_id.clone(), 0);
        saved.execution_pending_start = Some(pending);
        // Pending binds/reserves this original challenge through the sole
        // record. Its actual consumption belongs to the later real Book birth.
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionPendingReserved {
                request_id: original.request.request_id.clone(),
            },
            now,
        )?;
        append_session_event_v2(
            saved,
            SessionEventKindV2::SourceReserved {
                source_operation_id: source_id,
            },
            now,
        )?;
        if saved
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && saved.closing.is_none()
        {
            saved.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(Some(AcceptedExecutionPendingStartV2 {
            session_id: id,
            ingress_attempt_id: proof.ingress_attempt_id.into(),
            accepted_ordinal: ordinal,
        }))
    }
}

#[path = "test_session_execution_pending_restore_v2.rs"]
mod restore;
pub(in crate::provisioning) use restore::validate_pending_start_v2;
impl ExecutionPendingStartV2 {
    pub(in crate::provisioning) fn reserved_units(&self) -> u64 {
        134
    }
}

pub(in crate::provisioning) fn pending_transport_id_in_use_v2(
    s: &TestSessionAuthorityV2,
    key: &str,
) -> bool {
    s.execution_pending_ingresses
        .iter()
        .any(|(id, v)| id == key || v.ingress_ports.iter().any(|p| p == key))
        || s.execution_pending_start.as_ref().is_some_and(|p| {
            p.source_operation_id == key
                || p.source_verify_nonce == key
                || p.physical_ports.iter().any(|v| v.physical_id == key)
        })
}
pub(in crate::provisioning) fn pending_owns_fee_port_v2(
    s: &TestSessionAuthorityV2,
    key: &str,
) -> bool {
    s.execution_pending_ingresses
        .values()
        .any(|v| v.ingress_ports.iter().any(|p| p == key))
}

impl ExecutionPendingStartV2 {
    fn master_digest(&self, original: &ExecutionPendingIngressV2) -> Result<[u8; 32]> {
        let ports = self
            .physical_ports
            .iter()
            .map(|p| (&p.leaf, &p.physical_id))
            .collect::<Vec<_>>();
        let commitment = digest(&(
            "oasis7.custody.execution-pending-master.v2",
            &self.ingress_attempt_id,
            &original.request,
            &original.approved_input_digest,
            &original.signed_attempt_digest,
            &self.source_nonce_salt,
            &self.source_operation_id,
            &self.source_verify_nonce,
            self.accepted_at,
            self.absolute_until,
            self.accepted_lease_until,
            self.accepted_ordinal,
            ports,
        ))?;
        roots::hex_bytes(&commitment)?
            .try_into()
            .map_err(|_| invalid())
    }
}
pub(in crate::provisioning) fn pending_funds_source_v2(
    s: &TestSessionAuthorityV2,
    key: &str,
) -> bool {
    s.execution_pending_start.as_ref().is_some_and(|p| {
        p.source_operation_id == key
            && s.execution_pending_ingresses
                .get(&p.ingress_attempt_id)
                .is_some_and(|original| {
                    p.master_digest(original).is_ok_and(|master_digest| {
                        s.source_funding.get(key)
                            == Some(&SessionSourceFundingV1::ExecutionPendingStartV2 {
                                master_digest,
                            })
                    })
                })
    })
}

/// The first Directory Actor leaf, not an arbitrary network credential.
/// Storage commits the claim before the Worker can receive this handle.
pub struct PendingDirectoryAttemptV2 {
    accepted: AcceptedExecutionPendingStartV2,
    physical_id: String,
    claim_ordinal: u64,
    claimed_at: UnixMillis,
    until: UnixMillis,
    input_digest: String,
    scope: crate::role_key_slot::RoleKeySlotScopeV1,
}
impl PendingDirectoryAttemptV2 {
    pub fn session_id(&self) -> &str {
        &self.accepted.session_id
    }
    pub fn approved_input_digest(&self) -> &str {
        &self.input_digest
    }
    pub fn physical_id(&self) -> &str {
        &self.physical_id
    }
    pub fn reservation_ordinal(&self) -> u64 {
        self.accepted.accepted_ordinal
    }
    pub fn claim_ordinal(&self) -> u64 {
        self.claim_ordinal
    }
    pub fn scope(&self) -> &crate::role_key_slot::RoleKeySlotScopeV1 {
        &self.scope
    }
    pub fn until(&self) -> UnixMillis {
        self.until
    }
}
impl AccountProvisionApprovalState {
    fn pending_execution_local(
        &self,
        token: &AcceptedExecutionPendingStartV2,
        now: UnixMillis,
    ) -> Result<&TestSessionAuthorityV2> {
        let s = self.test_session_v2_record(&token.session_id)?;
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        let original = s
            .execution_pending_ingresses
            .get(&p.ingress_attempt_id)
            .ok_or_else(invalid)?;
        let root = s
            .roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?;
        if token.ingress_attempt_id != p.ingress_attempt_id
            || token.accepted_ordinal != p.accepted_ordinal
            || self.session_inventory != SessionInventoryV1::Known
            || self.frozen()
            || s.approved_at.is_none()
            || !s
                .native_machine_registration
                .as_ref()
                .is_some_and(|m| m.ready())
            || root.revoked
            || now.get() < p.accepted_at.get()
            || now.get() >= p.absolute_until.get()
            || now.get() >= root.deadline.get()
            || s.lease_until.is_none_or(|lease| now.get() >= lease.get())
            || s.input.request.context.security_version != self.provision.security_epoch()
            || root
                .credential
                .as_ref()
                .is_none_or(|c| c.generation != original.credential_generation)
            || super::v2_credential_http::control_exhausted(s)
            || s.roots
                .get(&SessionPurposeV1::ReconcileRead)
                .is_some_and(|r| r.budgets.transport.remaining() == Ok(0))
        {
            return Err(invalid());
        }
        if let Some((reason, at)) = s.closing {
            if reason != TestSessionClosingReasonV1::BudgetExhausted
                || at != p.accepted_at
                || root.budgets.transport.remaining().map_err(|_| invalid())? != 0
            {
                return Err(invalid());
            }
        } else if root.budgets.transport.remaining().map_err(|_| invalid())? == 0 {
            return Err(invalid());
        }
        if session_event_ordinal_v2(
            s,
            &SessionEventKindV2::ExecutionPendingReserved {
                request_id: original.request.request_id.clone(),
            },
        )? != Some(p.accepted_ordinal)
        {
            return Err(invalid());
        }
        for event in s
            .events
            .iter()
            .skip(usize::try_from(p.accepted_ordinal).map_err(|_| invalid())?)
        {
            match event.event() {
                SessionEventKindV2::Expired { .. } => return Err(invalid()),
                SessionEventKindV2::ControlAcknowledged { request_id } => {
                    if matches!(s.control_records.get(request_id),Some(SessionControlRecordV2::Control{request,..}) if matches!(request.kind,SessionControlKindV1::Finish|SessionControlKindV1::Abort))
                    {
                        return Err(invalid());
                    }
                }
                _ => (),
            }
        }
        Ok(s)
    }
    /// Consumes the first-only SQL2 handle; no raw session/request API can mint
    /// this first attempt from metadata after history/restart.
    pub fn claim_test_session_v2_pending_first_directory(
        &mut self,
        accepted: AcceptedExecutionPendingStartV2,
        native_scope: &NativeStageScope<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PendingDirectoryAttemptV2> {
        for job in self.test_session_v2_native_restore_checks(&accepted.session_id, native_scope)? {
            verifier.verify(&job)?;
        }
        let s = self.pending_execution_local(&accepted, now)?;
        let leaf = ExecutionPendingLeafV2::Business(inventory::StartBusinessLeafV2::Current {
            phase: inventory::StartCurrentPhaseV2::PreInspect,
            port: inventory::StartCurrentPortV2::ManagerAssociationsActor,
        });
        let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
        if p.physical_ports.iter().any(|p| p.claimed_ordinal.is_some()) {
            return Err(ProvisionApprovalError::Conflict);
        }
        let port = p
            .physical_ports
            .iter()
            .find(|p| p.leaf == leaf)
            .ok_or_else(invalid)?;
        let physical_id = port.physical_id.clone();
        let until = p.absolute_until;
        let scope = s.session.scope.role_scope.clone();
        let original = s
            .execution_pending_ingresses
            .get(&p.ingress_attempt_id)
            .ok_or_else(invalid)?;
        let request_id = original.request.request_id.clone();
        let input_digest = original.approved_input_digest.clone();
        let mut next = self.clone();
        let saved = next.test_session_v2_record_mut(&accepted.session_id)?;
        let ordinal = next_session_ordinal_v2(saved)?;
        let port = saved
            .execution_pending_start
            .as_mut()
            .ok_or_else(invalid)?
            .physical_ports
            .iter_mut()
            .find(|p| p.leaf == leaf)
            .ok_or_else(invalid)?;
        port.claimed_ordinal = Some(ordinal);
        port.claimed_at = Some(now);
        append_session_event_v2(
            saved,
            SessionEventKindV2::ExecutionPendingLeafClaimed { request_id, leaf },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(PendingDirectoryAttemptV2 {
            accepted,
            physical_id,
            claim_ordinal: ordinal,
            claimed_at: now,
            until,
            input_digest,
            scope,
        })
    }
}

impl ExecutionPendingStartV2 {
    pub(super) fn claimed_leaf_count(&self) -> usize {
        self.physical_ports
            .iter()
            .filter(|p| p.claimed_ordinal.is_some())
            .count()
    }
    pub(super) fn claimed_leaf_ordinal(&self, leaf: ExecutionPendingLeafV2) -> Option<u64> {
        self.physical_ports
            .iter()
            .find(|p| p.leaf == leaf)
            .and_then(|p| p.claimed_ordinal)
    }
}

impl ExecutionPendingStartV2 {
    pub(super) fn claimed_leaf_at(&self, leaf: ExecutionPendingLeafV2) -> Option<UnixMillis> {
        self.physical_ports
            .iter()
            .find(|p| p.leaf == leaf)
            .and_then(|p| p.claimed_at)
    }
}

#[path = "test_session_execution_pending_directory_signature_v2.rs"]
mod directory_signature;
use directory_signature::PendingDirectorySignatureV2;
pub use directory_signature::{
    ExecutionPendingDirectoryDispatchV2, ExecutionPendingDirectorySigningAuthorityV2,
};

impl ExecutionPendingStartV2 {
    pub(super) fn has_directory_signature(&self) -> bool {
        self.directory_signature.is_some()
    }
}

impl ExecutionPendingStartV2 {
    pub(super) fn directory_signature_recorded_at(&self) -> Option<UnixMillis> {
        self.directory_signature.as_ref().map(|r| r.recorded_at)
    }
}

#[path = "test_session_execution_pending_fence_v2.rs"]
mod fence;
pub use fence::{
    PendingFenceDispatchAuthorityV2, PendingFenceDispatchV2, PendingFenceReadAuthorityV2,
};
impl ExecutionPendingStartV2 {
    pub(super) fn has_fence_dispatch(&self) -> bool {
        self.fence_dispatch.is_some()
    }
    pub(super) fn has_fence_read(&self) -> bool {
        self.fence_read.is_some()
    }
    pub(super) fn fence_dispatch_recorded_at(&self) -> Option<UnixMillis> {
        self.fence_dispatch.as_ref().map(|r| r.recorded_at)
    }
    pub(super) fn fence_read_recorded_at(&self) -> Option<UnixMillis> {
        self.fence_read.as_ref().map(|r| r.recorded_at)
    }
    pub(super) fn expected_first_rf_leaf(&self, index: usize) -> Option<ExecutionPendingLeafV2> {
        fence::first_rf_sequence().get(index).copied()
    }
}
