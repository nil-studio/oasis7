//! The independent original-admission-input issuer purpose, never FullReadOnly.
use super::*;
use oasis7_net::AuthenticatedPeerRequestContext;
use oasis7_node::{
    NodeBftAdmissionInputAuthorizationV1, NodeBftAdmissionInputReadGateV1,
    NodeBftAdmissionInputReadScopeV1,
};
impl NodeBftAdmissionInputReadGateV1 for FullSyncV3ReadGate {
    fn validate_identity(
        &self,
        authorization: &NodeBftAdmissionInputAuthorizationV1,
    ) -> Result<(), String> {
        let body = &authorization.grant.body;
        let scope = BftCommittedReadScopeV1 {
            world: WorldIdentity {
                world_id: body.scope.world_id.clone(),
                genesis_digest: body.scope.genesis_hash.clone(),
            },
            chain_id: body.scope.chain_id.clone(),
            authority_epoch: self.v3_policy.body().authority_epoch,
        };
        self.validate_purpose_identity(&ReadPurpose::Admission {
            grant: &authorization.grant,
            scope: &scope,
            subject_public_key: &authorization.subject_public_key,
        })
        .map_err(|_| "original admission issuer identity rejected".into())
    }
    fn authorize(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        authorization: &NodeBftAdmissionInputAuthorizationV1,
        scope: &NodeBftAdmissionInputReadScopeV1<'_>,
        make_response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        let grant = &authorization.grant;
        let schedule = scope.view.schedule();
        if transport.peer_id().to_string() != scope.requester_peer_id
            || grant.body.requester_peer_id != scope.requester_peer_id
            || grant.body.serving_peer_id != scope.serving_peer_id
            || authorization.subject_public_key != scope.subject_public_key
            || grant.body.subject_public_key != scope.subject_public_key
            || grant.body.reference != scope.reference
            || grant.body.scope != *schedule.scope()
            || grant.body.schedule_digest != schedule.digest()?
            || grant.body.view_generation != scope.view.generation()
        {
            return Err(
                "original admission read differs from current Noise/view/issuer exact target"
                    .into(),
            );
        }
        // The Node authenticates each phase's subject signature and supplies its
        // currently guarded view. The issuer gate independently reconstructs the
        // new purpose's exact target, fresh witness and current provider lease.
        let requested = BftCommittedReadScopeV1 {
            world: WorldIdentity {
                world_id: schedule.scope().world_id.clone(),
                genesis_digest: schedule.scope().genesis_hash.clone(),
            },
            chain_id: schedule.scope().chain_id.clone(),
            authority_epoch: schedule.authority_epoch(),
        };
        self.authorize_target_with_response(
            ReadPurpose::Admission {
                grant,
                scope: &requested,
                subject_public_key: &scope.subject_public_key,
            },
            |deadline| {
                transport.restrict_reply_until(deadline).map_err(|_| {
                    FormalReadError::Internal("admission reply release restriction rejected".into())
                })?;
                make_response(deadline).map_err(FormalReadError::Internal)
            },
        )
        .map_err(|_| "original admission issuer authorization rejected".into())
    }
}

impl FullSyncV3ReadGate {
    /// Sign only inside the existing independently fresh new-role gate. A
    /// request's interval is a proposal, never an issuer state replacement.
    pub(crate) fn issue_original_admission_grant(
        &self,
        request: &crate::formal_bft_admission_grant::wire::GrantRequest,
        issuer: &dyn oasis7_distfs::protected_bft_v3::BftSigningKeyHandleV3,
        deadline: Instant,
    ) -> FormalReadResult<oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionReadGrantV1>
    {
        request
            .verify_subject()
            .map_err(FormalReadError::RequestRejected)?;
        if issuer.public_key_hex() != self.trust.issuer_public_key {
            return Err(FormalReadError::GrantRejected(
                "admission issuance key differs from pinned activation issuer".into(),
            ));
        }
        let scope = BftCommittedReadScopeV1 {
            world: WorldIdentity {
                world_id: request.body.scope.world_id.clone(),
                genesis_digest: request.body.scope.genesis_hash.clone(),
            },
            chain_id: request.body.scope.chain_id.clone(),
            authority_epoch: self.v3_policy.body().authority_epoch,
        };
        self.authorize_target_with_response_until(
            ReadPurpose::AdmissionIssue {
                body: &request.body,
                scope: &scope,
                subject_public_key: &request.body.subject_public_key,
            },
            Some(deadline),
            |_| {
                oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionReadGrantV1::sign(
                    request.body.clone(),
                    issuer,
                )
                .map_err(|_| FormalReadError::Internal("admission issuance signing failed".into()))
            },
        )
    }
}

