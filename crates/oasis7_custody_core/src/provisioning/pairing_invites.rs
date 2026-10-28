//! Account-local metadata only. Invites never create a grant or spend a purpose budget.
use super::*;
use crate::{
    client_enrollment::{HostTransportProfileV1, PairingInviteTargetRefV1, ProvedEnrollmentFactV1},
    time_units::UnixMillis,
};
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingInviteCreateRequestV1 {
    pub api_version: u8,
    pub request_id: String,
    pub transport_profile: HostTransportProfileV1,
    pub tool_profile: String,
}
impl PairingInviteCreateRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || !valid_id(&self.request_id)
            || self.tool_profile != "custody-native-v1"
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingInviteOriginalV1 {
    pub api_version: u8,
    pub domain: String,
    pub environment: String,
    pub service_epoch: DecimalU64,
    pub custody_account_id: String,
    pub provision_id: String,
    pub invite_id: String,
    pub issued_at: UnixMillis,
    pub expires_at: UnixMillis,
    pub transport_profile: HostTransportProfileV1,
    pub tool_profile: String,
}
impl PairingInviteOriginalV1 {
    pub fn digest(&self) -> Result<String> {
        let bytes =
            oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.pairing-invite.v1", self))
                .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        Ok(crate::approval::bytes_digest(&bytes))
    }
    pub fn target(&self) -> Result<PairingInviteTargetRefV1> {
        Ok(PairingInviteTargetRefV1 {
            custody_account_id: self.custody_account_id.clone(),
            provision_id: self.provision_id.clone(),
            invite_id: self.invite_id.clone(),
            invite_digest: self.digest()?,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingInviteRecordV1 {
    pub request: PairingInviteCreateRequestV1,
    pub original: PairingInviteOriginalV1,
    pub consumed_session: Option<String>,
}
/// Fixed private Enrollment namespace observation, not client-supplied JSON.
pub struct ProvedEnrollmentObservation<'a> {
    pub actual_actor_id: &'a str,
    pub expected_actor_id: &'a str,
    pub actual_realm: &'a str,
    pub expected_realm: &'a str,
    pub expected_public_origin: &'a str,
    pub fact: &'a ProvedEnrollmentFactV1,
}
impl ProvedEnrollmentObservation<'_> {
    pub fn validate_physical(&self) -> Result<()> {
        let r = &self.fact.registration;
        if self.actual_actor_id.is_empty()
            || self.actual_actor_id != self.expected_actor_id
            || self.actual_actor_id != r.physical_actor_id
            || self.actual_realm.is_empty()
            || self.actual_realm != self.expected_realm
            || self.actual_realm != r.realm
            || r.public_origin != self.expected_public_origin
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        self.fact
            .checks()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    pub fn pairing_invites(&self) -> &BTreeMap<String, PairingInviteRecordV1> {
        &self.pairing_invites
    }
    pub fn create_pairing_invite(
        &mut self,
        request: PairingInviteCreateRequestV1,
        server_invite_id: &str,
        current: &OwnerSetupManagementCurrent<'_>,
        now: UnixMillis,
    ) -> Result<PairingInviteOriginalV1> {
        request.validate()?;
        self.owner_setup_basis(current, now.get())?;
        if let Some(old) = self.pairing_invites.get(&request.request_id) {
            return if old.request == request {
                Ok(old.original.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if !valid_id(server_invite_id)
            || self
                .pairing_invites
                .values()
                .any(|v| v.original.invite_id == server_invite_id)
            || self.pairing_invites.len() >= 256
            || self
                .pairing_invites
                .values()
                .filter(|v| v.consumed_session.is_none() && now.get() < v.original.expires_at.get())
                .count()
                >= 8
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let original = PairingInviteOriginalV1 {
            api_version: 1,
            domain: "oasis7-custody-pairing-invite-v1".into(),
            environment: self.draft.entry.environment.clone(),
            service_epoch: current.authority.service_epoch,
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            invite_id: server_invite_id.into(),
            issued_at: now,
            expires_at: UnixMillis::new(
                now.get()
                    .checked_add(600_000)
                    .ok_or(ProvisionApprovalError::BindingMismatch)?,
            ),
            transport_profile: request.transport_profile,
            tool_profile: request.tool_profile.clone(),
        };
        let mut next = self.clone();
        next.pairing_invites.insert(
            request.request_id.clone(),
            PairingInviteRecordV1 {
                request,
                original: original.clone(),
                consumed_session: None,
            },
        );
        next.validate_persistence()?;
        *self = next;
        Ok(original)
    }
    pub fn pairing_invite(
        &self,
        target: &PairingInviteTargetRefV1,
    ) -> Result<&PairingInviteRecordV1> {
        target
            .validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        let record = self
            .pairing_invites
            .values()
            .find(|v| v.original.invite_id == target.invite_id)
            .ok_or(ProvisionApprovalError::BindingMismatch)?;
        if record.original.target()? != *target {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(record)
    }
    pub(in crate::provisioning) fn validate_pairing_invite_persistence(&self) -> Result<()> {
        if self.pairing_invites.len() > 256 {
            return Err(ProvisionApprovalError::InvalidPersistence);
        }
        let mut ids = std::collections::BTreeSet::new();
        for (id, r) in &self.pairing_invites {
            r.request.validate()?;
            let o = &r.original;
            if id != &r.request.request_id
                || o.api_version != 1
                || o.domain != "oasis7-custody-pairing-invite-v1"
                || o.environment != self.draft.entry.environment
                || o.custody_account_id != self.draft.entry.custody_account_id
                || o.provision_id != self.draft.entry.provision_id
                || o.service_epoch.0 == 0
                || !valid_id(&o.invite_id)
                || !ids.insert(o.invite_id.clone())
                || o.transport_profile != r.request.transport_profile
                || o.tool_profile != r.request.tool_profile
                || o.expires_at.get()
                    != o.issued_at
                        .get()
                        .checked_add(600_000)
                        .ok_or(ProvisionApprovalError::InvalidPersistence)?
                || r.consumed_session.as_ref().is_some_and(|v| !valid_id(v))
            {
                return Err(ProvisionApprovalError::InvalidPersistence);
            }
        }
        Ok(())
    }
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}
