//! Produces independent formal read evidence from verified immutable World cuts.
use super::{
    agent_authority, authority,
    formal_projection::{ProjectionAudienceV1, ProjectionCommitmentsV1},
    formal_read::{FormalReadHistoryV1, FormalReadVerifier},
    projection::WorldServiceProjection,
    wire::{SignedReadRequest, VIEW_PATH},
};
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::formal::*;
use oasis7_runtime::{World, blake3_hex};

/// Private full-state source; only its authorized projection leaves this API.
pub struct FormalReadPublication {
    anchor: VerifiedFormalReadAnchorV1,
    world: World,
    tree: ProjectionCommitmentsV1,
}
impl FormalReadPublication {
    pub fn new(anchor: VerifiedFormalReadAnchorV1, world: World) -> Result<Self, String> {
        let record = anchor.head().record();
        let snapshot = serde_cbor::to_vec(&world.snapshot()).map_err(|e| e.to_string())?;
        let manifest = &world.snapshot().chain_resource_manifest;
        if blake3_hex(&snapshot) != record.after_state_root
            || manifest.world_id != record.world.world_id
            || manifest.chain_id != record.world.chain_id
            || manifest.genesis_ref.as_deref() != Some(record.world.genesis_digest.as_str())
        {
            return Err("formal publication World differs from verified canonical cut".into());
        }
        let governing = world
            .current_cognition_runtime_binding()
            .map_err(|e| format!("{e:?}"))?
            .runtime_manifest_hash
            .to_string()
            .strip_prefix("blake3:")
            .ok_or("formal publication governing encoding")?
            .to_owned();
        let tree = crate::formal_projection::commitments_for_world(&world)?;
        if governing != record.governing_manifest_digest || tree.root()? != record.projection_root {
            return Err(
                "formal publication governing/projection root differs from canonical cut".into(),
            );
        }
        Ok(Self {
            anchor,
            world,
            tree,
        })
    }
    pub fn anchor(&self) -> &VerifiedFormalReadAnchorV1 {
        &self.anchor
    }
    /// Local access to the exact verified cut. Network adapters still authorize
    /// their audience before exporting any projection or private material.
    pub fn world(&self) -> &World {
        &self.world
    }
    /// Consume the private publication after its read response has been
    /// produced, retaining the exact opaque qualification and World together.
    /// Internal server paths may pass this typed cut to another verifier;
    /// wire DTOs cannot construct it.
    pub fn into_verified_cut(self) -> (VerifiedFormalReadAnchorV1, World) {
        (self.anchor, self.world)
    }
    /// Apply current owner/delegation rules before exporting even an older cut.
    pub fn authorize_audience(&self, scope: &str, reader: &str) -> Result<(), String> {
        self.projection(scope, reader).map(|_| ())
    }
    /// Export this exact qualified cut to an already authenticated subject.
    /// HTTP adapters verify the operation signature and current authorization
    /// before using this for a delegated historical result.
    pub fn view_at_own_anchor(
        &self,
        request: &ReadWorldViewRequest,
        reader: &str,
    ) -> Result<ReadWorldViewResponse<WorldServiceProjection>, String> {
        request
            .validate_formal_shape()
            .map_err(|error| error.to_string())?;
        let expected = FormalReadAnchorRefV1 {
            position: self.anchor.head().record().position,
            record_id: self.anchor.head().head().record_id,
            active_authority_id: self
                .anchor
                .authority()
                .authority_id()
                .map_err(|error| error.to_string())?,
        };
        if request.formal_anchor.as_ref() != Some(&expected)
            || request.fixed_commit.is_some()
            || request.min_commit.is_some()
            || request.world.world_id != self.anchor.head().record().world.world_id
            || request.world.genesis_digest != self.anchor.head().record().world.genesis_digest
        {
            return Err("exact publication request binding".into());
        }
        self.response(
            request,
            reader,
            &FormalReadHistoryV1 {
                schema_version: 1,
                steps: Vec::new(),
            },
            &self.anchor,
        )
    }
    fn projection(&self, scope: &str, reader: &str) -> Result<WorldServiceProjection, String> {
        let agent = if scope == "public" {
            None
        } else {
            let agent = scope
                .strip_prefix("agent:")
                .filter(|id| !id.is_empty())
                .ok_or("unsupported formal visibility scope")?;
            let generation = if agent_authority::owner_public_key(&self.world, agent)? == reader {
                0
            } else {
                self.world
                    .capability_revocation_state()
                    .agent_signer_delegations
                    .get(agent)
                    .ok_or("formal scope delegation unavailable")?
                    .generation
            };
            agent_authority::validate_agent_signer(&self.world, agent, reader, generation)?;
            Some(agent)
        };
        WorldServiceProjection::from_formal_committed_world(&self.world, agent, reader)
    }
    fn response(
        &self,
        request: &ReadWorldViewRequest,
        reader: &str,
        history: &FormalReadHistoryV1,
        origin: &VerifiedFormalReadAnchorV1,
    ) -> Result<ReadWorldViewResponse<WorldServiceProjection>, String> {
        let view = self.projection(&request.scope_id, reader)?;
        let audience = if request.scope_id == "public" {
            ProjectionAudienceV1::public()
        } else {
            ProjectionAudienceV1::agent(
                request
                    .scope_id
                    .strip_prefix("agent:")
                    .ok_or("formal agent scope")?,
                reader.into(),
            )?
        };
        let inclusion = self.tree.inclusion(&audience)?;
        inclusion.verify(
            &audience,
            &view,
            &self.anchor.head().record().projection_root,
        )?;
        let commit = committed_reference(&self.anchor, &view)?;
        let anchor = FormalReadAnchorRefV1 {
            position: origin.head().record().position,
            record_id: origin.head().head().record_id,
            active_authority_id: origin
                .authority()
                .authority_id()
                .map_err(|e| e.to_string())?,
        };
        let continuation = EventCursor {
            stream_id: authority::request_digest(
                "formal-events-v1",
                &(&commit.world, &request.scope_id),
            )?,
            scope_id: request.scope_id.clone(),
            era: view
                .formal_commit_header
                .as_ref()
                .ok_or("formal event header")?
                .event_id_era,
            sequence: view
                .formal_commit_header
                .as_ref()
                .ok_or("formal event header")?
                .journal_high_watermark,
            commit: commit.clone(),
        };
        Ok(ReadWorldViewResponse {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            execution_evidence_scope: None,
            formal_profile: Some(FormalReadProfileV1::controlled()),
            formal_proof: Some(FormalReadSidecarV1 {
                schema_version: 1,
                profile: FormalReadProfileV1::controlled(),
                anchor,
                history_cbor_hex: history.encode_hex()?,
                projection: FormalProjectionInclusionV1 {
                    schema_version: inclusion.schema_version,
                    audience: FormalProjectionAudienceV1 {
                        scope_id: inclusion.audience.scope_id,
                        reader_public_key: inclusion.audience.reader_public_key,
                    },
                    leaf_index: inclusion.leaf_index,
                    leaf_count: inclusion.leaf_count,
                    siblings: inclusion.siblings,
                },
            }),
            logical_tick: view.state.time,
            version: ProjectionVersion {
                commit,
                visibility_scope: request.scope_id.clone(),
                projection_revision: authority::request_digest("projection", &view)?,
            },
            continuation,
            view,
        })
    }
}