impl FullSyncV3ReadGate {
    fn authorize_admission_batch<T>(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        view: &oasis7_node::NodeBftAdmissionInputReadViewV1<'_>,
        subject: &oasis7_node::NodeBftVerifiedAdmissionInputBatchSubjectV2,
        response: &mut dyn FnMut(Instant) -> Result<T, String>,
    ) -> Result<T, String> {
        let read = subject.read();
        let grant = subject.grant();
        let schedule = view.schedule();
        if transport.peer_id().to_string() != read.requester_peer_id
            || grant.body.requester_peer_id != read.requester_peer_id
            || grant.body.serving_peer_id != read.serving_peer_id
            || grant.body.subject_public_key != subject.subject_public_key()
            || grant.body.reference != read.reference
            || grant.body.request_nonce != read.nonce
            || grant.body.scope != *schedule.scope()
            || grant.body.schedule_digest != schedule.digest()?
            || grant.body.view_generation != view.generation()
        {
            return Err("batch admission differs from current Noise/view/issuer target".into());
        }
        let requested = BftCommittedReadScopeV1 {
            world: WorldIdentity {
                world_id: schedule.scope().world_id.clone(),
                genesis_digest: schedule.scope().genesis_hash.clone(),
            },
            chain_id: schedule.scope().chain_id.clone(),
            authority_epoch: schedule.authority_epoch(),
        };
        self.authorize_target_with_response(
            ReadPurpose::Admission {
                grant,
                scope: &requested,
                subject_public_key: subject.subject_public_key(),
            },
            |deadline| {
                transport.restrict_reply_until(deadline).map_err(|_| {
                    FormalReadError::Internal("batch admission release restriction rejected".into())
                })?;
                response(deadline).map_err(FormalReadError::Internal)
            },
        )
        .map_err(|_| "batch admission issuer authorization rejected".into())
    }
}
impl oasis7_node::NodeBftAdmissionInputBatchReadGateV2 for FullSyncV3ReadGate {
    fn validate_identity(
        &self,
        authorization: &oasis7_node::NodeBftAdmissionInputBatchAuthorizationV2,
    ) -> Result<(), String> {
        let body = &authorization.grant.body;
        let scope = BftCommittedReadScopeV1 {
            world: WorldIdentity {
                world_id: body.scope.world_id.clone(),
                genesis_digest: body.scope.genesis_hash.clone(),
            },
            chain_id: body.scope.chain_id.clone(),
            authority_epoch: self.v3_policy.body().authority_epoch,
        };
        self.validate_purpose_identity(&ReadPurpose::Admission {
            grant: &authorization.grant,
            scope: &scope,
            subject_public_key: &authorization.subject_public_key,
        })
        .map_err(|_| "batch admission issuer identity rejected".into())
    }
    fn authorize_batch(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        scope: &oasis7_node::NodeBftAdmissionInputBatchScopeV2<'_>,
        response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        self.authorize_admission_batch(transport, &scope.view, scope.subject, response)
    }
    fn authorize_part(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        scope: &oasis7_node::NodeBftAdmissionInputPartScopeV2<'_>,
        inspect: &mut dyn FnMut(Instant) -> Result<(), String>,
    ) -> Result<(), String> {
        if !scope.subject.authorizes_part(scope.part_index) {
            return Err("batch admission part outside signed range".into());
        }
        self.authorize_admission_batch(transport, &scope.view, scope.subject, inspect)
    }
}
