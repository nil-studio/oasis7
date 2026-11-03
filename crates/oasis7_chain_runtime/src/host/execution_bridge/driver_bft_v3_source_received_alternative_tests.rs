//! Actual Source/Noise retention regression, stopped before consensus signing.
use super::super::super::super::super::formal_header::{
    ActivateBftPrivateSourceV1, ActualBftPrivateSourceCreationVerifierV1,
    FreshBftPrivateSourceInputsV1, PreparedBftPrivateSourceOriginV1,
};
use super::*;
#[path = "driver_bft_v3_source_cold_transfer_oracle_tests.rs"]
mod cold_transfer;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
const FINISHED: &str = "received-alternative regression stopped before consensus signing";
#[derive(Clone, Copy)]
enum Order {
    LocalFirst,
    ReceivedFirst,
    SameReceivedLocal,
    ColdSelected,
}
struct Probe {
    order: Order,
    state: BackendState,
    signatures: Arc<AtomicUsize>,
    result: Mutex<Option<Result<(), String>>>,
}
fn probes() -> &'static Mutex<BTreeMap<usize, Arc<Probe>>> {
    static PROBES: OnceLock<Mutex<BTreeMap<usize, Arc<Probe>>>> = OnceLock::new();
    PROBES.get_or_init(|| Mutex::new(BTreeMap::new()))
}
// Remove only our exact registration, including during unwinding.
struct ProbeRegistration {
    key: usize,
    probe: Arc<Probe>,
}
impl ProbeRegistration {
    fn register(key: usize, probe: Arc<Probe>) -> Self {
        let mut registered = probes().lock().unwrap();
        assert!(
            !registered.contains_key(&key),
            "duplicate exact driver probe"
        );
        registered.insert(key, probe.clone());
        Self { key, probe }
    }
}
impl Drop for ProbeRegistration {
    fn drop(&mut self) {
        let mut registered = probes().lock().unwrap_or_else(|poison| poison.into_inner());
        if registered
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.probe))
        {
            registered.remove(&self.key);
        }
    }
}
pub(super) fn intercept(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    context: &NodeBftExecutionContextV3,
    local: &NodeBftPrivateCandidateV3,
    deadline: Instant,
) -> Option<Result<String, String>> {
    let probe = probes()
        .lock()
        .unwrap()
        .get(&(Arc::as_ptr(actual) as usize))
        .cloned()?;
    if probe.result.lock().unwrap().is_some() {
        return Some(Err(FINISHED.into()));
    }
    let result = exercise(
        &probe,
        &mut actual.lock().unwrap(),
        binding,
        lease,
        context,
        local,
        deadline,
    );
    *probe.result.lock().unwrap() = Some(result);
    // Deliberate test-only barrier: do not allow the original callback to reach
    // proposal signing after inspecting the protected retention operation.
    Some(Err(FINISHED.into()))
}
fn exercise(
    probe: &Probe,
    driver: &mut FormalBftHeaderNodeDriverV3,
    binding: &BftSignerBindingV3,
    lease: &mut dyn BftSignerLeaseV3,
    context: &NodeBftExecutionContextV3,
    local: &NodeBftPrivateCandidateV3,
    deadline: Instant,
) -> Result<(), String> {
    let signatures = probe.signatures.load(Ordering::SeqCst);
    let local_package = driver.preview_received_package_for_test(context, &local.actions_cbor)?;
    if local_package.candidate != *local {
        return Err("setup: local actual package differs".into());
    }
    let empty_actions =
        serde_cbor::to_vec(&Vec::<oasis7_node::NodeConsensusAction>::new()).unwrap();
    let remote = driver.preview_received_package_for_test(context, &empty_actions)?;
    let local_id = local.public_value.block_id()?;
    let remote_id = remote.candidate.public_value.block_id()?;
    if local_id == remote_id {
        return Err("setup: submitted local action must differ from empty remote B".into());
    }
    let receive = |driver: &mut FormalBftHeaderNodeDriverV3,
                   lease: &mut dyn BftSignerLeaseV3,
                   package: &oasis7_node::NodeBftPrivateCandidatePackageV3|
     -> Result<(), String> {
        match driver.retain_received_bft_v3_protected_candidate(
            binding, lease, context, package, deadline,
        )? {
            oasis7_node::NodeBftProtectedCandidateAdmissionV3::Retained(_) => Ok(()),
            other => Err(format!("actual received alternative rejected: {other:?}")),
        }
    };
    if matches!(probe.order, Order::SameReceivedLocal | Order::ColdSelected) {
        receive(driver, lease, &local_package)?;
        if driver.pending_bft_v3_private_candidate(context)?.is_some() {
            return Err(
                "received original selected a local proposal before local admission".into(),
            );
        }
        let before = probe.state.lock().map_err(|e| e.to_string())?.state;
        let original = lease
            .observe_private_retention(deadline)
            .map_err(|e| e.to_string())?;
        if original.ledger().map.entries.len() != 1 {
            return Err("expected exactly one received original fact".into());
        }
        driver.retain_bft_v3_protected_candidate(binding, lease, context, local, deadline)?;
        let selected_state = probe.state.lock().map_err(|e| e.to_string())?.state;
        let selected = lease
            .observe_private_retention(deadline)
            .map_err(|e| e.to_string())?;
        let index = selected
            .ledger()
            .selection_index
            .as_ref()
            .ok_or("first received selection lacks protected choice")?;
        let mut unchanged = selected_state.clone();
        unchanged.revision = before.revision;
        unchanged.state_digest = before.state_digest.clone();
        unchanged.private_retention = before.private_retention.clone();
        if unchanged != before
            || selected_state.revision != before.revision.checked_add(1).ok_or("revision overflow")?
            || selected.ledger().map != original.ledger().map
            || selected.ledger().receipt.body.head.schema_version != 3
            || index.last_sequence != 1 || index.selections.len() != 1
            || index.selections[0].key != original.ledger().map.entries[0].key
            || index.selections[0].reference != oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1::from_entry(
                &original.ledger().map.source, &original.ledger().map.entries[0]).map_err(|e| e.to_string())?
        {
            return Err("first selection changed original admission or unrelated signer State".into());
        }
        if driver.pending_bft_v3_private_candidate(context)?.as_ref() != Some(local)
            || driver.export_bft_v3_private_candidate(&local.public_value)? != local_package
        {
            return Err(
                "local selection did not preserve the original received Data/actions/deps".into(),
            );
        }
        driver.retain_bft_v3_protected_candidate(binding, lease, context, local, deadline)?;
        receive(driver, lease, &local_package)?;
        let after = probe.state.lock().map_err(|e| e.to_string())?.state;
        let repeated = lease
            .observe_private_retention(deadline)
            .map_err(|e| e.to_string())?;
        if selected_state != after || selected.ledger() != repeated.ledger() {
            return Err(
                "retrying the same selected received key changed protected State/fact/budget"
                    .into(),
            );
        }
        if probe.signatures.load(Ordering::SeqCst) != signatures {
            return Err("same received selection minted a validator signature".into());
        }
        return Ok(());
    }
    match probe.order {
        Order::LocalFirst => {
            driver.retain_bft_v3_protected_candidate(binding, lease, context, local, deadline)?;
            receive(driver, lease, &remote)?;
        }
        Order::SameReceivedLocal | Order::ColdSelected => {
            unreachable!("handled without altering the two-alternative oracle")
        }
        Order::ReceivedFirst => {
            receive(driver, lease, &remote)?;
            if driver.pending_bft_v3_private_candidate(context)?.is_some() {
                return Err("received B selected a local proposal".into());
            }
            driver.retain_bft_v3_protected_candidate(binding, lease, context, local, deadline)?;
        }
    }
    if driver.pending_bft_v3_private_candidate(context)?.as_ref() != Some(local) {
        return Err("received B replaced original local A selection".into());
    }
    if driver.export_bft_v3_private_candidate(&local.public_value)? != local_package
        || driver.export_bft_v3_private_candidate(&remote.candidate.public_value)? != remote
    {
        return Err("actual A/B original packages changed".into());
    }
    // BackendState::lock currently returns an owned snapshot, not a guard.
    // End the complete read scope explicitly before any leased/network call.
    let before_state = {
        let snapshot = probe.state.lock().map_err(|e| e.to_string())?;
        snapshot.state
    };
    let observation = lease
        .observe_private_retention(deadline)
        .map_err(|e| e.to_string())?;
    let entries = &observation.ledger().map.entries;
    if entries.len() != 2
        || entries.iter().any(|entry| {
            entry.key.height != context.height
                || entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready
        })
        || !entries.iter().any(|entry| entry.key.header_id == local_id)
        || !entries.iter().any(|entry| entry.key.header_id == remote_id)
    {
        return Err("expected two distinct actual Ready facts at one height".into());
    }
    receive(driver, lease, &remote)?;
    receive(driver, lease, &local_package)?;
    driver.retain_bft_v3_protected_candidate(binding, lease, context, local, deadline)?;
    let after_state = {
        let snapshot = probe.state.lock().map_err(|e| e.to_string())?;
        snapshot.state
    };
    let repeated = lease
        .observe_private_retention(deadline)
        .map_err(|e| e.to_string())?;
    if before_state != after_state || observation.ledger() != repeated.ledger() {
        return Err("exact original retries changed full State/head/map/facts".into());
    }
    if probe.signatures.load(Ordering::SeqCst) != signatures {
        return Err("retention minted a new validator signature".into());
    }
    Ok(())
}
fn run(order: Order) {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let (actual, binding, state, source) = SourceFixture::prepare(&h, &root, &store);
    let old_driver = Arc::downgrade(&actual);
    let cold_parts = matches!(order, Order::ColdSelected).then(|| source.cold_parts_for_test());
    let signatures = Arc::new(AtomicUsize::new(0));
    let probe = Arc::new(Probe {
        order,
        state,
        signatures: signatures.clone(),
        result: Mutex::new(None),
    });
    let key = Arc::as_ptr(&actual) as usize;
    let _registration = ProbeRegistration::register(key, probe.clone());
    let service = Arc::new(source.service(
        &binding,
        vec![Box::new(ReadbackOnlyKey(signatures.clone()))],
        None,
    ));
    let (address, server) = tcp(service);
    let mut node = source.runtime(root.path(), &binding, address);
    node.submit_consensus_action_payload(34977, payload("received-alternative-original-A"))
        .unwrap();
    node.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let outcome = loop {
        if let Some(result) = probe.result.lock().unwrap().clone() {
            break result;
        }
        assert!(
            Instant::now() < deadline,
            "actual protected retention probe never ran: {:?}",
            node.bft_v3_status()
        );
        thread::sleep(Duration::from_millis(10));
    };
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    assert_eq!(
        signatures.load(Ordering::SeqCst),
        0,
        "test barrier must precede original signing"
    );
    outcome.unwrap();
    if let Some((request, role, backend, original_marker)) = cold_parts {
        let context = actual
            .lock()
            .unwrap()
            .current_bft_v3_execution_context(binding.initial_schedule())
            .unwrap();
        let selected = actual
            .lock()
            .unwrap()
            .pending_bft_v3_private_candidate(&context)
            .unwrap()
            .unwrap();
        let package = actual
            .lock()
            .unwrap()
            .export_bft_v3_private_candidate(&selected.public_value)
            .unwrap();
        drop(_registration);
        drop(source);
        drop(actual);
        while old_driver.upgrade().is_some() {
            assert!(
                Instant::now() < deadline,
                "old Noise/hook still retains the original driver/writer"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            old_driver.upgrade().is_none(),
            "cold open requires the old owner to be gone"
        );
        cold_reopen_selected(
            &binding,
            request,
            role,
            backend,
            original_marker,
            &package,
            signatures,
        );
    }
}
#[test]
fn actual_source_keeps_received_alternative_beside_local_selection() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run(Order::LocalFirst))
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn actual_source_received_first_never_selects_local_proposal() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run(Order::ReceivedFirst))
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn actual_source_selects_same_received_original_without_new_fact_or_charge() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run(Order::SameReceivedLocal))
        .unwrap()
        .join()
        .unwrap();
}

