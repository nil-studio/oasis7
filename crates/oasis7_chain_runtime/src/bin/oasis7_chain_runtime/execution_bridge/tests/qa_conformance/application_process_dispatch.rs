//! Isolated application protocol dispatch; the ignored test keeps its canonical path.
use super::*;
pub(super) fn run() {
    // The same-artifact child starts on libtest's 2 MiB worker stack rather
    // than an ordinary application's main stack. Debug typed World/View and
    // durable recovery values need the same budget as the native-memory
    // child. Keep the real OS sandbox, constructor, transport and serving
    // paths intact; this changes only the test process execution boundary.
    thread::Builder::new()
        .name("pre2-isolated-application".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(run_inner)
        .unwrap()
        .join()
        .unwrap();
}
fn run_inner() {
    if std::env::var("RUST_LOG").is_ok() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stdout)
            .try_init();
    }
    let env = |name: &str| std::env::var(name).unwrap();
    let denied = std::path::PathBuf::from(env("PRE2_DENIED_NODE_ROOT"));
    let error = fs::read(denied.join("world/world-service-identity.json")).unwrap_err();
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied,
        "node file must exist and be denied by the OS"
    );
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(
            denied
                .join("world/world-service-identity.json")
                .as_os_str()
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(unsafe { libc::truncate(path.as_ptr(), 0) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().kind(),
            std::io::ErrorKind::PermissionDenied,
            "Node truncate must be denied by the OS"
        );
        println!("PRE2_APPLICATION_OS_TRUNCATE_DENIAL_PROBE_PASSED");
    }
    println!("PRE2_APPLICATION_OS_DENIAL_PROBE_PASSED");
    let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: env("PRE2_APP_ENDPOINT"),
        trusted_service_public_key: env("PRE2_APP_TRUST"),
        expected_world: serde_json::from_str(&env("PRE2_APP_WORLD")).unwrap(),
        scope_id: "public".into(),
        read_private_key_hex: env("PRE2_APP_OWNER"),
        timeout: Duration::from_secs(2),
        max_response_bytes: 1_048_576,
    })
    .unwrap();
    if matches!(
        env("PRE2_APP_ADMISSION").as_str(),
        "fresh-metadata" | "fresh-paused" | "fresh-bad-store"
    ) {
        application_admission_boundaries::verify(&client, &env("PRE2_APP_ADMISSION"));
        return;
    }
    if matches!(
        env("PRE2_APP_ADMISSION").as_str(),
        "hosted-wait-resume-crash" | "hosted-wait-resume-recover"
    ) {
        application_hosted_wait_recovery::verify(
            &client,
            env("PRE2_APP_ADMISSION") == "hosted-wait-resume-crash",
        );
        return;
    }
    if matches!(
        env("PRE2_APP_ADMISSION").as_str(),
        "wait-admit-crash" | "wait-admit-recover"
    ) {
        application_hosted_wait_crash::verify(
            &client,
            env("PRE2_APP_ADMISSION") == "wait-admit-crash",
        );
        return;
    }
    if matches!(
        env("PRE2_APP_ADMISSION").as_str(),
        "resume-rejected"
            | "resume-rejection-write-failure"
            | "resume-rejection-crash"
            | "resume-rejection-recover"
    ) {
        application_hosted_resume_rejection::verify(&client);
        return;
    }
    if env("PRE2_APP_ADMISSION") == "wait-rejection-crash" {
        application_hosted_wait_rejection_recovery::verify(&client, true);
        return;
    }
    if env("PRE2_APP_ADMISSION") == "wait-rejection-recover" {
        application_hosted_wait_rejection::verify(&client);
        return;
    }
    if env("PRE2_APP_ADMISSION") == "wait-rejection-write-failure" {
        application_hosted_wait_rejection_write_failure::verify(&client);
        return;
    }
    if env("PRE2_APP_ADMISSION") == "wait-admit-rejected" {
        application_hosted_wait_rejection::verify(&client);
        return;
    }
    if matches!(
        env("PRE2_APP_ADMISSION").as_str(),
        "wait-capture-crash" | "wait-capture-recover" | "wait-capture-nonempty"
    ) {
        application_hosted_wait_capture_crash::verify(
            &client,
            matches!(
                env("PRE2_APP_ADMISSION").as_str(),
                "wait-capture-crash" | "wait-capture-nonempty"
            ),
        );
        return;
    }
    if env("PRE2_APP_ADMISSION") == "hosted-wait-write-failure" {
        application_hosted_wait_write_failure::verify(&client);
        return;
    }
    if env("PRE2_APP_ADMISSION") == "hosted-resume-write-failure" {
        application_hosted_resume_write_failure::verify(&client);
        return;
    }
    if matches!(
        env("PRE2_APP_ADMISSION").as_str(),
        "hosted-final-budget"
            | "hosted-final-budget-write-failure"
            | "hosted-final-budget-crash"
            | "hosted-final-budget-recover"
    ) {
        application_hosted_final_budget::verify(&client);
        return;
    }
    if env("PRE2_APP_ADMISSION") == "hosted-wait" {
        application_hosted_wait::verify(&client);
        return;
    }
    if env("PRE2_APP_ADMISSION") == "fresh-admission" {
        application_fresh::verify_fresh(&client);
        return;
    }
    if matches!(
        env("PRE2_APP_ADMISSION").as_str(),
        "repeated-turns" | "slow-consumer"
    ) {
        application_stream_boundaries::verify(&client, &env("PRE2_APP_ADMISSION"));
        return;
    }
    if env("PRE2_APP_ADMISSION").starts_with("memory-") {
        application_memory_recovery::verify_memory_process(&client, &env("PRE2_APP_ADMISSION"));
        return;
    }
    if env("PRE2_APP_ADMISSION") == "missing-store" {
        application_admission::verify_missing_store(&client);
        return;
    }
    if env("PRE2_APP_PERIODIC_FAIRNESS") == "1" {
        application_fairness::verify_periodic_view_gate(&client);
        return;
    }
    if env("PRE2_APP_FAIRNESS") == "1" {
        application_fairness::verify_service_fairness(&client);
        return;
    }
    if env("PRE2_APP_METADATA") == "1" {
        application_metadata::verify_metadata_responsiveness(&client);
        return;
    }
    if env("PRE2_APP_HOSTED") == "1" {
        application_hosted::verify_hosted(&client);
        return;
    }
    if env("PRE2_APP_RELEASE") == "1" {
        application_release::verify_release(&client);
        return;
    }
    if env("PRE2_APP_WAKE") == "1" {
        application_wake::verify_wait_wake(&client);
        if std::env::var("PRE2_APP_WAKE_DRIFT").unwrap() != "1" {
            println!("PRE2_APPLICATION_OS_DENIAL_AND_GENUINE_WAKE_PASSED");
        }
        return;
    }
    let original: SubmitIntentRequest<WorldServicePayloadV1> =
        serde_json::from_str(&env("PRE2_APP_REQUEST")).unwrap();
    client.describe().unwrap();
    client.submit(original.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let commit = loop {
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .unwrap();
        if let IntentOutcome::Committed { commit, .. } = response.outcome {
            break commit;
        }
        assert!(
            Instant::now() < deadline,
            "no application canonical receipt"
        );
        thread::sleep(Duration::from_millis(10));
    };
    let view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: "public".into(),
            min_commit: Some(*commit),
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    client
        .read_changes(ReadWorldChangesRequest {
            contract_version: 1,
            cursor: view.continuation().clone(),
            max_items: 32,
            max_bytes: 65_536,
        })
        .unwrap();
    let mut protected_config = client.config().clone();
    protected_config.scope_id = "agent:agent-a".into();
    let protected = RemoteWorldServiceClient::new(protected_config).unwrap();
    let baseline = protected
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: protected.config().expected_world.clone(),
            scope_id: protected.config().scope_id.clone(),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    // A real signed gameplay mutation crosses the same isolated application boundary.
    use oasis7::viewer::{CollectDataCommand, CollectDataRequest, sign_collect_data_auth_proof};
    let public = sign_read_request("owner", (), &env("PRE2_APP_OWNER"))
        .unwrap()
        .subject_public_key;
    let mut gameplay = CollectDataCommand::Submit {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "owner-a".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let proof =
        sign_collect_data_auth_proof(&gameplay, 10, &public, &env("PRE2_APP_OWNER")).unwrap();
    let CollectDataCommand::Submit { request } = &mut gameplay else {
        unreachable!()
    };
    request.auth = Some(proof);
    let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&gameplay).unwrap());
    let gameplay = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(client.config().expected_world.clone(), &payload).unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    client.submit(gameplay.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let gameplay_commit = loop {
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: gameplay.correlation.key.clone(),
                },
                gameplay.signed_payload.clone(),
            )
            .unwrap();
        match response.outcome {
            IntentOutcome::Committed { commit, receipt } => {
                assert!(
                    receipt["events"]
                        .as_array()
                        .is_some_and(|events| !events.is_empty())
                );
                break commit;
            }
            IntentOutcome::Unknown | IntentOutcome::Pending | IntentOutcome::Received { .. } => {}
            other => panic!("signed gameplay did not commit: {other:?}"),
        }
        assert!(Instant::now() < deadline, "signed gameplay receipt timeout");
        thread::sleep(Duration::from_millis(10));
    };
    let gameplay_view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: client.config().expected_world.clone(),
            scope_id: "public".into(),
            min_commit: Some(gameplay_commit.as_ref().clone()),
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    client
        .read_changes(ReadWorldChangesRequest {
            contract_version: 1,
            cursor: gameplay_view.continuation().clone(),
            max_items: 32,
            max_bytes: 65_536,
        })
        .unwrap();
    let continuation_request = ReadWorldChangesRequest {
        contract_version: 1,
        cursor: baseline.continuation().clone(),
        max_items: 1,
        max_bytes: 65_536,
    };
    let first = protected
        .read_changes(continuation_request.clone())
        .unwrap();
    assert!(
        !first.changes.is_empty(),
        "snapshot-to-changes boundary lost the gameplay event"
    );
    assert!(first.changes.len() <= 1);
    let repeated = protected.read_changes(continuation_request).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&repeated).unwrap(),
        "stable cursor replay changed delivery"
    );
    let mut cursor = baseline.continuation().clone();
    let mut delivered = Vec::new();
    loop {
        let changes = protected
            .read_changes(ReadWorldChangesRequest {
                contract_version: 1,
                cursor: cursor.clone(),
                max_items: 1,
                max_bytes: 65_536,
            })
            .unwrap();
        if changes.changes.is_empty() {
            break;
        }
        assert_ne!(
            cursor.sequence, changes.next_cursor.sequence,
            "cursor did not advance"
        );
        delivered.extend(changes.changes.into_iter().map(|change| change.change));
        cursor = changes.next_cursor;
        assert!(
            delivered.len() < 128,
            "bounded fixture change drain did not finish"
        );
    }
    assert!(delivered.iter().any(|change|matches!(serde_json::from_value::<oasis7_runtime::WorldEvent>(change.clone()).unwrap().body,oasis7_runtime::WorldEventBody::Domain(oasis7_runtime::DomainEvent::DataCollectedAuthenticated {collector_agent_id,electricity_cost:7,data_amount:11,player_id,nonce:10,..}) if collector_agent_id=="agent-a" && player_id=="owner-a")),"signed gameplay event missing from protected continuation");
    let mut wrong_era = cursor;
    wrong_era.era = wrong_era.era.saturating_add(1);
    assert!(
        protected
            .read_changes(ReadWorldChangesRequest {
                contract_version: 1,
                cursor: wrong_era,
                max_items: 1,
                max_bytes: 65_536
            })
            .is_err(),
        "cross-era cursor must demand resync"
    );
    protected
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: protected.config().expected_world.clone(),
            scope_id: protected.config().scope_id.clone(),
            min_commit: Some(*gameplay_commit),
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap();
    application_provider::verify_provider_closure(&client);
    println!(
        "PRE2_APPLICATION_OS_DENIAL_AND_FIVE_OPS_PASSED signed_gameplay=true signed_cognition=true"
    );
}
