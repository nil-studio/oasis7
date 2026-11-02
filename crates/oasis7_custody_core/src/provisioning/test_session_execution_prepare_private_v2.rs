//! Two real private-history RPCs, independently funded by Execution. Stored
//! metadata/history cannot recreate the first-read or observed Current token.
use super::*;
use crate::execution_private_current_v2::{
    AccountExecutionPrivateCurrentV2, EXECUTION_PRIVATE_CURRENT_ACTOR_PATH_V2,
    SignedAccountExecutionPrivateCurrentV2,
};
use crate::native_binding::SignedManagedNativeSessionLifetimeV1;
use crate::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
};
fn invalid() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPrivateCurrentHopV2 {
    AccountToIdentityHistory,
    IdentityHistoryToManifest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrivateCurrentPortV2 {
    pub hop: ExecutionPrivateCurrentHopV2,
    pub physical_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrivateCurrentMetadataV2 {
    pub session_ref: String,
    pub private_sid_generation: DecimalU64,
    pub lifetime_digest: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrivateCurrentReadV2 {
    #[serde(deserialize_with = "super::required_option")]
    pub proof: Option<SignedAccountExecutionPrivateCurrentV2>,
    pub claimed_at: UnixMillis,
    pub expires_at: UnixMillis,
    pub accepted_ordinal: DecimalU64,
    pub lifetime_digest: String,
    pub source_operation_id: String,
    pub ports: [ExecutionPrivateCurrentPortV2; 2],
    #[serde(deserialize_with = "super::required_option")]
    pub completed_at: Option<UnixMillis>,
    #[serde(deserialize_with = "super::required_option")]
    pub result: Option<ExecutionPrivateCurrentMetadataV2>,
}
impl core::fmt::Debug for ExecutionPrivateCurrentReadV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionPrivateCurrentReadV2(<public fixed metadata>)")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrivateCurrentTransportV2 {
    pub factory_key: String,
    pub hop: ExecutionPrivateCurrentHopV2,
    pub lifetime_digest: String,
    pub claimed_at: UnixMillis,
}
pub struct ExecutionPrivateCurrentPortsV2 {
    session_id: String,
    input_digest: String,
    action_request_id: String,
    stage_operation_id: String,
    factory_key: String,
    read: ExecutionPrivateCurrentReadV2,
    session: ParticipantSessionRefV1,
    lifetime: SignedManagedNativeSessionLifetimeV1,
}
impl ExecutionPrivateCurrentPortsV2 {
    pub fn certificate(
        &self,
        issuer: AccountMachineIssuerV2,
    ) -> Result<AccountExecutionPrivateCurrentV2> {
        let c = AccountExecutionPrivateCurrentV2 {
            api_version: 2,
            issuer,
            recipient_role: Role::Identity,
            recipient_path: EXECUTION_PRIVATE_CURRENT_ACTOR_PATH_V2.into(),
            session_id: self.session_id.clone(),
            approved_input_digest: self.input_digest.clone(),
            action_request_id: self.action_request_id.clone(),
            stage_operation_id: self.stage_operation_id.clone(),
            factory_key: self.factory_key.clone(),
            source_operation_id: self.read.source_operation_id.clone(),
            accepted_ordinal: self.read.accepted_ordinal,
            session: self.session.clone(),
            lifetime: self.lifetime.clone(),
            issued_at: DecimalU64(self.read.claimed_at.get()),
            expires_at: DecimalU64(self.read.expires_at.get()),
            ports: self.read.ports.clone(),
        };
        c.validate_shape().map_err(|_| invalid())?;
        Ok(c)
    }
    pub fn request(&self) -> Result<&SignedAccountExecutionPrivateCurrentV2> {
        self.read.proof.as_ref().ok_or_else(invalid)
    }
    pub fn session(&self) -> &ParticipantSessionRefV1 {
        &self.session
    }
    pub fn lifetime(&self) -> &SignedManagedNativeSessionLifetimeV1 {
        &self.lifetime
    }
    pub fn expires_at(&self) -> UnixMillis {
        self.read.expires_at
    }
}
pub struct ExecutionPrivateCurrentObservationV2 {
    session: ParticipantSessionRefV1,
    lifetime: SignedManagedNativeSessionLifetimeV1,
    source: String,
    observed_at: UnixMillis,
    expires_at: UnixMillis,
}
impl ExecutionPrivateCurrentObservationV2 {
    pub fn session(&self) -> &ParticipantSessionRefV1 {
        &self.session
    }
    pub fn lifetime(&self) -> &SignedManagedNativeSessionLifetimeV1 {
        &self.lifetime
    }
    pub fn source_operation_id(&self) -> &str {
        &self.source
    }
    pub fn observed_at(&self) -> UnixMillis {
        self.observed_at
    }
    pub fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }
}
fn ports(
    s: &TestSessionAuthorityV2,
    factory: &str,
    lf_digest: &str,
) -> Result<[ExecutionPrivateCurrentPortV2; 2]> {
    let make = |hop| -> Result<ExecutionPrivateCurrentPortV2> {
        Ok(ExecutionPrivateCurrentPortV2 {
            hop,
            physical_id: format!(
                "execution-private-current-v2:{}",
                digest(&(
                    "oasis7.custody.execution-private-current.v2",
                    &s.input.operation_id,
                    s.input_digest()?,
                    factory,
                    lf_digest,
                    hop
                ))?
            ),
        })
    };
    Ok([
        make(ExecutionPrivateCurrentHopV2::AccountToIdentityHistory)?,
        make(ExecutionPrivateCurrentHopV2::IdentityHistoryToManifest)?,
    ])
}
impl AccountProvisionApprovalState {
    pub fn test_session_v2_prepare_private_current_checks(
        &self,
        id: &str,
        native: &NativeStageScope<'_>,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        self.test_session_v2_execution_lifetime_archive_checks(
            id,
            b.archived_lifetime.as_ref().ok_or_else(invalid)?,
            native,
        )
    }
    pub fn claim_test_session_v2_prepare_private_current(
        &mut self,
        id: &str,
        factory_key: &str,
        native: &NativeStageScope<'_>,
        now: UnixMillis,
        current_until: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<Option<ExecutionPrivateCurrentPortsV2>> {
        let s = self.test_session_v2_local(id, SessionPurposeV1::Execution, now)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory)) =
            s.purpose_transports.get(factory_key)
        else {
            return Err(invalid());
        };
        if factory.kind != ExecutionPrepareFactoryKindV2::OwnerServiceAndAccountV2
            || factory.current_read.is_none()
            || b.lifetime_admitted_at.is_none()
            || !b.stages.is_empty()
            || s.source_oracle_reads.get(&b.source_operation_id) != Some(&5)
        {
            return Err(invalid());
        }
        factory.validate_factory_profile(s)?;
        if factory.private_current.is_some() {
            return Ok(None);
        }
        verifier.verify(&self.test_session_v2_prepare_private_current_checks(id, native)?)?;
        let lf = b.archived_lifetime.as_ref().ok_or_else(invalid)?.clone();
        let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
        let source_at = source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification
            .observed_at
            .0;
        // A caller can only tighten this original immutable window. The
        // typed Worker issuer independently binds it to the paid C3 holder.
        let expires = [
            current_until.get(),
            b.execution_deadline.get(),
            s.lease_until.ok_or_else(invalid)?.get(),
            lf.lifetime.status.expires_at.0,
            lf.lifetime
                .status
                .observed_at
                .0
                .checked_add(5000)
                .ok_or_else(invalid)?,
            lf.lifetime.session_expires_at.0,
            lf.lifetime.session_idle_expires_at.0,
            source.source.expires_at.0,
            source
                .source
                .verification
                .as_ref()
                .ok_or_else(invalid)?
                .verification
                .source_session_expires_at
                .0,
            source
                .source
                .verification
                .as_ref()
                .ok_or_else(invalid)?
                .verification
                .source_session_idle_expires_at
                .0,
            source_at.checked_add(5000).ok_or_else(invalid)?,
        ]
        .into_iter()
        .min()
        .ok_or_else(invalid)?;
        if now.get() >= expires
            || now.get() < source_at
            || now.get() < lf.lifetime.status.observed_at.0
        {
            return Err(invalid());
        }
        let lf_digest = lf.lifetime.digest().map_err(|_| invalid())?;
        let read = ExecutionPrivateCurrentReadV2 {
            proof: None,
            claimed_at: now,
            expires_at: UnixMillis::new(expires),
            accepted_ordinal: DecimalU64(super::super::test_session::next_session_ordinal_v2(s)?),
            lifetime_digest: lf_digest.clone(),
            source_operation_id: b.source_operation_id.clone(),
            ports: ports(s, factory_key, &lf_digest)?,
            completed_at: None,
            result: None,
        };
        for port in &read.ports {
            self.test_session_v2_transport_id_available(id, &port.physical_id)?;
        }
        let token = ExecutionPrivateCurrentPortsV2 {
            session_id: id.into(),
            input_digest: b.approved_input_digest.clone(),
            action_request_id: b.action_request_id.clone(),
            stage_operation_id: factory.intent.allocation.operation_id.clone(),
            factory_key: factory_key.into(),
            read: read.clone(),
            session: s.session.clone(),
            lifetime: lf,
        };
        let action = b.action_request_id.clone();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(2)
            .map_err(|_| invalid())?;
        for port in &read.ports {
            s.purpose_transports.insert(
                port.physical_id.clone(),
                SessionPurposeTransportV1::ExecutionPrivateCurrentV2(Box::new(
                    ExecutionPrivateCurrentTransportV2 {
                        factory_key: factory_key.into(),
                        hop: port.hop,
                        lifetime_digest: read.lifetime_digest.clone(),
                        claimed_at: now,
                    },
                )),
            );
        }
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(p)) =
            s.purpose_transports.get_mut(factory_key)
        else {
            return Err(invalid());
        };
        p.private_current = Some(read);
        super::super::test_session::append_session_event_v2(
            s,
            super::super::test_session::SessionEventKindV2::ExecutionPrivateCurrentClaimed {
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
        Ok(Some(token))
    }
    pub fn complete_test_session_v2_prepare_private_current(
        &mut self,
        id: &str,
        token: ExecutionPrivateCurrentPortsV2,
        result: ExecutionPrivateCurrentMetadataV2,
        now: UnixMillis,
    ) -> Result<Option<ExecutionPrivateCurrentObservationV2>> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory)) =
            s.purpose_transports.get(&token.factory_key)
        else {
            return Err(invalid());
        };
        if token.read.proof.is_none()
            || factory.private_current.as_ref() != Some(&token.read)
            || token.session != s.session
            || b.archived_lifetime.as_ref() != Some(&token.lifetime)
            || token.read.completed_at.is_some()
            || token.read.result.is_some()
            || now.get() < token.read.claimed_at.get()
            || result.session_ref != s.session.session_ref
            || result.private_sid_generation != s.session.private_sid_generation
            || result.lifetime_digest != token.read.lifetime_digest
        {
            return Err(invalid());
        }
        let fresh = now.get() < token.read.expires_at.get()
            && self
                .test_session_v2_local(id, SessionPurposeV1::Execution, now)
                .is_ok();
        let action = b.action_request_id.clone();
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(p)) =
            s.purpose_transports.get_mut(&token.factory_key)
        else {
            return Err(invalid());
        };
        let r = p.private_current.as_mut().ok_or_else(invalid)?;
        r.completed_at = Some(now);
        r.result = Some(result);
        super::super::test_session::append_session_event_v2(
            s,
            super::super::test_session::SessionEventKindV2::ExecutionPrivateCurrentObserved {
                request_id: action,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(fresh.then_some(ExecutionPrivateCurrentObservationV2 {
            session: token.session,
            lifetime: token.lifetime,
            source: token.read.source_operation_id,
            observed_at: token.read.claimed_at,
            expires_at: token.read.expires_at,
        }))
    }
}

