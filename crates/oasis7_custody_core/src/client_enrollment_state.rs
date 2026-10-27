use super::*;
use crate::approval_admission::ExactSignatureChecks;
use crate::http_pop::{
    CanonicalHttpTarget, EnrollmentHttpEvidenceV1, HttpPopProfileV1, HttpPopRoute,
    HttpPopSigningInput, HttpPossessionReceipt,
};
use crate::time_units::UnixMillis;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentRegistrationV1 {
    pub api_version: u8,
    pub enrollment_id: String,
    pub key_id: String,
    pub create_original_bytes: String,
    pub request: ClientEnrollmentCreateRequestV1,
    pub descriptor_digest: String,
    pub server_nonce: String,
    pub created_at: UnixMillis,
    pub expires_at: UnixMillis,
    pub challenge_expires_at: UnixMillis,
    pub public_origin: String,
    pub physical_actor_id: String,
    pub realm: String,
}
/// Values are taken from the fixed binding/stub and actual State.id, never JSON.
pub struct EnrollmentPhysicalCurrent<'a> {
    pub actual_actor_id: &'a str,
    pub expected_actor_id: &'a str,
    pub actual_realm: &'a str,
    pub expected_realm: &'a str,
    pub public_origin: &'a str,
}
pub struct EnrollmentServerIds<'a> {
    pub enrollment_id: &'a str,
    pub key_id: &'a str,
    pub nonce: &'a str,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvedEnrollmentFactV1 {
    pub registration: EnrollmentRegistrationV1,
    pub prove_original_bytes: String,
    pub prove_request: ClientEnrollmentProofRequestV1,
    pub http_evidence: EnrollmentHttpEvidenceV1,
    pub proved_at: UnixMillis,
}
impl ProvedEnrollmentFactV1 {
    pub fn digest(&self) -> Result<String, EnrollmentError> {
        let b =
            oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.proved-enrollment.v1", self))
                .map_err(|_| EnrollmentError::Invalid)?;
        Ok(bytes_digest(&b))
    }
    pub fn checks(&self) -> Result<ExactSignatureChecks, EnrollmentError> {
        validate_registration(&self.registration)?;
        validate_prove(
            &self.registration,
            &self.prove_request,
            &self.prove_original_bytes,
            self.proved_at,
        )?;
        let r = &self.registration;
        let target = CanonicalHttpTarget::new(
            &r.public_origin,
            HttpPopRoute::EnrollmentProve(r.enrollment_id.clone()),
        )
        .map_err(|_| EnrollmentError::Invalid)?;
        crate::http_pop::enrollment_evidence_checks(
            &self.http_evidence,
            &HttpPopSigningInput {
                profile: HttpPopProfileV1::EnrollmentProof,
                target: &target,
                key_id: &r.key_id,
                binding_id: &r.enrollment_id,
                request_id: &self.prove_request.request_id,
                nonce: Some(&r.server_nonce),
                token: None,
                created: self.http_evidence.created,
                expires: self.http_evidence.expires,
                body: self.prove_original_bytes.as_bytes(),
            },
            &r.request.descriptor.client_public_key,
            self.proved_at,
            r.challenge_expires_at,
        )
        .map_err(|_| EnrollmentError::Invalid)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawEnrollmentState")]
pub struct ClientEnrollmentStateV1 {
    registration: EnrollmentRegistrationV1,
    proved: Option<ProvedEnrollmentFactV1>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEnrollmentState {
    registration: EnrollmentRegistrationV1,
    proved: Option<ProvedEnrollmentFactV1>,
}
impl TryFrom<RawEnrollmentState> for ClientEnrollmentStateV1 {
    type Error = EnrollmentError;
    fn try_from(r: RawEnrollmentState) -> Result<Self, EnrollmentError> {
        validate_registration(&r.registration)?;
        if let Some(p) = &r.proved {
            if p.registration != r.registration {
                return Err(EnrollmentError::Invalid);
            }
            p.checks()?;
        }
        Ok(Self {
            registration: r.registration,
            proved: r.proved,
        })
    }
}
impl ClientEnrollmentStateV1 {
    pub fn new(
        original_bytes: &[u8],
        ids: EnrollmentServerIds<'_>,
        current: &EnrollmentPhysicalCurrent<'_>,
        now: UnixMillis,
    ) -> Result<Self, EnrollmentError> {
        physical(current)?;
        if original_bytes.len() > 32768
            || !id(ids.enrollment_id)
            || !id(ids.key_id)
            || !enrollment_nonce(ids.nonce)
        {
            return Err(EnrollmentError::Invalid);
        }
        let request: ClientEnrollmentCreateRequestV1 =
            serde_json::from_slice(original_bytes).map_err(|_| EnrollmentError::Invalid)?;
        request.validate()?;
        let r = EnrollmentRegistrationV1 {
            api_version: 1,
            enrollment_id: ids.enrollment_id.into(),
            key_id: ids.key_id.into(),
            create_original_bytes: std::str::from_utf8(original_bytes)
                .map_err(|_| EnrollmentError::Invalid)?
                .into(),
            descriptor_digest: request.descriptor.digest()?,
            request,
            server_nonce: ids.nonce.into(),
            created_at: now,
            expires_at: UnixMillis::new(
                now.get()
                    .checked_add(600_000)
                    .ok_or(EnrollmentError::Invalid)?,
            ),
            challenge_expires_at: UnixMillis::new(
                now.get()
                    .checked_add(30_000)
                    .ok_or(EnrollmentError::Invalid)?,
            ),
            public_origin: current.public_origin.into(),
            physical_actor_id: current.actual_actor_id.into(),
            realm: current.actual_realm.into(),
        };
        validate_registration(&r)?;
        Ok(Self {
            registration: r,
            proved: None,
        })
    }
    pub fn registration(&self) -> &EnrollmentRegistrationV1 {
        &self.registration
    }
    pub fn proved(&self) -> Option<&ProvedEnrollmentFactV1> {
        self.proved.as_ref()
    }
    /// Exact create retry is historical; it cannot replace the challenge/target.
    pub fn check_create_retry(
        &self,
        original_bytes: &[u8],
    ) -> Result<&EnrollmentRegistrationV1, EnrollmentError> {
        if original_bytes != self.registration.create_original_bytes.as_bytes() {
            return Err(EnrollmentError::Invalid);
        }
        Ok(&self.registration)
    }
    pub fn prove(
        &mut self,
        body: &[u8],
        receipt: HttpPossessionReceipt,
        current: &EnrollmentPhysicalCurrent<'_>,
        now: UnixMillis,
    ) -> Result<ProvedEnrollmentFactV1, EnrollmentError> {
        physical(current)?;
        if self.registration.physical_actor_id != current.actual_actor_id
            || self.registration.realm != current.actual_realm
            || self.registration.public_origin != current.public_origin
            || body.len() > 32768
        {
            return Err(EnrollmentError::Invalid);
        }
        let request: ClientEnrollmentProofRequestV1 =
            serde_json::from_slice(body).map_err(|_| EnrollmentError::Invalid)?;
        let text = std::str::from_utf8(body).map_err(|_| EnrollmentError::Invalid)?;
        validate_prove(&self.registration, &request, text, now)?;
        use sha2::Digest;
        let body_digest: [u8; 32] = sha2::Sha256::digest(body).into();
        let target = CanonicalHttpTarget::new(
            current.public_origin,
            HttpPopRoute::EnrollmentProve(self.registration.enrollment_id.clone()),
        )
        .map_err(|_| EnrollmentError::Invalid)?;
        if receipt.profile() != HttpPopProfileV1::EnrollmentProof
            || receipt.binding_id() != self.registration.enrollment_id
            || receipt.request_id() != request.request_id
            || receipt.key_id() != self.registration.key_id
            || receipt.public_key_hex() != self.registration.request.descriptor.client_public_key
            || receipt.nonce() != Some(self.registration.server_nonce.as_str())
            || receipt.target_uri() != target.uri()
            || receipt.body_digest() != &body_digest
            || receipt
                .expires()
                .checked_millis()
                .map_err(|_| EnrollmentError::Invalid)?
                .get()
                > self.registration.challenge_expires_at.get()
        {
            return Err(EnrollmentError::Invalid);
        }
        let evidence = receipt
            .enrollment_evidence()
            .map_err(|_| EnrollmentError::Invalid)?;
        if let Some(old) = &self.proved {
            return if old.prove_original_bytes == text && old.http_evidence == evidence {
                Ok(old.clone())
            } else {
                Err(EnrollmentError::Invalid)
            };
        }
        let fact = ProvedEnrollmentFactV1 {
            registration: self.registration.clone(),
            prove_original_bytes: text.into(),
            prove_request: request,
            http_evidence: evidence,
            proved_at: now,
        };
        fact.checks()?;
        self.proved = Some(fact.clone());
        Ok(fact)
    }
}
fn validate_registration(r: &EnrollmentRegistrationV1) -> Result<(), EnrollmentError> {
    r.request.validate()?;
    if r.api_version != 1
        || !id(&r.enrollment_id)
        || !id(&r.key_id)
        || !enrollment_nonce(&r.server_nonce)
        || r.create_original_bytes.len() > 32768
        || serde_json::from_str::<ClientEnrollmentCreateRequestV1>(&r.create_original_bytes)
            .map_err(|_| EnrollmentError::Invalid)?
            != r.request
        || r.descriptor_digest != r.request.descriptor.digest()?
        || r.expires_at.get()
            != r.created_at
                .get()
                .checked_add(600_000)
                .ok_or(EnrollmentError::Invalid)?
        || r.challenge_expires_at.get()
            != r.created_at
                .get()
                .checked_add(30_000)
                .ok_or(EnrollmentError::Invalid)?
        || !text(&r.physical_actor_id)
        || !text(&r.realm)
    {
        return Err(EnrollmentError::Invalid);
    }
    CanonicalHttpTarget::new(
        &r.public_origin,
        HttpPopRoute::EnrollmentProve(r.enrollment_id.clone()),
    )
    .map_err(|_| EnrollmentError::Invalid)?;
    Ok(())
}
fn validate_prove(
    r: &EnrollmentRegistrationV1,
    p: &ClientEnrollmentProofRequestV1,
    body: &str,
    now: UnixMillis,
) -> Result<(), EnrollmentError> {
    p.validate()?;
    if body.len() > 32768
        || serde_json::from_str::<ClientEnrollmentProofRequestV1>(body)
            .map_err(|_| EnrollmentError::Invalid)?
            != *p
        || p.enrollment_id != r.enrollment_id
        || p.target != r.request.target
        || p.descriptor_digest != r.descriptor_digest
        || p.client_public_key != r.request.descriptor.client_public_key
        || p.server_nonce != r.server_nonce
        || p.challenge_expires_at != r.challenge_expires_at
        || now.get() < r.created_at.get()
        || now.get() >= r.challenge_expires_at.get()
        || now.get() >= r.expires_at.get()
    {
        return Err(EnrollmentError::Invalid);
    }
    Ok(())
}
fn physical(c: &EnrollmentPhysicalCurrent<'_>) -> Result<(), EnrollmentError> {
    if !text(c.actual_actor_id)
        || c.actual_actor_id != c.expected_actor_id
        || !text(c.actual_realm)
        || c.actual_realm != c.expected_realm
    {
        return Err(EnrollmentError::Invalid);
    }
    Ok(())
}
