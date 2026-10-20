//! Actual validator CLI children consume real signed HTTP formal publications.
//! Test-authority fixtures grant no deployment/key-custody qualification.
use super::watch_tcp::{CHILD_ENV, CHILD_FILTER, observations, receive, send};
use crate::formal_read_fixtures as fixtures;
use fixtures::*;
use oasis7::world_service::{
    SignedReadRequest, authority,
    client::FormalReadTrustConfigV1,
    formal_read::{FormalReadHistoryV1, FormalReadStepV1},
    formal_read_service::{FormalCommittedReadService, FormalReadPublication},
};
use oasis7_client_api::world_service::{FormalReadCursorBundleV1, ReadWorldViewRequest};
use std::{
    fs,
    net::TcpListener,
    path::Path,
    process::{Command, Output},
    thread,
};

fn child(endpoint: &str, trust: &Path, input: Option<&Path>, output: Option<&Path>) -> Output {
    let mut args = vec![
        "validator".to_string(),
        "watch".into(),
        "--node-id".into(),
        "node".into(),
        "--timeout-ms".into(),
        "5000".into(),
        "--interval-ms".into(),
        "100".into(),
    ];
    if let Some(path) = input {
        args.extend(["--cursor-file".into(), path.to_str().unwrap().into()]);
    }
    if let Some(path) = output {
        args.extend(["--cursor-out".into(), path.to_str().unwrap().into()]);
    }
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD_FILTER, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, serde_json::to_string(&args).unwrap())
        .env("OASIS7_WORLD_SERVICE_ENDPOINT", endpoint)
        .env("OASIS7_WORLD_SERVICE_PUBLIC_KEY", public(9))
        .env("OASIS7_WORLD_SERVICE_WORLD_ID", "formal-fixture-world")
        .env("OASIS7_WORLD_SERVICE_GENESIS_DIGEST", h(8))
        .env("OASIS7_WORLD_SERVICE_SCOPE", "public")
        .env("OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY", h(7))
        .env("OASIS7_WORLD_SERVICE_FORMAL_TRUST_FILE", trust)
        .output()
        .unwrap()
}
#[test]
fn formal_validator_watch_disconnect_proof_cursor_restart_rejects_replay_and_older_cut() {
    let (world1, trust, anchor1, _) = actual_world_initial_fixture();
    let (world2, anchor2, proof2) = actual_world_successor_fixture(&anchor1, world1.clone());
    let (world3, anchor3, proof3) = actual_world_successor_fixture(&anchor2, world2.clone());
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir().join(format!(
        "oasis7-formal-watch-{}-{}",
        std::process::id(),
        hex::encode(random)
    ));
    fs::create_dir(&root).unwrap();
    let root = fs::canonicalize(root).unwrap();
    let trust_file = root.join("external-trust.json");
    let cursor_file = root.join("cursor.json");
    let checkpoint = anchor1.checkpoint();
    fs::write(
        &trust_file,
        serde_json::to_vec(&FormalReadTrustConfigV1 {
            initial_trust: trust.clone(),
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        })
        .unwrap(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let publication1 = FormalReadPublication::new(anchor1, world1).unwrap();
        let publication2 = FormalReadPublication::new(anchor2, world2).unwrap();
        let service2 = FormalCommittedReadService::new(
            trust.clone(),
            FormalReadPublication::new(publication2.anchor().clone(), publication2.world().clone())
                .unwrap(),
        )
        .unwrap();
        let service3 = FormalCommittedReadService::new(
            trust,
            FormalReadPublication::new(anchor3, world3).unwrap(),
        )
        .unwrap();
        let mut old_reply = None;
        let mut nonces = Vec::new();
        for round in 0..5 {
            let (mut stream, _) = listener.accept().unwrap();
            let signed = receive(&mut stream, "/v1/world/view");
            let signed: SignedReadRequest<ReadWorldViewRequest> =
                serde_json::from_value(serde_json::to_value(signed).unwrap()).unwrap();
            let nonce = signed.request.observation_nonce.clone().unwrap();
            assert!(!nonces.contains(&nonce));
            nonces.push(nonce);
            assert_eq!(
                signed
                    .request
                    .min_commit
                    .as_ref()
                    .map(|commit| commit.position),
                (round > 0 && round != 4).then_some(2)
            );
            let payload = if round == 0 || round == 4 {
                service2
                    .read(
                        &signed,
                        &publication1,
                        &FormalReadHistoryV1 {
                            schema_version: 1,
                            steps: vec![FormalReadStepV1::Commit(Box::new(proof2.clone()))],
                        },
                    )
                    .unwrap()
            } else if round == 2 {
                // The service really signs an older, independently valid
                // publication; its proof cannot satisfy the resumed C2 query.
                let mut older = signed.request.clone();
                older.min_commit = None;
                older.formal_anchor =
                    Some(oasis7_client_api::world_service::FormalReadAnchorRefV1 {
                        position: publication1.anchor().head().record().position,
                        record_id: publication1.anchor().head().head().record_id,
                        active_authority_id: publication1
                            .anchor()
                            .authority()
                            .authority_id()
                            .unwrap(),
                    });
                publication1
                    .view_at_own_anchor(&older, &signed.subject_public_key)
                    .unwrap()
            } else if round == 3 {
                service3
                    .read(
                        &signed,
                        &publication2,
                        &FormalReadHistoryV1 {
                            schema_version: 1,
                            steps: vec![FormalReadStepV1::Commit(Box::new(proof3.clone()))],
                        },
                    )
                    .unwrap()
            } else {
                // A resumed request carries the exact authenticated minimum.
                // Use the service's proof-aware minimum validator; the raw
                // own-anchor formatter deliberately accepts no minimum.
                service2
                    .read(
                        &signed,
                        &publication2,
                        &FormalReadHistoryV1 {
                            schema_version: 1,
                            steps: vec![],
                        },
                    )
                    .unwrap()
            };
            let reply = authority::sign_service_response(
                "/v1/world/view",
                authority::request_digest("/v1/world/view", &signed).unwrap(),
                serde_json::to_value(payload).unwrap(),
                &h(9),
            )
            .unwrap();
            if round == 0 {
                old_reply = Some(reply.clone());
            }
            send(
                &mut stream,
                if round == 1 || round == 4 {
                    old_reply.as_ref().unwrap()
                } else {
                    &reply
                },
            );
            drop(stream);
            if round == 0 || round == 3 {
                let (mut stream, _) = listener.accept().unwrap();
                let query = receive(&mut stream, "/v1/world/view");
                let query: ReadWorldViewRequest = serde_json::from_value(query.request).unwrap();
                assert!(!nonces.contains(query.observation_nonce.as_ref().unwrap()));
                assert_eq!(
                    query.min_commit.unwrap().position,
                    if round == 0 { 2 } else { 3 }
                );
                drop(stream); // Real transport disconnect terminates watch.
            }
        }
    });
    let stopped = child(&endpoint, &trust_file, None, Some(&cursor_file));
    assert!(!stopped.status.success());
    let emitted = observations(&stopped);
    assert_eq!(
        emitted.len(),
        1,
        "{}\n{}",
        String::from_utf8_lossy(&stopped.stdout),
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert_eq!(emitted[0]["cursor"]["position"], 2);
    assert_eq!(emitted[0]["formal_cursor"]["commit"]["position"], 2);
    let saved = super::super::cursor_io::read(&cursor_file).unwrap();
    assert_eq!(saved.frames.len(), 1);
    let replay = child(&endpoint, &trust_file, Some(&cursor_file), None);
    assert!(!replay.status.success());
    assert!(observations(&replay).is_empty());
    assert!(String::from_utf8_lossy(&replay.stderr).contains("response request mismatch"));
    let older = child(&endpoint, &trust_file, Some(&cursor_file), None);
    assert!(!older.status.success());
    assert!(observations(&older).is_empty());
    assert!(String::from_utf8_lossy(&older.stderr).contains("formal proof"));
    let resumed = child(&endpoint, &trust_file, Some(&cursor_file), None);
    assert!(!resumed.status.success());
    let resumed_output = observations(&resumed);
    assert_eq!(
        resumed_output.len(),
        1,
        "{}\n{}",
        String::from_utf8_lossy(&resumed.stdout),
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert_eq!(resumed_output[0]["cursor"]["position"], 3);
    assert_eq!(
        resumed_output[0]["assurance"],
        "independently_verified_formal_public_projection_not_runtime_application"
    );
    assert_eq!(resumed_output[0]["source_freshness"], "unknown");
    assert!(resumed_output[0]["applied_consensus_membership"].is_null());
    // Same external anchor, reader, scope and minimum as the original request:
    // the fresh OS nonce alone prevents replaying its old signed response.
    let nonce_only_replay = child(&endpoint, &trust_file, None, None);
    assert!(!nonce_only_replay.status.success());
    assert!(observations(&nonce_only_replay).is_empty());
    assert!(
        String::from_utf8_lossy(&nonce_only_replay.stderr).contains("response request mismatch")
    );
    server.join().unwrap();
    // Saved content is untrusted: repeating/reordering valid frames or changing
    // the final binding cannot seed a fresh client or produce CLI observations.
    let complete: FormalReadCursorBundleV1<
        oasis7::world_service::projection::WorldServiceProjection,
    > = serde_json::from_value(resumed_output[0]["formal_cursor"].clone()).unwrap();
    let mut repeated = saved.clone();
    repeated.frames.push(saved.frames[0].clone());
    fs::write(&cursor_file, serde_json::to_vec(&repeated).unwrap()).unwrap();
    let invalid = child(&endpoint, &trust_file, Some(&cursor_file), None);
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("repeats"),
        "{}",
        String::from_utf8_lossy(&invalid.stderr)
    );
    assert!(observations(&invalid).is_empty());
    let mut reordered = complete;
    reordered.frames.reverse();
    fs::write(&cursor_file, serde_json::to_vec(&reordered).unwrap()).unwrap();
    let invalid = child(&endpoint, &trust_file, Some(&cursor_file), None);
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("formal response is stale relative to local verified cache"),
        "{}",
        String::from_utf8_lossy(&invalid.stderr)
    );
    assert!(observations(&invalid).is_empty());
    let mut binding = saved;
    binding.commit.binding.permission_generation += 1;
    fs::write(&cursor_file, serde_json::to_vec(&binding).unwrap()).unwrap();
    let invalid = child(&endpoint, &trust_file, Some(&cursor_file), None);
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("final binding"),
        "{}",
        String::from_utf8_lossy(&invalid.stderr)
    );
    assert!(observations(&invalid).is_empty());
    fs::remove_dir_all(root).unwrap();
}
