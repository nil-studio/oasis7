use super::super::LocalInstance;
use super::*;
use crate::formal_live_service::{
    FormalLiveHistoryResolver, FormalLiveService, FormalLiveState, PublishedReadState,
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7::{
    runtime::{ChainResourceDerivationContext, World, blake3_hex},
    world_service::{
        authority,
        formal_read::{FormalReadHistoryV1, FormalReadStepV1, MAX_FORMAL_HISTORY_STEPS},
        formal_read_service::FormalReadPublication,
        wire::SignedServiceResponse,
    },
};
use oasis7_client_api::{
    world_service::{CommitRef, FormalReadAnchorRefV1, WorldIdentity},
    world_service_full_sync::{
        FORMAL_FULL_SYNC_MANIFEST_PATH, FormalFullSyncCutV1, FormalFullSyncManifestRequestV1,
        FormalFullSyncObjectChunkV1, FormalFullSyncObjectRequestV1, FormalNodeReadGrantBodyV1,
        FormalNodeReadGrantV1, FormalNodeReadRoleV1,
    },
};
use oasis7_distfs::controlled_authority::{
    fencing::EndpointInstanceBindingV1,
    formal::*,
    replicated_protocol::{ArtifactObject, ArtifactRole, ClosedRecord, EndpointRole},
};
use oasis7_node::{NodeConfig, NodeRole};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::formal_read_fixtures as fixtures;

#[derive(Clone)]
struct FullSyncCutFixture {
    world: World,
    anchor: VerifiedFormalReadAnchorV1,
    material: ClosedRecord,
    proof: FormalQualifiedCommitProofV1,
}

struct FullSyncResolver {
    cuts: Mutex<Vec<FullSyncCutFixture>>,
    advance_after_material: Mutex<Option<(Arc<PublishedReadState>, FullSyncCutFixture)>>,
    transition_steps: Mutex<BTreeMap<String, Vec<FormalReadStepV1>>>,
}

impl FullSyncResolver {
    fn index_of(
        cuts: &[FullSyncCutFixture],
        candidate: &VerifiedFormalReadAnchorV1,
    ) -> Option<usize> {
        cuts.iter()
            .position(|cut| cut.anchor.checkpoint() == candidate.checkpoint())
    }

    fn push(&self, cut: FullSyncCutFixture) {
        self.cuts.lock().unwrap().push(cut);
    }

    fn advance_after_next_material(
        &self,
        published: Arc<PublishedReadState>,
        cut: FullSyncCutFixture,
    ) {
        *self.advance_after_material.lock().unwrap() = Some((published, cut));
    }
}

impl FormalLiveHistoryResolver for FullSyncResolver {
    fn origin_publication(
        &self,
        request: &FormalReadAnchorRefV1,
    ) -> Result<FormalReadPublication, String> {
        let cuts = self
            .cuts
            .lock()
            .map_err(|_| "full-sync fixture history lock poisoned")?;
        let cut = cuts
            .iter()
            .find(|cut| {
                cut.anchor.head().record().position == request.position
                    && cut.anchor.head().head().record_id == request.record_id
                    && cut.anchor.authority().authority_id().ok().as_deref()
                        == Some(request.active_authority_id.as_str())
            })
            .ok_or("HistoryUnavailable: full-sync fixture cut is unknown")?;
        FormalReadPublication::new(cut.anchor.clone(), cut.world.clone())
    }

    fn fixed_publication(&self, _commit: &CommitRef) -> Result<FormalReadPublication, String> {
        Err("HistoryUnavailable: the full-sync fixture has only its pinned cut".into())
    }

    fn forward_publication(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        live: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadPublication, String> {
        let cuts = self
            .cuts
            .lock()
            .map_err(|_| "full-sync fixture history lock poisoned")?;
        let origin_index = Self::index_of(&cuts, origin)
            .ok_or("HistoryUnavailable: full-sync fixture origin is unknown")?;
        let live_index = Self::index_of(&cuts, live)
            .ok_or("HistoryUnavailable: full-sync fixture live cut is unknown")?;
        if max_steps == 0 || max_steps > MAX_FORMAL_HISTORY_STEPS || live_index < origin_index {
            return Err("HistoryUnavailable: full-sync fixture has no forward history".into());
        }
        let origin_position = cuts[origin_index].anchor.head().record().position;
        let live_position = cuts[live_index].anchor.head().record().position;
        let next = cuts
            .iter()
            .enumerate()
            .skip(origin_index + 1)
            .take(live_index.saturating_sub(origin_index))
            .find(|(_, cut)| {
                let position = cut.anchor.head().record().position;
                (position > origin_position
                    || cut.anchor.checkpoint() != cuts[origin_index].anchor.checkpoint())
                    && position <= live_position
                    && position - origin_position <= max_steps as u64
            })
            .map(|(_, cut)| cut)
            .ok_or("HistoryUnavailable: full-sync fixture has no bounded next cut")?;
        FormalReadPublication::new(next.anchor.clone(), next.world.clone())
    }

    fn history_page(
        &self,
        origin: &VerifiedFormalReadAnchorV1,
        target: &VerifiedFormalReadAnchorV1,
        max_steps: usize,
    ) -> Result<FormalReadHistoryV1, String> {
        let cuts = self
            .cuts
            .lock()
            .map_err(|_| "full-sync fixture history lock poisoned")?;
        let origin_index = Self::index_of(&cuts, origin)
            .ok_or("HistoryUnavailable: full-sync fixture origin is unknown")?;
        let target_index = Self::index_of(&cuts, target)
            .ok_or("HistoryUnavailable: full-sync fixture target is unknown")?;
        let start = cuts[origin_index].anchor.head().record().position;
        let end = cuts[target_index].anchor.head().record().position;
        if max_steps > MAX_FORMAL_HISTORY_STEPS
            || target_index < origin_index
            || end < start
            || end - start > max_steps as u64
        {
            return Err("HistoryUnavailable: full-sync fixture has no history page".into());
        }
        let transitions = self
            .transition_steps
            .lock()
            .map_err(|_| "full-sync fixture transitions lock poisoned")?;
        let mut steps = Vec::new();
        for cut in &cuts[origin_index + 1..=target_index] {
            if let Some(transition) = transitions.get(
                &cut.anchor
                    .checkpoint()
                    .digest()
                    .map_err(|e| e.to_string())?,
            ) {
                steps.extend(transition.iter().cloned());
            } else {
                steps.push(FormalReadStepV1::Commit(Box::new(cut.proof.clone())));
            }
        }
        Ok(FormalReadHistoryV1 {
            schema_version: 1,
            steps,
        })
    }

    fn full_sync_material(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
    ) -> Result<ClosedRecord, String> {
        let cuts = self
            .cuts
            .lock()
            .map_err(|_| "full-sync fixture history lock poisoned")?;
        let index = Self::index_of(&cuts, anchor)
            .ok_or("HistoryUnavailable: full-sync fixture cut differs")?;
        let material = cuts[index].material.clone();
        drop(cuts);
        if let Some((published, next)) = self
            .advance_after_material
            .lock()
            .map_err(|_| "full-sync fixture advance hook lock poisoned")?
            .take()
        {
            self.push(next.clone());
            published.publish(next.world, next.anchor)?;
        }
        Ok(material)
    }
}

fn test_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn test_hash(seed: u8) -> String {
    hex::encode([seed; 32])
}

fn test_sign(bytes: Vec<u8>, seed: u8) -> String {
    hex::encode(test_key(seed).sign(&bytes).to_bytes())
}

fn endpoint(
    role: EndpointRole,
    seed: u8,
    world: &FormalWorldIdentityV1,
) -> EndpointSignerBindingV1 {
    EndpointSignerBindingV1 {
        instance: EndpointInstanceBindingV1 {
            schema_version: 1,
            world_id: world.world_id.clone(),
            chain_id: world.chain_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            authority_epoch: 1,
            endpoint_role: role,
            endpoint_id: format!("full-sync-endpoint-{seed}"),
            endpoint_public_key: hex::encode(test_key(seed).verifying_key().to_bytes()),
            instance_id: test_hash(seed + 10),
            storage_id: test_hash(seed + 20),
            storage_fingerprint: test_hash(seed + 30),
        },
        fault_domain_id: format!("full-sync-domain-{seed}"),
    }
}

fn full_sync_fixture() -> (
    World,
    oasis7::world_service::client::FormalReadTrustConfigV1,
    VerifiedFormalReadAnchorV1,
    ClosedRecord,
    FormalQualifiedCommitProofV1,
) {
    let mut world = oasis7::runtime_host::world::new_production_hardened()
        .try_with_cognition_scheduler(
            oasis7_runtime::SchedulerPolicyV1 {
                schema_version: oasis7_runtime::SchedulerPolicyV1::SCHEMA_VERSION.into(),
                max_total_wakes_per_tick: 1,
                max_wakes_per_agent_per_tick: 1,
                aging_after_ticks: 1,
                max_starvation_ticks: 1,
                initial_priority: 0,
                comparator: oasis7_runtime::SchedulerPolicyV1::COMPARATOR.into(),
                service_order: oasis7_runtime::SchedulerPolicyV1::SERVICE_ORDER.into(),
            },
            1,
        )
        .unwrap();
    world
        .bind_cognition_runtime("formal-fixture-world", "main", 0, None, "pending", 0)
        .unwrap();
    world.initialize_empty_formal_history().unwrap();
    let genesis = test_hash(8);
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: "formal-fixture-world",
            chain_id: "formal-fixture-chain",
            genesis_ref: Some(&genesis),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: world.state().time,
        },
        test_hash(10),
        test_hash(11),
    );
    world = oasis7::runtime_host::world::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let snapshot = world.snapshot();
    let snapshot_bytes = serde_cbor::to_vec(&snapshot).unwrap();
    let journal_bytes = serde_cbor::to_vec(world.journal()).unwrap();
    let state_root = blake3_hex(&snapshot_bytes);
    let identity = FormalWorldIdentityV1 {
        world_id: "formal-fixture-world".into(),
        chain_id: "formal-fixture-chain".into(),
        genesis_digest: genesis,
    };
    let policy = RecoveryPolicyV1 {
        schema_version: 1,
        recovery_issuer_public_key: hex::encode(test_key(1).verifying_key().to_bytes()),
        fencing_provider_public_key: hex::encode(test_key(3).verifying_key().to_bytes()),
        issuer_anchor_public_key: hex::encode(test_key(2).verifying_key().to_bytes()),
        authorized_fencing_resource_bindings: vec![test_hash(35), test_hash(36)],
        allow_complete_prepared: true,
        allow_replay_frozen: true,
        successor_rule: SuccessorRuleV1::IncrementEpochDistinctBindings,
        terminates_at_profile_transition: true,
    };
    let mut role_bytes = BTreeMap::from([
        (ArtifactRole::Result, b"full-sync-result".to_vec()),
        (ArtifactRole::Snapshot, snapshot_bytes),
        (ArtifactRole::Journal, journal_bytes),
        (ArtifactRole::NonceIndex, b"full-sync-nonce-index".to_vec()),
        (
            ArtifactRole::EffectOutbox,
            b"full-sync-effect-outbox".to_vec(),
        ),
        (
            ArtifactRole::ExecutionManifest,
            b"full-sync-execution-manifest".to_vec(),
        ),
        (ArtifactRole::Wasm, b"full-sync-wasm".to_vec()),
        (ArtifactRole::Rules, b"full-sync-rules".to_vec()),
    ]);
    let role_roots = role_bytes
        .iter()
        .map(|(role, bytes)| (*role, blake3_hex(bytes)))
        .collect::<BTreeMap<_, _>>();
    let definition = AuthorityDefinitionV1 {
        schema_version: 1,
        world: identity.clone(),
        profile: FORMAL_PROFILE.into(),
        profile_version: 2,
        epoch: 1,
        writer_public_key: hex::encode(test_key(4).verifying_key().to_bytes()),
        primary: endpoint(EndpointRole::Primary, 5, &identity),
        replica: endpoint(EndpointRole::Replica, 6, &identity),
        recovery_policy_digest: policy.digest().unwrap(),
        predecessor_authority_id: None,
        valid_from_position: 1,
    };
    let governing = world
        .current_cognition_runtime_binding()
        .unwrap()
        .runtime_manifest_hash
        .to_string()
        .strip_prefix("blake3:")
        .unwrap()
        .to_owned();
    let body = InitialActivationBodyV1 {
        schema_version: 1,
        authority: definition,
        recovery_policy: policy.clone(),
        initial_state_root: state_root.clone(),
        runtime_manifest_digest: role_roots[&ArtifactRole::ExecutionManifest].clone(),
        governing_manifest_digest: governing.clone(),
        execution_policy_digest: test_hash(50),
        artifact_roots: role_roots.clone(),
    };
    let certificate = SignedInitialActivationV1 {
        signature_hex: test_sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let trust = InitialActivationTrustV1 {
        issuer_public_key: hex::encode(test_key(9).verifying_key().to_bytes()),
        world: identity.clone(),
        initial_state_root: state_root.clone(),
        governing_manifest_digest: governing.clone(),
        recovery_policy_digest: policy.digest().unwrap(),
    };
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    let input_bytes = initial_activation_input_bytes(&certificate).unwrap();
    let payload_digest = blake3_hex(&input_bytes);
    let mut roots = role_roots;
    roots.insert(ArtifactRole::Input, payload_digest.clone());
    role_bytes.insert(ArtifactRole::Input, input_bytes);
    let projection_root = oasis7::world_service::formal_projection::commitments_for_world(&world)
        .unwrap()
        .root()
        .unwrap();
    let record = CanonicalRecordV1 {
        schema_version: 1,
        durable_set_transition_digest: None,
        kind: FormalRecordKindV1::InitialActivation,
        world: identity.clone(),
        position: 1,
        parent_record_id: identity.genesis_head().unwrap().record_id,
        original_authority_id: authority.authority_id().unwrap(),
        original_epoch: 1,
        activation_ref: authority.activation_ref().into(),
        request_digest: test_hash(51),
        payload_digest: payload_digest.clone(),
        frozen_reservation_id: None,
        runtime_manifest_digest: certificate.body.runtime_manifest_digest.clone(),
        governing_manifest_digest: governing,
        execution_policy_digest: test_hash(50),
        before_state_root: state_root.clone(),
        after_state_root: state_root.clone(),
        action_root: test_hash(52),
        event_root: test_hash(53),
        result_root: test_hash(54),
        projection_root,
        artifact_roots: roots.clone(),
        handoff: None,
    };
    let proof = fixtures::qualified_proof(record, None, &authority, None, None, 4, 5, 6);
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    let objects = role_bytes
        .into_iter()
        .map(|(role, bytes)| ArtifactObject {
            content_hash: roots[&role].clone(),
            bytes,
            references: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut objects = objects;
    objects.sort_by(|left, right| left.content_hash.cmp(&right.content_hash));
    let material = ClosedRecord {
        payload_digest,
        before_state_root: state_root.clone(),
        after_state_root: state_root,
        roots,
        objects,
    };
    oasis7_distfs::controlled_authority::formal::verify_record_material(
        anchor.head().record(),
        &material,
    )
    .unwrap();
    let checkpoint = anchor.checkpoint();
    let formal_trust = oasis7::world_service::client::FormalReadTrustConfigV1 {
        initial_trust: trust,
        checkpoint_digest: checkpoint.digest().unwrap(),
        checkpoint,
    };
    (world, formal_trust, anchor, material, proof)
}

type LiveServiceFixture = (
    crate::formal_live_service::FormalHttpServer,
    String,
    String,
    Arc<Mutex<oasis7_node::NodeRuntime>>,
    Arc<PublishedReadState>,
    Arc<FullSyncResolver>,
);

fn start_live_service(
    world: World,
    trust: oasis7::world_service::client::FormalReadTrustConfigV1,
    anchor: VerifiedFormalReadAnchorV1,
    material: ClosedRecord,
    proof: FormalQualifiedCommitProofV1,
) -> LiveServiceFixture {
    let mut node_config = NodeConfig::new(
        "formal-full-sync-test-node",
        &anchor.head().record().world.world_id,
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_tick_interval(Duration::from_secs(60))
    .unwrap()
    .with_auto_attest_all_validators(false)
    .with_require_execution_on_commit(true)
    .with_require_peer_execution_hashes(false);
    node_config.gossip = None;
    node_config.replication = None;
    node_config.feedback_p2p = None;
    let mut runtime = oasis7_node::NodeRuntime::new(node_config);
    runtime.start().unwrap();
    let node = Arc::new(Mutex::new(runtime));
    let service_signer = test_key(91);
    let identity = WorldIdentity {
        world_id: anchor.head().record().world.world_id.clone(),
        genesis_digest: anchor.head().record().world.genesis_digest.clone(),
    };
    let published = Arc::new(PublishedReadState::default());
    published.publish(world.clone(), anchor.clone()).unwrap();
    let state = FormalLiveState::default();
    state.node_running.store(true, Ordering::Release);
    state.ready.store(true, Ordering::Release);
    let history = Arc::new(FullSyncResolver {
        cuts: Mutex::new(vec![FullSyncCutFixture {
            world,
            anchor,
            material,
            proof,
        }]),
        advance_after_material: Mutex::new(None),
        transition_steps: Mutex::new(BTreeMap::new()),
    });
    let history_service: Arc<dyn FormalLiveHistoryResolver> = history.clone();
    let service = Arc::new(FormalLiveService::new(
        "formal-full-sync-test-node".into(),
        identity,
        service_signer,
        Arc::clone(&node),
        Arc::clone(&published),
        trust,
        history_service,
        state,
        Arc::new(Mutex::new(())),
    ));
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);
    let server = service.serve_loopback(address).unwrap();
    (
        server,
        format!("http://{address}"),
        hex::encode(test_key(91).verifying_key().to_bytes()),
        node,
        published,
        history,
    )
}

fn signed_grant(node: &SigningKey, now: u64, epoch: u64) -> FormalNodeReadGrantV1 {
    FormalNodeReadGrantV1::sign(
        FormalNodeReadGrantBodyV1 {
            schema_version: 1,
            world: WorldIdentity {
                world_id: "formal-fixture-world".into(),
                genesis_digest: test_hash(8),
            },
            chain_id: "formal-fixture-chain".into(),
            node_public_key: hex::encode(node.verifying_key().to_bytes()),
            role: FormalNodeReadRoleV1::StateSync,
            grant_epoch: epoch,
            authority_epoch: epoch,
            not_before_unix_ms: now.saturating_sub(1_000),
            expires_unix_ms: now + 60_000,
        },
        &hex::encode([9; 32]),
    )
    .unwrap()
}

fn post_manifest(
    endpoint: &str,
    request_key: &SigningKey,
    grant: FormalNodeReadGrantV1,
    cut: FormalFullSyncCutV1,
) -> (u16, Vec<u8>) {
    let payload = FormalFullSyncManifestRequestV1 {
        schema_version: 1,
        grant,
        cut,
    };
    let signed = authority::sign_read_request(
        FORMAL_FULL_SYNC_MANIFEST_PATH,
        payload,
        &hex::encode(request_key.to_bytes()),
    )
    .unwrap();
    let mut stream = TcpStream::connect(endpoint.strip_prefix("http://").unwrap()).unwrap();
    let body = serde_json::to_vec(&signed).unwrap();
    write!(
        stream,
        "POST {FORMAL_FULL_SYNC_MANIFEST_PATH} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    let status = std::str::from_utf8(&response[..split])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, response[split..].to_vec())
}

fn post_object(
    endpoint: &str,
    request_key: &SigningKey,
    grant: FormalNodeReadGrantV1,
    cut: FormalFullSyncCutV1,
    content_hash: String,
) -> (u16, Vec<u8>) {
    let payload = FormalFullSyncObjectRequestV1 {
        schema_version: 1,
        grant,
        cut,
        content_hash,
        offset: 0,
        max_bytes: oasis7_client_api::world_service_full_sync::MAX_FULL_SYNC_CHUNK_BYTES as u32,
        retained_history_root_digest: None,
        business_path: None,
    };
    let signed = authority::sign_read_request(
        oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH,
        payload,
        &hex::encode(request_key.to_bytes()),
    )
    .unwrap();
    let mut stream = TcpStream::connect(endpoint.strip_prefix("http://").unwrap()).unwrap();
    let body = serde_json::to_vec(&signed).unwrap();
    write!(
        stream,
        "POST {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH,
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    let status = std::str::from_utf8(&response[..split])
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, response[split..].to_vec())
}

fn cut(anchor: &VerifiedFormalReadAnchorV1) -> FormalFullSyncCutV1 {
    let record = anchor.head().record();
    FormalFullSyncCutV1 {
        world: WorldIdentity {
            world_id: record.world.world_id.clone(),
            genesis_digest: record.world.genesis_digest.clone(),
        },
        chain_id: record.world.chain_id.clone(),
        position: record.position,
        record_id: record.record_id().unwrap(),
        after_state_root: record.after_state_root.clone(),
        active_authority_id: anchor.authority().authority_id().unwrap(),
        finalizing_authority_id: anchor.head().finalizing_authority_id().into(),
        authority_epoch: anchor.authority().definition().epoch,
    }
}

fn same_epoch_successor_fixture(
    world: &World,
    anchor: &VerifiedFormalReadAnchorV1,
    material: &ClosedRecord,
) -> FullSyncCutFixture {
    let authority = anchor.authority();
    let frozen_body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: authority.definition().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition().epoch,
        activation_ref: authority.activation_ref().into(),
        parent: anchor.head().head(),
        next_position: anchor.head().record().position + 1,
        request_digest: test_hash(60),
        payload_digest: anchor.head().record().payload_digest.clone(),
        runtime_manifest_digest: anchor.head().record().runtime_manifest_digest.clone(),
        governing_manifest_digest: anchor.head().record().governing_manifest_digest.clone(),
        execution_policy_digest: anchor.head().record().execution_policy_digest.clone(),
        pre_state_root: anchor.head().record().after_state_root.clone(),
        frozen_material_digest: test_hash(64),
        handoff: None,
    };
    let reservation_id = frozen_body.reservation_id().unwrap();
    let frozen_receipt = |role, binding: &EndpointSignerBindingV1, key| {
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: reservation_id.clone(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: test_sign(frozen_receipt_signing_bytes(&body).unwrap(), key),
            body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        reservation: SignedFormalFrozenV1 {
            signature_hex: test_sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
            body: frozen_body.clone(),
        },
        primary: frozen_receipt(EndpointRole::Primary, &authority.definition().primary, 5),
        replica: frozen_receipt(EndpointRole::Replica, &authority.definition().replica, 6),
    };
    let mut record = anchor.head().record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = frozen_body.next_position;
    record.parent_record_id = frozen_body.parent.record_id;
    record.request_digest = frozen_body.request_digest.clone();
    record.payload_digest = frozen_body.payload_digest.clone();
    record.frozen_reservation_id = Some(reservation_id);
    record.runtime_manifest_digest = frozen_body.runtime_manifest_digest.clone();
    record.governing_manifest_digest = frozen_body.governing_manifest_digest.clone();
    record.execution_policy_digest = frozen_body.execution_policy_digest.clone();
    record.before_state_root = frozen_body.pre_state_root.clone();
    record.after_state_root = frozen_body.pre_state_root;

    let proof = fixtures::qualified_proof(
        record,
        Some(frozen),
        authority,
        Some(anchor.head()),
        None,
        4,
        5,
        6,
    );
    let verified = verify_formal_qualified(&proof, authority, Some(anchor.head()), None).unwrap();
    let successor = anchor
        .after_verified_commit(authority.clone(), verified)
        .unwrap();
    oasis7_distfs::controlled_authority::formal::verify_record_material(
        successor.head().record(),
        material,
    )
    .unwrap();
    FullSyncCutFixture {
        world: world.clone(),
        anchor: successor,
        material: material.clone(),
        proof,
    }
}

fn externally_pinned_recovered_anchor(
    anchor: &VerifiedFormalReadAnchorV1,
    trust: &oasis7::world_service::client::FormalReadTrustConfigV1,
) -> VerifiedFormalReadAnchorV1 {
    // This fixture tests the service's current-epoch grant gate from a typed,
    // independently pinned checkpoint. Recovery-certificate validation is
    // covered by the formal-history suite, not reproduced here.
    let mut checkpoint = anchor.checkpoint();
    let predecessor = anchor.authority();
    let predecessor_id = predecessor.authority_id().unwrap();
    let mut successor = predecessor.definition().clone();
    successor.epoch += 1;
    successor.writer_public_key = hex::encode(test_key(101).verifying_key().to_bytes());
    successor.predecessor_authority_id = Some(predecessor_id.clone());
    successor.valid_from_position = anchor.head().record().position + 1;
    for (binding, key_id, role) in [
        (&mut successor.primary, 102, "primary"),
        (&mut successor.replica, 103, "replica"),
    ] {
        binding.instance.authority_epoch = successor.epoch;
        binding.instance.endpoint_id = format!("full-sync-recovered-{}-{role}", successor.epoch);
        binding.instance.endpoint_public_key =
            hex::encode(test_key(key_id).verifying_key().to_bytes());
        binding.instance.instance_id = test_hash(key_id + 10);
        binding.instance.storage_id = test_hash(key_id + 20);
        binding.instance.storage_fingerprint = test_hash(key_id + 30);
        binding.fault_domain_id = format!("full-sync-recovered-domain-{}-{role}", successor.epoch);
    }
    checkpoint.active_authority = successor;
    checkpoint.first_normal_position = Some(anchor.head().record().position + 2);
    checkpoint.recovery_cut = Some(anchor.head().head());
    checkpoint.retained_record_id = None;
    checkpoint.head_finalizing_authority_id = predecessor_id;
    let digest = checkpoint.digest().unwrap();
    VerifiedFormalReadAnchorV1::from_fixed_pin(&checkpoint, &digest, &trust.initial_trust).unwrap()
}

struct TestPrivateDir(std::path::PathBuf);

impl TestPrivateDir {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-full-sync-test-{}-{}",
            std::process::id(),
            super::super::local_io::random_id().unwrap()
        ));
        super::super::local_io::private_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for TestPrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn signed_http_full_sync_cold_restores_and_reopens_from_the_same_external_pin() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let expected_snapshot = world.snapshot();
    let (mut server, endpoint, service_key, node_runtime, _, _) =
        start_live_service(world, trust.clone(), anchor.clone(), material, proof);
    let node_key = test_key(92);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let grant = signed_grant(&node_key, now, 1);
    let cut = cut(&anchor);
    let state_dir = TestPrivateDir::new();
    let data = state_dir.0.join("data");
    super::super::local_io::private_dir(&data).unwrap();
    let state = data.join("state");
    super::super::local_io::private_dir(&state).unwrap();
    let sync_root = state.join("full-sync");
    let config = FormalFullSyncConfigV1 {
        api_version: API_VERSION.into(),
        endpoint,
        trusted_service_public_key: service_key,
        alternate_endpoints: vec![],
        world_id: trust.initial_trust.world.world_id.clone(),
        chain_id: trust.initial_trust.world.chain_id.clone(),
        genesis_digest: trust.initial_trust.world.genesis_digest.clone(),
        formal_read_trust: trust,
        grant: Some(grant),
        expected_role: FormalNodeReadRoleV1::StateSync,
        poll_interval_ms: 100,
        request_timeout_ms: 2_000,
        max_total_sync_bytes: 64 * 1024 * 1024,
    };
    let instance = LocalInstance {
        api_version: "oasis7.local-node-instance/v1".into(),
        action_id: "full-sync-test-action".into(),
        generation: 1,
        incarnation: "full-sync-test-incarnation".into(),
        config_digest: "a".repeat(64),
        supervisor_pid: std::process::id(),
        runtime_pid: None,
        started_unix_ms: 1,
        state: "running".into(),
        exit_code: None,
    };
    let node_private = hex::encode(node_key.to_bytes());
    let mut session =
        FormalFullSyncSession::new(config.clone(), node_private.clone(), &sync_root, &instance)
            .unwrap();
    let restored = session.sync_latest().unwrap();
    session.set_world_loaded(true);
    assert_eq!(restored.cut, cut);
    assert_eq!(restored.world.snapshot(), expected_snapshot);
    assert!(session.status().state_root_verified);
    assert!(session.status().world_loaded);
    assert!(session.status().ready);
    assert!(!session.status().can_propose);
    drop(session);

    let mut reopened =
        FormalFullSyncSession::new(config, node_private, &sync_root, &instance).unwrap();
    let reopened_world = reopened.sync_latest().unwrap();
    reopened.set_world_loaded(true);
    assert_eq!(reopened_world.world.snapshot(), expected_snapshot);
    assert_eq!(reopened_world.cut, cut);
    assert!(reopened.status().ready);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    for (request_key, grant, requested_cut, expected_status) in [
        (
            test_key(94),
            signed_grant(&node_key, now, 1),
            cut.clone(),
            403,
        ),
        (
            node_key.clone(),
            FormalNodeReadGrantV1::sign(
                FormalNodeReadGrantBodyV1 {
                    not_before_unix_ms: now.saturating_sub(2_000),
                    expires_unix_ms: now.saturating_sub(1_000),
                    ..signed_grant(&node_key, now, 1).body
                },
                &hex::encode([9; 32]),
            )
            .unwrap(),
            cut.clone(),
            403,
        ),
        (
            node_key.clone(),
            signed_grant(&node_key, now, 1),
            FormalFullSyncCutV1 {
                after_state_root: test_hash(222),
                ..cut.clone()
            },
            400,
        ),
    ] {
        let (status, body) = post_manifest(
            &reopened.config.endpoint,
            &request_key,
            grant.clone(),
            requested_cut.clone(),
        );
        assert_eq!(status, expected_status);
        let signed: SignedServiceResponse<serde_json::Value> =
            serde_json::from_slice(&body).unwrap();
        let request = FormalFullSyncManifestRequestV1 {
            schema_version: 1,
            grant,
            cut: requested_cut,
        };
        let signed_request = authority::sign_read_request(
            FORMAL_FULL_SYNC_MANIFEST_PATH,
            request,
            &hex::encode(request_key.to_bytes()),
        )
        .unwrap();
        authority::verify_service_response(
            FORMAL_FULL_SYNC_MANIFEST_PATH,
            &authority::request_digest(FORMAL_FULL_SYNC_MANIFEST_PATH, &signed_request).unwrap(),
            &signed,
            &reopened.config.trusted_service_public_key,
        )
        .unwrap();
    }
    server.stop().unwrap();
    node_runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn signed_full_sync_keeps_an_exact_snapshot_available_while_same_epoch_head_advances() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut server, endpoint, service_key, node_runtime, published, history) = start_live_service(
        world.clone(),
        trust,
        anchor.clone(),
        material.clone(),
        proof,
    );
    let node_key = test_key(92);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let grant = signed_grant(&node_key, now, 1);
    let original_cut = cut(&anchor);
    let (manifest_status, _) =
        post_manifest(&endpoint, &node_key, grant.clone(), original_cut.clone());
    assert_eq!(manifest_status, 200);

    let successor = same_epoch_successor_fixture(&world, &anchor, &material);
    published
        .publish(successor.world.clone(), successor.anchor.clone())
        .unwrap();
    history.push(successor);

    let snapshot_hash = material.roots[&ArtifactRole::Snapshot].clone();
    let (object_status, response) = post_object(
        &endpoint,
        &node_key,
        grant.clone(),
        original_cut.clone(),
        snapshot_hash.clone(),
    );
    assert_eq!(object_status, 200);
    let signed: SignedServiceResponse<FormalFullSyncObjectChunkV1> =
        serde_json::from_slice(&response).unwrap();
    let request = FormalFullSyncObjectRequestV1 {
        schema_version: 1,
        grant: grant.clone(),
        cut: original_cut.clone(),
        content_hash: snapshot_hash.clone(),
        offset: 0,
        max_bytes: oasis7_client_api::world_service_full_sync::MAX_FULL_SYNC_CHUNK_BYTES as u32,
        retained_history_root_digest: None,
        business_path: None,
    };
    let signed_request = authority::sign_read_request(
        oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH,
        request,
        &hex::encode(node_key.to_bytes()),
    )
    .unwrap();
    let request_digest = authority::request_digest(
        oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH,
        &signed_request,
    )
    .unwrap();
    authority::verify_service_response(
        oasis7_client_api::world_service_full_sync::FORMAL_FULL_SYNC_OBJECT_PATH,
        &request_digest,
        &signed,
        &service_key,
    )
    .unwrap();
    let expected_bytes = material
        .objects
        .iter()
        .find(|object| object.content_hash == snapshot_hash)
        .unwrap();
    assert_eq!(signed.payload.cut, original_cut);
    assert_eq!(
        signed.payload.total_bytes,
        expected_bytes.bytes.len() as u64
    );
    assert_eq!(signed.payload.bytes, expected_bytes.bytes);

    server.stop().unwrap();
    node_runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn signed_full_sync_requires_a_new_grant_after_the_published_authority_epoch_changes() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut server, endpoint, _service_key, node_runtime, published, history) = start_live_service(
        world.clone(),
        trust.clone(),
        anchor.clone(),
        material.clone(),
        proof.clone(),
    );
    let recovered = externally_pinned_recovered_anchor(&anchor, &trust);
    history.push(FullSyncCutFixture {
        world: world.clone(),
        anchor: recovered.clone(),
        material,
        proof,
    });
    published.publish(world, recovered.clone()).unwrap();

    let node_key = test_key(92);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let (old_grant_status, _) = post_manifest(
        &endpoint,
        &node_key,
        signed_grant(&node_key, now, 1),
        cut(&anchor),
    );
    assert_eq!(old_grant_status, 403);

    let (renewed_grant_status, _) = post_manifest(
        &endpoint,
        &node_key,
        signed_grant(&node_key, now, 2),
        cut(&recovered),
    );
    assert_eq!(renewed_grant_status, 200);

    server.stop().unwrap();
    node_runtime.lock().unwrap().stop().unwrap();
}

