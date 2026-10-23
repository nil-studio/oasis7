//! Independent, committed responses produced by the actual HTTP handler.
use super::*;

#[test]
fn signed_read_protocol_handler_fixtures() {
    let mut cases = Vec::new();
    let mut capture = |name: &str,
                       req: Request,
                       client: Option<&RemoteWorldServiceClient>,
                       decoded: Option<Value>| {
        let path = req.uri().path().to_owned();
        let response = handle(&req, client);
        let expected_status = match name {
            "missing-proof" => 428,
            "missing-task-control" => 404,
            "fixed-commit" | "unknown-field" | "duplicate-field" => 400,
            _ => 200,
        };
        assert_eq!(
            response.status().as_u16(),
            expected_status,
            "fixture {name}"
        );
        let body: Value = serde_json::from_str(response.body().as_ref().unwrap()).unwrap();
        let mut case =
            json!({"name":name,"path":path,"status":response.status().as_u16(),"body":body});
        if let Some(decoded) = decoded {
            case["request"] = decoded;
        }
        cases.push(case);
    };
    for (name, path) in [("info", "/v1/game/info"), ("openapi", READ_OPENAPI_PATH)] {
        capture(
            name,
            Request::builder().uri(path).body(()).unwrap(),
            None,
            None,
        );
    }
    for route in ["observation", "capabilities", "task-control"] {
        let caller = authority::sign_read_request("key", (), &hex::encode([8; 32])).unwrap();
        let control = json!({"schema_revision":1,"task_id":"task-a","world_id":"world","genesis_digest":"genesis",
            "branch_id":"branch","reorg_epoch":"0","agent_id":"a","owner_binding":"owner","owner_generation":"18446744073709551615",
            "owner_public_key":caller.subject_public_key,"task_revision":"18446744073709551615","task_status":"active",
            "latest_goal_revision":"18446744073709551615","active_goal_revision":"18446744073709551615",
            "active_goal_reference":{"goal_revision":"18446744073709551615","goal_ref":"cd".repeat(32),
                "prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"18446744073709551615"},
            "executor_epoch":"18446744073709551615","executor_id":"executor","executor_binding":{"executor_id":"executor",
                "executor_public_key":caller.subject_public_key,"delegation_generation":"18446744073709551615","executor_epoch":"18446744073709551615"},
            "created_operation_id":"create"});
        let (client, worker) = task_service(false, false, Some(control));
        let signed = proof(&client);
        let mut req = request(Some(&signed));
        *req.uri_mut() = format!("/v1/game/worlds/world/agents/a/{route}")
            .parse()
            .unwrap();
        capture(
            route,
            req,
            Some(&client),
            Some(serde_json::to_value(&signed.request).unwrap()),
        );
        worker.join().unwrap();
    }
    let client = client("http://127.0.0.1:1".into());
    capture("missing-proof", request(None), Some(&client), None);
    let mut signed = proof(&client);
    signed.request.fixed_commit = Some(response(&signed.request).version.commit);
    signed =
        authority::sign_read_request(VIEW_PATH, signed.request, &hex::encode([8; 32])).unwrap();
    capture(
        "fixed-commit",
        request(Some(&signed)),
        Some(&client),
        Some(serde_json::to_value(&signed.request).unwrap()),
    );
    for (name, duplicate) in [("unknown-field", false), ("duplicate-field", true)] {
        let mut value = serde_json::to_value(proof(&client)).unwrap();
        value["request"]["unknown"] = json!(true);
        let raw = if duplicate {
            serde_json::to_string(&proof(&client)).unwrap().replacen(
                "\"scope_id\":",
                "\"scope_id\":\"agent:a\",\"scope_id\":",
                1,
            )
        } else {
            value.to_string()
        };
        let req = Request::builder()
            .uri("/v1/game/worlds/world/agents/a/observation")
            .header("X-Oasis7-World-Read-Proof", hex::encode(raw))
            .body(())
            .unwrap();
        capture(name, req, Some(&client), None);
    }
    let (client, worker) = service(false, false);
    let mut req = request(Some(&proof(&client)));
    *req.uri_mut() = "/v1/game/worlds/world/agents/a/task-control"
        .parse()
        .unwrap();
    capture("missing-task-control", req, Some(&client), None);
    worker.join().unwrap();
    let actual = json!({"cases":cases});
    if std::env::var_os("OASIS7_UPDATE_PROTOCOL_FIXTURES").is_some() {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/signed_read_bootstrap.json"
            ),
            format!("{}\n", serde_json::to_string_pretty(&actual).unwrap()),
        )
        .unwrap();
    } else {
        let expected: Value =
            serde_json::from_str(include_str!("../tests/fixtures/signed_read_bootstrap.json"))
                .unwrap();
        assert_eq!(
            actual, expected,
            "handler wire changed; review and explicitly regenerate fixtures"
        );
    }
}
