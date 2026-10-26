//! Producer for the separately versioned, exact-cut formal BFT read route.
//!
//! The caller must obtain `committed_world` and its exact certificate,
//! commitment, and result delta from the durable, replay-verified execution
//! driver. This service independently checks the certified fields and actual
//! World roots; a QC by itself is not treated as execution evidence.
use crate::{
    CanonicalIntentResultV1, authority,
    formal_projection::{ProjectionAudienceV1, ProjectionCommitmentsV1},
    formal_read_error::{FormalReadError, FormalReadResult},
    formal_read_service::authorized_formal_agent_in_world,
    formal_result::{FormalBftResultSuccessorInputV1, FormalBusinessResultTreeV1},
    projection::{self, WorldServiceProjection},
};
use oasis7_client_api::{
    formal_projection::ProjectionInclusionV1,
    world_service::{
        FORMAL_BFT_READ_PATH_V2, FormalBftReadHeadV2, FormalBftReadProfileV2,
        FormalBftReadRequestV2, FormalBftReadResponseV2, FormalBftReadTargetV2,
        FormalProjectionAudienceV1, FormalProjectionInclusionV1, FormalReadAnchorRefV1,
        SignedReadRequest, WorldIdentity,
    },
};
use oasis7_formal_protocol::bft::{
    BftProfileTargetV1, FormalBftExecutionCommitmentV1, VerifiedBftHandoffTargetV1,
    verify_commit_certificate_token_v2, verify_formal_bft_handoff_v1,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalWorldIdentityV1, VerifiedFormalReadAnchorV1, business_index::BusinessIndexNodeReader,
};
use oasis7_proto::distributed_bft::v2::BftCommitCertificateV2;
use oasis7_runtime::{World, blake3_hex};

/// Inputs supplied by the route adapter from operator pins and the durable
/// execution driver's exact committed-cut reread. No network field is used
/// to select the target or the World.
pub struct FormalBftReadPublicationInputV2<'a> {
    pub handoff_anchor: VerifiedFormalReadAnchorV1,
    pub target_config: BftProfileTargetV1,
    pub exact_target: FormalBftReadTargetV2,
    pub certificate: &'a BftCommitCertificateV2,
    pub commitment: &'a FormalBftExecutionCommitmentV1,
    pub committed_world: &'a World,
    pub new_results: &'a [CanonicalIntentResultV1],
    pub h_result_tree: &'a FormalBusinessResultTreeV1,
    pub h_result_index_reader: &'a dyn BusinessIndexNodeReader,
}

/// Immutable publication for the one locally pinned H+1 block.
///
/// Construction is deliberately explicit about the H-qualified anchor and
/// operator-selected exact block ID. It does not follow a later head. Private
/// readers must additionally be authorized against a separately supplied
/// current World on every `read` call.
pub struct FormalBftReadPublicationV2 {
    handoff_anchor: FormalReadAnchorRefV1,
    target_config_digest: String,
    activation_digest: String,
    exact_target: FormalBftReadTargetV2,
    world_identity: WorldIdentity,
    chain_id: String,
    head: FormalBftReadHeadV2,
    certificate_cbor_hex: String,
    commitment_cbor_hex: String,
    committed_world: World,
    projection_tree: ProjectionCommitmentsV1,
}

