//! Shared test-only response builder, compiled with the owning verifier types.
use crate::{
    authority,
    formal_projection::ProjectionAudienceV1,
    formal_read::{FormalReadHistoryV1, FormalReadVerifier},
    formal_read_fixtures::{commit_ref, projection, public},
    projection::WorldServiceProjection,
};
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::formal::{
    InitialActivationTrustV1, VerifiedFormalReadAnchorV1,
};
pub(crate) fn response(
    world: &oasis7_runtime::World,
    anchor: &VerifiedFormalReadAnchorV1,
    history: FormalReadHistoryV1,
) -> (
    ReadWorldViewRequest,
    ReadWorldViewResponse<WorldServiceProjection>,
) {
    let view = projection(world);
    let commit = commit_ref(anchor, &view);
    let cache = FormalReadVerifier::new(
        &InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: anchor.authority().definition().world.clone(),
            initial_state_root: anchor.initial_certificate().body.initial_state_root.clone(),
            governing_manifest_digest: anchor
                .initial_certificate()
                .body
                .governing_manifest_digest
                .clone(),
            recovery_policy_digest: anchor.authority().policy().digest().unwrap(),
        },
        anchor.clone(),
    )
    .unwrap();
    let anchor_ref = cache.anchor_ref().unwrap();
    let inclusion = crate::formal_projection::commitments_for_world(world)
        .unwrap()
        .inclusion(&ProjectionAudienceV1::public())
        .unwrap();
    let request = ReadWorldViewRequest {
        observation_nonce: Some("07".repeat(32)),
        formal_anchor: Some(anchor_ref.clone()),
        contract_version: 1,
        world: commit.world.clone(),
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let response = ReadWorldViewResponse {
        formal_profile: Some(FormalReadProfileV1::controlled()),
        formal_proof: Some(FormalReadSidecarV1 {
            schema_version: 1,
            profile: FormalReadProfileV1::controlled(),
            anchor: anchor_ref,
            history_cbor_hex: history.encode_hex().unwrap(),
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
        execution_evidence_scope: None,
        contract_version: 1,
        logical_tick: view.state.time,
        version: ProjectionVersion {
            commit: commit.clone(),
            projection_revision: authority::request_digest("projection", &view).unwrap(),
            visibility_scope: "public".into(),
        },
        continuation: EventCursor {
            stream_id: authority::request_digest("formal-events-v1", &(&commit.world, &"public"))
                .unwrap(),
            scope_id: "public".into(),
            era: view.formal_commit_header.as_ref().unwrap().event_id_era,
            sequence: view
                .formal_commit_header
                .as_ref()
                .unwrap()
                .journal_high_watermark,
            commit,
        },
        view,
    };
    (request, response)
}
