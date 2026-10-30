//! Paid, challenge-bound Core component. No network dispatch or action grant.
use super::*;
use crate::approval_admission::{
    ContextSignatureFact, append_context_signature_checks, bind_live_context_checks,
};
#[path = "test_session_live_context_checks.rs"]
mod checks;
#[path = "test_session_live_context_nonce.rs"]
mod nonce;
#[path = "test_session_live_context_restore.rs"]
mod restore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveContextReadPortV1 {
    IdentityNative,
    NativeWorldView,
    NativeWorldLive,
}
const PORTS: [LiveContextReadPortV1; 3] = [
    LiveContextReadPortV1::IdentityNative,
    LiveContextReadPortV1::NativeWorldView,
    LiveContextReadPortV1::NativeWorldLive,
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveContextReadTransportKindV1 {
    LiveContextRead,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveContextReadTransportV1 {
    pub kind: LiveContextReadTransportKindV1,
    pub purpose: SessionPurposeV1,
    pub claim_id: String,
    pub port: LiveContextReadPortV1,
    pub context_operation_id: String,
    pub source_operation_id: String,
    pub claimed_at: UnixMillis,
    pub expected_scope: ContextBodyScopeV1,
    pub reference_digest: String,
    pub request_digest: String,
    pub observation_nonce: String,
    pub gateway_nonce: String,
}
impl core::fmt::Debug for LiveContextReadTransportV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("LiveContextReadTransportV1(<private>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveContextReadFactV1 {
    pub request_digest: String,
    pub signature_hex: String,
    pub payload_digest: String,
    pub payload_bytes: DecimalU64,
    pub read_commit: ws::CommitRef,
    pub capture_commit: ws::CommitRef,
    pub archived_at: UnixMillis,
}
impl core::fmt::Debug for LiveContextReadFactV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("LiveContextReadFactV1(<historical-not-authority>)")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveContextReadRecordV1 {
    pub request: TaskContextLiveReadV1,
    pub gateway_read: ws::ReadWorldViewRequest,
    pub source_operation_id: String,
    pub historical_reads: [String; 2],
    pub source_digest: String,
    pub lifetime_digest: String,
    pub source_ticket: u8,
    pub claimed_at: UnixMillis,
    pub deadline: UnixMillis,
    pub reference_digest: String,
    #[serde(deserialize_with = "required_option")]
    pub archive: Option<LiveContextReadFactV1>,
    #[serde(deserialize_with = "required_option")]
    pub admitted_at: Option<UnixMillis>,
    #[serde(deserialize_with = "required_option")]
    pub admission_observed_at: Option<UnixMillis>,
    #[serde(deserialize_with = "required_option")]
    pub admission_source_ticket: Option<u8>,
}
impl core::fmt::Debug for LiveContextReadRecordV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("LiveContextReadRecordV1(<not-a-dispatch-permit>)")
    }
}
/// Nonces are supplied only by the future trusted Rust service RNG. This Rust
/// input cannot prove RNG provenance and has no Deserialize implementation.
pub struct LiveContextReadClaim<'a> {
    pub attempt: &'a str,
    pub observation_nonce: &'a str,
    pub gateway_observation_nonce: &'a str,
    pub historical_reads: [&'a str; 2],
}
/// Saved unsigned requests and paid facts, not permission to sign/send them.
pub struct LiveContextReadDescriptor {
    pub request: TaskContextLiveReadV1,
    pub gateway_read: ws::ReadWorldViewRequest,
    pub paid_ports: [String; 3],
}
/// A short-lived in-process admission result. No serialization, Clone, Debug,
/// body accessor or dispatch/signing API. Restore cannot reconstruct it.
pub struct AdmittedLiveContext {
    _payload: zeroize::Zeroizing<Vec<u8>>,
}
pub struct LiveContextReadEvidence<'a> {
    pub historical_body: &'a ValidatedContextBody,
    pub response: &'a ws::SignedServiceResponse<CapturedTaskContextV1>,
}
fn required_option<'de, D, T>(deserializer: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
fn port_id(
    attempt: &str,
    port: LiveContextReadPortV1,
    record: &LiveContextReadRecordV1,
    reference: &ContextBodyReferenceV1,
) -> Result<String> {
    // Four meaningful canonical digests place this closed class outside the
    // legacy <=256-byte GenericOne ID space, without changing that old rule.
    Ok(format!(
        "live-context-v1:{}:{}:{}:{}",
        digest(&("oasis7.custody.live-context.claim.v1", attempt))?,
        digest(&("oasis7.custody.live-context.scope-reference.v1", reference))?,
        digest(&(
            "oasis7.custody.live-context.whole-request.v1",
            &record.request,
            &record.gateway_read,
        ))?,
        digest(&(
            "oasis7.custody.live-context.port-source.v1",
            port,
            &record.source_operation_id,
            &record.source_digest,
            record.claimed_at,
            record.source_ticket,
        ))?,
    ))
}
fn reference_digest(r: &ContextBodyReferenceV1) -> Result<String> {
    digest(&("oasis7.custody.live-context.reference.v1", r))
}
fn source_digest(s: &TestSessionAuthorityV1, source: &str) -> Result<String> {
    digest(&(
        "oasis7.custody.live-context.source.v1",
        source,
        s.sources.get(source).ok_or_else(invalid)?,
    ))
}
fn marker(
    attempt: &str,
    port: LiveContextReadPortV1,
    record: &LiveContextReadRecordV1,
    r: &ContextBodyReferenceV1,
) -> Result<SessionPurposeTransportV1> {
    Ok(SessionPurposeTransportV1::LiveContextRead(Box::new(
        LiveContextReadTransportV1 {
            kind: LiveContextReadTransportKindV1::LiveContextRead,
            purpose: SessionPurposeV1::Execution,
            claim_id: attempt.into(),
            port,
            context_operation_id: r.scope.context_operation_id.clone(),
            source_operation_id: record.source_operation_id.clone(),
            claimed_at: record.claimed_at,
            expected_scope: r.scope.clone(),
            reference_digest: record.reference_digest.clone(),
            request_digest: ws::request_digest(TASK_CONTEXT_LIVE_READ_PATH, &record.request)
                .map_err(|_| invalid())?,
            observation_nonce: record.request.observation_nonce.clone(),
            gateway_nonce: record
                .gateway_read
                .observation_nonce
                .clone()
                .ok_or_else(invalid)?,
        },
    )))
}
impl AccountProvisionApprovalState {
    pub fn claim_test_session_live_context_read(
        &mut self,
        id: &str,
        claim: LiveContextReadClaim<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<LiveContextReadDescriptor> {
        verifier.verify(&self.test_session_context_read_checks(
            id,
            ContextBodySlotKindV1::Captured,
            ContextBodyPortV1::RecoverBody,
            current,
            now,
        )?)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let c = s.context.as_ref().ok_or_else(invalid)?;
        let r = c
            .slots
            .get(&ContextBodySlotKindV1::Captured)
            .ok_or_else(invalid)?;
        if !context_attempt_id(claim.attempt)
            || !hex64(claim.observation_nonce)
            || !hex64(claim.gateway_observation_nonce)
            || claim.observation_nonce == claim.gateway_observation_nonce
            || Self::session_transport_attempt_occupied(s, claim.attempt)
            || Self::live_context_nonce_occupied(id, s, claim.observation_nonce)?
            || Self::live_context_nonce_occupied(id, s, claim.gateway_observation_nonce)?
            || c.live_reads.len() >= 400 / 3
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let request = TaskContextLiveReadV1 {
            original: c.get.clone().ok_or_else(invalid)?,
            observation_nonce: claim.observation_nonce.into(),
        };
        verify_live_context_read(&request).map_err(|_| invalid())?;
        let gateway_read = ws::ReadWorldViewRequest {
            contract_version: 1,
            observation_nonce: Some(claim.gateway_observation_nonce.into()),
            formal_anchor: None,
            world: ws::WorldIdentity {
                world_id: r.scope.world.world_id.clone(),
                genesis_digest: r.scope.world.genesis_digest.clone(),
            },
            scope_id: format!("agent:{}", request.original.request.context.fence.agent_id),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        gateway_read.validate().map_err(|_| invalid())?;
        let record = LiveContextReadRecordV1 {
            request,
            gateway_read,
            source_operation_id: current.source_operation_id.into(),
            historical_reads: claim.historical_reads.map(str::to_owned),
            source_digest: source_digest(s, current.source_operation_id)?,
            lifetime_digest: current.lifetime.lifetime.digest().map_err(|_| invalid())?,
            source_ticket: current.source_ticket,
            claimed_at: now,
            deadline: s
                .roots
                .get(&SessionPurposeV1::Execution)
                .ok_or_else(invalid)?
                .deadline,
            reference_digest: reference_digest(r)?,
            archive: None,
            admitted_at: None,
            admission_observed_at: None,
            admission_source_ticket: None,
        };
        self.validate_live_context_historical_reads(id, s, &record)?;
        let ids = PORTS
            .map(|port| port_id(claim.attempt, port, &record, r))
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        if ids
            .iter()
            .any(|p| p == claim.attempt || Self::session_transport_attempt_occupied(s, p))
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let charges = PORTS
            .into_iter()
            .map(|port| marker(claim.attempt, port, &record, r))
            .collect::<Result<Vec<_>>>()?;
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(3)
            .map_err(|_| invalid())?;
        for (key, charge) in ids.iter().cloned().zip(charges) {
            s.purpose_transports.insert(key, charge);
        }
        s.context
            .as_mut()
            .ok_or_else(invalid)?
            .live_reads
            .insert(claim.attempt.into(), record.clone());
        if s.closing.is_none() && session_purpose_transport_exhausted(s) {
            s.closing = Some((TestSessionClosingReasonV1::BudgetExhausted, now));
        }
        let paid_ports = ids.try_into().map_err(|_| invalid())?;
        next.validate_persistence()?;
        *self = next;
        Ok(LiveContextReadDescriptor {
            request: record.request,
            gateway_read: record.gateway_read,
            paid_ports,
        })
    }
    /// Fact-only SDK and exact signature verification; no live Current is implied.
    pub fn archive_test_session_live_context_read(
        &mut self,
        id: &str,
        attempt: &str,
        evidence: LiveContextReadEvidence<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<LiveContextReadFactV1> {
        let checks = self.test_session_live_context_archive_checks(id, attempt, &evidence)?;
        verifier.verify(&checks)?;
        let record = self.live_context_record(id, attempt)?;
        if now.get() < record.claimed_at.get() {
            return Err(invalid());
        }
        let fact = self.live_context_fact(
            id,
            attempt,
            &evidence,
            record.archive.as_ref().map_or(now, |v| v.archived_at),
        )?;
        if let Some(old) = &record.archive {
            return if old == &fact {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        next.live_context_record_mut(id, attempt)?.archive = Some(fact.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(fact)
    }
    pub fn admit_test_session_live_context_read(
        &mut self,
        id: &str,
        attempt: &str,
        evidence: LiveContextReadEvidence<'_>,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<AdmittedLiveContext> {
        verifier.verify(
            &self.test_session_live_context_checks(id, attempt, &evidence, current, now)?,
        )?;
        let payload = zeroize::Zeroizing::new(
            oasis7_wasm_abi::encode_canonical_cbor(&evidence.response.payload)
                .map_err(|_| invalid())?,
        );
        let mut next = self.clone();
        let record = next.live_context_record_mut(id, attempt)?;
        record.admitted_at = Some(now);
        record.admission_observed_at = Some(current.observed_at);
        record.admission_source_ticket = Some(current.source_ticket);
        next.validate_persistence()?;
        *self = next;
        Ok(AdmittedLiveContext { _payload: payload })
    }
    fn live_context_record(&self, id: &str, attempt: &str) -> Result<&LiveContextReadRecordV1> {
        self.test_sessions
            .get(id)
            .and_then(|s| s.context.as_ref())
            .and_then(|c| c.live_reads.get(attempt))
            .ok_or_else(invalid)
    }
    fn live_context_record_mut(
        &mut self,
        id: &str,
        attempt: &str,
    ) -> Result<&mut LiveContextReadRecordV1> {
        self.test_sessions
            .get_mut(id)
            .and_then(|s| s.context.as_mut())
            .and_then(|c| c.live_reads.get_mut(attempt))
            .ok_or_else(invalid)
    }
}
