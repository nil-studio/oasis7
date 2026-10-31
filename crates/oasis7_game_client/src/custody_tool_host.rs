//! Control-only Host. No Executor store, Native SID, business tools or raw signer API.
#[path = "custody_host_bootstrap.rs"]
mod bootstrap;
#[path = "custody_host_cli.rs"]
pub(crate) mod cli;
#[path = "custody_host_control.rs"]
mod control;
#[path = "custody_host_ipc.rs"]
pub(crate) mod ipc;
#[path = "custody_host_registry.rs"]
mod registry;
#[path = "custody_host_reply.rs"]
mod reply;
#[path = "custody_host_state.rs"]
pub(crate) mod state;
#[cfg(test)]
#[path = "custody_host_tests.rs"]
mod tests;
#[path = "custody_host_vault.rs"]
pub(crate) mod vault;
use oasis7_custody_core::{
    DecimalU64,
    http_pop::{CanonicalHttpTarget, HttpPopProfileV1, HttpPopRoute, HttpPopSigningInput},
    provisioning::SessionPurposeV1,
    time_units::{UnixMillis, http_proof_window},
};
use state::*;
use zeroize::Zeroizing;
pub(crate) struct Host {
    vault: vault::Vault,
    transport: super::custody_transport::CustodyTransport,
}
pub(super) struct SignedSend<'a> {
    route: HttpPopRoute,
    profile: HttpPopProfileV1,
    request_id: &'a str,
    nonce: Option<&'a str>,
    body: &'a [u8],
    purpose: Option<SessionPurposeV1>,
    expected_generation: Option<DecimalU64>,
    deadline: u64,
}
impl Host {
    fn open(vault: vault::Vault) -> Result<Self, String> {
        let origin = vault.read(|s| {
            ipc::verify_runtime_pins(&s.identity)?;
            Ok(s.identity.service_origin.clone())
        })?;
        Ok(Self {
            vault,
            transport: super::custody_transport::CustodyTransport::new(&origin)?,
        })
    }
    fn scoped(&self, route: HttpPopRoute) -> Result<HttpPopRoute, String> {
        self.vault.read(|s| {
            Ok(HttpPopRoute::ScopedClient {
                provision_id: s.identity.target.provision_id.clone(),
                custody_account_id: s.identity.target.custody_account_id.clone(),
                route: Box::new(route),
            })
        })
    }
    fn target(&self, route: HttpPopRoute) -> Result<CanonicalHttpTarget, String> {
        self.vault.read(|s| {
            CanonicalHttpTarget::new(&s.identity.service_origin, route).map_err(|_| denied())
        })
    }
    fn signed(
        &self,
        q: SignedSend<'_>,
    ) -> Result<super::custody_transport::PrivateTransportReply, String> {
        let (key, key_id, binding_id, token, generation) = self.vault.read(|s| {
            let seed = vault::secret32(&s.signing_seed)?;
            let key = ed25519_dalek::SigningKey::from_bytes(&seed);
            let e = s.enrollment.response.as_ref().ok_or_else(denied)?;
            if let Some(p) = q.purpose {
                let session = s.session.as_ref().ok_or_else(denied)?;
                let private = matches!(
                    q.profile,
                    HttpPopProfileV1::Custody | HttpPopProfileV1::CustodyChallenge
                );
                let credential = if private {
                    Some(session.credential(p)?)
                } else {
                    None
                };
                let generation = credential.map(|c| c.generation);
                if q.expected_generation.is_some() && generation != q.expected_generation && private
                {
                    return Err(denied());
                }
                Ok((
                    key,
                    e.key_id.clone(),
                    session.binding.session_id.clone(),
                    credential.map(|c| Zeroizing::new(c.token.clone())),
                    generation.or(q.expected_generation),
                ))
            } else {
                if q.profile != HttpPopProfileV1::EnrollmentProof {
                    return Err(denied());
                }
                Ok((key, e.key_id.clone(), e.enrollment_id.clone(), None, None))
            }
        })?;
        let target = self.target(q.route)?;
        let (created, expires) =
            http_proof_window(UnixMillis::new(vault::now()?), UnixMillis::new(q.deadline))
                .map_err(|_| denied())?;
        let attempt = self.vault.reserve(vault::Reservation {
            original_id: q.request_id,
            uri: target.uri(),
            body: q.body,
            profile: Some(q.profile),
            purpose: q.purpose,
            generation,
            bootstrap: q.purpose.is_none(),
        })?;
        if q.purpose.is_some() {
            self.vault.retain(q.request_id, &target, q.body)?;
            self.vault.mark_retained(&attempt)?;
        }
        self.transport.send_once(
            &HttpPopSigningInput {
                profile: q.profile,
                target: &target,
                key_id: &key_id,
                binding_id: &binding_id,
                request_id: q.request_id,
                nonce: q.nonce,
                token: token.as_deref().map(|s| s.as_str()),
                created,
                expires,
                body: q.body,
            },
            &key,
        )
    }
}
