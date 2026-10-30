//! Capacity behavior through normal Node ticks: no manual checkpoint or budget fill.
use super::super::super::super::formal_header::{
    ActualBftSuccessorWindowDriverVerifierV1, FormalBftHeaderActiveExecutionV1,
};
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
const ACTIONS: u64 = 129;
#[test]
fn wire3_normal_node_queued129_nonempty_business_requires_two_capacity_driven_rolls() {
    // All fixture/runtime/cold-proof operations use the harness default stack.
    case();
}
fn case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let records = root.path().join("ledger");
    // Explicit original-H provisioning is identical to the shipping print-binding path.
    let original = driver(&h, root.path(), &store);
    let binding = signer_binding(&original);
    drop(original);
    let private_root = Scratch::new();
    let public_root = Scratch::new();
    let archive = Arc::new(LocalBftSuccessorArchiveStoreV1::new(cas(private_root.path())).unwrap());
    let public = Arc::new(LocalBftSuccessorArchiveStoreV1::new(cas(public_root.path())).unwrap());
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        successor_archive_transfer: Some(BftSuccessorArchiveTransferLedgerV1::provisioned()),
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let verifier = Arc::new(
        ActualBftSuccessorWindowDriverVerifierV1::new(
            h.anchor.clone(),
            h.target.clone(),
            archive.clone(),
        )
        .unwrap()
        .with_installation_root(records.clone())
        .unwrap()
        .with_active_private_store(store.clone())
        .unwrap(),
    );
    let support = BftSuccessorWindowSupportV1::new(public, Duration::from_secs(120))
        .unwrap()
        .with_active_window_rebase()
        .with_driver_verifier(verifier.clone());
    // One elapsed origin is shared by every lease and never reset on stop/reopen.
    let clock_start = Arc::new(std::sync::OnceLock::<Instant>::new());
    let service = Arc::new(
        BftSignerServiceV3::open_with_successor_window(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(116))),
            vec![Box::new(MemoryTestSignerKey(fixtures::key(120)))],
            elapsed_clock::elapsed_backend(
                MemoryTestSignerBackend {
                    binding_digest: binding.digest().unwrap(),
                    fail_confirm_after_commit: None,
                    snapshot: snapshot.clone(),
                },
                clock_start.clone(),
            ),
            support,
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = thread::spawn(move || -> Result<(), String> {
        while !stopping.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                    service
                        .serve_tcp_connection(&mut stream)
                        .map_err(|e| e.to_string())?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
    });
    let pin = hex::encode(fixtures::key(116).verifying_key().to_bytes());
    let provider = Arc::new(
        RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(120),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: pin.clone(),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap(),
    );
    let (hook, _, _, identity) = FormalBftHeaderNodeDriverV3::deferred_existing(
        &records,
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        h.reader.clone(),
        Some(archive.clone()),
        pin.clone(),
        false,
    )
    .unwrap();
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
        NodeReplicationConfig::new(root.path().join("normal-capacity-replication")).unwrap(),
    );
    let mut node = NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .try_with_shared_execution_hook(Arc::new(Mutex::new(hook)))
        .unwrap()
        .with_tendermint_v3(binding.clone(), provider.clone())
        .unwrap();
    let outcome = drive(&mut node, &snapshot, &records, &clock_start);
    let _ = node.stop();
    drop(node);
    // Do not let Scratch teardown or a detached server hide the actual capacity failure.
    let proof = match &outcome {
        Ok(batches) => verify_business(
            &store, &records, &binding, &pin, &provider, &verifier, batches,
        ),
        Err(error) => Err(error.clone()),
    };
    stop.store(true, Ordering::Release);
    let transport = server.join().unwrap();
    assert!(
        proof.is_ok(),
        "normal queued business must cross 65 and finish {ACTIONS} valid actions with >=2 capacity-driven rolls: {}; TCP={transport:?}",
        proof.err().unwrap_or_default()
    );
    transport.unwrap();
}
fn drive(
    node: &mut NodeRuntime,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
    records: &Path,
    clock_start: &std::sync::OnceLock<Instant>,
) -> Result<Vec<oasis7_node::NodeCommittedActionBatch>, String> {
    node.submit_consensus_action_payload(200_001, payload(&name(1)))
        .map_err(|e| e.to_string())?;
    clock_start
        .set(Instant::now())
        .map_err(|_| "normal elapsed clock was already started")?;
    node.start()
        .map_err(|e| format!("normal fresh-H startup failed: {e}"))?;
    let mut batches = Vec::new();
    let mut original_prefix = None;
    for index in 1..=ACTIONS {
        let id = 200_000 + index;
        if index > 1 {
            node.submit_consensus_action_payload(id, payload(&name(index)))
                .map_err(|e| format!("queue action {index}: {e}"))?;
        }
        let deadline = Instant::now() + Duration::from_secs(90);
        let batch = loop {
            if let Some(batch) = node
                .drain_committed_action_batches()
                .into_iter()
                .find(|batch| batch.actions.iter().any(|action| action.action_id == id))
            {
                break batch;
            }
            let status = node.bft_v3_status().ok_or("normal Node status absent")?;
            if !status.running {
                return Err(format!(
                    "normal Node stopped after {} nonempty receipts, before action {index}: {status:?}",
                    batches.len()
                ));
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "normal Node timed out after {} receipts, before action {index}: {status:?}",
                    batches.len()
                ));
            }
            thread::sleep(Duration::from_millis(10));
        };
        if batch.actions.len() != 1 || batch.actions[0].action_id != id {
            return Err("normal tick did not preserve the exact queued action".into());
        }
        let expected = oasis7_node::NodeConsensusAction::from_payload(
            id,
            validator_id(0),
            payload(&name(index)),
        )
        .map_err(|e| e.to_string())?;
        if batch.actions[0] != expected
            || batch.action_root
                != oasis7_node::compute_consensus_action_root(&[expected])
                    .map_err(|e| e.to_string())?
        {
            return Err("normal batch action/root differs from original valid input".into());
        }
        if snapshot
            .lock()
            .map_err(|_| "capacity fixture snapshot poisoned")?
            .state
            .active_window
            .is_some()
        {
            let bytes = tree(&records.join("formal-bft-header-prefix-v1"))?;
            if let Some(original) = &original_prefix {
                if original != &bytes {
                    return Err(
                        "normal auto-roll rewrote the original H/private prefix bytes".into(),
                    );
                }
            } else {
                original_prefix = Some(bytes);
            }
        }
        batches.push(batch);
    }
    Ok(batches)
}
#[allow(clippy::too_many_arguments)]
fn verify_business(
    store: &LocalCasStore,
    records: &Path,
    binding: &BftSignerBindingV3,
    pin: &str,
    provider: &RemoteBftSignerProviderV3,
    verifier: &ActualBftSuccessorWindowDriverVerifierV1,
    batches: &[oasis7_node::NodeCommittedActionBatch],
) -> Result<(), String> {
    let mut lease = provider.acquire(binding).map_err(|e| e.to_string())?;
    let checkpoint = lease.checkpoint().map_err(|e| e.to_string())?;
    let extension = checkpoint
        .state
        .successor_window
        .as_ref()
        .ok_or("normal worker never checkpointed")?;
    if extension
        .requests
        .iter()
        .filter(|record| record.request.status == BftSuccessorWindowStatusV1::Adopted)
        .count()
        < 2
    {
        return Err(
            "normal worker must complete at least two real capacity-driven Adopt transitions"
                .into(),
        );
    }
    if checkpoint.state.pending_driver_apply.is_some() {
        return Err("normal worker left pending actual apply".into());
    }
    let mut states = vec![checkpoint.state.clone()];
    for record in &extension.requests {
        let body = &record.request.intent.body.checkpoint;
        let original = lease
            .read_successor_original_state(
                &body.request_id,
                &body.digest().map_err(|e| e.to_string())?,
                Instant::now() + Duration::from_secs(120),
            )
            .map_err(|e| e.to_string())?;
        states.push(original.original_state().clone());
    }
    for batch in batches {
        let certified = states
            .iter()
            .flat_map(|state| &state.history)
            .any(|certificate| {
                certificate.execution_commitment.height == batch.height
                    && certificate
                        .header
                        .block_id()
                        .is_ok_and(|id| id == batch.block_hash)
                    && certificate.execution_commitment.action_root == batch.action_root
            });
        let applied = states
            .iter()
            .flat_map(|state| &state.applied_installs)
            .any(|receipt| receipt.height == batch.height && receipt.header_id == batch.block_hash);
        if !certified || !applied {
            return Err(format!(
                "normal action receipt at {} lacks exact authenticated certificate/applied evidence",
                batch.height
            ));
        }
    }
    let active = lease
        .verified_active_window()
        .map_err(|e| e.to_string())?
        .ok_or("normal worker lacks typed active base")?;
    let record = active.original_request();
    let body = &record.request.intent.body.checkpoint;
    let original = lease
        .read_successor_original_state(
            &body.request_id,
            &body.digest().map_err(|e| e.to_string())?,
            Instant::now() + Duration::from_secs(120),
        )
        .map_err(|e| e.to_string())?;
    let installed = original
        .reconstruct_installed_pending(binding, pin, record)
        .map_err(|e| e.to_string())?;
    let base = verifier.recover_private_base(
        record,
        &original,
        Instant::now() + Duration::from_secs(120),
    )?;
    let window = FormalBftHeaderActiveExecutionV1::open(
        records,
        store.clone(),
        base,
        &active,
        binding,
        pin,
        &installed,
        Instant::now() + Duration::from_secs(120),
    )?;
    if batches.len() != ACTIONS as usize {
        return Err("normal business receipt count differs".into());
    }
    for index in 1..=ACTIONS {
        if !window.world().state().agents.contains_key(&name(index)) {
            return Err(format!(
                "normal action {index} did not create its actual World agent"
            ));
        }
    }
    if window.schedule().height() <= binding.handoff_position + 65 {
        return Err("actual World did not progress beyond 65 nonempty actions".into());
    }
    Ok(())
}
fn name(index: u64) -> String {
    format!("normal-capacity-business-{index}")
}

fn tree(path: &Path) -> Result<std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>, String> {
    fn walk(
        root: &Path,
        path: &Path,
        out: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                walk(root, &entry.path(), out)?;
            } else {
                out.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .into(),
                    std::fs::read(entry.path()).map_err(|e| e.to_string())?,
                );
            }
        }
        Ok(())
    }
    let mut out = std::collections::BTreeMap::new();
    walk(path, path, &mut out)?;
    Ok(out)
}

use super::membership_clock as elapsed_clock;
