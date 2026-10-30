//! Private Host state. Shape validation is never deployment acceptance or a grant.
use oasis7_custody_core::http_pop::HttpPopProfileV1;
use oasis7_custody_core::{
    DecimalU64,
    client_enrollment::{
        ClientEnrollmentCreateRequestV1, ClientEnrollmentCreateResponseV1,
        ClientEnrollmentProofRequestV1, ClientPairingBindingV1, HostDeploymentDescriptorV1,
        HostIsolationMechanismV1, HostTransportProfileV1, PairingInviteTargetRefV1,
    },
    provisioning::{
        SessionControlAckV1, SessionControlKindV1, SessionPurposeV1, TestSessionPlanV1,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
fn required_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}
use zeroize::Zeroize;

pub(super) fn denied() -> String {
    "custody Host unavailable; operator action required".into()
}
pub(super) fn id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 256
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}
pub(super) fn digest<T: Serialize>(domain: &str, value: &T) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let b = oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|_| denied())?;
    Ok(hex::encode(Sha256::digest(b)))
}
pub(crate) fn origin(v: &str) -> Result<(), String> {
    use oasis7_custody_core::http_pop::{CanonicalHttpTarget, HttpPopRoute};
    CanonicalHttpTarget::new(v, HttpPopRoute::ProofChallenges).map_err(|_| denied())?;
    Ok(())
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum NativePlacement {
    Local { uid: u32 },
    Remote { https_origin: String },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Isolation {
    pub host_uid: u32,
    pub model_uid: u32,
    pub native: NativePlacement,
}
impl Isolation {
    pub fn validate(&self) -> Result<(), String> {
        if self.host_uid == 0 || self.model_uid == 0 || self.host_uid == self.model_uid {
            return Err(denied());
        }
        match &self.native {
            NativePlacement::Local { uid }
                if *uid != 0 && *uid != self.host_uid && *uid != self.model_uid =>
            {
                Ok(())
            }
            NativePlacement::Remote { https_origin } => origin(https_origin),
            _ => Err(denied()),
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IdentityBinding {
    pub api_version: u8,
    pub enabled: bool,
    pub service_origin: String,
    pub skill_manifest_path: String,
    pub descriptor: HostDeploymentDescriptorV1,
    pub target: PairingInviteTargetRefV1,
    pub isolation: Isolation,
}
impl IdentityBinding {
    pub fn validate(&self) -> Result<(), String> {
        origin(&self.service_origin)?;
        if !std::path::Path::new(&self.skill_manifest_path).is_absolute()
            || self.skill_manifest_path.len() > 4096
        {
            return Err(denied());
        }
        self.descriptor.validate().map_err(|_| denied())?;
        self.target.validate().map_err(|_| denied())?;
        self.isolation.validate()?;
        if self.api_version != 1
            || !self.enabled
            || self.descriptor.transport_profile != HostTransportProfileV1::NativeBrokerCustodyV1
            || self.descriptor.isolation_mechanism != HostIsolationMechanismV1::SeparateOsPrincipal
        {
            return Err(denied());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        digest("oasis7.custody.host-identity.v1", self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionBinding {
    pub session_id: String,
    pub resource_id: String,
    pub pairing: ClientPairingBindingV1,
    pub plan: TestSessionPlanV1,
}
impl SessionBinding {
    pub fn validate(
        &self,
        host: &IdentityBinding,
        enrollment: &ClientEnrollmentCreateResponseV1,
    ) -> Result<(), String> {
        self.pairing.validate().map_err(|_| denied())?;
        self.plan.validate().map_err(|_| denied())?;
        if !id(&self.session_id)
            || !id(&self.resource_id)
            || self.pairing.target != host.target
            || self.pairing.descriptor_digest != host.descriptor.digest().map_err(|_| denied())?
            || self.pairing.client_public_key != host.descriptor.client_public_key
            || self.pairing.host_instance_id != host.descriptor.host_instance_id
            || self.pairing.client_key_id != enrollment.key_id
            || self.pairing.enrollment_id != enrollment.enrollment_id
            || self.pairing.connector_public_key.is_some()
            || self.plan.enrollment_id != enrollment.enrollment_id
            || self.plan.write_deadline.get() == 0
            || self.plan.read_until.get() < self.plan.write_deadline.get()
            || self.plan.cleanup_until.get() < self.plan.write_deadline.get()
            || !(1..=80).contains(&self.plan.control_budget.requests.0)
            || !(1..=240).contains(&self.plan.control_budget.transport_per_hop.0)
            || !(8..=160).contains(&self.plan.control_budget.identity_transport.0)
            || self.plan.control_budget.transport_hops != 1
        {
            return Err(denied());
        }
        if serde_json::to_vec(self).map_err(|_| denied())?.len() > 32 * 1024 {
            return Err(denied());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        digest("oasis7.custody.host-session.v1", self)
    }
    pub fn deadline(&self, p: SessionPurposeV1) -> Result<u64, String> {
        match p {
            SessionPurposeV1::Execution => Ok(self.plan.write_deadline.get()),
            SessionPurposeV1::ReconcileRead => Ok(self.plan.read_until.get()),
            _ => Err(denied()),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Enrollment {
    pub original: ClientEnrollmentCreateRequestV1,
    #[serde(deserialize_with = "required_option")]
    pub response: Option<ClientEnrollmentCreateResponseV1>,
    #[serde(deserialize_with = "required_option")]
    pub proof: Option<ClientEnrollmentProofRequestV1>,
    pub proved: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Attempt {
    pub id: String,
    pub original_id: String,
    pub at: u64,
    pub bootstrap: bool,
    pub retained: bool,
    pub target_digest: String,
    pub body_digest: String,
    #[serde(deserialize_with = "required_option")]
    pub profile: Option<HttpPopProfileV1>,
    #[serde(deserialize_with = "required_option")]
    pub purpose: Option<SessionPurposeV1>,
    #[serde(deserialize_with = "required_option")]
    pub generation: Option<DecimalU64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Credential {
    pub purpose: SessionPurposeV1,
    pub generation: DecimalU64,
    pub token: String,
    pub claim_request_id: String,
}
impl Drop for Credential {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
impl Credential {
    pub fn validate(&self) -> Result<(), String> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let b = zeroize::Zeroizing::new(URL_SAFE_NO_PAD.decode(&self.token).map_err(|_| denied())?);
        if b.len() != 32
            || self.token.len() != 43
            || URL_SAFE_NO_PAD.encode(&*b) != self.token
            || !(1..=4).contains(&self.generation.0)
            || self.purpose == SessionPurposeV1::TerminationPlan
        {
            return Err(denied());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Logical {
    pub digest: String,
    #[serde(deserialize_with = "required_option")]
    pub kind: Option<SessionControlKindV1>,
    pub created_at: u64,
    pub retained: bool,
    #[serde(deserialize_with = "required_option")]
    pub ack: Option<SessionControlAckV1>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Session {
    pub binding: SessionBinding,
    pub inbox_key: String,
    pub inbox_ready: bool,
    pub credentials: Vec<Credential>,
    pub retired_execution: u64,
    pub retired_read: u64,
    pub logicals: BTreeMap<String, Logical>,
    pub claims:
        BTreeMap<String, oasis7_custody_core::client_enrollment::ClientCredentialClaimRequestV1>,
    pub closing: bool,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.inbox_key.zeroize();
    }
}
impl Session {
    pub fn floor(&self, p: SessionPurposeV1) -> Result<u64, String> {
        match p {
            SessionPurposeV1::Execution => Ok(self.retired_execution),
            SessionPurposeV1::ReconcileRead => Ok(self.retired_read),
            _ => Err(denied()),
        }
    }
    pub fn credential(&self, p: SessionPurposeV1) -> Result<&Credential, String> {
        self.credentials
            .iter()
            .find(|c| c.purpose == p && c.generation.0 > self.floor(p).unwrap_or(u64::MAX))
            .ok_or_else(denied)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub api_version: u8,
    pub identity: IdentityBinding,
    pub signing_seed: String,
    pub enrollment: Enrollment,
    #[serde(deserialize_with = "required_option")]
    pub session: Option<Session>,
    pub attempts: Vec<Attempt>,
    pub time_floor: u64,
}
impl Drop for State {
    fn drop(&mut self) {
        self.signing_seed.zeroize();
    }
}
#[path = "custody_host_restore.rs"]
mod restore;
