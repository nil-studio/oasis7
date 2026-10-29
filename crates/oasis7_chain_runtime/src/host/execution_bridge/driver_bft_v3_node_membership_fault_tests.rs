//! Actual World/ledger + native TCP failures. Snapshot storage/clock are explicit
//! test doubles; object restarts do not attest physical protected persistence.
use super::super::super::super::super::super::formal_header::FormalBftHeaderCandidateV3;
use super::super::super::super::execution_tests::certify;
use super::*;
use oasis7_consensus::tendermint_v3::{BftVoteIntentV3, verify_commit_certificate_token_v3};
use oasis7_proto::distributed_bft::v3::{BftVotePhaseV3, BftVoteValueV3};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};

pub(super) fn open_actual(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Scratch,
    store: &LocalCasStore,
) -> Arc<Mutex<FormalBftHeaderNodeDriverV3>> {
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
    actual
        .lock()
        .unwrap()
        .enable_bft_v3_membership_execution()
        .unwrap();
    actual
}
fn service(
    binding: &BftSignerBindingV3,
    snapshot: &Arc<Mutex<BftSignerBackendSnapshotV3>>,
) -> Arc<BftSignerServiceV3> {
    Arc::new(
        BftSignerServiceV3::open(
            binding.clone(),
            Box::new(MemoryTestSignerKey(fixtures::key(203))),
            vec![Box::new(MemoryTestSignerKey(fixtures::key(123)))],
            Box::new(MemoryTestSignerBackend {
                binding_digest: binding.digest().unwrap(),
                snapshot: snapshot.clone(),
                fail_confirm_after_commit: None,
            }),
        )
        .unwrap(),
    )
}
pub(super) fn remote_at(
    binding: &BftSignerBindingV3,
    address: SocketAddr,
) -> RemoteBftSignerProviderV3 {
    RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(10),
        binding.clone(),
        fixtures::key(213),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: hex::encode(fixtures::key(203).verifying_key().to_bytes()),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap()
}
fn frame(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut len = [0; 4];
    stream.read_exact(&mut len)?;
    let size = u32::from_be_bytes(len) as usize;
    assert!(
        size <= 40 * 1024 * 1024,
        "test proxy keeps production frame bounded"
    );
    let mut bytes = vec![0; size + 4];
    bytes[..4].copy_from_slice(&len);
    stream.read_exact(&mut bytes[4..])?;
    Ok(bytes)
}
/// Forward genuine authenticated frames untouched. Only the original ACK reply
/// is withheld, after the real provider has returned its already-durable result.
fn lose_ack_reply(address: SocketAddr) -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = listener.local_addr().unwrap();
    let join = thread::spawn(move || {
        let (mut client, _) = listener.accept().unwrap();
        let mut provider = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        provider
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        loop {
            let request = frame(&mut client).unwrap();
            let value: serde_json::Value = serde_cbor::from_slice(&request[4..]).unwrap();
            let drop_reply = value.pointer("/body/command/kind").and_then(|v| v.as_str())
                == Some("CompleteTransitionInstall");
            provider.write_all(&request).unwrap();
            let response = frame(&mut provider).unwrap();
            if drop_reply {
                provider.shutdown(Shutdown::Both).unwrap();
                client.shutdown(Shutdown::Both).unwrap();
                return;
            }
            client.write_all(&response).unwrap();
        }
    });
    (proxy, join)
}
fn replay_actual(
    actual: &Arc<Mutex<FormalBftHeaderNodeDriverV3>>,
    binding: &BftSignerBindingV3,
    history: &[FormalBftCertifiedHeaderV3],
) -> NodeBftExecutionCommitResultV3 {
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(binding)
        .unwrap();
    let mut schedule = binding.initial_schedule().clone();
    let mut tip = None;
    for certificate in history {
        let token = verify_commit_certificate_token_v3(&schedule, certificate).unwrap();
        tip = Some(
            actual
                .lock()
                .unwrap()
                .commit_bft_v3_certificate(token.clone())
                .unwrap(),
        );
        schedule = token.next_height_schedule().clone();
    }
    tip.unwrap()
}

