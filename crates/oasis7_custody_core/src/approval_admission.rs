//! Pure, internal approval handoff. A recorded consumption is historical evidence,
//! never a grant, a key operation, or permission to dispatch. The adapter must
//! establish the association/revocation directory contract separately; this
//! engine cannot establish it or enable public Owner business endpoints.
use crate::{DecimalU64, approval::*, recovery::RecoveryFence};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_APPROVAL_ADMISSION_RECORDS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalSourceVersions {
    pub manager_account_id: String,
    pub manager_session_ref: String,
    pub origin_credential_id: String,
    pub origin_credential_version: DecimalU64,
    pub passkey_credential_id: String,
    pub passkey_credential_version: DecimalU64,
    pub manager_policy_version: DecimalU64,
    pub manager_security_version: DecimalU64,
    pub session_expires_at: DecimalU64,
    pub session_idle_expires_at: DecimalU64,
}

/// Supplied by trusted configuration/current authority reads, never request JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalAdmissionContext {
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub authority: ApprovalContextV1,
    pub allowed_managers: BTreeSet<String>,
    pub source: ApprovalSourceVersions,
    pub native_key_pins: BTreeMap<String, String>,
    pub fence: RecoveryFence,
    pub frozen: bool,
    pub handoff_until: DecimalU64,
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalAdmissionInput {
    pub operation_id: String,
    pub request: ApprovalOptionsRequestV1,
    pub approval: SignedCustodyApprovalV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalAdmissionError {
    Capacity,
    Invalid,
    Frozen,
    StaleContext,
    Expired,
    BindingMismatch,
    UnknownKey,
    SignatureRejected,
    Conflict,
    NonceReused,
    Missing,
    InvalidPersistence,
}
type Result<T> = std::result::Result<T, ApprovalAdmissionError>;

/// Immutable material for a trusted crypto adapter. No public constructor or
/// Deserialize implementation permits a request to supply "verified" jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SignatureCheck {
    key_id: String,
    public_key_hex: String,
    signature_base64url: String,
    canonical_bytes: Vec<u8>,
}
impl SignatureCheck {
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }
    pub fn signature_base64url(&self) -> &str {
        &self.signature_base64url
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExactSignatureChecks {
    operation_id: String,
    body_digest: String,
    context_digest: String,
    verify_nonce: String,
    verification_digest: Option<String>,
    checks: Vec<SignatureCheck>,
}
impl ExactSignatureChecks {
    pub fn checks(&self) -> &[SignatureCheck] {
        &self.checks
    }
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn body_digest(&self) -> &str {
        &self.body_digest
    }
    pub fn context_digest(&self) -> &str {
        &self.context_digest
    }
    pub fn verify_nonce(&self) -> &str {
        &self.verify_nonce
    }
    pub fn binding_digest(&self) -> Result<String> {
        canonical_digest("oasis7.custody.signature-checks.v1", self)
    }
}

/// Synchronous trusted port. An async Worker first verifies every job with
/// WebCrypto, then implements this port using a cache that compares the ENTIRE
/// ExactSignatureChecks value, not merely a key ID, operation ID or boolean.
pub trait ApprovalSignatureVerifier {
    fn verify(&mut self, checks: &ExactSignatureChecks) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalAdmissionStage {
    Pending,
    Consumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawAdmissionRecord")]
pub struct ApprovalAdmissionRecord {
    input: ApprovalAdmissionInput,
    body_digest: String,
    verify_nonce: String,
    context: ApprovalAdmissionContext,
    admitted_at: DecimalU64,
    handoff_until: DecimalU64,
    verification: Option<SignedApprovalVerificationV1>,
    consumed_at: Option<DecimalU64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAdmissionRecord {
    input: ApprovalAdmissionInput,
    body_digest: String,
    verify_nonce: String,
    context: ApprovalAdmissionContext,
    admitted_at: DecimalU64,
    handoff_until: DecimalU64,
    verification: Option<SignedApprovalVerificationV1>,
    consumed_at: Option<DecimalU64>,
}
impl TryFrom<RawAdmissionRecord> for ApprovalAdmissionRecord {
    type Error = &'static str;
    fn try_from(raw: RawAdmissionRecord) -> std::result::Result<Self, Self::Error> {
        let record = Self {
            input: raw.input,
            body_digest: raw.body_digest,
            verify_nonce: raw.verify_nonce,
            context: raw.context,
            admitted_at: raw.admitted_at,
            handoff_until: raw.handoff_until,
            verification: raw.verification,
            consumed_at: raw.consumed_at,
        };
        record
            .validate_persistence()
            .map_err(|_| "invalid persisted approval handoff")?;
        Ok(record)
    }
}
impl ApprovalAdmissionRecord {
    pub fn input(&self) -> &ApprovalAdmissionInput {
        &self.input
    }
    pub fn verify_nonce(&self) -> &str {
        &self.verify_nonce
    }
    pub fn consumed_at(&self) -> Option<u64> {
        self.consumed_at.map(|v| v.0)
    }
    pub fn handoff_until(&self) -> u64 {
        self.handoff_until.0
    }
    pub fn verification(&self) -> Option<&SignedApprovalVerificationV1> {
        self.verification.as_ref()
    }
    /// Reports historical storage state; it confers no present authority.
    pub fn stage(&self) -> ApprovalAdmissionStage {
        if self.consumed_at.is_some() {
            ApprovalAdmissionStage::Consumed
        } else {
            ApprovalAdmissionStage::Pending
        }
    }
    pub fn native_verify_request(&self) -> Result<ApprovalVerifyRequestV1> {
        Ok(ApprovalVerifyRequestV1 {
            approval: self.input.approval.clone(),
            proof_digest: self
                .input
                .approval
                .digest()
                .map_err(|_| ApprovalAdmissionError::Invalid)?,
            expected_audience: self.input.request.context.audience.clone(),
            expected_intent_digest: self
                .input
                .request
                .intent_digest()
                .map_err(|_| ApprovalAdmissionError::Invalid)?,
            purpose: VerificationPurposeV1::ConsumeApproval,
            verify_nonce: self.verify_nonce.clone(),
        })
    }
    fn validate_persistence(&self) -> Result<()> {
        if self.body_digest != input_digest(&self.input)? || !bounded(&self.verify_nonce) {
            return Err(ApprovalAdmissionError::InvalidPersistence);
        }
        validate_input(&self.input, &self.context, self.admitted_at.0)?;
        if self.handoff_until.0 > deadline(&self.input, &self.context)
            || self.handoff_until.0 <= self.admitted_at.0
        {
            return Err(ApprovalAdmissionError::InvalidPersistence);
        }
        match (&self.verification, self.consumed_at) {
            (None, None) => Ok(()),
            (Some(verification), Some(at)) if at.0 >= self.admitted_at.0 => {
                validate_verification(self, verification, &self.context, at.0)
            }
            _ => Err(ApprovalAdmissionError::InvalidPersistence),
        }
    }
}

/// Persist this whole value atomically with its audit event. Nonce reservations
/// are retained even for pending handoffs; expiration never frees a proof nonce.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawAdmissionBook")]
pub struct ApprovalAdmissionBook {
    records: BTreeMap<String, ApprovalAdmissionRecord>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAdmissionBook {
    records: BTreeMap<String, ApprovalAdmissionRecord>,
}
impl TryFrom<RawAdmissionBook> for ApprovalAdmissionBook {
    type Error = &'static str;
    fn try_from(raw: RawAdmissionBook) -> std::result::Result<Self, Self::Error> {
        if raw.records.len() > MAX_APPROVAL_ADMISSION_RECORDS {
            return Err("approval handoff capacity exceeded");
        }
        let mut nonces = BTreeSet::new();
        for (id, record) in &raw.records {
            if id != &record.input.operation_id
                || !nonces.insert(&record.input.approval.proof.nonce)
            {
                return Err("invalid persisted approval nonce index");
            }
        }
        Ok(Self {
            records: raw.records,
        })
    }
}
impl ApprovalAdmissionBook {
    /// Original evidence lookup only; callers still enforce their read ACL.
    pub(crate) fn records(&self) -> impl Iterator<Item = &ApprovalAdmissionRecord> {
        self.records.values()
    }
    /// Checks for returning immutable consumed history only. No current authority
    /// is asserted and callers must never use this path to mutate a plan.
    pub(crate) fn consumed_checks(
        &self,
        id: &str,
        verification: Option<&SignedApprovalVerificationV1>,
    ) -> Result<ExactSignatureChecks> {
        let record = self
            .records
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        let original = record
            .verification
            .as_ref()
            .ok_or(ApprovalAdmissionError::Missing)?;
        if verification.is_some_and(|v| v != original) {
            return Err(ApprovalAdmissionError::Conflict);
        }
        signature_checks(
            &record.input,
            &record.verify_nonce,
            &record.context,
            Some(original),
        )
    }
    pub fn original(&self, id: &str) -> Option<&ApprovalAdmissionRecord> {
        self.records.get(id)
    }
    pub fn begin_checks(
        &self,
        input: &ApprovalAdmissionInput,
        verify_nonce: &str,
        context: &ApprovalAdmissionContext,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let body_digest = input_digest(input)?;
        let nonce = if let Some(record) = self.records.get(&input.operation_id) {
            if record.body_digest != body_digest {
                return Err(ApprovalAdmissionError::Conflict);
            }
            check_current_context(&record.context, context)?;
            // Returning a historical original never creates another acceptance
            // window. Expired pending/consumed evidence remains queryable.
            validate_input(input, context, record.admitted_at.0)?;
            &record.verify_nonce
        } else {
            validate_input(input, context, now)?;
            if self.records.len() >= MAX_APPROVAL_ADMISSION_RECORDS {
                return Err(ApprovalAdmissionError::Capacity);
            }
            verify_nonce
        };
        if !bounded(nonce) {
            return Err(ApprovalAdmissionError::Invalid);
        }
        signature_checks(input, nonce, context, None)
    }
    pub fn begin(
        &mut self,
        input: ApprovalAdmissionInput,
        verify_nonce: String,
        context: &ApprovalAdmissionContext,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        let checks = self.begin_checks(&input, &verify_nonce, context, now)?;
        verifier.verify(&checks)?;
        if let Some(record) = self.records.get(&input.operation_id) {
            return Ok(record.clone());
        }
        if self
            .records
            .values()
            .any(|r| r.input.approval.proof.nonce == input.approval.proof.nonce)
        {
            return Err(ApprovalAdmissionError::NonceReused);
        }
        let record = ApprovalAdmissionRecord {
            body_digest: input_digest(&input)?,
            verify_nonce,
            context: context.clone(),
            admitted_at: DecimalU64(now),
            handoff_until: DecimalU64(deadline(&input, context)),
            input,
            verification: None,
            consumed_at: None,
        };
        self.records
            .insert(record.input.operation_id.clone(), record.clone());
        Ok(record)
    }
    pub fn commit_checks(
        &self,
        id: &str,
        verification: &SignedApprovalVerificationV1,
        context: &ApprovalAdmissionContext,
        now: u64,
    ) -> Result<ExactSignatureChecks> {
        let record = self
            .records
            .get(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        check_current_context(&record.context, context)?;
        // A repeat returns historical evidence, not a new acceptance window.
        let checked_at = if let Some(original) = &record.verification {
            if original != verification {
                return Err(ApprovalAdmissionError::Conflict);
            }
            record
                .consumed_at
                .ok_or(ApprovalAdmissionError::InvalidPersistence)?
                .0
        } else {
            now
        };
        validate_verification(record, verification, context, checked_at)?;
        signature_checks(
            &record.input,
            &record.verify_nonce,
            context,
            Some(verification),
        )
    }
    pub fn commit(
        &mut self,
        id: &str,
        verification: SignedApprovalVerificationV1,
        context: &ApprovalAdmissionContext,
        now: u64,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<ApprovalAdmissionRecord> {
        let checks = self.commit_checks(id, &verification, context, now)?;
        verifier.verify(&checks)?;
        let record = self
            .records
            .get_mut(id)
            .ok_or(ApprovalAdmissionError::Missing)?;
        if record.verification.is_none() {
            record.handoff_until =
                DecimalU64(record.handoff_until.0.min(deadline(&record.input, context)));
            record.context = context.clone();
            record.verification = Some(verification);
            record.consumed_at = Some(DecimalU64(now));
        }
        Ok(record.clone())
    }
}

fn bounded(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}
fn canonical_digest<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, value))
        .map(|v| bytes_digest(&v))
        .map_err(|_| ApprovalAdmissionError::Invalid)
}
fn input_digest(input: &ApprovalAdmissionInput) -> Result<String> {
    canonical_digest("oasis7.custody.admission-body.v1", input)
}
fn live_context(context: &ApprovalAdmissionContext) -> Result<()> {
    if context.frozen {
        return Err(ApprovalAdmissionError::Frozen);
    }
    if context
        .fence
        .check_epoch(context.authority.service_epoch.0)
        .is_err()
    {
        return Err(ApprovalAdmissionError::StaleContext);
    }
    Ok(())
}
fn check_current_context(
    original: &ApprovalAdmissionContext,
    current: &ApprovalAdmissionContext,
) -> Result<()> {
    live_context(current)?;
    let mut tightened = original.clone();
    if current.handoff_until.0 > original.handoff_until.0
        || current.source.session_expires_at.0 > original.source.session_expires_at.0
        || current.source.session_idle_expires_at.0 > original.source.session_idle_expires_at.0
    {
        return Err(ApprovalAdmissionError::StaleContext);
    }
    tightened.handoff_until = current.handoff_until;
    tightened.source.session_expires_at = current.source.session_expires_at;
    tightened.source.session_idle_expires_at = current.source.session_idle_expires_at;
    if &tightened != current {
        return Err(ApprovalAdmissionError::StaleContext);
    }
    Ok(())
}
fn deadline(input: &ApprovalAdmissionInput, context: &ApprovalAdmissionContext) -> u64 {
    let plan_deadline = match &input.request.intent {
        CustodyIntentV1::Provision { deadline, .. }
        | CustodyIntentV1::ManagedLogin { deadline, .. } => deadline.0,
        CustodyIntentV1::TestSession { write_deadline, .. } => write_deadline.0,
    };
    [
        plan_deadline,
        input.approval.proof.expires_at.0,
        context.handoff_until.0,
        context.source.session_expires_at.0,
        context.source.session_idle_expires_at.0,
    ]
    .into_iter()
    .min()
    .unwrap_or(0)
}
fn validate_input(
    input: &ApprovalAdmissionInput,
    context: &ApprovalAdmissionContext,
    now: u64,
) -> Result<()> {
    live_context(context)?;
    let p = &input.approval.proof;
    let s = &context.source;
    if !bounded(&input.operation_id)
        || !bounded(&p.approval_id)
        || !bounded(&p.nonce)
        || p.version != 1
        || !bounded(&context.identity_namespace)
        || context.identity_instance_epoch.0 == 0
        || [
            &s.manager_account_id,
            &s.manager_session_ref,
            &s.origin_credential_id,
            &s.passkey_credential_id,
        ]
        .iter()
        .any(|v| !bounded(v))
        || [
            s.origin_credential_version,
            s.passkey_credential_version,
            s.manager_policy_version,
            s.manager_security_version,
        ]
        .iter()
        .any(|v| v.0 == 0)
    {
        return Err(ApprovalAdmissionError::Invalid);
    }
    if p.issued_at.0 > now
        || p.expires_at
            .0
            .checked_sub(p.issued_at.0)
            .is_none_or(|ttl| ttl == 0 || ttl > 30_000)
        || now >= deadline(input, context)
    {
        return Err(ApprovalAdmissionError::Expired);
    }
    input
        .request
        .validate(p.issued_at.0)
        .map_err(|_| ApprovalAdmissionError::Invalid)?;
    input
        .request
        .validate(now)
        .map_err(|_| ApprovalAdmissionError::Expired)?;
    if p.context != context.authority
        || input.request.context != context.authority
        || p.identity_namespace != context.identity_namespace
        || p.identity_instance_epoch != context.identity_instance_epoch
        || !context.allowed_managers.contains(&p.manager_account_id)
        || p.approval_request_id != input.request.approval_request_id
        || p.intent_digest
            != input
                .request
                .intent_digest()
                .map_err(|_| ApprovalAdmissionError::Invalid)?
        || p.manager_account_id != s.manager_account_id
        || p.manager_session_ref != s.manager_session_ref
        || p.origin_credential_id != s.origin_credential_id
        || p.origin_credential_version != s.origin_credential_version
        || p.passkey_credential_id != s.passkey_credential_id
        || p.passkey_credential_version != s.passkey_credential_version
        || p.manager_policy_version != s.manager_policy_version
        || p.manager_security_version != s.manager_security_version
    {
        return Err(ApprovalAdmissionError::BindingMismatch);
    }
    Ok(())
}
fn validate_verification(
    record: &ApprovalAdmissionRecord,
    signed: &SignedApprovalVerificationV1,
    context: &ApprovalAdmissionContext,
    now: u64,
) -> Result<()> {
    validate_input(&record.input, context, now)?;
    if now >= record.handoff_until.0 {
        return Err(ApprovalAdmissionError::Expired);
    }
    let v = &signed.verification;
    let p = &record.input.approval.proof;
    let s = &context.source;
    if v.version != 1
        || v.purpose != VerificationPurposeV1::ConsumeApproval
        || v.proof_state != ProofStateV1::Active
        || v.origin_state != OriginStateV1::Active
        || v.identity_namespace != context.identity_namespace
        || v.proof_digest
            != record
                .input
                .approval
                .digest()
                .map_err(|_| ApprovalAdmissionError::Invalid)?
        || v.approval_request_id != p.approval_request_id
        || v.audience != p.context.audience
        || v.intent_digest != p.intent_digest
        || v.verify_nonce != record.verify_nonce
        || v.proof_expires_at != p.expires_at
        || v.identity_instance_epoch != context.identity_instance_epoch
        || v.manager_security_version != s.manager_security_version
        || v.manager_policy_version != s.manager_policy_version
        || v.credential_version != s.passkey_credential_version
        || v.origin_credential_version != s.origin_credential_version
        || v.source_session_expires_at != s.session_expires_at
        || v.source_session_idle_expires_at != s.session_idle_expires_at
        || v.observed_at.0 < record.admitted_at.0
        || v.observed_at.0 > now
    {
        return Err(ApprovalAdmissionError::BindingMismatch);
    }
    Ok(())
}
fn pinned_check(
    context: &ApprovalAdmissionContext,
    key_id: &str,
    signature: &str,
    canonical_bytes: Vec<u8>,
) -> Result<SignatureCheck> {
    let key = context
        .native_key_pins
        .get(key_id)
        .ok_or(ApprovalAdmissionError::UnknownKey)?;
    if !bounded(key_id)
        || key.len() != 64
        || !key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || !bounded(signature)
    {
        return Err(ApprovalAdmissionError::Invalid);
    }
    Ok(SignatureCheck {
        key_id: key_id.into(),
        public_key_hex: key.clone(),
        signature_base64url: signature.into(),
        canonical_bytes,
    })
}
fn signature_checks(
    input: &ApprovalAdmissionInput,
    nonce: &str,
    context: &ApprovalAdmissionContext,
    verification: Option<&SignedApprovalVerificationV1>,
) -> Result<ExactSignatureChecks> {
    let mut checks = vec![pinned_check(
        context,
        &input.approval.proof.key_id,
        &input.approval.signature,
        input
            .approval
            .proof
            .signing_bytes()
            .map_err(|_| ApprovalAdmissionError::Invalid)?,
    )?];
    if let Some(v) = verification {
        checks.push(pinned_check(
            context,
            &v.verification.key_id,
            &v.signature,
            v.verification
                .signing_bytes()
                .map_err(|_| ApprovalAdmissionError::Invalid)?,
        )?);
    }
    Ok(ExactSignatureChecks {
        operation_id: input.operation_id.clone(),
        body_digest: input_digest(input)?,
        context_digest: canonical_digest("oasis7.custody.admission-context.v1", context)?,
        verify_nonce: nonce.into(),
        verification_digest: verification
            .map(|v| canonical_digest("oasis7.custody.admission-verification.v1", v))
            .transpose()?,
        checks,
    })
}

/// Internal factory for an already consumed, registered approval source. It does
/// not validate freshness or authorize dispatch; the provisioning state binds
/// the exact persisted inspection and checks the verification before use.
pub(crate) fn registered_origin_checks(
    input: &ApprovalAdmissionInput,
    operation_id: &str,
    nonce: &str,
    context: &ApprovalAdmissionContext,
    signed: &SignedApprovalVerificationV1,
) -> Result<ExactSignatureChecks> {
    let mut checks = signature_checks(input, nonce, context, Some(signed))?;
    checks.operation_id = operation_id.into();
    checks.body_digest = canonical_digest(
        "oasis7.custody.registered-origin-inspection.v1",
        &(operation_id, nonce, input),
    )?;
    Ok(checks)
}

#[path = "native_fact_checks.rs"]
mod native_fact_checks;
pub(crate) use native_fact_checks::*;

#[path = "participant_signature_checks.rs"]
mod participant_signature_checks;
pub(crate) use participant_signature_checks::*;
