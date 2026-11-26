//! HTTP acquisition uses real provider signatures and reconstructed issuer
//! decisions; witness/clock remain explicitly simulated fixtures.
use super::*;
use crate::formal_live_service::full_sync_read_gate::{
    BftCandidateReadSubjectV1,
    remote_evidence::{FullSyncReadEvidenceQueryV3, RemoteFullSyncReadEvidenceSourceV3},
};
use std::io::{Read, Write};

fn read_query(
    listener: &std::net::TcpListener,
) -> (std::net::TcpStream, FullSyncReadEvidenceQueryV3) {
    let until = Instant::now() + Duration::from_secs(4);
    let mut stream = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < until,
                    "lease relay did not receive expected original request"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("test accept failed: {e}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        stream.read_exact(&mut b).unwrap();
        header.push(b[0]);
        assert!(header.len() < 8192);
    }
    let size = String::from_utf8(header)
        .unwrap()
        .lines()
        .find_map(|line| {
            let (k, v) = line.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    assert!(size < 16 * 1024);
    let mut body = vec![0; size];
    stream.read_exact(&mut body).unwrap();
    (stream, serde_json::from_slice(&body).unwrap())
}
fn query_request(q: &FullSyncReadEvidenceQueryV3) -> FullSyncIssuerReadEvidenceRequestV3 {
    let start = Instant::now();
    FullSyncIssuerReadEvidenceRequestV3 {
        observation_nonce: hex::decode(&q.observation_nonce)
            .unwrap()
            .try_into()
            .unwrap(),
        minimum_anchor: q.minimum_anchor.clone(),
        provider_public_key: q.provider_public_key.clone(),
        provider_incarnation: q.provider_incarnation.clone(),
        target: q.target.clone(),
        target_generation: q.target_generation,
        started_at: start,
        deadline: start + Duration::from_secs(3),
    }
}
fn subject_for(f: &IssuerFixture) -> BftCandidateReadSubjectV1 {
    BftCandidateReadSubjectV1 {
        world: WorldIdentity {
            world_id: f.trust.world.world_id.clone(),
            genesis_digest: f.trust.world.genesis_digest.clone(),
        },
        chain_id: f.trust.world.chain_id.clone(),
        authority_epoch: f.verified_v3.body().authority_epoch,
        node_public_key: f.target.node_public_key.clone(),
        policy_digest: f.verified_v3.policy_digest().into(),
        grant_generation: 1,
    }
}
#[test]
fn remote_provider_lease_retry_keeps_original_signature_head_and_ids() {
    run_lease_retry(RetryOutcome::Uncommitted);
}

#[test]
fn remote_provider_lease_recovers_after_competing_cas_advances_exact_prefix() {
    run_lease_retry(RetryOutcome::CompetingCas);
}

#[test]
fn remote_provider_lease_recovers_committed_original_after_lost_reply() {
    run_lease_retry(RetryOutcome::Committed);
}

#[test]
fn remote_provider_lease_acknowledged_operation_is_not_resubmitted() {
    run_lease_retry(RetryOutcome::Acknowledged);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RetryOutcome {
    Uncommitted,
    CompetingCas,
    Committed,
    Acknowledged,
}

fn run_lease_retry(outcome: RetryOutcome) {
    let (_, trust, _, _, _) = full_sync_fixture();
    let f = issue_lease_fixture_role(
        trust.initial_trust.clone(),
        now_ms(),
        FullSyncReadRoleV2::BftPendingCandidateV1,
    );
    let witness = Arc::new(SignedEvidenceSource::new(&f, f.lease_not_before));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/read", listener.local_addr().unwrap());
    let server_witness = witness.clone();
    let v2 = f.verified_v2.clone();
    let v3 = f.verified_v3.clone();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let server_captured = captured.clone();
    let server = std::thread::spawn(move || {
        let mut nonces = std::collections::BTreeSet::new();
        for index in 0..4 {
            let (mut stream, query) = read_query(&listener);
            assert!(nonces.insert(query.observation_nonce.clone()));
            let request = query_request(&query);
            let before = server_witness.fetch(&request).unwrap();
            let read_only = index % 2 == 0
                || (index == 3
                    && matches!(outcome, RetryOutcome::Committed | RetryOutcome::Acknowledged));
            assert_eq!(query.provider_lease_request.is_none(), read_only);
            if let Some(signed) = query.provider_lease_request {
                signed.verify(&v3).unwrap();
                assert_eq!(signed.body.provider_incarnation, issuer_hash(29));
                assert_eq!(signed.body.target, request.target);
                assert_eq!(signed.body.target_generation, 1);
                server_captured.lock().unwrap().push(signed.clone());
                if index == 1 && outcome == RetryOutcome::Uncommitted {
                    // The original CAS has not committed. A lost result alone
                    // cannot authorize replacement of its signed request.
                    stream.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    continue;
                }
                let state = VerifiedFullSyncIssuerStateV3::from_witness_observation(
                    v2.clone(),
                    v3.clone(),
                    before.history.clone(),
                    &before.observation,
                )
                .unwrap();
                // The issuer performs its own exact-head witness query for
                // the CAS. A read client's external minimum is not that query.
                let lease_observation = v3_observation_for_test(
                    &v3,
                    state.anchor(),
                    77 + index,
                    before.observation.body.protected_time.latest_unix_ms,
                );
                let signed = if index == 1 && outcome == RetryOutcome::CompetingCas {
                    // A different incarnation wins the same exact CAS head.
                    // The original is absent from the resulting signed prefix.
                    let mut competing = signed.body.clone();
                    competing.provider_incarnation = issuer_hash(180);
                    competing.lease_id = issuer_hash(181);
                    competing.request_id = issuer_hash(182);
                    competing.request_nonce = issuer_hash(183);
                    oasis7_distfs::controlled_authority::formal::full_sync_issuer::SignedFullSyncProviderLeaseRequestV3::sign(
                        competing,
                        &key(PROVIDER_KEY),
                    )
                    .unwrap()
                } else {
                    signed
                };
                let prepared = state
                    .prepare_lease(signed, &lease_observation, &key(LEASE_SIGNER_KEY))
                    .unwrap();
                let record = prepared.record().unwrap().clone();
                let receipt = SignedFullSyncIssuerReceiptV3::sign(
                    FullSyncIssuerReceiptObservationV3::V3(lease_observation),
                    &record,
                    &key(WITNESS_KEY),
                )
                .unwrap();
                let decision = prepared.commit(receipt).unwrap();
                server_witness
                    .history
                    .lock()
                    .unwrap()
                    .v3_entries
                    .push(decision.entry().clone());
                if index == 1 && outcome != RetryOutcome::Acknowledged {
                    stream.write_all(b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    continue;
                }
            }
            let evidence = server_witness.fetch(&request).unwrap();
            let body = serde_json::to_vec(&evidence).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    let remote = RemoteFullSyncReadEvidenceSourceV3::new(&endpoint)
        .unwrap()
        .with_provider_lease_requests(
            f.verified_v2.clone(),
            f.verified_v3.clone(),
            key(PROVIDER_KEY),
        )
        .unwrap();
    let gate = FullSyncV3ReadGate::new(
        f.trust.clone(),
        f.signed_v2.clone(),
        f.signed_v3.clone(),
        f.minimum_v3.clone(),
        public(PROVIDER_KEY),
        issuer_hash(29),
        Arc::new(remote),
    )
    .unwrap();
    let calls = AtomicU64::new(0);
    let first = gate.authorize_candidate_with_response(&subject_for(&f), |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![7])
    });
    let first_released = u64::from(outcome == RetryOutcome::Acknowledged);
    if outcome == RetryOutcome::Acknowledged {
        assert_eq!(first.unwrap(), vec![7]);
    } else {
        assert!(first.is_err());
    }
    assert_eq!(calls.load(Ordering::SeqCst), first_released);
    assert_eq!(
        gate.authorize_candidate_with_response(&subject_for(&f), |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![7])
        })
        .unwrap(),
        vec![7]
    );
    assert_eq!(calls.load(Ordering::SeqCst), first_released + 1);
    server.join().unwrap();
    let signed = captured.lock().unwrap();
    assert_eq!(
        signed.len(),
        if matches!(outcome, RetryOutcome::Committed | RetryOutcome::Acknowledged) {
            1
        } else {
            2
        }
    );
    assert_eq!(
        signed[0].body.expected_head,
        f.history.v3_entries.last().unwrap().record.next_anchor
    );
    if outcome == RetryOutcome::CompetingCas {
        assert_ne!(signed[0], signed[1]);
        assert_ne!(signed[0].body.request_id, signed[1].body.request_id);
        assert_eq!(
            signed[1].body.expected_head.revision,
            signed[0].body.expected_head.revision + 1
        );
    } else if outcome == RetryOutcome::Uncommitted {
        assert_eq!(signed[0], signed[1]);
    }
    let history = witness.history.lock().unwrap();
    let appended = &history.v3_entries[f.history.v3_entries.len()..];
    assert_eq!(
        appended.len(),
        if outcome == RetryOutcome::CompetingCas {
            2
        } else {
            1
        }
    );
    match &appended.last().unwrap().record.event {
        FullSyncIssuerEventV3::Lease { request, .. } => {
            assert_eq!(request.as_ref(), signed.last().unwrap());
        }
        _ => panic!("recovery did not preserve the original lease operation"),
    }
}
