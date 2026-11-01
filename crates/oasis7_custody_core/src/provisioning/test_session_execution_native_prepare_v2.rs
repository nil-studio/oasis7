//! Account-owned Native10 after the exact two-hop factory and immutable SDK original.
//! No caller Context, private body, ports, units or serialized Current is accepted.
use super::*;
use crate::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
};
use crate::native_execution_prepare_v2::*;
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionNativePrepareRecordV2 {
    pub proof: SignedAccountNativeExecutionPrepareV2,
    pub ledger_keys: [String; 10],
    pub claimed_at: UnixMillis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_original_cbor_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_recorded_at: Option<UnixMillis>,
}
impl core::fmt::Debug for ExecutionNativePrepareRecordV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionNativePrepareRecordV2(<public fixed original>)")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionNativePrepareTransportV2 {
    pub edge: ExecutionPreparePhysicalEdgeV2,
    pub proof_digest: String,
    pub claimed_at: UnixMillis,
}
/// Created from actual private Current + strongly verified archived public SDK
/// original in the Account transaction. Not reconstructible from persistence.
pub struct ExecutionNativePrepareAuthorityV2 {
    certificate: AccountNativeExecutionPrepareV2,
    original: ExecutionPrepareSignedOriginalV2,
    before_digest: String,
}
impl ExecutionNativePrepareAuthorityV2 {
    pub fn certificate(&self) -> &AccountNativeExecutionPrepareV2 {
        &self.certificate
    }
    pub fn original(&self) -> &ExecutionPrepareSignedOriginalV2 {
        &self.original
    }
}
/// First-commit-only fixed graph; history and reboot never return this handle.
pub struct ExecutionNativePrepareDispatchV2 {
    request: NativeExecutionPrepareRequestV2,
}
impl ExecutionNativePrepareDispatchV2 {
    pub fn request(&self) -> &NativeExecutionPrepareRequestV2 {
        &self.request
    }
    pub fn into_request(self) -> NativeExecutionPrepareRequestV2 {
        self.request
    }
}
fn edge_order() -> [ExecutionPreparePhysicalEdgeV2; 10] {
    [
        ExecutionPreparePhysicalEdgeV2::AccountToIdentityService,
        ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
        ExecutionPreparePhysicalEdgeV2::SidCipherReadBefore,
        ExecutionPreparePhysicalEdgeV2::SidCipherReadAfter,
        ExecutionPreparePhysicalEdgeV2::CsrfCipherReadBefore,
        ExecutionPreparePhysicalEdgeV2::CsrfCipherReadAfter,
        ExecutionPreparePhysicalEdgeV2::IdentityActorToNative,
        ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent,
        ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
        ExecutionPreparePhysicalEdgeV2::IdentityActorToContextBody,
    ]
}
fn ledger_keys(request: &NativeExecutionPrepareRequestV2) -> Result<[String; 10]> {
    let c = &request.proof.certificate;
    let proof = request.proof.digest().map_err(|_| invalid())?;
    let keys = c
        .ports
        .iter()
        .map(|p| {
            Ok(format!(
                "execution-native10-v2:{}",
                digest(&(
                    "oasis7.custody.execution-native10-ledger.v2",
                    &c.session_id,
                    &c.approved_input_digest,
                    &proof,
                    &p.attempt_id,
                    p.edge,
                ))?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    keys.try_into().map_err(|_| invalid())
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_native_prepare_checks(
        &self,
        id: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<Vec<ExactSignatureChecks>> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let service = b
            .stages
            .get(&MoveExecutionStageV2::Prepare)
            .and_then(|stage| stage.service_record.as_ref())
            .ok_or_else(invalid)?;
        let mut checks = self.test_session_v2_native_restore_checks(id, current.native_scope)?;
        checks.push(self.test_session_v2_prepare_dispatch_checks(
            id,
            &service.permits,
            service.accepted_ordinal,
            current,
            now,
        )?);
        Ok(checks)
    }
    pub fn prepare_test_session_v2_native_prepare(
        &self,
        id: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        issuer: AccountMachineIssuerV2,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ExecutionNativePrepareAuthorityV2> {
        let s = self.test_session_v2_local(id, SessionPurposeV1::Execution, now)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let stage = b
            .stages
            .get(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?;
        if stage.native_prepare.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        let service = stage.service_record.as_ref().ok_or_else(invalid)?;
        let SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory) = s
            .purpose_transports
            .get(&stage.allocation.factory_ingress_id)
            .ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        if factory.kind != ExecutionPrepareFactoryKindV2::OwnerServiceAndAccountV2
            || factory.factory_ports.is_none()
        {
            return Err(invalid());
        }
        factory.validate_factory_profile(s)?;
        verifier.verify(&self.test_session_v2_prepare_dispatch_checks(
            id,
            &service.permits,
            service.accepted_ordinal,
            current,
            now,
        )?)?;
        for jobs in self.test_session_v2_native_restore_checks(id, current.native_scope)? {
            verifier.verify(&jobs)?;
        }
        let original = self
            .test_session_v2_prepare_original_history(id)?
            .ok_or_else(invalid)?;
        let (registration_ref, active_digest) =
            self.test_session_v2_stored_native_ready_identity(id)?;
        let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
        let source_observed = source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at;
        let expires = service.permits.sign.permit.expires_at.get();
        if now.get() >= expires || issuer.environment != self.draft.entry.environment {
            return Err(invalid());
        }
        let accepted = super::super::test_session::next_session_ordinal_v2(s)?;
        let ports = edge_order()
            .into_iter()
            .map(|edge| {
                Ok(ExecutionPreparePhysicalPortV2 {
                    edge,
                    attempt_id: digest(&(
                        "oasis7.custody.execution-native10-attempt.v2",
                        id,
                        &b.approved_input_digest,
                        &b.action_request_id,
                        &stage.allocation.operation_id,
                        &stage.allocation.source_digest,
                        &source.source.verify_nonce,
                        accepted,
                        edge,
                    ))?,
                })
            })
            .collect::<Result<Vec<_>>>()?
            .try_into()
            .map_err(|_| invalid())?;
        let certificate = AccountNativeExecutionPrepareV2 {
            api_version: 2,
            issuer,
            session_id: id.into(),
            approved_input_digest: b.approved_input_digest.clone(),
            session_intent_digest: b.session_intent_digest.clone(),
            execution_deadline: DecimalU64(b.execution_deadline.get()),
            action_request_id: b.action_request_id.clone(),
            stage_operation_id: stage.allocation.operation_id.clone(),
            source_operation_id: b.source_operation_id.clone(),
            source_nonce: source.source.verify_nonce.clone(),
            source_record_digest: digest(source)?,
            source_observed_at: source_observed,
            source_expires_at: source.source.expires_at,
            credential_generation: b.credential_generation,
            accepted_ordinal: DecimalU64(accepted),
            factory_record_key: stage.allocation.factory_ingress_id.clone(),
            factory_ports_digest: digest(factory.factory_ports.as_ref().ok_or_else(invalid)?)?,
            private_current_proof_digest: factory
                .private_current
                .as_ref()
                .and_then(|r| r.proof.as_ref())
                .ok_or_else(invalid)?
                .digest()
                .map_err(|_| invalid())?,
            opaque_registration_ref: registration_ref.into(),
            active_certificate_digest: active_digest,
            native_binding: current.lifetime.lifetime.status.receipt.clone(),
            native_session_ref: s.session.session_ref.clone(),
            private_sid_generation: s.session.private_sid_generation,
            native_binding_generation: s.session.native_binding_generation,
            session: s.session.clone(),
            world: b.world.clone(),
            original_digest: digest(&original)?,
            issued_at: DecimalU64(now.get()),
            expires_at: DecimalU64(expires),
            ports,
        };
        certificate.validate_shape().map_err(|_| invalid())?;
        Ok(ExecutionNativePrepareAuthorityV2 {
            certificate,
            original,
            before_digest: digest(self)?,
        })
    }
    pub fn record_test_session_v2_native_prepare(
        &mut self,
        id: &str,
        authority: ExecutionNativePrepareAuthorityV2,
        signed: SignedAccountNativeExecutionPrepareV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<ExecutionNativePrepareDispatchV2> {
        if signed.certificate != authority.certificate
            || authority.before_digest != digest(self)?
            || signed.certificate.session_id != id
            || signed.certificate.issued_at.0 != now.get()
        {
            return Err(invalid());
        }
        signed
            .verify_issuer(pin, floor, now.get())
            .map_err(|_| invalid())?;
        let request = NativeExecutionPrepareRequestV2 {
            proof: signed,
            original: Box::new(authority.original),
        };
        request.wire_bytes().map_err(|_| invalid())?;
        let keys = ledger_keys(&request)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::Execution, now)?;
        for key in keys.iter().map(String::as_str).chain(
            request
                .proof
                .certificate
                .ports
                .iter()
                .map(|p| p.attempt_id.as_str()),
        ) {
            self.test_session_v2_transport_id_available(id, key)?;
        }
        let action = s
            .execution_book
            .as_ref()
            .ok_or_else(invalid)?
            .action_request_id
            .clone();
        let proof_digest = request.proof.digest().map_err(|_| invalid())?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(10)
            .map_err(|_| invalid())?;
        for (key, port) in keys.iter().zip(&request.proof.certificate.ports) {
            s.purpose_transports.insert(
                key.clone(),
                SessionPurposeTransportV1::ExecutionNativePrepareV2(Box::new(
                    ExecutionNativePrepareTransportV2 {
                        edge: port.edge,
                        proof_digest: proof_digest.clone(),
                        claimed_at: now,
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
            .native_prepare = Some(ExecutionNativePrepareRecordV2 {
            proof: request.proof.clone(),
            ledger_keys: keys,
            claimed_at: now,
            public_original_cbor_hex: None,
            public_recorded_at: None,
        });
        super::super::test_session::append_session_event_v2(
            s,
            super::super::test_session::SessionEventKindV2::ExecutionNativePrepareClaimed {
                request_id: action,
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
        Ok(ExecutionNativePrepareDispatchV2 { request })
    }
}

impl AccountProvisionApprovalState {
    pub(super) fn validate_native_prepare_record_v2(
        &self,
        s: &TestSessionAuthorityV2,
        b: &SessionExecutionBookV2,
    ) -> Result<()> {
        let count = s
            .purpose_transports
            .values()
            .filter(|t| matches!(t, SessionPurposeTransportV1::ExecutionNativePrepareV2(_)))
            .count();
        let Some(record) = b.native_prepare_record() else {
            return if count == 0 { Ok(()) } else { Err(invalid()) };
        };
        if count != 10 {
            return Err(invalid());
        }
        let stage = b
            .stages
            .get(&MoveExecutionStageV2::Prepare)
            .ok_or_else(invalid)?;
        let service = stage.service_record.as_ref().ok_or_else(invalid)?;
        let original = self
            .test_session_v2_prepare_original_history(&b.session_id)?
            .ok_or_else(invalid)?;
        let request = NativeExecutionPrepareRequestV2 {
            proof: record.proof.clone(),
            original: Box::new(original),
        };
        request.wire_bytes().map_err(|_| invalid())?;
        if record.public_original_cbor_hex.is_some() != record.public_recorded_at.is_some() {
            return Err(invalid());
        }
        if let Some(raw) = &record.public_original_cbor_hex {
            let result = decode_public_result(raw)?;
            result.validate_public(&request).map_err(|_| invalid())?;
            let at = record.public_recorded_at.ok_or_else(invalid)?;
            if at.get() < record.claimed_at.get()
                || at.get() < result.native_receipt.receipt.completed_at.0
            {
                return Err(invalid());
            }
        }
        let c = &record.proof.certificate;
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory)) = s
            .purpose_transports
            .get(&stage.allocation.factory_ingress_id)
        else {
            return Err(invalid());
        };
        let (reference, active_digest) =
            self.test_session_v2_stored_native_ready_identity(&b.session_id)?;
        let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
        let observed = source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at;
        let lf = b.archived_lifetime.as_ref().ok_or_else(invalid)?;
        let original_at = stage.original.as_ref().ok_or_else(invalid)?.recorded_at;
        if factory.kind != ExecutionPrepareFactoryKindV2::OwnerServiceAndAccountV2
            || factory.factory_ports.is_none()
            || c.factory_record_key != stage.allocation.factory_ingress_id
            || c.factory_ports_digest
                != digest(factory.factory_ports.as_ref().ok_or_else(invalid)?)?
            || c.private_current_proof_digest
                != factory
                    .private_current
                    .as_ref()
                    .and_then(|r| r.proof.as_ref())
                    .ok_or_else(invalid)?
                    .digest()
                    .map_err(|_| invalid())?
            || c.session_id != b.session_id
            || c.approved_input_digest != b.approved_input_digest
            || c.session_intent_digest != b.session_intent_digest
            || c.execution_deadline.0 != b.execution_deadline.get()
            || c.action_request_id != b.action_request_id
            || c.stage_operation_id != stage.allocation.operation_id
            || c.source_operation_id != b.source_operation_id
            || c.source_nonce != source.source.verify_nonce
            || c.source_record_digest != digest(source)?
            || c.source_observed_at != observed
            || c.source_expires_at != source.source.expires_at
            || c.expires_at.0
                > source
                    .source
                    .verification
                    .as_ref()
                    .ok_or_else(invalid)?
                    .verification
                    .source_session_expires_at
                    .0
            || c.expires_at.0
                > source
                    .source
                    .verification
                    .as_ref()
                    .ok_or_else(invalid)?
                    .verification
                    .source_session_idle_expires_at
                    .0
            || c.credential_generation != b.credential_generation
            || c.session != s.session
            || c.native_binding != lf.lifetime.status.receipt
            || c.private_sid_generation != s.session.private_sid_generation
            || c.native_binding_generation != s.session.native_binding_generation
            || c.native_session_ref != s.session.session_ref
            || c.world != b.world
            || c.opaque_registration_ref != reference
            || c.active_certificate_digest != active_digest
            || c.issuer != service.permits.sign.permit.issuer
            || c.issued_at.0 != record.claimed_at.get()
            || record.claimed_at.get() < original_at.get()
            || c.expires_at.0 > service.permits.sign.permit.expires_at.get()
            || c.expires_at.0 > b.execution_deadline.get()
            || c.expires_at.0 > lf.lifetime.status.expires_at.0
            || c.expires_at.0 > lf.lifetime.session_expires_at.0
            || c.expires_at.0 > lf.lifetime.session_idle_expires_at.0
            || c.expires_at.0
                > lf.lifetime
                    .status
                    .observed_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?
            || record.ledger_keys != ledger_keys(&request)?
        {
            return Err(invalid());
        }
        factory.validate_factory_profile(s)?;
        lf.lifetime
            .validate_for(
                &s.native_lifetime
                    .as_ref()
                    .ok_or_else(invalid)?
                    .lifetime
                    .status
                    .receipt,
                &self.test_session_v2_lifetime_nonce(&b.session_id, &b.source_operation_id)?,
                s.session.native_binding_generation,
                s.input.request.context.service_epoch,
                record.claimed_at.get(),
            )
            .map_err(|_| invalid())?;
        let expected_ordinal = super::super::test_session::session_event_ordinal_v2(
            s,
            &super::super::test_session::SessionEventKindV2::ExecutionNativePrepareClaimed {
                request_id: b.action_request_id.clone(),
            },
        )?;
        if expected_ordinal != Some(c.accepted_ordinal.0) {
            return Err(invalid());
        }
        let proof_digest = record.proof.digest().map_err(|_| invalid())?;
        for (key, port) in record.ledger_keys.iter().zip(&c.ports) {
            let expected = SessionPurposeTransportV1::ExecutionNativePrepareV2(Box::new(
                ExecutionNativePrepareTransportV2 {
                    edge: port.edge,
                    proof_digest: proof_digest.clone(),
                    claimed_at: record.claimed_at,
                },
            ));
            if s.purpose_transports.get(key) != Some(&expected) {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

impl AccountProvisionApprovalState {
    /// External configuration pins, not packet-provided keys. Historical
    /// verification returns no dispatch token and does not refresh any window.
    pub fn test_session_v2_native_prepare_issuer_history(
        &self,
        id: &str,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        self.validate_native_prepare_record_v2(s, b)?;
        if let Some(record) = b.native_prepare_record() {
            record
                .proof
                .verify_issuer(pin, floor, record.claimed_at.get())
                .map_err(|_| invalid())?;
        }
        Ok(())
    }
}

impl AccountProvisionApprovalState {
    pub fn archive_test_session_v2_prepared_public_result(
        &mut self,
        id: &str,
        result: ExecutionPreparedPublicResultV2,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let record = b.native_prepare_record().ok_or_else(invalid)?;
        let q = NativeExecutionPrepareRequestV2 {
            proof: record.proof.clone(),
            original: Box::new(
                self.test_session_v2_prepare_original_history(id)?
                    .ok_or_else(invalid)?,
            ),
        };
        result
            .verify_native(&q, native.native_root_key_id, native.native_root_public_key)
            .map_err(|_| invalid())?;
        if record.proof.certificate.issuer.native_audience_origin != native.http_origin
            || now.get() < record.claimed_at.get()
            || now.get() < result.native_receipt.receipt.completed_at.0
        {
            return Err(invalid());
        }
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&result).map_err(|_| invalid())?;
        let encoded: String = bytes.iter().map(|v| format!("{v:02x}")).collect();
        if let Some(old) = &record.public_original_cbor_hex {
            return if old == &encoded {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        let request_id = b.action_request_id.clone();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let record = s
            .execution_book
            .as_mut()
            .ok_or_else(invalid)?
            .stages
            .get_mut(&MoveExecutionStageV2::Prepare)
            .and_then(|stage| stage.native_prepare.as_mut())
            .ok_or_else(invalid)?;
        record.public_original_cbor_hex = Some(encoded);
        record.public_recorded_at = Some(now);
        super::super::test_session::append_session_event_v2(
            s,
            super::super::test_session::SessionEventKindV2::ExecutionPreparedPublicArchived {
                request_id,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}

fn decode_public_result(raw: &str) -> Result<ExecutionPreparedPublicResultV2> {
    if raw.is_empty()
        || raw.len() > 131072
        || raw.len() % 2 != 0
        || !raw
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
    {
        return Err(invalid());
    }
    let bytes = raw
        .as_bytes()
        .chunks_exact(2)
        .map(|b| {
            let digit = |v: u8| if v <= b'9' { v - b'0' } else { v - b'a' + 10 };
            (digit(b[0]) << 4) | digit(b[1])
        })
        .collect::<Vec<_>>();
    let value: ExecutionPreparedPublicResultV2 =
        serde_cbor::from_slice(&bytes).map_err(|_| invalid())?;
    if oasis7_wasm_abi::encode_canonical_cbor(&value).map_err(|_| invalid())? != bytes {
        return Err(invalid());
    }
    Ok(value)
}

impl AccountProvisionApprovalState {
    /// External deployment pin is mandatory after restore. Public state alone
    /// never chooses its Native verifier or turns a fact into fresh Current.
    pub(in crate::provisioning) fn test_session_v2_execution_public_native_history(
        &self,
        id: &str,
        native: &NativeStageScope<'_>,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let Some(record) = s
            .execution_book
            .as_ref()
            .and_then(|b| b.native_prepare_record())
        else {
            return Ok(());
        };
        let Some(raw) = &record.public_original_cbor_hex else {
            return Ok(());
        };
        let result = decode_public_result(raw)?;
        let q = NativeExecutionPrepareRequestV2 {
            proof: record.proof.clone(),
            original: Box::new(
                self.test_session_v2_prepare_original_history(id)?
                    .ok_or_else(invalid)?,
            ),
        };
        if q.proof.certificate.issuer.native_audience_origin != native.http_origin {
            return Err(invalid());
        }
        result
            .verify_native(&q, native.native_root_key_id, native.native_root_public_key)
            .map_err(|_| invalid())
    }
}