// Ready-only cold readback. No original-input reader is configured here, so this
// cannot be mistaken for proof of independent Reserved recovery transport.
fn cold_reopen_selected(
    binding: &BftSignerBindingV3,
    request: FreshBftPrivateSourceInputsV1,
    role: BftPrivateRetentionAttestorV1,
    mut backend: BftPrivateRetentionMemoryBackendV1,
    original_marker: BftPrivateRetentionCreationMarkerV1,
    package: &oasis7_node::NodeBftPrivateCandidatePackageV3,
    signatures: Arc<AtomicUsize>,
) {
    let digest = binding.digest().unwrap();
    let before = backend.load_private_retention_current(&digest).unwrap();
    let ledger = before.retention.as_ref().unwrap();
    assert_eq!(ledger.map.entries.len(), 1);
    let entry = &ledger.map.entries[0];
    assert_eq!(entry.lifecycle, BftPrivateRetentionLifecycleV1::Ready);
    let input = before.inputs.values().next().unwrap();
    let choice = ledger
        .selection_index
        .as_ref()
        .unwrap()
        .selections
        .iter()
        .find(|choice| choice.key == entry.key)
        .unwrap();

    #[derive(serde::Deserialize)]
    struct OriginalKind {
        schema_version: u32,
    }
    let kind: OriginalKind = serde_cbor::from_slice(
        &input
            .objects
            .iter()
            .find(|object| object.kind == BftPrivateRetentionInputObjectKindV1::OriginalData)
            .unwrap()
            .bytes,
    )
    .unwrap();
    assert_eq!(
        kind.schema_version, 10,
        "cold target must really be original Received10"
    );
    let private = request
        .records
        .join("formal-bft-header-prefix-v1/node-private-v1");
    let selection_path = private.join(format!("selection-{:020}.cbor", entry.key.height));
    let data_path = private.join(format!(
        "candidate-{:020}-{}.cbor",
        entry.key.height, entry.key.header_id
    ));
    let original_selection = std::fs::read(&selection_path).unwrap();
    let original_data = std::fs::read(&data_path).unwrap();
    let service = Arc::new(
        BftSignerServiceV3::open_with_private_retention(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(ReadbackOnlyKey(signatures.clone()))],
            Box::new(backend.clone()),
            None,
            BftPrivateRetentionSupportV1::new(
                role.clone(),
                Arc::new(
                    ActualBftPrivateSourceCreationVerifierV1::for_original_source(request.clone())
                        .unwrap(),
                ),
                Duration::from_secs(120),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let (service_address, server) = tcp(service);
    let deadline = Instant::now() + Duration::from_secs(120);
    let relay = cold_transfer::Relay::start(service_address, deadline);
    let address = relay.address();
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(120),
        binding.clone(),
        fixtures::key(117),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: role.provider_public_key.clone(),
            binding_digest: digest.clone(),
        },
    )
    .unwrap()
    .with_private_retention_attestor(role.clone())
    .unwrap();
    let mut lease = provider.acquire(binding).unwrap();
    let acquired = backend.load_private_retention_current(&digest).unwrap();
    // Read the original only under the newly acquired, unfenced owner. No RPC
    // or mutation is introduced; the relay is configured before any cold read.
    let original = backend
        .read_private_retention_operation(
            acquired.signer.lease_generation,
            &acquired.signer.lease_token_digest,
            &choice.operation_digest,
        )
        .unwrap();
    let original_proof = oasis7_distfs::protected_bft_v3::BftPrivateRetentionOperationProofV1 {
        state: original.signer.state,
        ledger: original.retention,
    };
    let proof_bytes = oasis7_cas::to_canonical_cbor(&original_proof).unwrap();
    let expected_proof = cold_transfer::ExpectedProof::new(
        choice.operation_digest.clone(),
        acquired.signer.state.revision,
        acquired.signer.state.state_digest.clone(),
        proof_bytes,
    );

    relay.configure_proof(expected_proof);
    let transfer_baseline = relay.snapshot();
    assert!(acquired.signer.lease_generation > before.signer.lease_generation);
    assert_ne!(
        acquired.signer.lease_token_digest,
        before.signer.lease_token_digest
    );
    // Memory acquire changes only the separate lease fields; quota and all
    // canonical source State/facts/input allocations remain exactly original.
    assert_eq!(before.signer.state, acquired.signer.state);
    assert_eq!(
        before.signer.successor_archive_transfer,
        acquired.signer.successor_archive_transfer
    );
    assert_eq!(before.retention, acquired.retention);
    assert_eq!(before.inputs.len(), acquired.inputs.len());
    for (key, input) in &before.inputs {
        assert!(Arc::ptr_eq(input, &acquired.inputs[key]));
    }
    assert_eq!(
        original_marker.digest().unwrap(),
        role.creation_marker_digest
    );
    let mut driver = PreparedBftPrivateSourceOriginV1::reopen_covered_empty(
        request,
        ActivateBftPrivateSourceV1 {
            binding,
            fixed_attestor: &role,
            original_marker: &original_marker,
            lease: lease.as_mut(),
            deadline,
        },
    )
    .unwrap();
    driver
        .prepare_bft_v3_execution(binding, lease.as_mut(), deadline)
        .unwrap();
    driver.configure_bft_v3_execution(binding).unwrap();
    let context = driver
        .current_bft_v3_execution_context(binding.initial_schedule())
        .unwrap();
    assert_eq!(
        driver
            .pending_bft_v3_private_candidate(&context)
            .unwrap()
            .as_ref(),
        Some(&package.candidate)
    );
    driver
        .preview_bft_v3_candidate(&context, &package.candidate)
        .unwrap();
    assert_eq!(
        &driver
            .export_bft_v3_private_candidate(&package.candidate.public_value)
            .unwrap(),
        package
    );
    assert!(
        std::fs::read(selection_path).unwrap() == original_selection,
        "cold original selection bytes changed"
    );
    assert!(
        std::fs::read(data_path).unwrap() == original_data,
        "cold original received Data bytes changed"
    );
    let after = backend.load_private_retention_current(&digest).unwrap();
    assert_eq!(acquired.signer.state, after.signer.state);
    relay.assert_two_observations(
        &transfer_baseline,
        acquired.signer.successor_archive_transfer.as_ref().unwrap(),
        after.signer.successor_archive_transfer.as_ref().unwrap(),
    );
    assert_eq!(acquired.retention, after.retention);
    assert_eq!(acquired.inputs.len(), after.inputs.len());
    for (key, input) in &acquired.inputs {
        assert!(Arc::ptr_eq(input, &after.inputs[key]));
    }
    assert_eq!(signatures.load(Ordering::SeqCst), 0);
    drop(driver);
    drop(lease);
    relay.finish().unwrap();
    server.join().unwrap().unwrap();
}
#[test]
fn actual_source_cold_reopens_same_selected_received_original_without_new_fact() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run(Order::ColdSelected))
        .unwrap()
        .join()
        .unwrap();
}
