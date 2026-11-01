//! Dedicated N peer forwards only a closed signed reservation with H's own PoP.
use super::*;
use oasis7_custody_core::{
    native_model_proxy_query_v2::SignedNativeModelReservationQueryV2,
    provisioning::ModelBudgetApprovalContractV1,
};
use serde::{Deserialize, Serialize};
use std::os::unix::net::UnixStream;
pub(super) struct NativePeer {
    uid: u32,
    binding: ipc::PublicBinding,
}
impl NativePeer {
    pub(super) fn observe(
        stream: &UnixStream,
        identity: &IdentityBinding,
        binding: &ipc::PublicBinding,
    ) -> Result<Self, String> {
        let NativePlacement::Local { uid } = &identity.isolation.native else {
            return Err(denied());
        };
        if *uid == 0
            || *uid == identity.isolation.host_uid
            || *uid == identity.isolation.model_uid
            || ipc::peer_uid(stream)? != *uid
            || vault::actual_uid() != identity.isolation.host_uid
        {
            return Err(denied());
        }
        ipc::verify_runtime_pins(identity)?;
        if binding.identity_digest != identity.digest()? || binding.isolation != identity.isolation
        {
            return Err(denied());
        }
        registry::check_native(binding)?;
        Ok(Self {
            uid: *uid,
            binding: binding.clone(),
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PendingStatus {
    PaidPending,
    ExactHistory,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingReply {
    pub api_version: u8,
    pub domain: String,
    pub session_id: String,
    pub reservation_id: String,
    pub query_digest: String,
    pub status: PendingStatus,
}
impl PendingReply {
    pub(super) fn validate(&self, q: &SignedNativeModelReservationQueryV2) -> Result<(), String> {
        if self.api_version != 2
            || self.domain != "oasis7.custody.native-model-reservation-pending.v2"
            || self.session_id != q.query.session_id
            || self.reservation_id != q.query.reservation_id
            || self.query_digest
                != oasis7_custody_core::approval::bytes_digest(
                    &serde_json::to_vec(q).map_err(|_| denied())?,
                )
        {
            return Err(denied());
        }
        Ok(())
    }
}
impl Host {
    pub(super) fn reserve_native_model(
        &self,
        peer: NativePeer,
        q: &SignedNativeModelReservationQueryV2,
    ) -> Result<PendingReply, String> {
        registry::check_native(&peer.binding)?;
        let (session_id, generation, deadline) = self.vault.read(|s| {
            let NativePlacement::Local { uid } = &s.identity.isolation.native else {
                return Err(denied());
            };
            let session = s.session.as_ref().ok_or_else(denied)?;
            let HostSessionPlan::V2(plan) = &session.binding.plan else {
                return Err(denied());
            };
            let budget = plan.model_budget.as_ref().ok_or_else(denied)?;
            let now = vault::now()?;
            if peer.uid != *uid
                || peer.binding.identity_digest != s.identity.digest()?
                || peer.binding.session_digest != session.binding.digest()?
                || session.closing
                || budget.approval_contract
                    != ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
                || q.query.session_id != session.binding.session_id
                || q.query.provision_id != s.identity.target.provision_id
                || q.query.custody_account_id != s.identity.target.custody_account_id
                || q.query.budget_digest
                    != budget.digest(plan.write_deadline.0).map_err(|_| denied())?
                || q.query.issued_at.0 > now
                || now >= q.query.admit_until.0
                || q.query.admit_until.0 > plan.write_deadline.0
                || q.query.admit_until.0 > budget.deadline.0
            {
                return Err(denied());
            }
            q.query
                .physical_graph
                .validate_budget(budget)
                .map_err(|_| denied())?;
            q.verify_instance_key(&budget.native_instance_public_key)
                .map_err(|_| denied())?;
            ipc::verify_runtime_pins(&s.identity)?;
            Ok((
                session.binding.session_id.clone(),
                session.credential(SessionPurposeV1::Execution)?.generation,
                plan.write_deadline.0.min(q.query.admit_until.0),
            ))
        })?;
        let body = serde_json::to_vec(q).map_err(|_| denied())?;
        registry::check_native(&peer.binding)?;
        let response = self.signed(SignedSend {
            route: self.scoped(HttpPopRoute::ModelReservations(session_id))?,
            profile: HttpPopProfileV1::NativeModelCustodyV2,
            request_id: &q.query.request_id,
            nonce: Some(&q.query.query_nonce),
            body: &body,
            purpose: Some(SessionPurposeV1::Execution),
            expected_generation: Some(generation),
            deadline,
        })?;
        if response.status != 200 {
            return Err(denied());
        }
        let reply: PendingReply = serde_json::from_slice(response.body()).map_err(|_| denied())?;
        reply.validate(q)?;
        Ok(reply)
    }
}
