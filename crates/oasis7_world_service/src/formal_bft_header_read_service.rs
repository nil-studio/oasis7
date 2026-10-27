//! Header-only public proofs for an exact, durable wire-3 execution cut.
//! The route adapter supplies the actual driver cut; protocol QC verification
//! alone never establishes execution, storage or current authorization.
use crate::{
    authority,
    formal_bft_read_service::{
        to_wire_inclusion, validate_committed_world, validate_world_identity,
    },
    formal_projection::{ProjectionAudienceV1, ProjectionCommitmentsV1},
    formal_read_error::{FormalReadError, FormalReadResult},
    formal_read_service::authorized_formal_agent_in_world,
    formal_result::FormalBusinessResultTreeV1,
    projection::{self, WorldServiceProjection},
};
use oasis7_client_api::world_service::{
    FORMAL_BFT_HEADER_READ_PATH_V4, FormalBftHeaderReadHeadV4, FormalBftHeaderReadProfileV4,
    FormalBftHeaderReadRequestV4, FormalBftHeaderReadResponseV4, FormalBftHeaderReadTargetV4,
    FormalReadAnchorRefV1, SignedReadRequest, WorldIdentity,
};
use oasis7_formal_protocol::{
    bft::{BftProfileTargetV2, FormalBftCertifiedHeaderPrefixV3, verify_formal_bft_handoff_v2},
    controlled_authority::formal::VerifiedFormalReadAnchorV1,
};
use oasis7_runtime::World;

/// All state and evidence come from the same strict durable cut reread.
/// The current private authorization provider is separate and required at read.
pub struct FormalBftHeaderReadPublicationInputV4<'a> {
    pub handoff_anchor: VerifiedFormalReadAnchorV1,
    pub target_config: BftProfileTargetV2,
    pub exact_target: FormalBftHeaderReadTargetV4,
    pub certified_prefix: &'a FormalBftCertifiedHeaderPrefixV3,
    pub committed_world: &'a World,
    pub result_tree: &'a FormalBusinessResultTreeV1,
}

pub struct FormalBftHeaderReadPublicationV4 {
    handoff_anchor: FormalReadAnchorRefV1,
    target_config_digest: String,
    activation_digest: String,
    exact_target: FormalBftHeaderReadTargetV4,
    world_identity: WorldIdentity,
    chain_id: String,
    head: FormalBftHeaderReadHeadV4,
    prefix_cbor_hex: String,
    committed_world: World,
    projection_tree: ProjectionCommitmentsV1,
}
impl FormalBftHeaderReadPublicationV4 {
    pub fn new(input: FormalBftHeaderReadPublicationInputV4<'_>) -> FormalReadResult<Self> {
        input
            .exact_target
            .validate()
            .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
        let record = input.handoff_anchor.head().record();
        let handoff = verify_formal_bft_handoff_v2(&input.handoff_anchor, input.target_config)
            .map_err(FormalReadError::Integrity)?;
        let proof = input
            .certified_prefix
            .verify(&handoff)
            .map_err(FormalReadError::Integrity)?;
        let block = &proof.certificate().certified_header().header;
        let commitment = proof.commitment();
        if commitment.height != input.exact_target.height
            || proof.certificate().block_id() != input.exact_target.header_id
        {
            return Err(FormalReadError::Integrity(
                "formal BFT prefix differs from independently pinned exact cut".into(),
            ));
        }
        validate_committed_world(
            input.committed_world,
            &record.world,
            commitment.height,
            &commitment.state_root,
            &commitment.execution_state_root,
            &commitment.post_manifest_hash,
        )?;
        let projection_tree =
            crate::formal_projection::commitments_for_world(input.committed_world)
                .map_err(FormalReadError::Integrity)?;
        let projection_root = projection_tree.root().map_err(FormalReadError::Integrity)?;
        if projection_root != commitment.projection_root()
            || input
                .result_tree
                .root()
                .map_err(FormalReadError::Integrity)?
                != commitment.result_root()
        {
            return Err(FormalReadError::Integrity(
                "formal BFT prefix World/result tree differs from certified roots".into(),
            ));
        }
        let authority_id = input
            .handoff_anchor
            .authority()
            .authority_id()
            .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
        let handoff_anchor = FormalReadAnchorRefV1 {
            position: record.position,
            record_id: input.handoff_anchor.head().head().record_id.clone(),
            active_authority_id: authority_id,
        };
        let world_identity = WorldIdentity {
            world_id: record.world.world_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
        };
        let head = FormalBftHeaderReadHeadV4 {
            height: commitment.height,
            header_id: proof.certificate().block_id().to_owned(),
            execution_commitment_hash: block.execution_commitment_hash.clone(),
            state_root: commitment.state_root.clone(),
            post_manifest_hash: commitment.post_manifest_hash.clone(),
            projection_root,
            logical_tick: commitment.height(),
        };
        let prefix_cbor_hex = hex::encode(
            input
                .certified_prefix
                .canonical_bytes()
                .map_err(FormalReadError::Integrity)?,
        );
        Ok(Self {
            handoff_anchor,
            target_config_digest: handoff.target_digest().to_owned(),
            activation_digest: handoff.activation_digest().to_owned(),
            exact_target: input.exact_target,
            world_identity,
            chain_id: record.world.chain_id.clone(),
            head,
            prefix_cbor_hex,
            committed_world: input.committed_world.clone(),
            projection_tree,
        })
    }
    pub fn exact_target(&self) -> &FormalBftHeaderReadTargetV4 {
        &self.exact_target
    }

