//! Real-gate/provider Noise consumer; no substitute accepting gate.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReaderV1, BftPrivateRetentionAdmissionInputRefV1,
    BftPrivateRetentionAdmissionInputV1,
};
use oasis7_net::{Libp2pNetwork, Libp2pNetworkConfig};
use oasis7_node::{
    NodeBftAdmissionInputServerConfigV2, NodeBftBackendAdmissionInputSourceV1,
    NodeBftLibp2pAdmissionInputReaderV2, NodeBftLibp2pAdmissionInputServerV2,
    NodeBftPrivateTransportScopeV1,
};

pub(super) fn read_actual_provider_original_v2(
    service: Arc<BftSignerServiceV3>,
    backend: BftPrivateRetentionMemoryBackendV1,
    binding: &BftSignerBindingV3,
    reference: &BftPrivateRetentionAdmissionInputRefV1,
    expected: &BftPrivateRetentionAdmissionInputV1,
    provider_seed: u8,
) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let provider_key = service
        .with_observed_original_input_view(deadline, |v| {
            assert_eq!(v.binding_digest(), binding.digest()?.as_str());
            assert_eq!(v.schedule().scope(), &binding.scope);
            eprintln!(
                "batch-v2 actual schedule authority_epoch={}",
                v.schedule().authority_epoch()
            );
            Ok(v.provider_public_key().to_owned())
        })
        .unwrap();
    let (gate, issuer, target, issuer_key) =
        crate::node_lifecycle::formal_full_sync::tests::read_gate_tests::admission_noise_gate_bridge(
            &binding.scope,
            104,
            provider_seed,
            &provider_key,
            105,
        );
    let address = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let mut issuer_server = issuer.serve_loopback(address).unwrap();
    let networks: [Arc<Libp2pNetwork>; 2] = std::array::from_fn(|_| {
        Arc::new(Libp2pNetwork::new(Libp2pNetworkConfig {
            listen_addrs: vec!["/ip4/127.0.0.1/tcp/0".parse().unwrap()],
            request_response_timeout: Duration::from_secs(10),
            ..Default::default()
        }))
    });
    while networks.iter().any(|n| n.listening_addrs().is_empty()) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    networks[0]
        .dial(networks[1].listening_addrs()[0].clone())
        .unwrap();
    while networks.iter().any(|n| n.connected_peers().is_empty()) {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let serving_scope = NodeBftPrivateTransportScopeV1::new(
        vec![networks[0].peer_id().to_string()],
        Duration::from_secs(120),
    )
    .unwrap();
    let receiving_scope = NodeBftPrivateTransportScopeV1::new(
        vec![networks[1].peer_id().to_string()],
        Duration::from_secs(120),
    )
    .unwrap();
    let source = Arc::new(
        NodeBftBackendAdmissionInputSourceV1::new(
            binding.digest().unwrap(),
            Box::new(backend.clone()),
            serving_scope.buffers(),
        )
        .unwrap(),
    );
    let oracle = Arc::new(FeeOracle {
        backend: std::sync::Mutex::new(backend),
        binding_digest: binding.digest().unwrap(),
        lookups: std::sync::atomic::AtomicUsize::new(0),
        checks: std::sync::atomic::AtomicUsize::new(0),
        responses: std::sync::atomic::AtomicUsize::new(0),
    });
    let _server = NodeBftLibp2pAdmissionInputServerV2::new_for_provider(
        networks[1].clone(),
        NodeBftAdmissionInputServerConfigV2 {
            binding: binding.clone(),
            peers: vec![networks[0].peer_id().to_string()],
        },
        service,
        serving_scope,
        Arc::new(SourceProbe {
            inner: source,
            oracle: oracle.clone(),
            events: std::sync::atomic::AtomicUsize::new(0),
        }),
        Arc::new(GateProbe {
            inner: gate,
            oracle: oracle.clone(),
            events: std::sync::atomic::AtomicUsize::new(0),
        }),
    )
    .unwrap();
    use crate::formal_bft_admission_grant::client::{
        AdmissionGrantClientConfigV1, RemoteAdmissionGrantCredentialsV1,
    };
    let credentials = Arc::new(
        RemoteAdmissionGrantCredentialsV1::new(
            AdmissionGrantClientConfigV1 {
                endpoint: format!("http://{address}/oasis7/bft/original-admission-input/grant/v1"),
                issuer_public_key: issuer_key,
                target,
            },
            networks[0].clone(),
            fixtures::key(104),
        )
        .unwrap(),
    );
    // The issuer/hint acquisition is deliberately outside the rejection fee window.
    assert_wrong_pop_before_material(
        &networks,
        credentials.as_ref(),
        reference,
        deadline,
        &oracle,
    );
    let hints_and_rejection_floor = oracle.counter();
    let reader_response_floor = oracle.responses.load(std::sync::atomic::Ordering::SeqCst);
    let reader = NodeBftLibp2pAdmissionInputReaderV2::new(
        networks[0].clone(),
        networks[1].peer_id().to_string(),
        credentials,
        receiving_scope.buffers(),
    )
    .unwrap();
    let read = reader.read_original(reference, deadline).unwrap();
    assert_eq!(
        read.input().canonical_bytes().unwrap(),
        expected.canonical_bytes().unwrap()
    );
    reference.validate_input(read.input()).unwrap();
    drop(read);
    let expected_reader_responses = 1 + reference.object.bytes.div_ceil(3 * 16 * 1024);
    assert_eq!(
        u64::try_from(
            oracle
                .responses
                .load(std::sync::atomic::Ordering::SeqCst)
                .checked_sub(reader_response_floor)
                .unwrap()
        )
        .unwrap(),
        expected_reader_responses,
        "exact reader manifest plus batches; raw control and hints excluded"
    );
    assert!(oracle.counter() > hints_and_rejection_floor);
    issuer_server.stop().unwrap();
}