#[test]
fn wire3_membership_native_pending_install_cold_resume_and_lost_ack_reply_keep_original_receipt() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(actual_fault_case)
        .unwrap()
        .join()
        .unwrap();
}
fn actual_fault_case() {
    let h = handoff();
    let root = Scratch::new();
    let store = cas(root.path());
    let mut actual = open_actual(&h, &root, &store);
    let binding = binding_for(&actual, 3);
    actual
        .lock()
        .unwrap()
        .configure_bft_v3_execution(&binding)
        .unwrap();
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    assert!(snapshot.lock().unwrap().state.journal.is_none());
    let (address, server) = tcp(service(&binding, &snapshot));
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    let initial = lease.checkpoint().unwrap();
    let mut checkpoint = lease
        .enable_membership(initial.state.revision, &initial.state.state_digest)
        .unwrap();
    let mut schedule = binding.initial_schedule().clone();
    let mut tip = None;
    let mut originals = Vec::new();
    for height in 3..=5 {
        let actions = if height == 3 {
            admission_payloads(&actual.lock().unwrap())
        } else {
            vec![(
                9900 + height,
                payload(&format!("membership-fault-agent-{height}")),
            )]
        };
        let actions = actions
            .into_iter()
            .map(|(id, p)| {
                oasis7_node::NodeConsensusAction::from_payload(id, validator_id(0), p).unwrap()
            })
            .collect();
        let context = NodeBftExecutionContextV3::from_schedule(&schedule);
        let candidate = actual
            .lock()
            .unwrap()
            .build_bft_v3_candidate(NodeBftBuildContextV3 {
                execution: context.clone(),
                actions,
            })
            .unwrap();
        let digest = actual
            .lock()
            .unwrap()
            .retain_bft_v3_private_candidate(&context, &candidate)
            .unwrap();
        originals.push((candidate.clone(), digest));
        let certificate = certify(
            &schedule,
            &FormalBftHeaderCandidateV3 {
                header: candidate.public_value.header.clone(),
                commitment: candidate.public_value.execution_commitment.clone(),
            },
        );
        let token = verify_commit_certificate_token_v3(&schedule, &certificate).unwrap();
        if height == 4 {
            assert_eq!(
                certificate
                    .execution_commitment
                    .transition
                    .as_ref()
                    .unwrap()
                    .activation_height,
                6
            );
        }
        assert!(
            certificate
                .certificate
                .precommits
                .iter()
                .all(|v| v.validator_id != validator_id(3))
        );
        checkpoint = lease
            .append_committed_certificate(
                checkpoint.state.revision,
                &checkpoint.state.state_digest,
                certificate,
            )
            .unwrap();
        let installed = actual
            .lock()
            .unwrap()
            .commit_bft_v3_certificate(token.clone())
            .unwrap();
        checkpoint = lease
            .confirm_applied(
                checkpoint.state.revision,
                &checkpoint.state.state_digest,
                installed.height,
                &installed.header_id,
                &installed.driver_install_digest,
            )
            .unwrap();
        tip = Some(installed);
        schedule = token.next_height_schedule().clone();
        if height < 5 {
            assert!(checkpoint.state.journal.is_none());
        }
    }
    let tip = tip.unwrap();
    assert_eq!(schedule.height(), 6);
    assert_eq!(schedule.active_set().validators().len(), 4);
    let before = checkpoint.state.clone();
    assert!(
        lease.start_height_timer(before.revision).is_err(),
        "missing membership ACK must not start the new key timer"
    );
    let vote = BftVoteIntentV3 {
        height: 6,
        round: 0,
        phase: BftVotePhaseV3::Prevote,
        value: BftVoteValueV3::Nil,
        validator_id: validator_id(3),
        schedule_policy_hash: schedule.policy_hash().into(),
    };
    assert!(lease.sign_vote(before.revision, &vote).is_err());
    assert_eq!(lease.checkpoint().unwrap().state, before);
    let request_id = "native-membership-original-request".to_string();
    let original_revision = checkpoint.state.revision;
    let original_digest = checkpoint.state.state_digest.clone();
    let preparation = lease
        .prepare_transition_install(
            original_revision,
            &original_digest,
            request_id.clone(),
            4,
            binding.operator_instance_id.clone(),
        )
        .unwrap();
    let BftTransitionInstallPreparationV3::Pending {
        intent,
        checkpoint: prepared,
    } = preparation
    else {
        panic!("first install must persist pending intent")
    };
    let evidence = BftLocalInstallEvidenceV3 {
        schema_version: 3,
        runtime_instance_id: binding.operator_instance_id.clone(),
        installed_head_height: tip.height,
        installed_head_commit_ref: tip.header_id.clone(),
        installed_state_root: tip.state_root.clone(),
        installed_manifest_hash: tip.post_manifest_hash.clone(),
        installed_active_set_hash: schedule.active_set().set_hash().into(),
        installed_authority_epoch: schedule.authority_epoch(),
        local_validator_id: validator_id(3),
        local_signing_public_key: hex::encode(fixtures::key(123).verifying_key().to_bytes()),
        wal_install_digest: prepared.state.journal.as_ref().unwrap().digest().unwrap(),
        driver_install_digest: tip.driver_install_digest.clone(),
    };
    assert_eq!(prepared.state.history, before.history);
    assert_eq!(prepared.state.journal, before.journal);
    drop(lease);
    server.join().unwrap().unwrap();
    drop(actual);
    actual = open_actual(&h, &root, &store);
    let replayed = replay_actual(&actual, &binding, &snapshot.lock().unwrap().state.history);
    assert_eq!(replayed, tip);
    for (candidate, digest) in &originals {
        assert_eq!(
            actual
                .lock()
                .unwrap()
                .load_bft_v3_private_candidate(&candidate.public_value)
                .unwrap(),
            *candidate
        );
        assert_eq!(hex::decode(digest).unwrap().len(), 32);
    }
    let (address, server) = tcp(service(&binding, &snapshot));
    let direct = remote_at(&binding, address);
    let mut direct_lease = direct.acquire(&binding).unwrap();
    let pending_original = direct_lease.checkpoint().unwrap().state;
    assert_eq!(
        pending_original
            .membership_install
            .as_ref()
            .unwrap()
            .pending
            .as_ref(),
        Some(&intent)
    );
    let mut wrong = evidence.clone();
    wrong.wal_install_digest = "a".repeat(64);
    assert!(
        direct_lease
            .complete_transition_install(intent.clone(), wrong)
            .is_err()
    );
    let mut wrong = evidence.clone();
    wrong.driver_install_digest = "b".repeat(64);
    assert!(
        direct_lease
            .complete_transition_install(intent.clone(), wrong)
            .is_err()
    );
    assert!(
        direct_lease
            .prepare_transition_install(
                original_revision,
                &original_digest,
                request_id.clone(),
                3,
                binding.operator_instance_id.clone()
            )
            .is_err()
    );
    assert_eq!(direct_lease.checkpoint().unwrap().state, pending_original);
    drop(direct_lease);
    server.join().unwrap().unwrap();
    let (address, server) = tcp(service(&binding, &snapshot));
    let (proxy, relay) = lose_ack_reply(address);
    let provider = remote_at(&binding, proxy);
    let mut lease = provider.acquire(&binding).unwrap();
    let resumed = lease
        .prepare_transition_install(
            original_revision,
            &original_digest,
            request_id.clone(),
            4,
            binding.operator_instance_id.clone(),
        )
        .unwrap();
    assert!(
        matches!(resumed,BftTransitionInstallPreparationV3::Pending{intent:ref original,..} if *original==intent)
    );
    let mut bad = evidence.clone();
    bad.wal_install_digest = "a".repeat(64);
    // Wrong requests are tested on the authenticated direct session below;
    // the loss proxy intentionally drops the first completion response only.
    assert_ne!(bad.wal_install_digest, evidence.wal_install_digest);
    assert!(
        lease
            .complete_transition_install(intent.clone(), evidence.clone())
            .is_err(),
        "real ACK committed but original reply was lost"
    );
    relay.join().unwrap();
    drop(lease);
    let _ = server.join().unwrap();
    let committed = snapshot.lock().unwrap().state.clone();
    let original_receipt = committed.membership_install.as_ref().unwrap().receipts[0].clone();
    assert!(
        committed
            .membership_install
            .as_ref()
            .unwrap()
            .pending
            .is_none()
    );
    let (address, server) = tcp(service(&binding, &snapshot));
    let provider = remote_at(&binding, address);
    let mut lease = provider.acquire(&binding).unwrap();
    assert!(
        lease
            .complete_transition_install(intent.clone(), bad)
            .is_err()
    );
    let mut bad = evidence.clone();
    bad.driver_install_digest = "b".repeat(64);
    assert!(
        lease
            .complete_transition_install(intent.clone(), bad)
            .is_err()
    );
    assert!(
        lease
            .prepare_transition_install(
                original_revision,
                &original_digest,
                request_id.clone(),
                3,
                binding.operator_instance_id.clone()
            )
            .is_err()
    );
    let retried = lease
        .complete_transition_install(intent.clone(), evidence)
        .unwrap();
    assert_eq!(retried.receipt, original_receipt);
    assert_eq!(retried.checkpoint.state, committed);
    let completed = lease
        .prepare_transition_install(
            original_revision,
            &original_digest,
            request_id,
            4,
            binding.operator_instance_id.clone(),
        )
        .unwrap();
    assert!(
        matches!(completed,BftTransitionInstallPreparationV3::Completed{receipt,..} if receipt==original_receipt)
    );
    let after = lease.start_height_timer(committed.revision).unwrap();
    assert!(after.state.journal.unwrap().height_started_at_ms.is_some());
    assert!(
        actual
            .lock()
            .unwrap()
            .world()
            .state()
            .agents
            .contains_key("membership-fault-agent-5")
    );
    drop(lease);
    server.join().unwrap().unwrap();
}