    /// Verify the caller and produce the exact fixed-cut view. `current_auth`
    /// is required for private scopes and must be read from the current
    /// authorization provider, never substituted with the historical World.
    pub fn read(
        &self,
        signed_request: &SignedReadRequest<FormalBftHeaderReadRequestV4>,
        current_auth: Option<&World>,
    ) -> FormalReadResult<FormalBftHeaderReadResponseV4<WorldServiceProjection>> {
        authority::verify_read_request(FORMAL_BFT_HEADER_READ_PATH_V4, signed_request)
            .map_err(FormalReadError::RequestRejected)?;
        let request = &signed_request.request;
        request
            .validate()
            .map_err(|error| FormalReadError::RequestRejected(error.to_string()))?;
        if request.world != self.world_identity
            || request.handoff_anchor != self.handoff_anchor
            || request.target_config_digest != self.target_config_digest
            || request.activation_digest != self.activation_digest
            || request.exact_target != self.exact_target
        {
            return Err(FormalReadError::RequestRejected(
                "formal BFT request differs from exact publication".into(),
            ));
        }

        let authorized_agent = if request.scope_id == "public" {
            None
        } else {
            let current_auth = current_auth.ok_or_else(|| {
                FormalReadError::NotReady(
                    "current authorization World is unavailable for private BFT read".into(),
                )
            })?;
            validate_world_identity(current_auth, &self.world_identity, Some(&self.chain_id))
                .map_err(FormalReadError::GrantRejected)?;
            authorized_formal_agent_in_world(
                current_auth,
                &request.scope_id,
                &signed_request.subject_public_key,
            )?
        };
        let view = projection::from_formal_committed_world(
            &self.committed_world,
            authorized_agent,
            &signed_request.subject_public_key,
        )
        .map_err(FormalReadError::Integrity)?;
        let audience = if let Some(agent) = authorized_agent {
            ProjectionAudienceV1::agent(agent, signed_request.subject_public_key.clone())
                .map_err(FormalReadError::Integrity)?
        } else {
            ProjectionAudienceV1::public()
        };
        let inclusion = self
            .projection_tree
            .inclusion(&audience)
            .map_err(FormalReadError::Integrity)?;
        inclusion
            .verify(&audience, &view, &self.head.projection_root)
            .map_err(FormalReadError::Integrity)?;
        let response = FormalBftHeaderReadResponseV4 {
            schema_version: 4,
            contract_version: 4,
            profile: FormalBftHeaderReadProfileV4::bft_v1(),
            world: self.world_identity.clone(),
            scope_id: request.scope_id.clone(),
            observation_nonce: request.observation_nonce.clone(),
            handoff_anchor: self.handoff_anchor.clone(),
            target_config_digest: self.target_config_digest.clone(),
            activation_digest: self.activation_digest.clone(),
            exact_target: self.exact_target.clone(),
            head: self.head.clone(),
            certified_prefix_cbor_hex: self.prefix_cbor_hex.clone(),
            projection: to_wire_inclusion(inclusion),
            view,
        };
        response
            .validate_structure(request)
            .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
        Ok(response)
    }
}
