use super::*;
use serde_json::json;
struct Fixture {
    root: PathBuf,
    c: BrokerConfig,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn artifact(path: PathBuf, bytes: &[u8], mode: u32) -> PinnedArtifact {
    std::fs::write(&path, bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    PinnedArtifact {
        path,
        sha256: hex::encode(Sha256::digest(bytes)),
    }
}
fn fixture(budget: u64) -> Fixture {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let root = Path::new("/tmp")
        .canonicalize()
        .unwrap()
        .join(format!("o7-broker-{}", hex::encode(random)));
    private_dir(&root).unwrap();
    let source_revision = "a".repeat(40);
    let source_attestation = artifact(root.join("source"), source_revision.as_bytes(), 0o600);
    let skill = artifact(root.join("skill"), b"private test Skill", 0o600);
    // Compile a synthetic child, never a shell or real identity/runtime endpoint.
    let src = root.join("helper.rs");
    std::fs::write(&src,r#"use std::io::Read;fn main(){let a=std::env::args().collect::<Vec<_>>();assert_eq!(a[1],"--executor-config");assert!(std::env::var("BROKER_TEST_SECRET").is_err());let mut b=String::new();std::io::stdin().read_to_string(&mut b).unwrap();assert_eq!(b,"private-unlock-test");if a[3]=="industry"{std::thread::sleep(std::time::Duration::from_secs(2));}else if a[3]=="task"{print!("{}","\u{1}".repeat(300000));}else if a[3]=="capabilities"{print!("{}","x".repeat(1024));}else{println!("{{\"executor\":\"{}\"}}",a[2]);}}"#).unwrap();
    let binary = root.join("helper");
    assert!(
        Command::new("rustc")
            .arg(&src)
            .arg("-o")
            .arg(&binary)
            .status()
            .unwrap()
            .success()
    );
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let client_binary = PinnedArtifact {
        sha256: hex::encode(Sha256::digest(std::fs::read(&binary).unwrap())),
        path: binary,
    };
    let mut resources = BTreeMap::new();
    for name in ["A", "B"] {
        let vault = root.join(format!("vault-{name}"));
        let identity = root.join(format!("identity-{name}"));
        let state = root.join(format!("state-{name}"));
        let budget_directory = root.join(format!("budget-{name}"));
        for p in [&vault, &identity, &state, &budget_directory] {
            private_dir(p).unwrap();
        }
        oasis7_client_storage::identity_store::ExecutorControlStore::create(
            &vault,
            b"private-unlock-test",
        )
        .unwrap();
        let config = json!({"identity":{"endpoint":"https://identity.invalid","identity_service_id":"issuer","environment_id":"private","audience":"game","state_directory":identity},"login_operation":"existing-login","executor_vault_directory":vault,"service_public_key":"11".repeat(32),"state_directory":state,"fence":{"world_id":"w","genesis_digest":"22".repeat(32),"branch_id":"b","reorg_epoch":"0","agent_id":name,"owner_binding":"owner","owner_generation":"1","owner_public_key":"33".repeat(32),"task_id":"t","task_revision":"1","task_status":"active","latest_goal_revision":"0","active_goal_revision":"0","active_goal_reference":null,"executor_epoch":"1","executor_binding":null}});
        let executor_config = artifact(
            root.join(format!("executor-{name}.json")),
            &serde_json::to_vec(&config).unwrap(),
            0o600,
        );
        resources.insert(
            name.into(),
            BrokerResource {
                executor_config,
                executor_vault_directory: vault,
                identity_state_directory: identity,
                state_directory: state,
                budget_directory,
                max_tool_requests: budget,
            },
        );
    }
    let c = BrokerConfig {
        profile_version: 1,
        runtime_id: "codex".into(),
        runtime_version: "test-1.0".into(),
        runtime_binary: artifact(
            root.join("runtime-artifact"),
            b"synthetic runtime never executed",
            0o600,
        ),
        approved: true,
        source_revision,
        source_attestation,
        skill,
        client_binary,
        socket_directory: root.join("socket"),
        socket_name: "game.sock".into(),
        timeout_ms: 5000,
        max_output_bytes: 512,
        resources,
    };
    c.validate().unwrap();
    Fixture { root, c }
}
fn q(resource: &str, id: &str, cmd: &str) -> BrokerRequest {
    BrokerRequest {
        resource_id: resource.into(),
        request_id: id.into(),
        args: vec![cmd.into()],
    }
}
#[test]
fn broker_real_child_independent_resources_durable_budget_and_original_conflict() {
    let f = fixture(3);
    let secret = b"private-unlock-test";
    let a = q("A", "same", "describe");
    let out = execute(&f.c, secret, &a).unwrap();
    assert!(out.contains("executor-A"));
    assert!(!out.contains("unlock"));
    assert_eq!(execute(&f.c, secret, &a).unwrap(), out);
    let mut changed = a.clone();
    changed.args = vec!["task".into()];
    assert!(
        execute(&f.c, secret, &changed)
            .unwrap_err()
            .contains("conflicts")
    );
    // Reconstructing the profile/broker cannot reset durable request accounting.
    let c: BrokerConfig = serde_json::from_slice(&serde_json::to_vec(&f.c).unwrap()).unwrap();
    assert!(execute(&c, secret, &a).is_ok());
    assert!(execute(&c, secret, &a).unwrap_err().contains("budget"));
    assert!(
        execute(&c, secret, &q("B", "b", "describe"))
            .unwrap()
            .contains("executor-B")
    );
}
#[test]
fn broker_rejects_owner_raw_options_hash_changes_aliases_and_unsupported_profile() {
    let f = fixture(5);
    for args in [
        vec!["owner"],
        vec!["identity-operation-id"],
        vec!["describe", "--config", "/tmp/x"],
        vec!["lookup", "--kind", "prefix", "--kind", "action"],
        vec!["move", "--signer-file", "/tmp/key"],
    ] {
        let mut request = q("A", "r", "describe");
        request.args = args.iter().map(|v| v.to_string()).collect();
        assert!(execute(&f.c, b"private-unlock-test", &request).is_err());
    }
    assert!(execute(&f.c, b"private-unlock-test", &q("C", "r", "describe")).is_err());
    let mut c = f.c.clone();
    c.runtime_id = "unknown".into();
    assert!(c.validate().is_err());
    let mut c = f.c.clone();
    c.resources.get_mut("B").unwrap().budget_directory = c.resources["A"].budget_directory.clone();
    assert!(c.validate().is_err());
    std::fs::write(&f.c.skill.path, b"changed").unwrap();
    assert!(
        execute(&f.c, b"private-unlock-test", &q("A", "r", "describe"))
            .unwrap_err()
            .contains("hash")
    );
}
#[test]
fn broker_timeout_and_output_bounds_are_charged() {
    let mut f = fixture(2);
    f.c.timeout_ms = 500;
    assert!(
        execute(&f.c, b"private-unlock-test", &q("A", "timeout", "industry"))
            .unwrap_err()
            .contains("timeout")
    );
    assert!(
        execute(
            &f.c,
            b"private-unlock-test",
            &q("A", "output", "capabilities")
        )
        .unwrap_err()
        .contains("bound")
    );
    assert!(
        execute(&f.c, b"private-unlock-test", &q("A", "later", "describe"))
            .unwrap_err()
            .contains("budget")
    );
}
#[test]
fn broker_real_unix_rpc_has_no_secret_or_path_argument() {
    let f = fixture(2);
    let socket = f.c.socket_directory.join("rpc.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut c = f.c.clone();
    c.max_output_bytes = 400000;
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let q = receive(&mut s).unwrap();
        let response = BrokerResponse {
            request_id: q.request_id.clone(),
            ok: true,
            stdout: execute(&c, b"private-unlock-test", &q).unwrap(),
            error: None,
        };
        s.write_all(&serde_json::to_vec(&response).unwrap())
            .unwrap();
    });
    let response = call(&socket, &q("B", "socket-op", "task")).unwrap();
    assert!(response.ok);
    assert_eq!(response.stdout.len(), 300000);
    assert!(response.stdout.bytes().all(|b| b == 1));
    server.join().unwrap();
}

#[test]
fn broker_artifact_writable_ancestor_is_rejected_before_private_stdin() {
    let f = fixture(1);
    let parent = f.root.join("writable");
    std::fs::create_dir(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    let binary = parent.join("client");
    std::fs::copy(&f.c.client_binary.path, &binary).unwrap();
    let mut c = f.c.clone();
    c.client_binary.path = binary;
    assert!(
        execute(&c, b"private-unlock-test", &q("A", "ancestor", "describe"))
            .unwrap_err()
            .contains("ancestor")
    );
}

#[test]
fn broker_runtime_version_and_artifact_are_pinned_without_resetting_budget() {
    let f = fixture(2);
    let request = q("A", "version", "describe");
    execute(&f.c, b"private-unlock-test", &request).unwrap();
    let mut changed = f.c.clone();
    changed.runtime_version = String::new();
    assert!(changed.validate().is_err());
    changed.runtime_version = "test-2.0".into();
    assert!(
        execute(&changed, b"private-unlock-test", &request)
            .unwrap_err()
            .contains("profile changed")
    );
    changed = f.c.clone();
    changed.runtime_binary.sha256 = "00".repeat(32);
    assert!(
        execute(&changed, b"private-unlock-test", &request)
            .unwrap_err()
            .contains("hash")
    );
    changed = f.c.clone();
    changed.runtime_binary = artifact(
        f.root.join("runtime-artifact-v2"),
        b"different runtime artifact never executed",
        0o600,
    );
    assert!(
        execute(&changed, b"private-unlock-test", &request)
            .unwrap_err()
            .contains("profile changed")
    );
    execute(&f.c, b"private-unlock-test", &request).unwrap();
    assert!(
        execute(&f.c, b"private-unlock-test", &request)
            .unwrap_err()
            .contains("budget")
    );
}
