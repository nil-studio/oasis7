//! Fixed-cut formal BFT light reader. It verifies the complete prefix and projection proof;
//! it does not claim to re-execute the World or establish current/fresh state.
use crate::{WorldServiceClientError, WorldServiceTransport};
use oasis7_client_api::{
    formal_projection::{ProjectionAudienceV1, ProjectionInclusionV1},
    world_service::{
        FORMAL_BFT_READ_PATH_V3, FormalBftReadRequestV3, FormalBftReadResponseV3,
        FormalBftReadTargetV3, FormalReadAnchorRefV1, MAX_FORMAL_BFT_PREFIX_BYTES_V3,
        SignedReadRequest, WorldIdentity,
    },
};
use oasis7_formal_protocol::bft::{
    BftProfileTargetV1, FormalBftCertifiedPrefixV1, FormalBftExecutionCommitmentV1,
    VerifiedBftCommitCertificateV2, VerifiedBftHandoffTargetV1, VerifiedFormalBftPrefixV1,
    verify_formal_bft_handoff_v1,
};
use oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1;
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

const _: [(); MAX_FORMAL_BFT_PREFIX_BYTES_V3] =
    [(); oasis7_formal_protocol::bft::BFT_FORMAL_PREFIX_MAX_BYTES_V1];
const _: [(); oasis7_client_api::world_service::MAX_FORMAL_BFT_PREFIX_HEIGHTS_V3 as usize] =
    [(); oasis7_formal_protocol::bft::BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1];

/// Network and local-authority settings for the explicit formal BFT endpoint.
/// The exact target is an operator pin and is never learned from a reply.
pub struct FormalBftReadClientConfigV3 {
    pub endpoint: String,
    pub trusted_service_public_key: String,
    pub read_private_key_hex: String,
    pub timeout: Duration,
    pub scope_id: String,
    pub exact_target: FormalBftReadTargetV3,
}

impl std::fmt::Debug for FormalBftReadClientConfigV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormalBftReadClientConfigV3")
            .field("endpoint", &self.endpoint)
            .field("scope_id", &self.scope_id)
            .field("exact_target", &self.exact_target)
            .finish_non_exhaustive()
    }
}

/// Consumer trust is built from an already verified controlled H anchor and
/// a locally fixed exact H-to-tip block ID. No response can advance this cut.
#[derive(Clone)]
pub struct FormalBftReadVerifierV3 {
    handoff: VerifiedBftHandoffTargetV1,
    handoff_anchor: FormalReadAnchorRefV1,
    world: WorldIdentity,
    exact_target: FormalBftReadTargetV3,
}

impl FormalBftReadVerifierV3 {
    pub fn new(
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV1,
        exact_target: FormalBftReadTargetV3,
    ) -> Result<Self, String> {
        exact_target.validate().map_err(|error| error.to_string())?;
        let record = anchor.head().record();
        let scope = target.scope();
        if scope.world_id != record.world.world_id
            || scope.chain_id != record.world.chain_id
            || scope.genesis_hash != record.world.genesis_digest
            || !exact_target
                .height
                .checked_sub(record.position)
                .is_some_and(|distance| {
                    (1..=oasis7_client_api::world_service::MAX_FORMAL_BFT_PREFIX_HEIGHTS_V3)
                        .contains(&distance)
                })
        {
            return Err("formal BFT read target is outside the bounded H-anchored prefix".into());
        }
        let handoff = verify_formal_bft_handoff_v1(&anchor, target)?;
        let handoff_anchor = FormalReadAnchorRefV1 {
            position: record.position,
            record_id: anchor.head().head().record_id,
            active_authority_id: anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        };
        let world = WorldIdentity {
            world_id: record.world.world_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
        };
        Ok(Self {
            handoff,
            handoff_anchor,
            world,
            exact_target,
        })
    }

    pub fn handoff(&self) -> &VerifiedBftHandoffTargetV1 {
        &self.handoff
    }

    pub fn exact_target(&self) -> &FormalBftReadTargetV3 {
        &self.exact_target
    }

    pub fn request(
        &self,
        scope_id: &str,
        nonce: [u8; 32],
    ) -> Result<FormalBftReadRequestV3, String> {
        let request = FormalBftReadRequestV3 {
            schema_version: 3,
            contract_version: 3,
            world: self.world.clone(),
            scope_id: scope_id.to_owned(),
            observation_nonce: hex::encode(nonce),
            handoff_anchor: self.handoff_anchor.clone(),
            target_config_digest: self.handoff.target_digest().to_owned(),
            activation_digest: self.handoff.activation_digest().to_owned(),
            exact_target: self.exact_target.clone(),
        };
        request.validate().map_err(|error| error.to_string())?;
        Ok(request)
    }

