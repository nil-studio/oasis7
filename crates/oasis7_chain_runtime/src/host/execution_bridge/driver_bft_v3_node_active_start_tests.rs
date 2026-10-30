//! Real Node startup over an adopted/pending base; keys/backends are explicit doubles.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    store: LocalCasStore,
    binding: &BftSignerBindingV3,
    pin: &str,
    address: SocketAddr,
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    snapshot: Arc<Mutex<BftSignerBackendSnapshotV3>>,
    signatures: Arc<std::sync::atomic::AtomicUsize>,
    negatives: bool,
    cli: bool,
) {
    let records = root.path().join("ledger");
    let frozen = tree(&records.join("formal-bft-header-prefix-v1"));
    if negatives {
        let before = snapshot.lock().unwrap().state.clone();
        let calls = signatures.load(std::sync::atomic::Ordering::SeqCst);
        let mut node = runtime_for(
            h,
            root,
            &store,
            binding,
            pin,
            address,
            None,
            "missing-archive",
        );
        assert!(
            node.start().is_err(),
            "active checkpoint cannot fall back to H without private archive"
        );
        drop(node);
        assert_eq!(snapshot.lock().unwrap().state, before);
        assert_eq!(signatures.load(std::sync::atomic::Ordering::SeqCst), calls);
        assert_eq!(tree(&records.join("formal-bft-header-prefix-v1")), frozen);
        let checkpoint_digest = before
            .active_window
            .as_ref()
            .unwrap()
            .checkpoint_digest
            .clone();
        let origin = records
            .join("successor-active-v1")
            .join(checkpoint_digest)
            .join("formal-bft-header-window-v1/origin.cbor");
        let retained = origin.with_extension("original-retained");
        fs::rename(&origin, &retained).unwrap();
        let mut node = runtime_for(
            h,
            root,
            &store,
            binding,
            pin,
            address,
            Some(archive.clone()),
            "missing-origin",
        );
        assert!(
            node.start().is_err(),
            "missing committed Origin cannot initialize another active segment"
        );
        drop(node);
        assert!(!origin.exists());
        fs::rename(&retained, &origin).unwrap();
        assert_eq!(snapshot.lock().unwrap().state, before);
        assert_eq!(signatures.load(std::sync::atomic::Ordering::SeqCst), calls);
    }
    let (business_height, business_header) = if cli {
        cli_case::run(h, root, &store, binding, pin, address, &archive);
        let state = snapshot.lock().unwrap().state.clone();
        let actions = vec![
            oasis7_node::NodeConsensusAction::from_payload(
                98_001,
                validator_id(0),
                payload("native-active-start-nonempty-agent"),
            )
            .unwrap(),
        ];
        let action_root = oasis7_node::compute_consensus_action_root(&actions).unwrap();
        let certificate = state
            .history
            .iter()
            .find(|certificate| certificate.execution_commitment.action_root == action_root)
            .expect("shipping CLI must commit nonempty action");
        (
            certificate.execution_commitment.height,
            certificate.header.block_id().unwrap(),
        )
    } else {
        let mut node = runtime_for(
            h,
            root,
            &store,
            binding,
            pin,
            address,
            Some(archive.clone()),
            "positive",
        );
        node.submit_consensus_action_payload(98_001, payload("native-active-start-nonempty-agent"))
            .unwrap();
        node.start()
            .expect("native Node must reconstruct the authenticated private active base");
        let batch = wait_batch(&node, 98_001);
        assert!(batch.height > binding.handoff_position);
        assert!(
            batch
                .actions
                .iter()
                .any(|action| action.action_id == 98_001)
        );
        node.stop().unwrap();
        drop(node);
        (batch.height, batch.block_hash)
    };
    let provider = provider(binding, pin, address);
    let mut lease = provider.acquire(binding).unwrap();
    let checkpoint = lease.checkpoint().unwrap();
    assert!(checkpoint.state.pending_driver_apply.is_none());
    let active = lease.verified_active_window().unwrap().unwrap();
    let record = active.original_request();
    let id = &record.request.intent.body.checkpoint.request_id;
    let digest = record.request.intent.body.checkpoint.digest().unwrap();
    let original = lease
        .read_successor_original_state(id, &digest, Instant::now() + Duration::from_secs(120))
        .unwrap();
    let installed = original
        .reconstruct_installed_pending(binding, pin, record)
        .unwrap();
    let verifier =
        ActualBftSuccessorWindowDriverVerifierV1::new(h.anchor.clone(), h.target.clone(), archive)
            .unwrap()
            .with_installation_root(records.clone())
            .unwrap()
            .with_active_private_store(store.clone())
            .unwrap();
    let base = verifier
        .recover_private_base(record, &original, Instant::now() + Duration::from_secs(120))
        .unwrap();
    let window = FormalBftHeaderActiveExecutionV1::open(
        &records,
        store,
        base,
        &active,
        binding,
        pin,
        &installed,
        Instant::now() + Duration::from_secs(120),
    )
    .unwrap();
    assert!(
        window
            .world()
            .state()
            .agents
            .contains_key("native-active-start-nonempty-agent")
    );
    let receipt = checkpoint
        .state
        .applied_installs
        .iter()
        .find(|receipt| receipt.height == business_height)
        .expect("business result requires exact protected applied receipt");
    assert_eq!(receipt.header_id, business_header);
    assert_eq!(tree(&records.join("formal-bft-header-prefix-v1")), frozen);
}
fn provider(
    binding: &BftSignerBindingV3,
    pin: &str,
    address: SocketAddr,
) -> Arc<RemoteBftSignerProviderV3> {
    Arc::new(
        RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(120),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: pin.into(),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap(),
    )
}
#[allow(clippy::too_many_arguments)]
fn runtime_for(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    store: &LocalCasStore,
    binding: &BftSignerBindingV3,
    pin: &str,
    address: SocketAddr,
    archive: Option<Arc<LocalBftSuccessorArchiveStoreV1>>,
    name: &str,
) -> NodeRuntime {
    let (hook, handoff, initial, identity) = FormalBftHeaderNodeDriverV3::deferred_existing(
        &root.path().join("ledger"),
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        h.reader.clone(),
        archive,
        pin.into(),
        false,
    )
    .unwrap();
    assert_eq!(handoff.target_digest(), binding.handoff().target_digest());
    assert_eq!(&initial, binding.initial_schedule());
    assert_eq!(identity, binding.material_identity);
    let config = NodeConfig::new(
        validator_id(0),
        binding.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_tick_interval(Duration::from_millis(200))
    .unwrap()
    .with_replication(
        NodeReplicationConfig::new(root.path().join(format!("native-active-{name}"))).unwrap(),
    );
    NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .try_with_shared_execution_hook(Arc::new(Mutex::new(hook)))
        .unwrap()
        .with_tendermint_v3(binding.clone(), provider(binding, pin, address))
        .unwrap()
}
fn tree(path: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for item in fs::read_dir(path).unwrap() {
            let item = item.unwrap();
            if item.file_type().unwrap().is_dir() {
                visit(root, &item.path(), out)
            } else {
                out.insert(
                    item.path().strip_prefix(root).unwrap().into(),
                    fs::read(item.path()).unwrap(),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    visit(path, path, &mut out);
    out
}

#[path = "driver_bft_v3_node_active_cli_tests.rs"]
mod cli_case;
