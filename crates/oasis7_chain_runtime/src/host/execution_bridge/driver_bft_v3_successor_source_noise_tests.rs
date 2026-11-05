//! Real encrypted component transport with an independently keyed purpose
//! issuer fixture. This is not a deployed issuer/witness or failure domain.
use super::*;
use oasis7_net::{AuthenticatedPeerRequestContext, Libp2pNetwork, Libp2pNetworkConfig};
use oasis7_node::*;
use std::collections::BTreeMap;
#[path = "driver_bft_v3_source_provider_egress_tests.rs"]
mod provider_egress;
pub(super) use provider_egress::{DeferredOriginalReader, ProviderEgressFixture};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
struct Issuer {
    state: Arc<NodeBftPeerExecutionStateV3>,
    requester: String,
    serving: String,
    grants: Mutex<BTreeMap<String, BftPrivateRetentionAdmissionReadGrantV1>>,
    revoked: std::sync::atomic::AtomicBool,
}
impl NodeBftAdmissionInputReadCredentialsV1 for Issuer {
    fn authorize(
        &self,
        read: &NodeBftAdmissionInputReadRequestV1,
        deadline: Instant,
    ) -> Result<NodeBftAdmissionInputAuthorizationV1, String> {
        if Instant::now() >= deadline {
            return Err("issuer original deadline expired".into());
        }
        let view = self.state.view()?.ok_or("issuer current view absent")?;
        if self.revoked.load(std::sync::atomic::Ordering::Acquire)
            || read.requester_peer_id != self.requester
            || read.serving_peer_id != self.serving
        {
            return Err("issuer endpoints/revocation rejected".into());
        }
        let subject = fixtures::key(102);
        let subject_public_key = hex::encode(subject.verifying_key().to_bytes());
        let mut grants = self.grants.lock().map_err(|_| "issuer grant lock")?;
        let grant = if let Some(grant) = grants.get(&read.nonce) {
            if grant.body.reference != read.reference
                || grant.body.view_generation != view.generation
            {
                return Err("issuer original nonce ref/view changed".into());
            }
            grant.clone()
        } else {
            let start = now_ms();
            let grant = BftPrivateRetentionAdmissionReadGrantV1::sign(
                BftPrivateRetentionAdmissionReadGrantBodyV1 {
                    schema_version: 1,
                    purpose: BftPrivateRetentionAdmissionReadPurposeV1::OriginalAdmissionInput,
                    scope: view.schedule.scope().clone(),
                    reference: read.reference.clone(),
                    subject_public_key: subject_public_key.clone(),
                    requester_peer_id: self.requester.clone(),
                    serving_peer_id: self.serving.clone(),
                    schedule_digest: view.schedule.digest()?,
                    view_generation: view.generation,
                    request_nonce: read.nonce.clone(),
                    grant_epoch: 1,
                    not_before_unix_ms: start,
                    expires_unix_ms: start + 120_000,
                },
                &MemoryTestSignerKey(fixtures::key(101)),
            )
            .map_err(|e| e.to_string())?;
            grants.insert(read.nonce.clone(), grant.clone());
            grant
        };
        Ok(NodeBftAdmissionInputAuthorizationV1 {
            signature_hex: hex::encode(subject.sign(&read.signing_bytes(&grant)?).to_bytes()),
            subject_public_key,
            grant,
        })
    }
}
impl NodeBftAdmissionInputReadGateV1 for Issuer {
    fn validate_identity(
        &self,
        authorization: &NodeBftAdmissionInputAuthorizationV1,
    ) -> Result<(), String> {
        authorization
            .grant
            .verify_signature(&hex::encode(fixtures::key(101).verifying_key().to_bytes()))
            .map_err(|e| e.to_string())
    }
    fn authorize(
        &self,
        transport: &AuthenticatedPeerRequestContext,
        authorization: &NodeBftAdmissionInputAuthorizationV1,
        scope: &NodeBftAdmissionInputReadScopeV1<'_>,
        make_response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        let check = || -> Result<(), String> {
            let grant = &authorization.grant;
            grant
                .verify_signature(&hex::encode(fixtures::key(101).verifying_key().to_bytes()))
                .map_err(|e| e.to_string())?;
            let current = self.state.view()?.ok_or("fresh issuer view absent")?;
            if self.revoked.load(std::sync::atomic::Ordering::Acquire)
                || !self.state.is_serving()
                || current.generation != scope.view.generation()
                || grant.body.grant_epoch != 1
                || grant.body.view_generation != current.generation
                || grant.body.scope != *current.schedule.scope()
                || grant.body.schedule_digest != current.schedule.digest()?
                || grant.body.reference != scope.reference
                || grant.body.requester_peer_id != self.requester
                || grant.body.serving_peer_id != self.serving
                || scope.requester_peer_id != self.requester
                || scope.serving_peer_id != self.serving
                || transport.peer_id().to_string() != self.requester
                || authorization.subject_public_key
                    != hex::encode(fixtures::key(102).verifying_key().to_bytes())
                || grant.body.subject_public_key != authorization.subject_public_key
                || now_ms() < grant.body.not_before_unix_ms
                || now_ms() >= grant.body.expires_unix_ms
            {
                return Err("independent issuer exact current grant rejected".into());
            }
            let originals = self.grants.lock().map_err(|_| "issuer originals lock")?;
            if originals.get(&grant.body.request_nonce) != Some(grant) {
                return Err("issuer permission was not independently issued".into());
            }
            Ok(())
        };
        check()?;
        let remaining = authorization
            .grant
            .body
            .expires_unix_ms
            .saturating_sub(now_ms());
        let deadline = Instant::now() + Duration::from_millis(remaining);
        transport
            .restrict_reply_until(deadline)
            .map_err(|_| "admission fixture reply deadline restriction rejected")?;
        let reply = make_response(deadline)?;
        check()?;
        Ok(reply)
    }
}
struct NoOldRead;
impl NodeBftPrivateReadGateV3 for NoOldRead {
    fn authorize(
        &self,
        _: &AuthenticatedPeerRequestContext,
        _: &NodeBftPrivateReadAuthorizationV3,
        _: &NodeBftPrivateReadScopeV3,
        _: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        Err("old private read is not admission permission".into())
    }
}
pub(super) struct NoiseFixture {
    pub(super) publisher: Arc<NodeBftAdmissionInputPublisherV1>,
    pub(super) reader: Arc<NodeBftLibp2pAdmissionInputReaderV1>,
    _networks: [Arc<Libp2pNetwork>; 2],
    _server: NodeBftLibp2pAdmissionInputServerV1,
    private: Arc<NodeBftLibp2pPrivateCandidateExchangeV3>,
    public: Arc<NodeBftLibp2pPeerTransportV3>,
    state: Arc<NodeBftPeerExecutionStateV3>,
    hook: NodeBftSharedExecutionHookV3,
    provider_scope: NodeBftPrivateTransportScopeV1,
}
impl NoiseFixture {
    pub(super) fn publish_from_lease_for_test(
        &self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
    ) {
        self.state
            .publish_from_lease_for_test(binding, lease)
            .unwrap();
    }