impl AccountProvisionApprovalState {
    /// Sign and persist only the first token in its original SQL time. Missing
    /// proof after restart is paid-unknown, never an invitation to reissue it.
    pub fn record_test_session_v2_prepare_private_proof(
        &mut self,
        id: &str,
        mut token: ExecutionPrivateCurrentPortsV2,
        proof: SignedAccountExecutionPrivateCurrentV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: UnixMillis,
    ) -> Result<ExecutionPrivateCurrentPortsV2> {
        let s = self.test_session_v2_record(id)?;
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory)) =
            s.purpose_transports.get(&token.factory_key)
        else {
            return Err(invalid());
        };
        if id != token.session_id
            || factory.private_current.as_ref() != Some(&token.read)
            || token.read.proof.is_some()
            || token.read.claimed_at != now
            || proof.certificate != token.certificate(proof.certificate.issuer.clone())?
        {
            return Err(invalid());
        }
        proof
            .verify_issuer(pin, floor, now.get())
            .map_err(|_| invalid())?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory)) =
            s.purpose_transports.get_mut(&token.factory_key)
        else {
            return Err(invalid());
        };
        let r = factory.private_current.as_mut().ok_or_else(invalid)?;
        r.proof = Some(proof.clone());
        token.read.proof = Some(proof);
        next.validate_persistence()?;
        *self = next;
        Ok(token)
    }
}

