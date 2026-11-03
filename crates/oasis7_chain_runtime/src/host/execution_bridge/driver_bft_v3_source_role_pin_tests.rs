//! Genuine separately provisioned namespaces exercise the hot consumer's pin.
//! The foreign creation verifier is only a namespace/transport component double;
//! it asserts no World admission and cannot publish a private input.
use super::*;
struct ForeignNamespaceOnly;
impl BftPrivateRetentionMaterialVerifierV1 for ForeignNamespaceOnly {
    fn verify_source_creation(
        &self,
        _: &BftSignerBindingV3,
        _: &BftSignerStateV3,
        _: &ValidatedBftPrivateRetentionCreationV1,
        _: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Ok(())
    }
    fn verify_reservation(
        &self,
        _: &BftSignerBindingV3,
        _: &BftSignerStateV3,
        _: &BftPrivateRetentionEntryV1,
        _: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(BftSignerErrorV3(
            "foreign component cannot admit private material".into(),
        ))
    }
    fn verify_ready(
        &self,
        _: &BftSignerBindingV3,
        _: &BftSignerStateV3,
        _: &BftPrivateRetentionEntryV1,
        _: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(BftSignerErrorV3(
            "foreign component cannot publish private material".into(),
        ))
    }
}
fn assert_original_source_role_is_fixed(
    driver: &mut FormalBftHeaderNodeDriverV3,
    binding: &BftSignerBindingV3,
    marker: &BftPrivateRetentionCreationMarkerV1,
    original_role: &BftPrivateRetentionAttestorV1,
    original_backend: &BftPrivateRetentionMemoryBackendV1,
    request: &FreshBftPrivateSourceInputsV1,
    root: &Path,
) {
    let deadline = || Instant::now() + Duration::from_secs(10);
    let before = original_backend
        .clone()
        .load_provisioned(&binding.digest().unwrap())
        .unwrap()
        .state;
    // Reacquire the same namespace immediately before its positive check. No
    // original connection remains idle during foreign namespace construction.
    let (provider, server) = endpoint(
        binding,
        original_backend.clone(),
        original_role.clone(),
        ActualBftPrivateSourceCreationVerifierV1::for_original_source(request.clone()).unwrap(),
    );
    let mut lease = provider.acquire(binding).unwrap();
    driver
        .prepare_bft_v3_execution(binding, lease.as_mut(), deadline())
        .unwrap();
    assert_eq!(lease.checkpoint().unwrap().state, before);
    drop(lease);
    server.join().unwrap();
    let private_root = root.join("ledger/formal-bft-header-prefix-v1/node-private-v1");
    let original = fs::read(private_root.join("origin.cbor")).unwrap();
    for provider_changed in [false, true] {
        let namespace = BftPrivateRetentionFreshSeedV1::generate()
            .unwrap()
            .bind_source(binding, marker.ledger_origin_digest.clone())
            .unwrap();
        let mut foreign_role = original_role.clone();
        foreign_role.creation_marker_digest = namespace.marker().digest().unwrap();
        let provider_key = if provider_changed { 118 } else { 116 };
        foreign_role.provider_public_key =
            hex::encode(fixtures::key(provider_key).verifying_key().to_bytes());
        let backend = BftPrivateRetentionMemoryBackendV1::create_fresh(
            namespace,
            foreign_role.clone(),
            empty_transfer(),
            100,
        )
        .unwrap();
        let service = BftSignerServiceV3::open_with_private_retention(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(provider_key))),
            vec![Box::new(MemoryTestSignerKey(fixtures::key(120)))],
            Box::new(backend),
            None,
            BftPrivateRetentionSupportV1::new(
                foreign_role.clone(),
                Arc::new(ForeignNamespaceOnly),
                Duration::from_secs(10),
            )
            .unwrap(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            service.serve_tcp_connection(&mut stream).unwrap();
        });
        let provider = RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(10),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: foreign_role.provider_public_key.clone(),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap()
        .with_private_retention_attestor(foreign_role.clone())
        .unwrap();
        let mut lease = provider.acquire(binding).unwrap();
        lease.provision_private_retention(deadline()).unwrap();
        let observation = lease.observe_private_retention(deadline()).unwrap();
        assert_eq!(observation.ledger().map.source, original_role.source);
        assert_ne!(
            foreign_role.digest().unwrap(),
            original_role.digest().unwrap()
        );
        assert_eq!(
            observation.checkpoint().provider_public_key,
            foreign_role.provider_public_key
        );
        let foreign_before = lease.checkpoint().unwrap().state;
        let error = driver
            .prepare_bft_v3_execution(binding, lease.as_mut(), deadline())
            .unwrap_err();
        assert!(
            error.contains("original fixed provider/role"),
            "valid foreign current must fail the original pin: {error}"
        );
        assert_eq!(lease.checkpoint().unwrap().state, foreign_before);
        assert!(
            lease
                .get_private_retention(deadline())
                .unwrap()
                .map
                .entries
                .is_empty()
        );
        assert_eq!(
            fs::read(private_root.join("origin.cbor")).unwrap(),
            original
        );
        assert!(
            !private_root
                .join(format!(
                    "candidate-{:020}.cbor",
                    binding.initial_schedule().height()
                ))
                .exists()
        );
        drop(lease);
        server.join().unwrap();
    }
    assert_eq!(
        original_backend
            .clone()
            .load_provisioned(&binding.digest().unwrap())
            .unwrap()
            .state,
        before
    );
}

#[test]
fn wire3_source_current_pin_rejects_valid_foreign_provider_and_role_before_publication() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let request = inputs(&h, &root, &store);
    let prepared = Box::new(
        PreparedBftPrivateSourceOriginV1::prepare(
            request.clone(),
            BftPrivateRetentionFreshSeedV1::generate().unwrap(),
        )
        .unwrap(),
    );
    let binding = binding(&prepared);
    let namespace = prepared.finalize_namespace(&binding).unwrap();
    let marker = namespace.marker().clone();
    let fixed = role(&binding, &marker);
    let backend = BftPrivateRetentionMemoryBackendV1::create_fresh(
        namespace,
        fixed.clone(),
        empty_transfer(),
        1000,
    )
    .unwrap();
    let (provider, server) = endpoint(
        &binding,
        backend.clone(),
        fixed.clone(),
        prepared.source_verifier(),
    );
    let mut lease = provider.acquire(&binding).unwrap();
    lease
        .provision_private_retention(Instant::now() + Duration::from_secs(10))
        .unwrap();
    let mut driver = prepared
        .activate(ActivateBftPrivateSourceV1 {
            binding: &binding,
            fixed_attestor: &fixed,
            original_marker: &marker,
            lease: lease.as_mut(),
            deadline: Instant::now() + Duration::from_secs(120),
        })
        .unwrap();
    drop(lease);
    server.join().unwrap();
    assert_original_source_role_is_fixed(
        &mut driver,
        &binding,
        &marker,
        &fixed,
        &backend,
        &request,
        root.path(),
    );
}
