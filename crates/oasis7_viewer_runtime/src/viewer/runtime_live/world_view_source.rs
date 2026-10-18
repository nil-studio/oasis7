//! Binds display freshness to the verified source actually mapped by Runtime.
use super::*;
use crate::viewer::world_view_authority::{SourceAuthority, spatial_state_digest};
use crate::world_service::verified_view::ReadEvidencePolicy;

impl ViewerRuntimeLiveServer {
    pub(super) fn world_view_source_authority(&self, audience: &str) -> SourceAuthority {
        let Some(config) = &self.config.world_service else {
            return if self.chain_link_enabled() {
                SourceAuthority::unknown()
            } else {
                spatial_state_digest(self.world.state())
                    .map(|digest| {
                        SourceAuthority::local_bound(digest).with_events(
                            &self.world.journal().events,
                            &self.config.world_id,
                            self.reorg_epoch,
                        )
                    })
                    .unwrap_or_else(SourceAuthority::unknown)
            };
        };
        let Some(view) = &self.verified_world_view else {
            return SourceAuthority::unknown();
        };
        let Ok(reader) = config.read_authority() else {
            return SourceAuthority::unknown();
        };
        if view.version().commit.world != config.expected_world
            || self.config.world_id != config.expected_world.world_id
            || view.version().visibility_scope != config.scope_id
            || !view.belongs_to(&reader)
            || view.evidence_policy() != ReadEvidencePolicy::Ordinary
            || view.execution_evidence_scope().is_some()
            || view.logical_tick() != self.world.state().time
        {
            return SourceAuthority::unknown();
        }
        let Some(actual_digest) = spatial_state_digest(self.world.state()) else {
            return SourceAuthority::unknown();
        };
        if spatial_state_digest(&view.projection().state).as_ref() != Some(&actual_digest) {
            return SourceAuthority::unknown();
        }
        let version = view.version();
        let execution = &version.commit.binding;
        let audience_hash = blake3::hash(audience.as_bytes()).to_hex().to_string();
        let evidence = if view.formal_commit().is_some() {
            "formal_verified"
        } else {
            "signed_service_assertion"
        };
        let binding = serde_json::json!({"version":version,"viewer_epoch":self.reorg_epoch.to_string(),"audience_hash":audience_hash,"evidence":evidence});
        let scope_binding = serde_json::json!({
            "world":version.commit.world,"provider_world_id":execution.provider_world_id,
            "branch_id":execution.branch_id,"reorg_generation":execution.reorg_generation.to_string(),
            "authority_generation":execution.authority_generation.to_string(),"permission_generation":execution.permission_generation.to_string(),
            "governing_manifest_ref":execution.governing_manifest_ref,"projection_revision":version.projection_revision,
            "visibility_scope":version.visibility_scope,"viewer_epoch":self.reorg_epoch.to_string(),
            "audience_hash":audience_hash,"evidence":evidence,
            "contract_version":oasis7_client_api::world_service::WORLD_SERVICE_CONTRACT_VERSION,
            "reader_binding":reader.cache_partition_key()
        });
        SourceAuthority::remote(binding, scope_binding, actual_digest).with_events(
            &view.projection().events,
            &self.config.world_id,
            self.reorg_epoch,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_service::{
        client::WorldServiceClientConfig, projection::WorldServiceProjection,
        verified_view::VerifiedWorldView,
    };
    use oasis7_client_api::world_service::*;

    fn authenticated_fixture_view(
        response: ReadWorldViewResponse<WorldServiceProjection>,
        request: &ReadWorldViewRequest,
        config: &WorldServiceClientConfig,
        policy: ReadEvidencePolicy,
    ) -> VerifiedWorldView {
        use crate::world_service::{authority, client::RemoteWorldServiceClient};
        let signed =
            authority::sign_read_request(VIEW_PATH, request.clone(), &config.read_private_key_hex)
                .unwrap();
        let digest = authority::request_digest(VIEW_PATH, &signed).unwrap();
        let response =
            authority::sign_service_response(VIEW_PATH, digest, response, &hex::encode([9; 32]))
                .unwrap();
        RemoteWorldServiceClient::new_with_evidence_policy(config.clone(), policy)
            .unwrap()
            .accept_view_response(response, signed)
            .unwrap()
    }
    fn fixture() -> ViewerRuntimeLiveServer {
        fixture_bound(1, 1, false)
    }
    fn fixture_bound(position: u64, permission: u64, controlled: bool) -> ViewerRuntimeLiveServer {
        let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
            WorldScenario::Minimal,
        ))
        .unwrap();
        server.config.world_id = "view-world".into();
        let mut state = crate::runtime::WorldState::default();
        state.agents.insert(
            "a".into(),
            crate::runtime::AgentCell::new(
                crate::models::AgentState::new("a", crate::GeoPos::new(10, 20, 30)),
                0,
            ),
        );
        server.world = crate::runtime_host::world::new_with_state(state);
        let world = WorldIdentity {
            world_id: "view-world".into(),
            genesis_digest: "genesis".into(),
        };
        let config = WorldServiceClientConfig {
            endpoint: "http://127.0.0.1:1".into(),
            trusted_service_public_key: crate::world_service::authority::sign_read_request(
                "fixture",
                (),
                &hex::encode([9; 32]),
            )
            .unwrap()
            .subject_public_key,
            expected_world: world.clone(),
            scope_id: "public".into(),
            read_private_key_hex: hex::encode([7; 32]),
            timeout: Duration::from_secs(1),
            max_response_bytes: 2 * 1024 * 1024,
            formal_read_trust: None,
        };
        let request = ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: world.clone(),
            scope_id: "public".into(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        let commit = CommitRef {
            world,
            binding: ExecutionBinding {
                provider_world_id: "view-world".into(),
                branch_id: "branch".into(),
                finality_ref: "finality".into(),
                reorg_generation: 0,
                governing_manifest_ref: "manifest".into(),
                authority_generation: 1,
                permission_generation: permission,
            },
            position,
            execution_block_hash: "block".into(),
            state_root_ref: "root".into(),
        };
        let mut projection = WorldServiceProjection::from_world(&server.world, None).unwrap();
        projection.runtime_binding = None;
        let response = ReadWorldViewResponse {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            formal_profile: None,
            formal_proof: None,
            execution_evidence_scope: controlled.then(|| {
                crate::world_service::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.to_string()
            }),
            version: ProjectionVersion {
                commit: commit.clone(),
                projection_revision: "projection-v1".into(),
                visibility_scope: "public".into(),
            },
            logical_tick: server.world.state().time,
            continuation: EventCursor {
                stream_id: "events".into(),
                scope_id: "public".into(),
                era: 0,
                sequence: 1,
                commit,
            },
            view: projection,
        };
        server.verified_world_view = Some(authenticated_fixture_view(
            response,
            &request,
            &config,
            if controlled {
                ReadEvidencePolicy::AllowControlledPrerequisite
            } else {
                ReadEvidencePolicy::Ordinary
            },
        ));
        server.config.world_service = Some(config);
        server
    }
    #[test]
    fn verified_source_binds_current_to_actual_snapshot_without_private_reader_identity() {
        let mut server = fixture();
        let source = server.world_view_source_authority("alice");
        assert!(source.current());
        assert_eq!(source.source_ref(), "verified_world_service_cut");
        let snapshot = server.compat_snapshot_for_session(&mut RuntimeLiveSession::new());
        assert!(source.validate_snapshot(&snapshot));
        let mut tampered = snapshot.clone();
        tampered.model.agents.get_mut("a").unwrap().pos.x_cm += 1;
        assert!(!source.validate_snapshot(&tampered));
        let mut missing = snapshot.clone();
        missing.runtime_snapshot = None;
        assert!(!source.validate_snapshot(&missing));
        assert_eq!(
            source.binding().unwrap()["version"]["commit"]["position"],
            "1"
        );
        let wire = source.binding().unwrap().to_string();
        assert!(!wire.contains("local-read-authority"));
        assert!(!wire.contains(&hex::encode([7; 32])));
    }
    #[test]
    fn stale_local_projection_or_reader_config_cannot_promote_remote_freshness() {
        let mut server = fixture();
        let mut changed = server.world.state().clone();
        changed.agents.get_mut("a").unwrap().state.pos.x_cm += 1;
        server.world = crate::runtime_host::world::new_with_state(changed);
        assert!(!server.world_view_source_authority("alice").current());
        let mut server = fixture();
        server
            .config
            .world_service
            .as_mut()
            .unwrap()
            .read_private_key_hex = hex::encode([8; 32]);
        assert!(!server.world_view_source_authority("alice").current());
        let first = fixture().world_view_source_authority("alice");
        let config = server.config.world_service.as_ref().unwrap();
        let view = server.verified_world_view.as_ref().unwrap();
        let response = ReadWorldViewResponse {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            formal_profile: None,
            formal_proof: None,
            execution_evidence_scope: view.execution_evidence_scope().map(str::to_owned),
            version: view.version().clone(),
            logical_tick: view.logical_tick(),
            continuation: view.continuation().clone(),
            view: view.projection().clone(),
        };
        let request = ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: config.expected_world.clone(),
            scope_id: config.scope_id.clone(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        server.verified_world_view = Some(authenticated_fixture_view(
            response,
            &request,
            config,
            ReadEvidencePolicy::Ordinary,
        ));
        let rebound = server.world_view_source_authority("alice");
        assert!(rebound.current());
        assert_ne!(
            first.scope_key(),
            rebound.scope_key(),
            "new reader identity must not reuse old cache"
        );
        assert_eq!(
            first.binding(),
            rebound.binding(),
            "private reader binding stays out of wire data"
        );
        let mut server = fixture();
        server.config.world_service.as_mut().unwrap().scope_id = "agent:a".into();
        assert!(!server.world_view_source_authority("alice").current());
        let mut server = fixture();
        server.config.world_id = "other-world".into();
        assert!(!server.world_view_source_authority("alice").current());
    }
    #[test]
    fn audience_and_viewer_epoch_fence_frozen_scope_without_using_commit_position() {
        let mut server = fixture();
        let first = server.world_view_source_authority("alice");
        assert_ne!(
            first.scope_key(),
            server.world_view_source_authority("bob").scope_key()
        );
        server.reorg_epoch += 1;
        assert_ne!(
            first.scope_key(),
            server.world_view_source_authority("alice").scope_key()
        );
    }
    #[test]
    fn commit_advancement_can_finish_frozen_pages_but_permission_change_cannot() {
        let first = fixture_bound(1, 1, false).world_view_source_authority("alice");
        let next = fixture_bound(2, 1, false).world_view_source_authority("alice");
        assert!(next.current());
        assert_eq!(first.scope_key(), next.scope_key());
        assert_ne!(first.binding(), next.binding());
        let changed = fixture_bound(2, 2, false).world_view_source_authority("alice");
        assert_ne!(first.scope_key(), changed.scope_key());
        assert!(
            !fixture_bound(1, 1, true)
                .world_view_source_authority("alice")
                .current()
        );
    }
    #[test]
    fn display_proof_ignores_private_intent_but_covers_spatial_dependencies() {
        let server = fixture();
        let mut state = server.world.state().clone();
        let digest = spatial_state_digest(&state);
        // Private cognition journals/owner intent are not source authority for geometry.
        let intent=serde_json::from_value(serde_json::json!({"schema_version":2,"agent_id":"a","intent_id":"private","kind":"goal","summary":"PRIVATE_COGNITION","status":"active","source":"owner","logical_time":0,"event_seq":0,"updated_at":0})).unwrap();
        state.agents.get_mut("a").unwrap().intent = Some(intent);
        assert_eq!(digest, spatial_state_digest(&state));
        state.agent_intent_ledger.insert(
            "a".into(),
            state.agents.get("a").unwrap().intent.clone().unwrap(),
        );
        let private_ledger = (0..10_000)
            .map(|i| (format!("PRIVATE_UNRELATED_LEDGER_{i}"), i as i64))
            .collect();
        state.material_ledgers.insert(
            crate::runtime::MaterialLedgerId::Agent("private".into()),
            private_ledger,
        );
        assert_eq!(
            digest,
            spatial_state_digest(&state),
            "unrelated ledgers and private cognition cannot change spatial identity"
        );
        state.module_visual_entities.insert(
            "facility".into(),
            oasis7_agent_api::world_entity::ModuleVisualEntity::default(),
        );
        assert_ne!(digest, spatial_state_digest(&state));
        let mut state = server.world.state().clone();
        state.logistics_routes.insert(
            "route".into(),
            crate::runtime::LogisticsRouteV1 {
                route_id: "route".into(),
                from_ledger: crate::runtime::MaterialLedgerId::World,
                to_ledger: crate::runtime::MaterialLedgerId::Agent("a".into()),
                kind: "freight".into(),
                distance_km: 1,
                priority: Default::default(),
                owner_agent_id: "a".into(),
                available: true,
                capacity_units: 1,
                reserved_capacity_units: 0,
                tariff_electricity_per_unit: 0,
            },
        );
        assert_ne!(digest, spatial_state_digest(&state));
        let mut state = server.world.state().clone();
        state
            .location_anchors
            .insert("site".into(), crate::runtime::LocationAnchorV1::default());
        assert_ne!(digest, spatial_state_digest(&state));
        let mut state = server.world.state().clone();
        state.factory_site_authorities.insert(
            "site".into(),
            crate::runtime::FactorySiteAuthorityV1::default(),
        );
        assert_ne!(digest, spatial_state_digest(&state));
    }
    #[test]
    fn observer_and_unbound_assertion_are_unknown() {
        let mut server = fixture();
        server.verified_world_view = None;
        assert!(!server.world_view_source_authority("alice").current());
        server.config.world_service = None;
        server.config.chain_status_bind = Some("127.0.0.1:1".into());
        assert!(!server.world_view_source_authority("alice").current());
    }
}
