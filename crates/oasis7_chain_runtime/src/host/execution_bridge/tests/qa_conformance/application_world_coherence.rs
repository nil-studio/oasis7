//! Real scoped collection interleaves two independently authenticated reads.
use super::application_stream_boundaries::Session;
use super::*;
use oasis7_viewer_runtime::viewer::{
    CollectDataCommand, CollectDataRequest, ViewerRuntimeLiveServer, sign_collect_data_auth_proof,
};
use std::fs;
#[test]
fn real_tcp_view_changes_interleaving_completes_coherently() {
    let fixture = Fixture::with_controlled_commits(true);
    let root = temp_dir("pre2-world-coherence");
    fs::create_dir_all(&root).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let mut connection = fixture.client.config().clone();
    connection.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(connection).unwrap();
    let baseline = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        &client,
        false,
        Duration::from_secs(60),
        Some(root.join("private-lineage.json")),
    ))
    .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let mut session = Session::start(&shared, false);
    let warmed = session.warm();
    *fixture.world_gate.root.lock().unwrap() = Some(root.clone());
    fs::write(root.join("world-concurrent-ready"), b"ready").unwrap();
    fixture.concurrent_dispatch.store(true, Ordering::SeqCst);
    fixture
        .world_gate
        .arm_coherence_changes(baseline.continuation().clone());
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1701}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 after Pause","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !root.join("coherence-changes-original.json").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    let held = root.join("coherence-changes-original.json").exists();
    let public = sign_read_request("owner", (), &fixture.owner)
        .unwrap()
        .subject_public_key;
    let mut command = CollectDataCommand::Submit {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "owner-a".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let proof = sign_collect_data_auth_proof(&command, 9, &public, &fixture.owner).unwrap();
    let CollectDataCommand::Submit { request } = &mut command else {
        unreachable!()
    };
    request.auth = Some(proof);
    let original = fixture.request(WorldServicePayloadV1::GameplayJson(
        serde_json::to_vec(&command).unwrap(),
    ));
    let submit = client.submit(original.clone());
    if submit.is_ok() {
        let mut driver = fixture.driver.lock().unwrap();
        let height = driver.state.last_applied_committed_height + 1;
        commit_request(&mut driver, height, Some(original.clone()));
    }
    let mut committed = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .is_ok_and(|r| matches!(r.outcome, IntentOutcome::Committed { .. }))
        {
            committed = true;
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let post = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: client.config().scope_id.clone(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    let changes = client
        .read_changes(ReadWorldChangesRequest {
            contract_version: 1,
            cursor: baseline.continuation().clone(),
            max_items: 256,
            max_bytes: 1_048_576,
        })
        .unwrap();
    let same_binding = baseline.version().commit.world == post.version().commit.world
        && baseline.version().commit.binding == post.version().commit.binding;
    let same_stream = changes.next_cursor.stream_id == baseline.continuation().stream_id
        && changes.next_cursor.scope_id == baseline.continuation().scope_id
        && changes.next_cursor.era == baseline.continuation().era;
    let advanced = changes.next_cursor.sequence > baseline.continuation().sequence
        && changes.next_cursor.commit.position > baseline.version().commit.position;
    fixture.world_gate.release_coherence_changes();
    let coherent = session.snapshot_ordered(Duration::from_secs(3), true);
    let worker = session.close();
    let http = fixture.finish_http_workers();
    let joins = fixture.world_gate.coherence_join_proof();
    let evidence = temp_dir("pre2-world-coherence-evidence");
    fs::create_dir_all(&evidence).unwrap();
    fs::set_permissions(&evidence, fs::Permissions::from_mode(0o700)).unwrap();
    for (name, value) in [
        (
            "sampled-view.json",
            serde_json::json!({"version":baseline.version(),"continuation":baseline.continuation(),"projection":baseline.projection()}),
        ),
        (
            "post-view.json",
            serde_json::json!({"version":post.version(),"continuation":post.continuation(),"projection":post.projection()}),
        ),
        ("post-changes.json", serde_json::to_value(&changes).unwrap()),
    ] {
        application_hosted_wait_rejection::private_write(
            &evidence.join(name),
            &serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    println!(
        "coherence_interleaving evidence_dir={} warmed={warmed} held={held} committed={committed} same_binding={same_binding} same_stream_scope_era={same_stream} advanced_cursor_commit={advanced} coherent={coherent} viewer_worker={worker:?} http_join={http:?} actual_gate_return_join={joins}",
        evidence.display()
    );
    assert!(
        warmed && held && submit.is_ok() && committed && same_binding && same_stream && advanced,
        "real scoped canonical interleaving prerequisites"
    );
    assert!(
        http.is_ok() && joins,
        "actual gate handler and all HTTP workers must join"
    );
    assert!(
        coherent && worker.is_ok(),
        "legitimate authenticated View/Changes interleaving must complete coherently"
    );
}
