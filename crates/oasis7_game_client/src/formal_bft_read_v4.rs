//! Fixed-cut formal BFT light reader. It verifies the complete prefix and projection proof;
//! it does not claim to re-execute the World or establish current/fresh state.
use crate::{WorldServiceClientError, WorldServiceTransport};
use oasis7_client_api::{
    formal_projection::{ProjectionAudienceV1, ProjectionInclusionV1},
    world_service::{
        FORMAL_BFT_HEADER_READ_PATH_V4, FormalBftHeaderReadRequestV4,
        FormalBftHeaderReadResponseV4, FormalBftHeaderReadTargetV4, FormalReadAnchorRefV1,
        MAX_FORMAL_BFT_HEADER_PREFIX_BYTES_V4, SignedReadRequest, WorldIdentity,
    },
};
use oasis7_formal_protocol::bft::{
    BftProfileTargetV2, FormalBftCertifiedHeaderPrefixV3, FormalBftExecutionCommitmentV1,
    VerifiedBftHandoffTargetV2, VerifiedBftHeaderCertificateV3, VerifiedFormalBftHeaderPrefixV3,
    verify_formal_bft_handoff_v2,
};
use oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1;
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;

const _: [(); MAX_FORMAL_BFT_HEADER_PREFIX_BYTES_V4] =
    [(); oasis7_formal_protocol::bft::BFT_FORMAL_HEADER_PREFIX_MAX_BYTES_V3];
const _: [(); oasis7_client_api::world_service::MAX_FORMAL_BFT_HEADER_PREFIX_HEIGHTS_V4 as usize] =
    [(); oasis7_formal_protocol::bft::BFT_FORMAL_HEADER_PREFIX_MAX_HEIGHTS_V3];

/// Network and local-authority settings for the explicit formal BFT endpoint.
/// The exact target is an operator pin and is never learned from a reply.
pub struct FormalBftHeaderReadClientConfigV4 {
    pub endpoint: String,
    pub trusted_service_public_key: String,
    pub read_private_key_hex: String,
    pub timeout: Duration,
    pub scope_id: String,
    pub exact_target: FormalBftHeaderReadTargetV4,
}

impl std::fmt::Debug for FormalBftHeaderReadClientConfigV4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormalBftHeaderReadClientConfigV4")
            .field("endpoint", &self.endpoint)
            .field("scope_id", &self.scope_id)
            .field("exact_target", &self.exact_target)
            .finish_non_exhaustive()
    }
}

/// Consumer trust is built from an already verified controlled H anchor and
/// a locally fixed exact H-to-tip header ID. No response can advance this cut.
#[derive(Clone)]
pub struct FormalBftHeaderReadVerifierV4 {
    handoff: VerifiedBftHandoffTargetV2,
    handoff_anchor: FormalReadAnchorRefV1,
    world: WorldIdentity,
    exact_target: FormalBftHeaderReadTargetV4,
}