    pub fn verify_response<T: Serialize>(
        &self,
        signed_request: &SignedReadRequest<FormalBftReadRequestV3>,
        response: FormalBftReadResponseV3<T>,
    ) -> Result<VerifiedFormalBftReadPageV3<T>, String> {
        crate::authority::verify_read_request(FORMAL_BFT_READ_PATH_V3, signed_request)?;
        let request = &signed_request.request;
        response
            .validate_structure(request)
            .map_err(|error| error.to_string())?;
        if request.world != self.world
            || request.handoff_anchor != self.handoff_anchor
            || request.exact_target != self.exact_target
            || request.target_config_digest != self.handoff.target_digest()
            || request.activation_digest != self.handoff.activation_digest()
        {
            return Err("formal BFT request differs from fixed local trust".into());
        }

        let prefix_bytes = hex::decode(&response.certified_prefix_cbor_hex)
            .map_err(|error| format!("formal BFT prefix hex: {error}"))?;
        if prefix_bytes.is_empty() || prefix_bytes.len() > MAX_FORMAL_BFT_PREFIX_BYTES_V3 {
            return Err("formal BFT prefix byte bound".into());
        }
        let prefix = FormalBftCertifiedPrefixV1::decode_canonical(&prefix_bytes)?;
        let verified = prefix.verify(&self.handoff)?;
        let block = verified.certificate().block();
        let commitment = verified.commitment();
        if verified.certificate().block_id() != self.exact_target.block_id
            || block.height != self.exact_target.height
            || response.head.execution_block_hash != block.execution_block_hash
            || response.head.state_root != block.state_root
            || response.head.post_manifest_hash != block.post_manifest_hash
            || response.head.projection_root != commitment.projection_root()
            || response.head.logical_tick != commitment.height()
        {
            return Err("formal BFT prefix proof differs from exact pinned head".into());
        }
        let audience = if request.scope_id == "public" {
            ProjectionAudienceV1::public()
        } else {
            ProjectionAudienceV1::agent(
                request
                    .scope_id
                    .strip_prefix("agent:")
                    .ok_or("unsupported formal BFT audience")?,
                signed_request.subject_public_key.clone(),
            )?
        };
        if response.projection.audience.scope_id != audience.scope_id
            || response.projection.audience.reader_public_key != audience.reader_public_key
        {
            return Err("formal BFT projection audience differs from signed request".into());
        }
        let inclusion = ProjectionInclusionV1 {
            schema_version: response.projection.schema_version,
            audience: audience.clone(),
            leaf_index: response.projection.leaf_index,
            leaf_count: response.projection.leaf_count,
            siblings: response.projection.siblings.clone(),
        };
        inclusion.verify(&audience, &response.view, commitment.projection_root())?;
        Ok(VerifiedFormalBftReadPageV3 {
            request: request.clone(),
            response,
            proof: verified,
        })
    }
}

/// A proof-checked fixed historical page. This is not an execution proof,
/// latest-head assertion, or current-readiness result.
pub struct VerifiedFormalBftReadPageV3<T> {
    request: FormalBftReadRequestV3,
    response: FormalBftReadResponseV3<T>,
    proof: VerifiedFormalBftPrefixV1,
}

impl<T> VerifiedFormalBftReadPageV3<T> {
    pub fn request(&self) -> &FormalBftReadRequestV3 {
        &self.request
    }
    pub fn response(&self) -> &FormalBftReadResponseV3<T> {
        &self.response
    }
    pub fn certificate(&self) -> &VerifiedBftCommitCertificateV2 {
        self.proof.certificate()
    }
    pub fn proof(&self) -> &VerifiedFormalBftPrefixV1 {
        &self.proof
    }
    pub fn commitment(&self) -> &FormalBftExecutionCommitmentV1 {
        self.proof.commitment()
    }
    pub fn view(&self) -> &T {
        &self.response.view
    }
}

/// Authenticated transport and semantic verifier for the explicit V3 route.
pub struct FormalBftReadClientV3 {
    transport: WorldServiceTransport,
    verifier: FormalBftReadVerifierV3,
    scope_id: String,
}

impl FormalBftReadClientV3 {
    pub fn new(
        config: FormalBftReadClientConfigV3,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV1,
    ) -> Result<Self, WorldServiceClientError> {
        let verifier = FormalBftReadVerifierV3::new(anchor, target, config.exact_target.clone())
            .map_err(WorldServiceClientError::Configuration)?;
        crate::authority::sign_read_request(
            FORMAL_BFT_READ_PATH_V3,
            (),
            &config.read_private_key_hex,
        )
        .map_err(WorldServiceClientError::Configuration)?;
        let private_scope = config.scope_id.starts_with("agent:");
        let mut transport = WorldServiceTransport::new(crate::WorldServiceClientConfig {
            endpoint: config.endpoint,
            trusted_service_public_key: config.trusted_service_public_key,
            expected_world: verifier.world.clone(),
            scope_id: config.scope_id.clone(),
            read_private_key_hex: config.read_private_key_hex.clone(),
            timeout: config.timeout,
            max_response_bytes:
                oasis7_client_api::world_service::MAX_FORMAL_BFT_READ_RESPONSE_BYTES_V3,
        })?;
        if private_scope {
            // Keep the generic signed response codec and the explicit V3 bound, but
            // route confidential reads through the no-proxy, no-redirect client.
            transport.use_private_connection()?;
        }
        Ok(Self {
            transport,
            verifier,
            scope_id: config.scope_id,
        })
    }

    pub fn read<T: Serialize + DeserializeOwned>(
        &self,
    ) -> Result<VerifiedFormalBftReadPageV3<T>, WorldServiceClientError> {
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce)
            .map_err(|error| WorldServiceClientError::Configuration(error.to_string()))?;
        let request = self
            .verifier
            .request(&self.scope_id, nonce)
            .map_err(WorldServiceClientError::Assurance)?;
        let signed_request = crate::authority::sign_read_request(
            FORMAL_BFT_READ_PATH_V3,
            request,
            &self.transport.config().read_private_key_hex,
        )
        .map_err(WorldServiceClientError::Configuration)?;
        let response: FormalBftReadResponseV3<T> = self
            .transport
            .call(FORMAL_BFT_READ_PATH_V3, &signed_request)?;
        self.verifier
            .verify_response(&signed_request, response)
            .map_err(WorldServiceClientError::Assurance)
    }
}
