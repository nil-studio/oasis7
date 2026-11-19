//! Read-only, independently authenticated endpoint diagnostics for the private owner.
use super::FormalLiveRuntime;
use oasis7_distfs::controlled_authority::{
    formal::{FormalHeadRefV1, transport::RemoteFormalEndpoint},
    replicated_protocol::EndpointRole,
};
use oasis7_client_api::world_service::WorldIdentity;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ProtectedPairObservation {
    api_version: String,
    pub(crate) world: WorldIdentity,
    pub(crate) node_id: String,
    pub(crate) service_public_key: String,
    pub(crate) verified_published_cut: PublishedCutObservation,
    pub(crate) primary: EndpointObservation,
    pub(crate) replica: EndpointObservation,
    pub(crate) both_heads_match_unchanged_cut: bool,
    current_writer_qualification: WriterQualification,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WriterQualification {
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PublishedCutObservation {
    pub(crate) head: FormalHeadRefV1,
    pub(crate) finalizing_authority_id: String,
    active_authority_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum EndpointObservation {
    Authenticated {
        head: FormalHeadRefV1,
        instance_digest: String,
        checkpoint_revision: u64,
        epoch_floor: u64,
        sealed: bool,
        native_request_pending: bool,
    },
    Unavailable,
}

impl EndpointObservation {
    fn matches(&self, head: &FormalHeadRefV1) -> bool {
        matches!(self, Self::Authenticated { head: observed, .. } if observed == head)
    }
}

impl FormalLiveRuntime {
    pub(crate) fn observe_protected_pair(
        &self,
        budget: Duration,
    ) -> Result<ProtectedPairObservation, String> {
        if !self.started || budget.is_zero() {
            return Err("protected observation requires an active owner and work budget".into());
        }
        let deadline = Instant::now() + budget.min(Duration::from_secs(2));
        let (cut, authority) = self.published.observe(|cut| {
            let committed = cut.anchor.head();
            Ok((
                PublishedCutObservation {
                    head: committed.head(),
                    finalizing_authority_id: committed.finalizing_authority_id().to_owned(),
                    active_authority_id: cut.anchor.authority().authority_id()
                        .map_err(|_| "published authority identity unavailable")?,
                },
                cut.anchor.authority().clone(),
            ))
        })?;
        // Never hold the publication, admission or Node lock across network I/O.
        // The two role-bound reads share an absolute deadline and no credentials
        // or unverified transport error strings enter the operator result.
        let probe = |address, role| {
            let observation = RemoteFormalEndpoint::new(
                address,
                self.validated.timeout,
                authority.clone(),
                role,
                self.validated.writer_key.clone(),
                cut.head.clone(),
            )
            .and_then(|mut endpoint| {
                endpoint.set_operation_deadline(deadline);
                endpoint.observe_authority()
            });
            match observation {
                Ok(observed) => EndpointObservation::Authenticated {
                    head: observed.head().clone(),
                    instance_digest: observed.instance_digest().to_owned(),
                    checkpoint_revision: observed.checkpoint().revision,
                    epoch_floor: observed.epoch_floor(),
                    sealed: observed.seal_digest().is_some(),
                    native_request_pending: observed.native_request_pending(),
                },
                Err(_) => EndpointObservation::Unavailable,
            }
        };
        let primary = probe(self.validated.config.primary, EndpointRole::Primary);
        let replica = probe(self.validated.config.replica, EndpointRole::Replica);
        let both_heads_match_unchanged_cut = primary.matches(&cut.head)
            && replica.matches(&cut.head)
            && self.published.observe(|current| {
                Ok(current.anchor.head().head() == cut.head
                    && current.anchor.head().finalizing_authority_id() == cut.finalizing_authority_id
                    && current.anchor.authority().authority_id().is_ok_and(|id| id == cut.active_authority_id))
            })?;
        Ok(ProtectedPairObservation {
            api_version: "oasis7.formal-protected-pair-observation/v1".into(),
            world: self.world.clone(),
            node_id: self.validated.config.node_id.clone(),
            service_public_key: hex::encode(self.node_signer.verifying_key().to_bytes()),
            verified_published_cut: cut,
            primary,
            replica,
            both_heads_match_unchanged_cut,
            current_writer_qualification: WriterQualification::Unknown,
        })
    }
}