impl FormalBftHeaderReadVerifierV4 {
    pub fn new(
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        exact_target: FormalBftHeaderReadTargetV4,
    ) -> Result<Self, String> {
        exact_target.validate().map_err(|error| error.to_string())?;
        let record = anchor.head().record();
        let scope = &target.scope;
        if scope.world_id != record.world.world_id
            || scope.chain_id != record.world.chain_id
            || scope.genesis_hash != record.world.genesis_digest
            || !exact_target
                .height
                .checked_sub(record.position)
                .is_some_and(|distance| {
                    (1..=oasis7_client_api::world_service::MAX_FORMAL_BFT_HEADER_PREFIX_HEIGHTS_V4)
                        .contains(&distance)
                })
        {
            return Err("formal BFT read target is outside the bounded H-anchored prefix".into());
        }
        let handoff = verify_formal_bft_handoff_v2(&anchor, target)?;
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

    pub fn handoff(&self) -> &VerifiedBftHandoffTargetV2 {
        &self.handoff
    }

    pub fn exact_target(&self) -> &FormalBftHeaderReadTargetV4 {
        &self.exact_target
    }

    pub fn request(
        &self,
        scope_id: &str,
        nonce: [u8; 32],
    ) -> Result<FormalBftHeaderReadRequestV4, String> {
        let request = FormalBftHeaderReadRequestV4 {
            schema_version: 4,
            contract_version: 4,
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
        signed_request: &SignedReadRequest<FormalBftHeaderReadRequestV4>,
        response: FormalBftHeaderReadResponseV4<T>,
    ) -> Result<VerifiedFormalBftHeaderReadPageV4<T>, String> {
        crate::authority::verify_read_request(FORMAL_BFT_HEADER_READ_PATH_V4, signed_request)?;
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
        if prefix_bytes.is_empty() || prefix_bytes.len() > MAX_FORMAL_BFT_HEADER_PREFIX_BYTES_V4 {
            return Err("formal BFT prefix byte bound".into());
        }
        let prefix = FormalBftCertifiedHeaderPrefixV3::decode_canonical(&prefix_bytes)?;
        let verified = prefix.verify(&self.handoff)?;
        let header = &verified.certificate().certified_header().header;
        let commitment = verified.commitment();
        if verified.certificate().block_id() != self.exact_target.header_id
            || commitment.height != self.exact_target.height
            || response.head.execution_commitment_hash != header.execution_commitment_hash
            || response.head.state_root != commitment.state_root
            || response.head.post_manifest_hash != commitment.post_manifest_hash
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
        Ok(VerifiedFormalBftHeaderReadPageV4 {
            request: request.clone(),
            response,
            proof: verified,
        })
    }
}

/// A proof-checked fixed historical page. This is not an execution proof,
/// latest-head assertion, or current-readiness result.
pub struct VerifiedFormalBftHeaderReadPageV4<T> {
    request: FormalBftHeaderReadRequestV4,
    response: FormalBftHeaderReadResponseV4<T>,
    proof: VerifiedFormalBftHeaderPrefixV3,
}

impl<T> VerifiedFormalBftHeaderReadPageV4<T> {
    pub fn request(&self) -> &FormalBftHeaderReadRequestV4 {
        &self.request
    }
    pub fn response(&self) -> &FormalBftHeaderReadResponseV4<T> {
        &self.response
    }
    pub fn certificate(&self) -> &VerifiedBftHeaderCertificateV3 {
        self.proof.certificate()
    }
    pub fn proof(&self) -> &VerifiedFormalBftHeaderPrefixV3 {
        &self.proof
    }
    pub fn commitment(&self) -> &FormalBftExecutionCommitmentV1 {
        self.proof.commitment()
    }
    pub fn view(&self) -> &T {
        &self.response.view
    }
}

/// Authenticated transport and semantic verifier for the independent contract-4 route.
pub struct FormalBftHeaderReadClientV4 {
    transport: WorldServiceTransport,
    verifier: FormalBftHeaderReadVerifierV4,
    scope_id: String,
}

impl FormalBftHeaderReadClientV4 {
    pub fn new(
        config: FormalBftHeaderReadClientConfigV4,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
    ) -> Result<Self, WorldServiceClientError> {
        let verifier =
            FormalBftHeaderReadVerifierV4::new(anchor, target, config.exact_target.clone())
                .map_err(WorldServiceClientError::Configuration)?;
        crate::authority::sign_read_request(
            FORMAL_BFT_HEADER_READ_PATH_V4,
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
                oasis7_client_api::world_service::MAX_FORMAL_BFT_HEADER_READ_RESPONSE_BYTES_V4,
        })?;
        if private_scope {
            // Keep the generic signed response codec and the explicit contract-4 bound, but
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
    ) -> Result<VerifiedFormalBftHeaderReadPageV4<T>, WorldServiceClientError> {
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce)
            .map_err(|error| WorldServiceClientError::Configuration(error.to_string()))?;
        let request = self
            .verifier
            .request(&self.scope_id, nonce)
            .map_err(WorldServiceClientError::Assurance)?;
        let signed_request = crate::authority::sign_read_request(
            FORMAL_BFT_HEADER_READ_PATH_V4,
            request,
            &self.transport.config().read_private_key_hex,
        )
        .map_err(WorldServiceClientError::Configuration)?;
        let response: FormalBftHeaderReadResponseV4<T> = self
            .transport
            .call(FORMAL_BFT_HEADER_READ_PATH_V4, &signed_request)?;
        self.verifier
            .verify_response(&signed_request, response)
            .map_err(WorldServiceClientError::Assurance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_read_client_debug_does_not_expose_signing_material() {
        let config = FormalBftHeaderReadClientConfigV4 {
            endpoint: "https://example.invalid".into(),
            trusted_service_public_key: "ab".repeat(32),
            read_private_key_hex: "private-wire4-secret-canary".into(),
            timeout: Duration::from_secs(2),
            scope_id: "agent:one".into(),
            exact_target: FormalBftHeaderReadTargetV4 {
                height: 4,
                header_id: "cd".repeat(32),
            },
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("agent:one"));
        assert!(!debug.contains("private-wire4-secret-canary"));
        assert!(!debug.contains(&config.trusted_service_public_key));
    }
}
