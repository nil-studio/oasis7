//! Real HTTP and provider signatures; witness/time are test simulations only.
use super::*;
use crate::formal_live_config::FormalLiveReadAuthorizationV3;
use crate::formal_live_service::full_sync_read_gate::remote_evidence::FullSyncReadEvidenceQueryV3;
use std::io::{Read, Write};

pub(crate) struct StartupV3Fixture {
    config: FormalLiveReadAuthorizationV3,
    grant_not_before: u64,
    grant_expires: u64,
    server: std::thread::JoinHandle<Vec<String>>,
}
impl StartupV3Fixture {
    pub(crate) fn new(trust: InitialActivationTrustV1) -> Self {
        let base = now_ms().saturating_sub(2_000);
        let f = issue_lease_fixture_keys(
            trust,
            base,
            FullSyncReadRoleV2::StateSync,
            READER_KEY,
            90,
            PROVIDER_INCARNATION,
            &[90],
        );
        let witness = SignedEvidenceSource::new(&f, f.lease_not_before);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let config = FormalLiveReadAuthorizationV3 {
            evidence_endpoint: format!("http://{}/read", listener.local_addr().unwrap()),
            signed_v2_policy: f.signed_v2,
            signed_v3_policy: f.signed_v3,
            minimum_anchor: f.minimum_v3,
        };
        let observed_time = now_ms();
        let observed_at = Instant::now();
        let server = std::thread::spawn(move || {
            let mut incarnations = Vec::new();
            // Each fresh runtime performs observation + lease acquisition on
            // the pre-material check, then observes again before release.
            for _ in 0..6 {
                let (mut stream, q) = read_query(&listener);
                assert_eq!(q.provider_public_key, public(90));
                let start = Instant::now();
                let request = FullSyncIssuerReadEvidenceRequestV3 {
                    observation_nonce: hex::decode(&q.observation_nonce)
                        .unwrap()
                        .try_into()
                        .unwrap(),
                    minimum_anchor: q.minimum_anchor,
                    provider_public_key: q.provider_public_key,
                    provider_incarnation: q.provider_incarnation,
                    target: q.target,
                    target_generation: q.target_generation,
                    started_at: start,
                    deadline: start + Duration::from_secs(3),
                };
                let protected_time = observed_time + observed_at.elapsed().as_millis() as u64;
                witness.set_time(protected_time);
                let before = witness.fetch(&request).unwrap();
                if let Some(signed) = q.provider_lease_request {
                    signed.verify(&f.verified_v3).unwrap();
                    assert_eq!(
                        signed.body.provider_incarnation,
                        request.provider_incarnation
                    );
                    assert_eq!(signed.body.target, request.target);
                    incarnations.push(signed.body.provider_incarnation.clone());
                    let state = VerifiedFullSyncIssuerStateV3::from_witness_observation(
                        f.verified_v2.clone(),
                        f.verified_v3.clone(),
                        before.history,
                        &before.observation,
                    )
                    .unwrap();
                    let observation =
                        v3_observation_for_test(&f.verified_v3, state.anchor(), 77, protected_time);
                    let prepared = state
                        .prepare_lease(signed, &observation, &key(LEASE_SIGNER_KEY))
                        .unwrap();
                    let record = prepared.record().unwrap().clone();
                    let receipt = SignedFullSyncIssuerReceiptV3::sign(
                        FullSyncIssuerReceiptObservationV3::V3(observation),
                        &record,
                        &key(WITNESS_KEY),
                    )
                    .unwrap();
                    let decision = prepared.commit(receipt).unwrap();
                    witness
                        .history
                        .lock()
                        .unwrap()
                        .v3_entries
                        .push(decision.entry().clone());
                }
                let evidence = witness.fetch(&request).unwrap();
                let body = serde_json::to_vec(&evidence).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            incarnations
        });
        Self {
            config,
            server,
            grant_not_before: base.saturating_sub(1_000),
            grant_expires: base + 60_000,
        }
    }
    pub(crate) fn configuration(&self) -> FormalLiveReadAuthorizationV3 {
        self.config.clone()
    }
    pub(crate) fn read_manifest(&self, endpoint: &str, anchor: &VerifiedFormalReadAnchorV1) -> u16 {
        let requested = cut(anchor);
        let reader = key(READER_KEY);
        let grant = FormalNodeReadGrantV1::sign(
            FormalNodeReadGrantBodyV1 {
                schema_version: 1,
                world: requested.world.clone(),
                chain_id: requested.chain_id.clone(),
                node_public_key: public(READER_KEY),
                role: FormalNodeReadRoleV1::StateSync,
                grant_epoch: 1,
                authority_epoch: requested.authority_epoch,
                not_before_unix_ms: self.grant_not_before,
                expires_unix_ms: self.grant_expires,
            },
            &hex::encode(key(9).to_bytes()),
        )
        .unwrap();
        post_manifest(endpoint, &reader, grant, requested).0
    }
    pub(crate) fn finish(self) {
        let incarnations = self.server.join().unwrap();
        assert_eq!(
            incarnations.len(),
            2,
            "both real starts must acquire their own lease"
        );
        assert_ne!(
            incarnations[0], incarnations[1],
            "reopen must not reuse incarnation"
        );
    }
}
fn read_query(
    listener: &std::net::TcpListener,
) -> (std::net::TcpStream, FullSyncReadEvidenceQueryV3) {
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "ordinary runtime did not reach V3 evidence RPC"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("accept evidence: {e}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        header.push(byte[0]);
        assert!(header.len() < 8192);
    }
    let header = String::from_utf8(header).unwrap();
    assert!(header.starts_with("POST /read HTTP/1.1"));
    let size = header
        .lines()
        .find_map(|line| {
            let (k, v) = line.split_once(':')?;
            k.eq_ignore_ascii_case("content-length")
                .then(|| v.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    assert!(size < 16 * 1024);
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).unwrap();
    (stream, serde_json::from_slice(&bytes).unwrap())
}