impl FormalBftReadPublicationV2 {
    pub fn new(input: FormalBftReadPublicationInputV2<'_>) -> FormalReadResult<Self> {
        let record = input.handoff_anchor.head().record();
        let handoff = verify_formal_bft_handoff_v1(&input.handoff_anchor, input.target_config)
            .map_err(FormalReadError::Integrity)?;
        validate_exact_target(&handoff, &input.exact_target)?;
        let h_result_root = input
            .h_result_tree
            .root()
            .map_err(FormalReadError::Integrity)?;
        if h_result_root != record.result_root {
            return Err(FormalReadError::Integrity(
                "BFT publication H result tree differs from qualified anchor".into(),
            ));
        }

        let block = &input.certificate.proposal.block;
        let schedule = handoff
            .initial_schedule_for_commitment(input.commitment, block)
            .map_err(FormalReadError::Integrity)?;
        let verified = verify_commit_certificate_token_v2(&schedule, input.certificate)
            .map_err(FormalReadError::Integrity)?;
        if verified.block_id() != input.exact_target.block_id {
            return Err(FormalReadError::Integrity(
                "BFT certificate differs from operator-pinned exact block".into(),
            ));
        }
        input
            .certificate
            .validate_wire_bounds()
            .map_err(FormalReadError::Integrity)?;

        validate_committed_world(
            input.committed_world,
            &record.world,
            block.height,
            block.state_root.as_str(),
            block.execution_state_root.as_str(),
            block.post_manifest_hash.as_str(),
        )?;

        let projection_tree =
            crate::formal_projection::commitments_for_world(input.committed_world)
                .map_err(FormalReadError::Integrity)?;
        let projection_root = projection_tree.root().map_err(FormalReadError::Integrity)?;
        if projection_root != input.commitment.projection_root() {
            return Err(FormalReadError::Integrity(
                "BFT committed World differs from certified projection root".into(),
            ));
        }

        let result_update =
            FormalBusinessResultTreeV1::extend_bft_successor(FormalBftResultSuccessorInputV1 {
                world: input.committed_world,
                identity: &record.world,
                height: block.height,
                parent_result_root: &record.result_root,
                execution_state_root: input.commitment.execution_state_root(),
                previous: input.h_result_tree,
                new_results: input.new_results,
                reader: input.h_result_index_reader,
            })
            .map_err(FormalReadError::Integrity)?;
        if result_update
            .tree
            .root()
            .map_err(FormalReadError::Integrity)?
            != input.commitment.result_root()
        {
            return Err(FormalReadError::Integrity(
                "BFT committed World differs from certified result root".into(),
            ));
        }

        let certificate_bytes = oasis7_wasm_abi::encode_canonical_cbor(input.certificate)
            .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
        let commitment_bytes = input
            .commitment
            .canonical_bytes()
            .map_err(FormalReadError::Integrity)?;
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
        let head = FormalBftReadHeadV2 {
            height: block.height,
            block_id: verified.block_id().to_owned(),
            execution_block_hash: block.execution_block_hash.clone(),
            state_root: block.state_root.clone(),
            post_manifest_hash: block.post_manifest_hash.clone(),
            projection_root,
            logical_tick: input.commitment.height(),
        };
        Ok(Self {
            handoff_anchor,
            target_config_digest: handoff.target_digest().to_owned(),
            activation_digest: handoff.activation_digest().to_owned(),
            exact_target: input.exact_target,
            world_identity,
            chain_id: record.world.chain_id.clone(),
            head,
            certificate_cbor_hex: hex::encode(certificate_bytes),
            commitment_cbor_hex: hex::encode(commitment_bytes),
            committed_world: input.committed_world.clone(),
            projection_tree,
        })
    }

    pub fn exact_target(&self) -> &FormalBftReadTargetV2 {
        &self.exact_target
    }

    /// Verify the caller and produce the exact fixed-cut view. `current_auth`
    /// is required for private scopes and must be read from the current
    /// authorization provider, never substituted with the historical World.
    pub fn read(
        &self,
        signed_request: &SignedReadRequest<FormalBftReadRequestV2>,
        current_auth: Option<&World>,
    ) -> FormalReadResult<FormalBftReadResponseV2<WorldServiceProjection>> {
        authority::verify_read_request(FORMAL_BFT_READ_PATH_V2, signed_request)
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
        let response = FormalBftReadResponseV2 {
            schema_version: 2,
            contract_version: 2,
            profile: FormalBftReadProfileV2::bft_v1(),
            world: self.world_identity.clone(),
            scope_id: request.scope_id.clone(),
            observation_nonce: request.observation_nonce.clone(),
            handoff_anchor: self.handoff_anchor.clone(),
            target_config_digest: self.target_config_digest.clone(),
            activation_digest: self.activation_digest.clone(),
            exact_target: self.exact_target.clone(),
            head: self.head.clone(),
            certificate_cbor_hex: self.certificate_cbor_hex.clone(),
            execution_commitment_cbor_hex: self.commitment_cbor_hex.clone(),
            projection: to_wire_inclusion(inclusion),
            view,
        };
        response
            .validate_structure(request)
            .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
        Ok(response)
    }
}