/// A verified publisher still validates the complete light path from the
/// caller's independently anchored cut before returning a response.
pub struct FormalCommittedReadService {
    trust: InitialActivationTrustV1,
    current: FormalReadPublication,
}
impl FormalCommittedReadService {
    pub fn new(
        trust: InitialActivationTrustV1,
        current: FormalReadPublication,
    ) -> Result<Self, String> {
        FormalReadVerifier::new(&trust, current.anchor.clone())?;
        Ok(Self { trust, current })
    }
    pub fn publication(&self) -> &FormalReadPublication {
        &self.current
    }
    /// Return the same selected publication after response verification. This
    /// lets an internal follow-up reuse its opaque cut without rebuilding it
    /// from a wire CommitRef or restoring it from storage again.
    pub fn into_publication(self) -> FormalReadPublication {
        self.current
    }
    /// `origin` is resolved from private verified history, never a raw
    /// checkpoint supplied by the request. Fixed older cuts select their own
    /// service/publication rather than replacing the latest authority.
    pub fn read(
        &self,
        signed: &SignedReadRequest<ReadWorldViewRequest>,
        origin: &FormalReadPublication,
        history: &FormalReadHistoryV1,
    ) -> Result<ReadWorldViewResponse<WorldServiceProjection>, String> {
        authority::verify_read_request(VIEW_PATH, signed)?;
        let request = &signed.request;
        request.validate_formal_shape().map_err(|e| e.to_string())?;
        let world = &self.current.anchor.head().record().world;
        if request.world.world_id != world.world_id
            || request.world.genesis_digest != world.genesis_digest
        {
            return Err("formal read fixed world identity mismatch".into());
        }
        let mut verifier = FormalReadVerifier::new(&self.trust, origin.anchor.clone())?;
        if request.formal_anchor.as_ref() != Some(&verifier.anchor_ref()?) {
            return Err("formal read caller anchor is unavailable in verified history".into());
        }
        // Certify the origin's full binding before using it as a minimum. A
        // naked height or checkpoint alias is never enough to seed this cache.
        if request.min_commit.is_some() {
            let mut seed_request = request.clone();
            seed_request.min_commit = None;
            seed_request.fixed_commit = None;
            let seed = origin.response(
                &seed_request,
                &signed.subject_public_key,
                &FormalReadHistoryV1 {
                    schema_version: 1,
                    steps: vec![],
                },
                &origin.anchor,
            )?;
            verifier.verify_view(&seed, &seed_request, &signed.subject_public_key)?;
        }
        let response =
            self.current
                .response(request, &signed.subject_public_key, history, &origin.anchor)?;
        let proved = verifier.verify_view(&response, request, &signed.subject_public_key)?;
        if proved.commit().head() != self.current.anchor.head().head()
            || verifier.anchor_ref()?.active_authority_id
                != self
                    .current
                    .anchor
                    .authority()
                    .authority_id()
                    .map_err(|e| e.to_string())?
        {
            return Err("formal read light path differs from publisher's verified cut".into());
        }
        Ok(response)
    }
}