impl AccountProvisionApprovalState {
    pub(super) fn validate_prepare_private_current_v2(
        &self,
        s: &TestSessionAuthorityV2,
        b: &SessionExecutionBookV2,
    ) -> Result<()> {
        let count = s
            .purpose_transports
            .values()
            .filter(|v| matches!(v, SessionPurposeTransportV1::ExecutionPrivateCurrentV2(_)))
            .count();
        let mut records = 0usize;
        for (key, v) in &s.purpose_transports {
            let SessionPurposeTransportV1::ExecutionFactoryCallbackV2(factory) = v else {
                continue;
            };
            let Some(r) = &factory.private_current else {
                continue;
            };
            records += 1;
            let lf = b.archived_lifetime.as_ref().ok_or_else(invalid)?;
            let source = s.sources.get(&b.source_operation_id).ok_or_else(invalid)?;
            let v = source
                .source
                .verification
                .as_ref()
                .ok_or_else(invalid)?
                .verification
                .observed_at
                .0;
            let current = factory.current_read.as_ref().ok_or_else(invalid)?;
            let ordinal = super::super::test_session::session_event_ordinal_v2(
                s,
                &super::super::test_session::SessionEventKindV2::ExecutionPrivateCurrentClaimed {
                    request_id: b.action_request_id.clone(),
                },
            )?;
            if factory.kind != ExecutionPrepareFactoryKindV2::OwnerServiceAndAccountV2
                || r.claimed_at.get() < current.claimed_at.get()
                || r.claimed_at.get() < b.lifetime_admitted_at.ok_or_else(invalid)?.get()
                || r.source_operation_id != b.source_operation_id
                || r.lifetime_digest != lf.lifetime.digest().map_err(|_| invalid())?
                || r.ports != ports(s, key, &r.lifetime_digest)?
                || ordinal != Some(r.accepted_ordinal.0)
                || r.claimed_at.get() >= r.expires_at.get()
                || r.expires_at.get() > b.execution_deadline.get()
                || r.expires_at.get() > lf.lifetime.status.expires_at.0
                || r.expires_at.get()
                    > lf.lifetime
                        .status
                        .observed_at
                        .0
                        .checked_add(5000)
                        .ok_or_else(invalid)?
                || r.expires_at.get() > lf.lifetime.session_expires_at.0
                || r.expires_at.get() > lf.lifetime.session_idle_expires_at.0
                || r.expires_at.get() > source.source.expires_at.0
                || r.expires_at.get()
                    > source
                        .source
                        .verification
                        .as_ref()
                        .ok_or_else(invalid)?
                        .verification
                        .source_session_expires_at
                        .0
                || r.expires_at.get()
                    > source
                        .source
                        .verification
                        .as_ref()
                        .ok_or_else(invalid)?
                        .verification
                        .source_session_idle_expires_at
                        .0
                || r.expires_at.get() > v.checked_add(5000).ok_or_else(invalid)?
                || r.claimed_at.get() < v
                || r.claimed_at.get() < lf.lifetime.status.observed_at.0
                || r.completed_at.is_some() != r.result.is_some()
                || r.completed_at
                    .is_some_and(|at| at.get() < r.claimed_at.get())
            {
                return Err(invalid());
            }
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
                    r.claimed_at.get(),
                )
                .map_err(|_| invalid())?;
            if let Some(meta) = &r.result {
                if r.proof.is_none()
                    || meta.session_ref != s.session.session_ref
                    || meta.private_sid_generation != s.session.private_sid_generation
                    || meta.lifetime_digest != r.lifetime_digest
                {
                    return Err(invalid());
                }
            }
            if let Some(proof) = &r.proof {
                let token = ExecutionPrivateCurrentPortsV2 {
                    session_id: b.session_id.clone(),
                    input_digest: b.approved_input_digest.clone(),
                    action_request_id: b.action_request_id.clone(),
                    stage_operation_id: factory.intent.allocation.operation_id.clone(),
                    factory_key: key.clone(),
                    read: r.clone(),
                    session: s.session.clone(),
                    lifetime: lf.clone(),
                };
                proof.wire_bytes().map_err(|_| invalid())?;
                if proof.certificate != token.certificate(proof.certificate.issuer.clone())? {
                    return Err(invalid());
                }
            }
            for port in &r.ports {
                let expected = SessionPurposeTransportV1::ExecutionPrivateCurrentV2(Box::new(
                    ExecutionPrivateCurrentTransportV2 {
                        factory_key: key.clone(),
                        hop: port.hop,
                        lifetime_digest: r.lifetime_digest.clone(),
                        claimed_at: r.claimed_at,
                    },
                ));
                if s.purpose_transports.get(&port.physical_id) != Some(&expected) {
                    return Err(invalid());
                }
            }
        }
        if records > 1 || count != records.checked_mul(2).ok_or_else(invalid)? {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn test_session_v2_prepare_private_issuer_history(
        &self,
        id: &str,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        let b = s.execution_book.as_ref().ok_or_else(invalid)?;
        self.validate_prepare_private_current_v2(s, b)?;
        if let Some(r) = b.private_current_read(s)
            && let Some(proof) = &r.proof
        {
            proof
                .verify_issuer(pin, floor, r.claimed_at.get())
                .map_err(|_| invalid())?;
        }
        Ok(())
    }
}