thread_local! { static ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(std::cell::Cell::get)
}
#[test]
fn actual_source_provider_batch_v2_real_issuer_http_and_noise() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            ENABLED.with(|v| v.set(true));
            install_case_with_retention(
                false,
                false,
                66,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
pub(super) fn exercise(
    source: &SourceFixture,
    service: Arc<BftSignerServiceV3>,
    binding: &BftSignerBindingV3,
) {
    use oasis7_distfs::protected_bft_v3::BftSignerBackendV3;
    let (_, _, mut backend, _) = source.cold_parts_for_test();
    let current = backend
        .load_private_retention_current(&binding.digest().unwrap())
        .unwrap();
    let ledger = current.retention.as_ref().unwrap();
    let entry = ledger
        .map
        .entries
        .iter()
        .find(|e| {
            e.lifecycle == oasis7_distfs::protected_bft_v3::BftPrivateRetentionLifecycleV1::Ready
        })
        .unwrap();
    let reference =
        BftPrivateRetentionAdmissionInputRefV1::from_entry(&ledger.map.source, entry).unwrap();
    let input = backend
        .load_private_retention_input(&binding.digest().unwrap(), &reference)
        .unwrap();
    read_actual_provider_original_v2(service, backend, binding, &reference, &input, 116);
}

struct GateProbe {
    oracle: Arc<FeeOracle>,
    inner: Arc<crate::formal_live_service::full_sync_read_gate::FullSyncV3ReadGate>,
    events: std::sync::atomic::AtomicUsize,
}
impl oasis7_node::NodeBftAdmissionInputBatchReadGateV2 for GateProbe {
    fn validate_identity(
        &self,
        authorization: &oasis7_node::NodeBftAdmissionInputBatchAuthorizationV2,
    ) -> Result<(), String> {
        let result = oasis7_node::NodeBftAdmissionInputBatchReadGateV2::validate_identity(
            self.inner.as_ref(),
            authorization,
        );
        eprintln!("batch-v2 fixed issuer identity accepted={}", result.is_ok());
        result
    }
    fn authorize_batch(
        &self,
        transport: &oasis7_net::AuthenticatedPeerRequestContext,
        scope: &oasis7_node::NodeBftAdmissionInputBatchScopeV2<'_>,
        response: &mut dyn FnMut(Instant) -> Result<Vec<u8>, String>,
    ) -> Result<Vec<u8>, String> {
        let print = self
            .events
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            < 8;
        if print {
            eprintln!("batch-v2 actual batch gate entered");
        }
        let counter_before = self.oracle.counter();
        let mut callback_ran = false;
        let mut wrapped = |deadline| {
            callback_ran = true;
            if print {
                eprintln!("batch-v2 actual response callback entered");
            }
            let result = response(deadline);
            if print {
                eprintln!(
                    "batch-v2 actual response callback accepted={}",
                    result.is_ok()
                );
            }
            result
        };
        let result = oasis7_node::NodeBftAdmissionInputBatchReadGateV2::authorize_batch(
            self.inner.as_ref(),
            transport,
            scope,
            &mut wrapped,
        );
        drop(wrapped);
        if let Ok(payload) = &result {
            let outer = oasis7_proto::distributed_net::NetworkResponse {
                payload: payload.clone(),
            };
            let expected = u64::try_from(serde_cbor::to_vec(&outer).unwrap().len()).unwrap();
            assert_eq!(
                self.oracle.counter().checked_sub(counter_before),
                Some(expected),
                "actual backend must reserve exactly the independently encoded outer response"
            );
            self.oracle
                .responses
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        if print {
            eprintln!(
                "batch-v2 actual batch gate accepted={} callback_ran={} authority_epoch={}",
                result.is_ok(),
                callback_ran,
                scope.view.schedule().authority_epoch()
            );
        }
        result
    }
    fn authorize_part(
        &self,
        transport: &oasis7_net::AuthenticatedPeerRequestContext,
        scope: &oasis7_node::NodeBftAdmissionInputPartScopeV2<'_>,
        inspect: &mut dyn FnMut(Instant) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.events.load(std::sync::atomic::Ordering::Relaxed) < 8 {
            eprintln!(
                "batch-v2 actual part gate entered index={}",
                scope.part_index
            );
        }
        let result = oasis7_node::NodeBftAdmissionInputBatchReadGateV2::authorize_part(
            self.inner.as_ref(),
            transport,
            scope,
            inspect,
        );
        if self.events.load(std::sync::atomic::Ordering::Relaxed) < 8 {
            eprintln!(
                "batch-v2 actual part gate accepted={} index={}",
                result.is_ok(),
                scope.part_index
            );
        }
        result
    }
}
struct SourceProbe {
    oracle: Arc<FeeOracle>,
    inner: Arc<NodeBftBackendAdmissionInputSourceV1>,
    events: std::sync::atomic::AtomicUsize,
}
impl oasis7_node::NodeBftAdmissionInputSourceV1 for SourceProbe {
    fn original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        deadline: Instant,
    ) -> Result<Arc<oasis7_node::NodeBftAdmissionInputBytesV1>, String> {
        self.oracle
            .lookups
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let result = oasis7_node::NodeBftAdmissionInputSourceV1::original(
            self.inner.as_ref(),
            reference,
            deadline,
        );
        if self
            .events
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            < 8
        {
            eprintln!("batch-v2 actual source lookup accepted={}", result.is_ok());
        }
        result
    }
    fn revalidate_original(
        &self,
        original: &oasis7_node::NodeBftAdmissionInputBytesV1,
        deadline: Instant,
    ) -> Result<bool, String> {
        self.oracle
            .checks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let result = oasis7_node::NodeBftAdmissionInputSourceV1::revalidate_original(
            self.inner.as_ref(),
            original,
            deadline,
        );
        if self
            .events
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            < 8
        {
            eprintln!(
                "batch-v2 actual source recheck accepted={}",
                matches!(&result, Ok(true))
            );
        }
        result
    }
}

// Test-only observation of the exact same backend; no accepting authority stub.
struct FeeOracle {
    backend: std::sync::Mutex<BftPrivateRetentionMemoryBackendV1>,
    binding_digest: String,
    lookups: std::sync::atomic::AtomicUsize,
    checks: std::sync::atomic::AtomicUsize,
    responses: std::sync::atomic::AtomicUsize,
}
impl FeeOracle {
    fn counter(&self) -> u64 {
        use oasis7_distfs::protected_bft_v3::BftSignerBackendV3;
        self.backend
            .lock()
            .unwrap()
            .load_private_retention_current(&self.binding_digest)
            .unwrap()
            .signer
            .successor_archive_transfer
            .expect("actual transfer ledger")
            .reserved_response_bytes
    }
}
fn assert_wrong_pop_before_material(
    networks: &[Arc<Libp2pNetwork>; 2],
    credentials: &dyn oasis7_node::NodeBftAdmissionInputBatchCredentialsV2,
    reference: &BftPrivateRetentionAdmissionInputRefV1,
    deadline: Instant,
    oracle: &FeeOracle,
) {
    #[derive(serde::Serialize)]
    struct Request<'a> {
        domain: &'static str,
        read: &'a oasis7_node::NodeBftAdmissionInputBatchReadRequestV2,
        authorization: oasis7_node::NodeBftAdmissionInputBatchAuthorizationV2,
    }
    let read = oasis7_node::NodeBftAdmissionInputBatchReadRequestV2 {
        reference: reference.clone(),
        requester_peer_id: networks[0].peer_id().to_string(),
        serving_peer_id: networks[1].peer_id().to_string(),
        nonce: hex::encode([0x71u8; 32]),
        operation: oasis7_node::NodeBftAdmissionInputBatchOperationV2::Manifest,
    };
    let mut authorization = credentials.authorize(&read, deadline).unwrap();
    // A valid actual-server control establishes this independently serialized wire shape/domain.
    let control = serde_cbor::to_vec(&Request {
        domain: "oasis7.bft.admission-input.exchange.v2",
        read: &read,
        authorization: authorization.clone(),
    })
    .unwrap();
    let control_counter = oracle.counter();
    let control_lookups = oracle.lookups.load(std::sync::atomic::Ordering::SeqCst);
    let control_responses = oracle.responses.load(std::sync::atomic::Ordering::SeqCst);
    let control_reply = networks[0]
        .request_authenticated_to_peer_with_timeout(
            "/oasis7/bft/admission-input/2.0.0",
            &control,
            networks[1].peer_id(),
            deadline.saturating_duration_since(Instant::now()),
        )
        .unwrap();
    assert!(
        !control_reply.is_empty(),
        "valid same-wire manifest must succeed"
    );
    assert!(oracle.counter() > control_counter);
    assert!(oracle.lookups.load(std::sync::atomic::Ordering::SeqCst) > control_lookups);
    assert_eq!(
        oracle.responses.load(std::sync::atomic::Ordering::SeqCst),
        control_responses + 1
    );
    // Preserve shape, issuer signature, grant and coordinates; corrupt only subject PoP.
    let replacement = if authorization.signature_hex.starts_with('0') {
        "1"
    } else {
        "0"
    };
    authorization.signature_hex.replace_range(..1, replacement);
    let bytes = serde_cbor::to_vec(&Request {
        domain: "oasis7.bft.admission-input.exchange.v2",
        read: &read,
        authorization,
    })
    .unwrap();
    // Decode both through the same representation and prove only signature_hex changed.
    let mut control_value: serde_cbor::Value = serde_cbor::from_slice(&control).unwrap();
    let bad_value: serde_cbor::Value = serde_cbor::from_slice(&bytes).unwrap();
    if let serde_cbor::Value::Map(fields) = &mut control_value {
        let auth = fields
            .get_mut(&serde_cbor::Value::Text("authorization".into()))
            .unwrap();
        if let serde_cbor::Value::Map(fields) = auth {
            let bad_auth = match &bad_value {
                serde_cbor::Value::Map(fields) => fields
                    .get(&serde_cbor::Value::Text("authorization".into()))
                    .unwrap(),
                _ => unreachable!(),
            };
            let bad_signature = match bad_auth {
                serde_cbor::Value::Map(fields) => fields
                    .get(&serde_cbor::Value::Text("signature_hex".into()))
                    .unwrap()
                    .clone(),
                _ => unreachable!(),
            };
            fields.insert(
                serde_cbor::Value::Text("signature_hex".into()),
                bad_signature,
            );
        } else {
            panic!("authorization must be a map");
        }
    } else {
        panic!("request must be a map");
    }
    assert_eq!(
        control_value, bad_value,
        "negative changes only the subject PoP"
    );
    let before = (
        oracle.counter(),
        oracle.lookups.load(std::sync::atomic::Ordering::SeqCst),
        oracle.checks.load(std::sync::atomic::Ordering::SeqCst),
        oracle.responses.load(std::sync::atomic::Ordering::SeqCst),
    );
    let rejected = networks[0].request_authenticated_to_peer_with_timeout(
        "/oasis7/bft/admission-input/2.0.0",
        &bytes,
        networks[1].peer_id(),
        deadline.saturating_duration_since(Instant::now()),
    );
    assert!(
        rejected.is_err(),
        "actual server must reject corrupted subject PoP"
    );
    assert_eq!(
        (
            oracle.counter(),
            oracle.lookups.load(std::sync::atomic::Ordering::SeqCst),
            oracle.checks.load(std::sync::atomic::Ordering::SeqCst),
            oracle.responses.load(std::sync::atomic::Ordering::SeqCst)
        ),
        before,
        "bad PoP must precede material lookup, recheck, response gate and transfer charge"
    );
}