#[test]
fn full_sync_reports_a_proof_verified_lag_when_the_head_advances_during_transfer() {
    let (world, trust, anchor, material, proof) = full_sync_fixture();
    let (mut server, endpoint, service_key, node_runtime, published, history) = start_live_service(
        world.clone(),
        trust.clone(),
        anchor.clone(),
        material.clone(),
        proof,
    );
    let successor = same_epoch_successor_fixture(&world, &anchor, &material);
    history.advance_after_next_material(Arc::clone(&published), successor.clone());
    let node_key = test_key(92);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let config = FormalFullSyncConfigV1 {
        api_version: API_VERSION.into(),
        endpoint,
        trusted_service_public_key: service_key,
        alternate_endpoints: vec![],
        world_id: trust.initial_trust.world.world_id.clone(),
        chain_id: trust.initial_trust.world.chain_id.clone(),
        genesis_digest: trust.initial_trust.world.genesis_digest.clone(),
        formal_read_trust: trust,
        grant: Some(signed_grant(&node_key, now, 1)),
        expected_role: FormalNodeReadRoleV1::StateSync,
        poll_interval_ms: 100,
        request_timeout_ms: 2_000,
        max_total_sync_bytes: 64 * 1024 * 1024,
    };
    let state_dir = TestPrivateDir::new();
    let data = state_dir.0.join("data");
    super::super::local_io::private_dir(&data).unwrap();
    let state = data.join("state");
    super::super::local_io::private_dir(&state).unwrap();
    let sync_root = state.join("full-sync");
    let instance = LocalInstance {
        api_version: "oasis7.local-node-instance/v1".into(),
        action_id: "full-sync-lag-test-action".into(),
        generation: 1,
        incarnation: "full-sync-lag-test-incarnation".into(),
        config_digest: "b".repeat(64),
        supervisor_pid: std::process::id(),
        runtime_pid: None,
        started_unix_ms: 1,
        state: "running".into(),
        exit_code: None,
    };
    let mut session = FormalFullSyncSession::new(
        config,
        hex::encode(node_key.to_bytes()),
        &sync_root,
        &instance,
    )
    .unwrap();
    let restored = session.sync_latest().unwrap();
    assert_eq!(restored.cut, cut(&anchor));
    assert_eq!(session.status().loaded_cut, Some(cut(&anchor)));
    assert_eq!(
        session.status().last_verified_cut,
        Some(cut(&successor.anchor))
    );
    assert_eq!(session.status().lag_positions, Some(1));
    assert!(session.status().catch_up_pending);
    assert!(session.status().state_root_verified);
    assert!(!session.status().world_loaded);
    session.set_world_loaded(true);
    assert!(session.status().ready);
    assert!(!session.status().can_propose);
    let caught_up = session.sync_latest().unwrap();
    assert_eq!(caught_up.cut, cut(&successor.anchor));
    assert_eq!(session.status().loaded_cut, Some(cut(&successor.anchor)));
    assert_eq!(session.status().lag_positions, Some(0));
    assert!(!session.status().catch_up_pending);

    server.stop().unwrap();
    node_runtime.lock().unwrap().stop().unwrap();
}

#[path = "formal_full_sync_cli_process_tests.rs"]
mod cli_process_tests;

#[path = "formal_full_sync_failover_tests.rs"]
mod failover_tests;

#[path = "formal_full_sync_grant_reconcile_tests.rs"]
mod grant_reconcile_tests;
