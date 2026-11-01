//! Paid callback inventory and one-leaf Account transaction claims.
use super::*;
use crate::execution_dispatch_gate_v2::*;
use crate::native_cleanup_v2::AccountMachinePurposeV2;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionNativeReentryTransportV2 {
    pub native_proof_digest: String,
    pub business_edge: ExecutionPreparePhysicalEdgeV2,
    pub hop: ExecutionReentryHopV2,
    pub reserved_at: UnixMillis,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionNativeGateRecordV2 {
    pub reserved_at: UnixMillis,
    pub accepted_ordinal: u64,
    pub callback_ports: Vec<ExecutionNativeGatePortV2>,
    pub native_callback_nonces: [String; 2],
    pub identity_origin: String,
    pub claims: Vec<ExecutionNativeGateClaimRecordV2>,
}
pub type ExecutionNativeGatePortV2 = ExecutionReentryInventoryPortV2;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionNativeGateClaimRecordV2 {
    pub claim: ExecutionDispatchClaimV2,
    #[serde(deserialize_with = "required_option")]
    pub signature_hex: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub signature_recorded_at: Option<UnixMillis>,
    pub issuer_pin: AccountMachineIssuerPinV2,
    pub issuer_floor: AccountMachineIssuerFloorV2,
    #[serde(deserialize_with = "required_option")]
    pub native_query: Option<SignedNativeExecutionReentryQueryV2>,
}
impl core::fmt::Debug for ExecutionNativeGateClaimRecordV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionNativeGateClaimRecordV2(<fixed leaf>)")
    }
}
fn callbacks(request: &NativeExecutionPrepareRequestV2) -> Result<Vec<ExecutionNativeGatePortV2>> {
    execution_reentry_inventory(request).map_err(|_| invalid())
}
/// The complete Native10+24 reservation is atomic. Old ten-leaf history is
/// preserved, but cannot become this handle or a gated receiver authority.
pub struct ExecutionNativeGatedDispatchV2 {
    pub(super) request: NativeExecutionPrepareRequestV2,
}
impl ExecutionNativeGatedDispatchV2 {
    pub fn request(&self) -> &NativeExecutionPrepareRequestV2 {
        &self.request
    }
}
impl AccountProvisionApprovalState {
    pub fn record_test_session_v2_native_prepare_gated(
        &mut self,
        id: &str,
        authority: ExecutionNativePrepareAuthorityV2,
        signed: SignedAccountNativeExecutionPrepareV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<ExecutionNativeGatedDispatchV2> {
        // No externally visible intermediate ten-fee commit on N-1 failure.
        let mut next = self.clone();
        let dispatch =
            next.record_test_session_v2_native_prepare(id, authority, signed, pin, floor, now)?;
        let ports = callbacks(dispatch.request())?;
        for p in &ports {
            next.test_session_v2_transport_id_available(id, &p.port.attempt_id)?;
        }
        let native_nonces = [
            native_reentry_nonce(
                dispatch.request(),
                ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent,
            )
            .map_err(|_| invalid())?,
            native_reentry_nonce(
                dispatch.request(),
                ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
            )
            .map_err(|_| invalid())?,
        ];
        let mut allocated = std::collections::BTreeSet::new();
        for id in ports
            .iter()
            .map(|p| p.port.attempt_id.as_str())
            .chain(native_nonces.iter().map(String::as_str))
        {
            if !allocated.insert(id) {
                return Err(invalid());
            }
        }
        for nonce in &native_nonces {
            next.test_session_v2_transport_id_available(id, nonce)?;
        }
        let proof = dispatch.request().proof.digest().map_err(|_| invalid())?;
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(24)
            .map_err(|_| invalid())?;
        let ordinal = super::super::super::test_session::next_session_ordinal_v2(s)?;
        let request_id = s
            .execution_book
            .as_ref()
            .ok_or_else(invalid)?
            .action_request_id
            .clone();
        for p in &ports {
            s.purpose_transports.insert(
                p.port.attempt_id.clone(),
                SessionPurposeTransportV1::ExecutionNativeReentryV2(Box::new(
                    ExecutionNativeReentryTransportV2 {
                        native_proof_digest: proof.clone(),
                        business_edge: p.edge,
                        hop: p.port.hop,
                        reserved_at: now,
                    },
                )),
            );
        }
        s.execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .stages
            .get_mut(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?
            .native_prepare
            .as_mut()
            .ok_or_else(invalid)?
            .dispatch_gate = Some(ExecutionNativeGateRecordV2 {
            reserved_at: now,
            accepted_ordinal: ordinal,
            callback_ports: ports,
            native_callback_nonces: native_nonces,
            identity_origin: s.enrollment.registration.public_origin.clone(),
            claims: Vec::new(),
        });
        super::super::super::test_session::append_session_event_v2(
            s,
            super::super::super::test_session::SessionEventKindV2::ExecutionNativeGateReserved {
                request_id,
            },
            now,
        )?;
        if s.roots
            .get(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .remaining()
            .map_err(|_| invalid())?
            == 0
            && s.closing.is_none()
        {
            s.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        next.validate_persistence()?;
        *self = next;
        Ok(ExecutionNativeGatedDispatchV2 {
            request: dispatch.into_request(),
        })
    }
}

impl AccountProvisionApprovalState {
    pub(super) fn validate_native_gate_v2(
        &self,
        s: &TestSessionAuthorityV2,
        b: &SessionExecutionBookV2,
        request: &NativeExecutionPrepareRequestV2,
    ) -> Result<()> {
        let count = s
            .purpose_transports
            .values()
            .filter(|v| matches!(v, SessionPurposeTransportV1::ExecutionNativeReentryV2(_)))
            .count();
        let record = b.native_prepare_record().ok_or_else(invalid)?;
        let Some(gate) = &record.dispatch_gate else {
            return if count == 0 { Ok(()) } else { Err(invalid()) };
        };
        let expected = callbacks(request)?;
        if gate.identity_origin != s.enrollment.registration.public_origin
            || gate.native_callback_nonces
                != [
                    native_reentry_nonce(
                        request,
                        ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent,
                    )
                    .map_err(|_| invalid())?,
                    native_reentry_nonce(
                        request,
                        ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
                    )
                    .map_err(|_| invalid())?,
                ]
            || count != 24
            || gate.callback_ports != expected
            || gate.reserved_at != record.claimed_at
            || gate.accepted_ordinal == 0
            || gate.claims.len() > 10
        {
            return Err(invalid());
        }
        let proof = request.proof.digest().map_err(|_| invalid())?;
        for p in &expected {
            let marker = SessionPurposeTransportV1::ExecutionNativeReentryV2(Box::new(
                ExecutionNativeReentryTransportV2 {
                    native_proof_digest: proof.clone(),
                    business_edge: p.edge,
                    hop: p.port.hop,
                    reserved_at: gate.reserved_at,
                },
            ));
            if s.purpose_transports.get(&p.port.attempt_id) != Some(&marker) {
                return Err(invalid());
            }
        }
        let request_id = b.action_request_id.clone();
        if super::super::super::test_session::session_event_ordinal_v2(
            s,
            &super::super::super::test_session::SessionEventKindV2::ExecutionNativeGateReserved {
                request_id: request_id.clone(),
            },
        )? != Some(gate.accepted_ordinal)
        {
            return Err(invalid());
        }
        let mut allocated = std::collections::BTreeSet::new();
        for id in gate
            .callback_ports
            .iter()
            .map(|p| p.port.attempt_id.as_str())
            .chain(gate.native_callback_nonces.iter().map(String::as_str))
        {
            if !allocated.insert(id) {
                return Err(invalid());
            }
        }
        let original_ordinal = super::super::super::test_session::session_event_ordinal_v2(
            s,
            &super::super::super::test_session::SessionEventKindV2::ExecutionNativePrepareClaimed {
                request_id: request_id.clone(),
            },
        )?
        .ok_or_else(invalid)?;
        if original_ordinal.checked_add(1) != Some(gate.accepted_ordinal) {
            return Err(invalid());
        }
        let mut previous = gate.accepted_ordinal;
        for (i, stored) in gate.claims.iter().enumerate() {
            let c = &stored.claim;
            c.validate_relative(request).map_err(|_| invalid())?;
            let ports: Vec<_> = expected
                .iter()
                .filter(|p| p.edge == c.leaf.edge)
                .map(|p| p.port.clone())
                .collect();
            if c.reservation_ordinal.0 != gate.accepted_ordinal
                || c.leaf != request.proof.certificate.ports[i]
                || c.callbacks != ports
                || c.claim_ordinal.0 <= previous
            {
                return Err(invalid());
            }
            if super::super::super::test_session::session_event_ordinal_v2(s,&super::super::super::test_session::SessionEventKindV2::ExecutionNativeDispatchClaimed{request_id:request_id.clone(),edge:c.leaf.edge})?!=Some(c.claim_ordinal.0)
                || stored.signature_hex.is_some()!=stored.signature_recorded_at.is_some(){return Err(invalid());}
            for event in s
                .events
                .iter()
                .skip(gate.accepted_ordinal as usize)
                .take((c.claim_ordinal.0 - gate.accepted_ordinal - 1) as usize)
            {
                if matches!(
                    event.event(),
                    super::super::super::test_session::SessionEventKindV2::Expired { .. }
                ) || matches!(event.event(),super::super::super::test_session::SessionEventKindV2::ControlAcknowledged{request_id} if matches!(s.control_records.get(request_id),Some(super::super::super::test_session::SessionControlRecordV2::Control{request,..}) if matches!(request.kind,SessionControlKindV1::Finish|SessionControlKindV1::Abort)))
                {
                    return Err(invalid());
                }
            }
            if let Some(at) = stored.signature_recorded_at {
                if at.get() < c.issued_at.0 || at.get() >= c.expires_at.0 {
                    return Err(invalid());
                }
                SignedExecutionDispatchClaimV2 {
                    claim: c.clone(),
                    signature_hex: stored.signature_hex.clone().ok_or_else(invalid)?,
                }
                .verify_issuer(request, &stored.issuer_pin, &stored.issuer_floor, at.get())
                .map_err(|_| invalid())?;
            }
            match (&stored.native_query, c.leaf.edge) {
                (
                    Some(q),
                    ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent
                    | ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
                ) => {
                    q.query.validate_relative(request).map_err(|_| invalid())?;
                    if q.query.issued_at.0 > c.issued_at.0
                        || c.issued_at.0 >= q.query.expires_at.0
                        || c.expires_at.0 > q.query.expires_at.0
                        || q.query.leaf != c.leaf
                        || q.query.body_digest != c.body_digest
                        || q.query
                            .callback_attempt_ids
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            != c.callbacks
                                .iter()
                                .map(|p| p.attempt_id.as_str())
                                .collect::<Vec<_>>()
                    {
                        return Err(invalid());
                    }
                }
                (
                    None,
                    ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent
                    | ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
                )
                | (Some(_), _) => return Err(invalid()),
                (None, _) => (),
            }
            previous = c.claim_ordinal.0;
        }
        Ok(())
    }
}
impl SessionExecutionBookV2 {
    pub(in crate::provisioning) fn owns_native_gate_transport(
        &self,
        s: &TestSessionAuthorityV2,
        key: &str,
    ) -> bool {
        let Some(gate) = self
            .native_prepare_record()
            .and_then(|r| r.dispatch_gate.as_ref())
        else {
            return false;
        };
        let Some(SessionPurposeTransportV1::ExecutionNativeReentryV2(marker)) =
            s.purpose_transports.get(key)
        else {
            return false;
        };
        let Some(record) = self.native_prepare_record() else {
            return false;
        };
        record.proof.digest().ok().as_ref() == Some(&marker.native_proof_digest)
            && marker.reserved_at == gate.reserved_at
            && gate.callback_ports.iter().any(|p| {
                p.port.attempt_id == key
                    && p.edge == marker.business_edge
                    && p.port.hop == marker.hop
            })
    }
}

/// Fixed deployment inputs, not a serialized authority or a caller-selected key.
pub struct ExecutionDispatchIssueContextV2<'a> {
    pub native_scope: &'a NativeStageScope<'a>,
    pub native_prepare_pin: &'a AccountMachineIssuerPinV2,
    pub native_prepare_floor: &'a AccountMachineIssuerFloorV2,
    pub dispatch_pin: &'a AccountMachineIssuerPinV2,
    pub dispatch_floor: &'a AccountMachineIssuerFloorV2,
}
/// One SQL-committed physical claim. No Clone/Serde/Debug or public constructor.
pub struct ExecutionNativeLeafSigningAuthorityV2 {
    request: NativeExecutionPrepareRequestV2,
    claim: ExecutionDispatchClaimV2,
    before_digest: String,
}
impl ExecutionNativeLeafSigningAuthorityV2 {
    pub fn claim(&self) -> &ExecutionDispatchClaimV2 {
        &self.claim
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.claim
            .signing_bytes(&self.request)
            .map_err(|_| invalid())
    }
}
impl AccountProvisionApprovalState {
    pub fn claim_test_session_v2_native_first_leaf(
        &mut self,
        id: &str,
        dispatch: ExecutionNativeGatedDispatchV2,
        configured: &ExecutionDispatchIssueContextV2<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionNativeLeafSigningAuthorityV2> {
        let s = self.test_session_v2_record(id)?;
        let record = s
            .execution_book
            .as_ref()
            .and_then(|b| b.native_prepare_record())
            .ok_or_else(invalid)?;
        let gate = record.dispatch_gate.as_ref().ok_or_else(invalid)?;
        if !gate.claims.is_empty()
            || record.proof != dispatch.request.proof
            || self.test_session_v2_prepare_original_history(id)?.as_ref()
                != Some(dispatch.request.original.as_ref())
        {
            return Err(invalid());
        }
        let body =
            crate::approval::bytes_digest(&dispatch.request.wire_bytes().map_err(|_| invalid())?);
        self.claim_test_session_v2_native_leaf_inner(
            id,
            (
                ExecutionPreparePhysicalEdgeV2::AccountToIdentityService,
                &body,
            ),
            configured,
            None,
            now,
            verifier,
        )
    }
    pub fn claim_test_session_v2_native_identity_leaf(
        &mut self,
        id: &str,
        leaf_body: (ExecutionPreparePhysicalEdgeV2, &str),
        configured: &ExecutionDispatchIssueContextV2<'_>,
        actual_role: crate::http::Role,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionNativeLeafSigningAuthorityV2> {
        let (edge, body_digest) = leaf_body;
        if actual_role != crate::http::Role::Identity
            || matches!(
                edge,
                ExecutionPreparePhysicalEdgeV2::AccountToIdentityService
                    | ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent
                    | ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare
            )
        {
            return Err(invalid());
        }
        self.claim_test_session_v2_native_leaf_inner(
            id,
            (edge, body_digest),
            configured,
            None,
            now,
            verifier,
        )
    }
    pub fn claim_test_session_v2_native_world_leaf(
        &mut self,
        id: &str,
        query: SignedNativeExecutionReentryQueryV2,
        configured: &ExecutionDispatchIssueContextV2<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionNativeLeafSigningAuthorityV2> {
        let native = configured.native_scope;
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let r = b.native_prepare_record().ok_or_else(invalid)?;
        let request = NativeExecutionPrepareRequestV2 {
            proof: r.proof.clone(),
            original: Box::new(
                self.test_session_v2_prepare_original_history(id)?
                    .ok_or_else(invalid)?,
            ),
        };
        if r.proof.certificate.issuer.native_audience_origin != native.http_origin {
            return Err(invalid());
        }
        query
            .verify_native(
                &request,
                native.native_root_key_id,
                native.native_root_public_key,
                &s.enrollment.registration.public_origin,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let gate = r.dispatch_gate.as_ref().ok_or_else(invalid)?;
        let ids: Vec<_> = gate
            .callback_ports
            .iter()
            .filter(|p| p.edge == query.query.leaf.edge)
            .map(|p| p.port.attempt_id.as_str())
            .collect();
        if ids
            != query
                .query
                .callback_attempt_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        {
            return Err(invalid());
        }
        let edge = query.query.leaf.edge;
        let body = query.query.body_digest.clone();
        self.claim_test_session_v2_native_leaf_inner(
            id,
            (edge, &body),
            configured,
            Some(query),
            now,
            verifier,
        )
    }
    fn claim_test_session_v2_native_leaf_inner(
        &mut self,
        id: &str,
        leaf_body: (ExecutionPreparePhysicalEdgeV2, &str),
        configured: &ExecutionDispatchIssueContextV2<'_>,
        native_query: Option<SignedNativeExecutionReentryQueryV2>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionNativeLeafSigningAuthorityV2> {
        let (edge, body_digest) = leaf_body;
        let issuer = configured.dispatch_pin;
        let floor = configured.dispatch_floor;
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let r = b.native_prepare_record().ok_or_else(invalid)?;
        let gate = r.dispatch_gate.as_ref().ok_or_else(invalid)?;
        self.test_session_v2_prepaid_native_gate_local(id, gate, now)?;
        for jobs in self.test_session_v2_native_restore_checks(id, configured.native_scope)? {
            verifier.verify(&jobs)?;
        }
        r.proof
            .verify_issuer(
                configured.native_prepare_pin,
                configured.native_prepare_floor,
                now.get(),
            )
            .map_err(|_| invalid())?;

        let request = NativeExecutionPrepareRequestV2 {
            proof: r.proof.clone(),
            original: Box::new(
                self.test_session_v2_prepare_original_history(id)?
                    .ok_or_else(invalid)?,
            ),
        };
        let c = &r.proof.certificate;
        let expected = c.ports.get(gate.claims.len()).ok_or_else(invalid)?;
        if expected.edge != edge
            || !crate::native_cleanup_v2::hex64(body_digest)
            || gate
                .claims
                .last()
                .is_some_and(|p| p.signature_hex.is_none())
            || now.get() < c.issued_at.0
            || now.get() >= c.expires_at.0
            || s.roots
                .get(&SessionPurposeV1::Execution)
                .and_then(|r| r.credential.as_ref())
                .is_none_or(|credential| credential.generation != c.credential_generation)
        {
            return Err(invalid());
        }
        let ordinal = super::super::super::test_session::next_session_ordinal_v2(s)?;
        let expires = c.expires_at.0.min(issuer.not_after.0).min(
            native_query
                .as_ref()
                .map_or(u64::MAX, |q| q.query.expires_at.0),
        );
        let claim = ExecutionDispatchClaimV2 {
            api_version: 2,
            issuer: AccountMachineIssuerV2 {
                namespace: issuer.namespace.clone(),
                service_id: issuer.service_id.clone(),
                environment: issuer.environment.clone(),
                key_id: issuer.key_id.clone(),
                issuer_epoch: issuer.issuer_epoch,
                config_generation: issuer.config_generation,
                native_audience_origin: issuer.native_audience_origin.clone(),
            },
            native_proof_digest: r.proof.digest().map_err(|_| invalid())?,
            reservation_ordinal: DecimalU64(gate.accepted_ordinal),
            claim_ordinal: DecimalU64(ordinal),
            session_id: id.into(),
            approved_input_digest: c.approved_input_digest.clone(),
            source_record_digest: c.source_record_digest.clone(),
            leaf: expected.clone(),
            body_digest: body_digest.into(),
            callbacks: gate
                .callback_ports
                .iter()
                .filter(|p| p.edge == edge)
                .map(|p| p.port.clone())
                .collect(),
            issued_at: DecimalU64(now.get()),
            expires_at: DecimalU64(expires),
        };
        claim.validate_relative(&request).map_err(|_| invalid())?;
        claim
            .issuer
            .verify_pin(
                issuer,
                floor,
                AccountMachinePurposeV2::ExecutionDispatchClaimV2,
                now.get(),
                claim.expires_at.0,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let request_id = b.action_request_id.clone();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .stages
            .get_mut(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?
            .native_prepare
            .as_mut()
            .ok_or_else(invalid)?
            .dispatch_gate
            .as_mut()
            .ok_or_else(invalid)?
            .claims
            .push(ExecutionNativeGateClaimRecordV2 {
                claim: claim.clone(),
                signature_hex: None,
                signature_recorded_at: None,
                issuer_pin: issuer.clone(),
                issuer_floor: floor.clone(),
                native_query,
            });
        super::super::super::test_session::append_session_event_v2(
            s,
            super::super::super::test_session::SessionEventKindV2::ExecutionNativeDispatchClaimed {
                request_id,
                edge,
            },
            now,
        )?;
        next.validate_persistence()?;
        let stored = next
            .test_session_v2_record(id)?
            .execution_book
            .as_ref()
            .and_then(|b| b.native_prepare_record())
            .and_then(|r| r.dispatch_gate.as_ref())
            .and_then(|g| g.claims.last())
            .ok_or_else(invalid)?;
        let before_digest = digest(&(request.proof.digest().map_err(|_| invalid())?, stored))?;
        *self = next;
        Ok(ExecutionNativeLeafSigningAuthorityV2 {
            request,
            claim,
            before_digest,
        })
    }
}

impl AccountProvisionApprovalState {
    pub fn record_test_session_v2_native_leaf_signature(
        &mut self,
        id: &str,
        authority: ExecutionNativeLeafSigningAuthorityV2,
        signature_hex: String,
        now: UnixMillis,
    ) -> Result<SignedExecutionDispatchClaimV2> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let r = b.native_prepare_record().ok_or_else(invalid)?;
        let gate = r.dispatch_gate.as_ref().ok_or_else(invalid)?;
        let stored = gate
            .claims
            .iter()
            .find(|c| c.claim.claim_ordinal == authority.claim.claim_ordinal)
            .ok_or_else(invalid)?;
        if stored.claim != authority.claim
            || stored.signature_hex.is_some()
            || authority.before_digest
                != digest(&(r.proof.digest().map_err(|_| invalid())?, stored))?
            || now.get() < stored.claim.issued_at.0
            || now.get() >= stored.claim.expires_at.0
        {
            return Err(invalid());
        }
        let signed = SignedExecutionDispatchClaimV2 {
            claim: stored.claim.clone(),
            signature_hex,
        };
        signed
            .verify_issuer(
                &authority.request,
                &stored.issuer_pin,
                &stored.issuer_floor,
                now.get(),
            )
            .map_err(|_| invalid())?;
        let request_id = b.action_request_id.clone();
        let edge = stored.claim.leaf.edge;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let claim = s
            .execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .stages
            .get_mut(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?
            .native_prepare
            .as_mut()
            .ok_or_else(invalid)?
            .dispatch_gate
            .as_mut()
            .ok_or_else(invalid)?
            .claims
            .iter_mut()
            .find(|c| c.claim.claim_ordinal == authority.claim.claim_ordinal)
            .ok_or_else(invalid)?;
        claim.signature_hex = Some(signed.signature_hex.clone());
        claim.signature_recorded_at = Some(now);
        super::super::super::test_session::append_session_event_v2(s,super::super::super::test_session::SessionEventKindV2::ExecutionNativeDispatchReceiptRecorded{request_id,edge},now)?;
        next.validate_persistence()?;
        *self = next;
        Ok(signed)
    }
}

impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn test_session_v2_native_gate_native_history(
        &self,
        id: &str,
        native: &NativeStageScope<'_>,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let Some(b) = s.execution_book.as_ref() else {
            return Ok(());
        };
        let Some(r) = b.native_prepare_record() else {
            return Ok(());
        };
        let Some(gate) = &r.dispatch_gate else {
            return Ok(());
        };
        if r.proof.certificate.issuer.native_audience_origin != native.http_origin {
            return Err(invalid());
        }
        let request = NativeExecutionPrepareRequestV2 {
            proof: r.proof.clone(),
            original: Box::new(
                self.test_session_v2_prepare_original_history(id)?
                    .ok_or_else(invalid)?,
            ),
        };
        for claim in &gate.claims {
            if let Some(query) = &claim.native_query {
                query
                    .verify_native(
                        &request,
                        native.native_root_key_id,
                        native.native_root_public_key,
                        &s.enrollment.registration.public_origin,
                        claim.claim.issued_at.0,
                    )
                    .map_err(|_| invalid())?;
            }
        }
        Ok(())
    }
}

#[path = "test_session_execution_native_gate_caller_v2.rs"]
mod caller;