fn committed_reference(
    anchor: &VerifiedFormalReadAnchorV1,
    view: &WorldServiceProjection,
) -> Result<CommitRef, String> {
    let record = anchor.head().record();
    let binding = view
        .runtime_binding
        .as_ref()
        .ok_or("formal projection missing runtime binding")?;
    let header = view
        .formal_commit_header
        .as_ref()
        .ok_or("formal projection missing committed header")?;
    Ok(CommitRef {
        world: WorldIdentity {
            world_id: record.world.world_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
        },
        position: record.position,
        execution_block_hash: record.record_id().map_err(|e| e.to_string())?,
        state_root_ref: record.after_state_root.clone(),
        binding: ExecutionBinding {
            provider_world_id: binding.world_id.clone(),
            branch_id: binding.branch_id.clone(),
            finality_ref: authority::request_digest(
                "finality",
                &(
                    binding.finality_epoch,
                    &binding.finality_status,
                    &binding.finality_block_hash,
                ),
            )?,
            reorg_generation: binding.reorg_epoch,
            governing_manifest_ref: binding.runtime_manifest_hash.to_string(),
            authority_generation: header.authority_generation,
            permission_generation: header.permission_generation,
        },
    })
}

#[cfg(test)]
use crate::formal_read_fixtures as fixtures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_produces_verified_public_read_and_rejects_unproved_minimum() {
        let (world, trust, anchor, _) = fixtures::actual_world_initial_fixture();
        let origin = FormalReadPublication::new(anchor.clone(), world.clone()).unwrap();
        let service = FormalCommittedReadService::new(
            trust.clone(),
            FormalReadPublication::new(anchor.clone(), world).unwrap(),
        )
        .unwrap();
        let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
        let request = ReadWorldViewRequest {
            observation_nonce: Some("07".repeat(32)),
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: WorldIdentity {
                world_id: trust.world.world_id.clone(),
                genesis_digest: trust.world.genesis_digest.clone(),
            },
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
            formal_anchor: Some(verifier.anchor_ref().unwrap()),
        };
        let signed =
            authority::sign_read_request(VIEW_PATH, request.clone(), &hex::encode([7u8; 32]))
                .unwrap();
        let history = FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        };
        let response = service.read(&signed, &origin, &history).unwrap();
        assert_eq!(
            serde_json::to_value(&response.view).unwrap(),
            serde_json::to_value(fixtures::projection(&origin.world)).unwrap()
        );
        assert_eq!(
            response.version.commit,
            fixtures::commit_ref(&anchor, &response.view)
        );
        let verified = verifier
            .verify_view(&response, &request, &signed.subject_public_key)
            .unwrap();
        assert_eq!(verified.commit().head(), anchor.head().head());
        let mut minimum = request;
        minimum.min_commit = Some(response.version.commit.clone());
        let signed =
            authority::sign_read_request(VIEW_PATH, minimum.clone(), &hex::encode([7u8; 32]))
                .unwrap();
        service.read(&signed, &origin, &history).unwrap();
        minimum
            .min_commit
            .as_mut()
            .unwrap()
            .binding
            .permission_generation += 1;
        let signed =
            authority::sign_read_request(VIEW_PATH, minimum, &hex::encode([7u8; 32])).unwrap();
        assert!(service.read(&signed, &origin, &history).is_err());
    }
}
