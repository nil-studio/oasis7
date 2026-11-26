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
    BftProfileTargetV2, BftSuccessorWindowSourceV1, FormalBftCertifiedHeaderPrefixV3,
    FormalBftExecutionCommitmentV1,
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

/// An externally selected successor cut identity. This is an untrusted pin,
/// not a signer lease, FullReadOnly grant, archive closure, or install receipt.
/// The caller must separately qualify the original H and complete cut/archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormalBftOriginalSuccessorCutPinV1 {
    pub source_origin_digest: String,
    pub height: u64,
    pub header_id: String,
    pub execution_commitment_digest: String,
    pub semantic_state_root: String,
    pub post_manifest_hash: String,
}

impl FormalBftOriginalSuccessorCutPinV1 {
    fn validate(&self) -> Result<(), String> {
        let valid_digest = |digest: &str| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        };
        if self.height == 0
            || ![
                &self.source_origin_digest,
                &self.header_id,
                &self.execution_commitment_digest,
                &self.semantic_state_root,
                &self.post_manifest_hash,
            ]
            .into_iter()
            .all(|digest| valid_digest(digest))
        {
            return Err("original successor cut pin is invalid".into());
        }
        Ok(())
    }
}

/// A verified read4 page already passed the original signed request, certified
/// H-to-tip prefix, exact QC/commitment, and projection proof verification.
/// This additional comparison binds it to an externally qualified successor
/// source and independently fixed cut, without promoting any reader authority.
impl<T> VerifiedFormalBftHeaderReadPageV4<T> {
    pub fn verify_original_successor_cut_candidate(
        &self,
        original: &BftSuccessorWindowSourceV1,
        selected: &FormalBftOriginalSuccessorCutPinV1,
    ) -> Result<(), String> {
        check_untrusted_original_successor_cut(&self.request, &self.response.head, original, selected)
    }
}

// Private structural check; only the VerifiedFormalBftHeaderReadPageV4 method
// exposes this to callers, after the full read4 proof has been checked.
fn check_untrusted_original_successor_cut(
    request: &FormalBftHeaderReadRequestV4,
    head: &oasis7_client_api::world_service::FormalBftHeaderReadHeadV4,
    original: &BftSuccessorWindowSourceV1,
    selected: &FormalBftOriginalSuccessorCutPinV1,
) -> Result<(), String> {
    original.validate()?;
    request.validate().map_err(|error| error.to_string())?;
    head.validate().map_err(|error| error.to_string())?;
    selected.validate()?;
    if selected.source_origin_digest != original.origin_digest()?
        || request.world.world_id != original.scope.world_id
        || request.world.genesis_digest != original.scope.genesis_hash
        || request.handoff_anchor.position != original.handoff_position
        || request.handoff_anchor.record_id != original.handoff_record_id
        || request.target_config_digest != original.target_digest
        || request.activation_digest != original.activation_digest
    {
        return Err("read4 original qualified H/source mismatch".into());
    }
    if request.exact_target.height != selected.height
        || request.exact_target.header_id != selected.header_id
        || head.height != selected.height
        || head.header_id != selected.header_id
        || head.execution_commitment_hash != selected.execution_commitment_digest
        || head.state_root != selected.semantic_state_root
        || head.post_manifest_hash != selected.post_manifest_hash
    {
        return Err("read4 certified head differs from selected successor cut".into());
    }
    Ok(())
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

    #[test]
    fn original_successor_cut_binding_rejects_foreign_h_and_changed_head() {
        use oasis7_proto::distributed_bft::v2::BftScopeV2;
        let hash = |n: u8| format!("{n:02x}").repeat(32);
        let original = BftSuccessorWindowSourceV1 {
            scope: BftScopeV2 {
                world_id: "world".into(),
                chain_id: "chain".into(),
                genesis_hash: hash(1),
            },
            binding_digest: hash(2),
            handoff_position: 100,
            handoff_record_id: hash(3),
            handoff_snapshot_root: hash(4),
            target_digest: hash(5),
            activation_digest: hash(6),
            material_identity: hash(7),
        };
        let request = FormalBftHeaderReadRequestV4 {
            schema_version: 4,
            contract_version: 4,
            world: WorldIdentity {
                world_id: "world".into(),
                genesis_digest: hash(1),
            },
            scope_id: "public".into(),
            observation_nonce: hash(8),
            handoff_anchor: FormalReadAnchorRefV1 {
                position: 100,
                record_id: hash(3),
                active_authority_id: hash(9),
            },
            target_config_digest: hash(5),
            activation_digest: hash(6),
            exact_target: FormalBftHeaderReadTargetV4 {
                height: 101,
                header_id: hash(10),
            },
        };
        let head = oasis7_client_api::world_service::FormalBftHeaderReadHeadV4 {
            height: 101,
            header_id: hash(10),
            execution_commitment_hash: hash(11),
            state_root: hash(12),
            post_manifest_hash: hash(13),
            projection_root: hash(14),
            logical_tick: 101,
        };
        let selected = FormalBftOriginalSuccessorCutPinV1 {
            source_origin_digest: original.origin_digest().unwrap(),
            height: 101,
            header_id: hash(10),
            execution_commitment_digest: hash(11),
            semantic_state_root: hash(12),
            post_manifest_hash: hash(13),
        };
        let check = |request: &FormalBftHeaderReadRequestV4,
                     head: &oasis7_client_api::world_service::FormalBftHeaderReadHeadV4,
                     original: &BftSuccessorWindowSourceV1,
                     selected: &FormalBftOriginalSuccessorCutPinV1| {
            check_untrusted_original_successor_cut(request, head, original, selected)
        };
        assert!(check(&request, &head, &original, &selected).is_ok());

        let mut foreign = original.clone();
        foreign.scope.world_id = "other-world".into();
        assert!(check(&request, &head, &foreign, &selected).is_err());
        foreign = original.clone();
        foreign.scope.chain_id = "other-chain".into();
        assert!(check(&request, &head, &foreign, &selected).is_err());
        foreign = original.clone();
        foreign.handoff_record_id = hash(22);
        assert!(check(&request, &head, &foreign, &selected).is_err());

        let mut changed = request.clone();
        changed.activation_digest = hash(23);
        assert!(check(&changed, &head, &original, &selected).is_err());
        changed = request.clone();
        changed.handoff_anchor.position = 99;
        assert!(check(&changed, &head, &original, &selected).is_err());
        changed = request.clone();
        changed.exact_target.height = 165;
        assert!(check(&changed, &head, &original, &selected).is_err());

        let mut changed_head = head.clone();
        changed_head.state_root = hash(24);
        assert!(check(&request, &changed_head, &original, &selected).is_err());
        changed_head = head.clone();
        changed_head.execution_commitment_hash = hash(25);
        assert!(check(&request, &changed_head, &original, &selected).is_err());
        changed_head = head.clone();
        changed_head.post_manifest_hash = hash(26);
        assert!(check(&request, &changed_head, &original, &selected).is_err());

        let mut changed_pin = selected.clone();
        changed_pin.header_id = hash(27);
        assert!(check(&request, &head, &original, &changed_pin).is_err());
        changed_pin = selected.clone();
        changed_pin.source_origin_digest = hash(28);
        assert!(check(&request, &head, &original, &changed_pin).is_err());
        changed_pin = selected.clone();
        changed_pin.source_origin_digest = "AA".repeat(32);
        assert!(check(&request, &head, &original, &changed_pin).is_err());
    }
}