    pub(super) fn new(
        binding: &BftSignerBindingV3,
        actual: Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    ) -> Self {
        let hook: NodeBftSharedExecutionHookV3 = Arc::new(Mutex::new(Box::new(ActualHook(
            actual,
            None,
            CandidateRetentionProfile::PublicationReserved,
        ))));
        Self::new_with_hook(binding, hook)
    }
    pub(super) fn new_with_hook(
        binding: &BftSignerBindingV3,
        hook: NodeBftSharedExecutionHookV3,
    ) -> Self {
        Self::new_with_hook_and_original_reader(binding, hook, None)
    }
    pub(super) fn new_with_hook_and_original_reader(
        binding: &BftSignerBindingV3,
        hook: NodeBftSharedExecutionHookV3,
        recovery_reader: Option<
            Arc<dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1>,
        >,
    ) -> Self {
        let networks = std::array::from_fn(|_| {
            Arc::new(Libp2pNetwork::new(Libp2pNetworkConfig {
                listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
                request_response_timeout: Duration::from_secs(10),
                ..Default::default()
            }))
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        while networks.iter().any(|n| n.listening_addrs().is_empty()) {
            assert!(Instant::now() < deadline, "admission Noise listeners");
            thread::sleep(Duration::from_millis(10));
        }
        networks[1]
            .dial(networks[0].listening_addrs()[0].clone())
            .unwrap();
        while networks.iter().any(|n| n.connected_peers().is_empty()) {
            assert!(Instant::now() < deadline, "admission Noise connection");
            thread::sleep(Duration::from_millis(10));
        }
        let state = Arc::new(NodeBftPeerExecutionStateV3::default());
        let private = Arc::new(
            NodeBftLibp2pPrivateCandidateExchangeV3::new(
                networks[0].clone(),
                NodeBftPrivateExchangeConfigV3 {
                    binding: binding.clone(),
                    candidate_policy_digest: "ad".repeat(32),
                    candidate_grant_generation: 1,
                    peers: vec![networks[1].peer_id().to_string()],
                    operation_timeout: Duration::from_secs(120),
                },
                state.clone(),
                hook.clone(),
                Arc::new(NoOldRead),
                None,
            )
            .unwrap(),
        );
        let issuer = Arc::new(Issuer {
            state: state.clone(),
            requester: networks[1].peer_id().to_string(),
            serving: networks[0].peer_id().to_string(),
            grants: Mutex::new(BTreeMap::new()),
            revoked: std::sync::atomic::AtomicBool::new(false),
        });
        let publisher = NodeBftAdmissionInputPublisherV1::new(private.transport_buffers());
        let publisher = Arc::new(match recovery_reader {
            Some(reader) => publisher.with_original_reader(reader),
            None => publisher,
        });
        hook.lock()
            .unwrap()
            .configure_bft_v3_admission_input_io(Arc::new(publisher.clone()))
            .unwrap();
        let server = NodeBftLibp2pAdmissionInputServerV1::new(
            networks[0].clone(),
            NodeBftAdmissionInputServerConfigV1 {
                binding: binding.clone(),
                peers: vec![networks[1].peer_id().to_string()],
            },
            state.clone(),
            private.clone(),
            publisher.clone(),
            issuer.clone(),
        )
        .unwrap();
        let provider_scope = NodeBftPrivateTransportScopeV1::new(
            vec![networks[0].peer_id().to_string()],
            Duration::from_secs(120),
        )
        .unwrap();
        let reader = Arc::new(
            NodeBftLibp2pAdmissionInputReaderV1::new(
                networks[1].clone(),
                networks[0].peer_id().to_string(),
                issuer,
                provider_scope.buffers(),
            )
            .unwrap(),
        );
        let public = Arc::new(
            NodeBftLibp2pPeerTransportV3::new(
                networks[0].clone(),
                oasis7_consensus::tendermint_v3::BftPeerContextV3::from_handoff(binding.handoff()),
                vec![],
            )
            .unwrap(),
        );
        Self {
            publisher,
            reader,
            _networks: networks,
            _server: server,
            private,
            public,
            state,
            hook,
            provider_scope,
        }
    }
    pub(super) fn runtime(
        &self,
        root: &Path,
        binding: &BftSignerBindingV3,
        address: SocketAddr,
        role: BftPrivateRetentionAttestorV1,
    ) -> NodeRuntime {
        self.try_runtime(root, binding, address, role).unwrap()
    }
    fn try_runtime(
        &self,
        root: &Path,
        binding: &BftSignerBindingV3,
        address: SocketAddr,
        role: BftPrivateRetentionAttestorV1,
    ) -> Result<NodeRuntime, NodeError> {
        let provider = RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(120),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: hex::encode(fixtures::key(116).verifying_key().to_bytes()),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap()
        .with_private_retention_attestor(role)
        .unwrap();
        let config = NodeConfig::new(
            validator_id(0),
            binding.scope.world_id.clone(),
            NodeRole::Sequencer,
        )
        .unwrap()
        .with_tick_interval(Duration::from_millis(200))
        .unwrap()
        .with_replication(NodeReplicationConfig::new(root.join("replication")).unwrap());
        NodeRuntime::new(config)
            .with_replication_network_consensus_enabled(false)
            .try_with_shared_execution_hook(self.hook.clone())
            .unwrap()
            .with_tendermint_v3_network(
                binding.clone(),
                Arc::new(provider),
                self.public.clone(),
                self.private.clone(),
                self.state.clone(),
            )
    }
}

#[test]
fn actual_standalone_constructor_accepts_private_admission_without_public_peer() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
            let root = Scratch::new();
            let store = cas(root.path());
            let (_, binding, _, source) = SourceFixture::prepare(&h, &root, &store);
            assert!(source.noise.public.peers().is_empty());
            let runtime = source.noise.try_runtime(
                root.path(),
                &binding,
                "127.0.0.1:9".parse().unwrap(),
                source.role.clone(),
            );
            assert!(
                runtime.is_ok(),
                "qualified single-local Node must admit its independent private plane"
            );
            assert!(!source.noise.state.is_serving());
            assert!(
                source.noise.state.view().unwrap().is_none(),
                "constructor must not synthesize a serving execution view"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn actual_multivalidator_constructor_rejects_empty_public_routes() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            let h = qualified_handoff_v3_with_count_and_setup(2, |_| {});
            let root = Scratch::new();
            let store = cas(root.path());
            let actual = Arc::new(Mutex::new(
                FormalBftHeaderNodeDriverV3::open_from_handoff_with_peer_network(
                    &root.path().join("ledger"),
                    store.clone(),
                    h.anchor.clone(),
                    h.target.clone(),
                    h.material.clone(),
                    h.reader.clone(),
                )
                .unwrap(),
            ));
            let binding = signer_binding(&actual);
            let noise = NoiseFixture::new(&binding, actual);
            let namespace = BftPrivateRetentionFreshSeedV1::generate()
                .unwrap()
                .bind_source(&binding, "a1".repeat(32))
                .unwrap();
            let role = source_creation_tests::role(&binding, namespace.marker());
            let rejected = match noise.try_runtime(
                root.path(),
                &binding,
                "127.0.0.1:9".parse().unwrap(),
                role,
            ) {
                Ok(_) => {
                    panic!("multiple validators must retain nonempty public route requirement")
                }
                Err(error) => error,
            };
            assert!(format!("{rejected:?}").contains("peer routes missing or oversized"));
        })
        .unwrap()
        .join()
        .unwrap();
}