fn validate_exact_target(
    handoff: &VerifiedBftHandoffTargetV1,
    exact_target: &FormalBftReadTargetV2,
) -> FormalReadResult<()> {
    exact_target
        .validate()
        .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
    if exact_target.height
        != handoff
            .activation_height()
            .map_err(FormalReadError::Integrity)?
    {
        return Err(FormalReadError::NotReady(
            "formal BFT publication is not the exact H+1 cut".into(),
        ));
    }
    Ok(())
}

fn validate_committed_world(
    world: &World,
    identity: &FormalWorldIdentityV1,
    height: u64,
    semantic_state_root: &str,
    execution_state_root: &str,
    post_manifest_hash: &str,
) -> FormalReadResult<()> {
    validate_world_identity(
        world,
        &WorldIdentity {
            world_id: identity.world_id.clone(),
            genesis_digest: identity.genesis_digest.clone(),
        },
        Some(identity.chain_id.as_str()),
    )
    .map_err(FormalReadError::Integrity)?;
    if world.state().time != height {
        return Err(FormalReadError::Integrity(
            "BFT publication World tick differs from certified height".into(),
        ));
    }
    let snapshot_bytes = oasis7_world_state::codec::snapshot_cbor(&world.snapshot())
        .map_err(|error| FormalReadError::Integrity(error.to_string()))?;
    if blake3_hex(&snapshot_bytes) != execution_state_root {
        return Err(FormalReadError::Integrity(
            "BFT publication snapshot differs from certified execution root".into(),
        ));
    }
    let state_root = world
        .execution_state_identity()
        .map_err(|error| FormalReadError::Integrity(format!("{error:?}")))?
        .state_root_hash;
    if state_root != semantic_state_root {
        return Err(FormalReadError::Integrity(
            "BFT publication semantic state differs from certified root".into(),
        ));
    }
    let manifest = world
        .current_manifest_hash()
        .map_err(|error| FormalReadError::Integrity(format!("{error:?}")))?;
    if manifest != post_manifest_hash {
        return Err(FormalReadError::Integrity(
            "BFT publication manifest differs from certified post manifest".into(),
        ));
    }
    Ok(())
}

fn validate_world_identity(
    world: &World,
    identity: &WorldIdentity,
    chain_id: Option<&str>,
) -> Result<(), String> {
    let manifest = world.chain_resource_manifest();
    if manifest.world_id != identity.world_id
        || manifest.genesis_ref.as_deref() != Some(identity.genesis_digest.as_str())
        || chain_id.is_some_and(|expected| manifest.chain_id != expected)
    {
        return Err("BFT read World identity differs from the fixed publication".into());
    }
    Ok(())
}

fn to_wire_inclusion(inclusion: ProjectionInclusionV1) -> FormalProjectionInclusionV1 {
    FormalProjectionInclusionV1 {
        schema_version: inclusion.schema_version,
        audience: FormalProjectionAudienceV1 {
            scope_id: inclusion.audience.scope_id,
            reader_public_key: inclusion.audience.reader_public_key,
        },
        leaf_index: inclusion.leaf_index,
        leaf_count: inclusion.leaf_count,
        siblings: inclusion.siblings,
    }
}

#[cfg(test)]
#[path = "formal_bft_read_service_tests.rs"]
mod tests;
