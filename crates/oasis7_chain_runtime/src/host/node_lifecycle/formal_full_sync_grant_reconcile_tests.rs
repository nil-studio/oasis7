//! Lifecycle records bind grant renewal to the original stopped incarnation.
use super::super::super::{LocalActionRecord, LocalInstance, local_io, model, process};
use super::super::grant_reconcile::require_completed_start;
use super::*;

struct GrantLifecycleFixture {
    _root: TestPrivateDir,
    data: std::path::PathBuf,
    pointer: LocalActionRecord,
    terminal: LocalInstance,
}
impl GrantLifecycleFixture {
    fn new() -> Self {
        let root = TestPrivateDir::new();
        let data = root.0.join("local-node");
        let chain = root.0.join("chain.json");
        let (_, trust, _, _, _) = full_sync_fixture();
        let config = FormalFullSyncConfigV1 {
            api_version: API_VERSION.into(),
            endpoint: "http://127.0.0.1:9001".into(),
            trusted_service_public_key: hex::encode(test_key(91).verifying_key().to_bytes()),
            alternate_endpoints: vec![],
            world_id: trust.initial_trust.world.world_id.clone(),
            chain_id: trust.initial_trust.world.chain_id.clone(),
            genesis_digest: trust.initial_trust.world.genesis_digest.clone(),
            formal_read_trust: trust,
            grant: None,
            expected_role: FormalNodeReadRoleV1::StateSync,
            poll_interval_ms: 100,
            request_timeout_ms: 2000,
            max_total_sync_bytes: 64 * 1024 * 1024,
        };
        local_io::write(&chain,&serde_json::json!({"api_version":"oasis7.chain-config/v3","runtime_profile":PROFILE,"formal_full_sync":config})).unwrap();
        let node = model::initialize(
            model::ChainConfig::load(&chain).unwrap(),
            &data,
            Some("grant-reconcile-node"),
            "127.0.0.1:9002",
        )
        .unwrap();
        let digest = local_io::digest(&node).unwrap();
        let pointer = LocalActionRecord {
            api_version: "oasis7.local-node-action/v1".into(),
            action_id: "original-start".into(),
            payload_digest: local_io::digest(&("start", digest.clone(), 0_u64)).unwrap(),
            kind: "start".into(),
            expected_generation: 0,
            target: LocalInstance {
                api_version: "oasis7.local-node-instance/v1".into(),
                action_id: "original-start".into(),
                generation: 1,
                incarnation: local_io::random_id().unwrap(),
                config_digest: digest,
                supervisor_pid: 0,
                runtime_pid: None,
                started_unix_ms: 1,
                state: "queued".into(),
                exit_code: None,
            },
            runtime_artifact: None,
        };
        let mut terminal = pointer.target.clone();
        terminal.state = "stopped".into();
        terminal.supervisor_pid = 120;
        terminal.exit_code = Some(0);
        local_io::write(&data.join("start-action.json"), &pointer).unwrap();
        let fixture = Self {
            _root: root,
            data,
            pointer,
            terminal,
        };
        fixture.save_terminal();
        fixture
    }
    fn save_terminal(&self) {
        let mut canonical = self.pointer.clone();
        canonical.target = self.terminal.clone();
        process::save_action(&self.data, &canonical).unwrap();
        local_io::write(&self.data.join("instance.json"), &self.terminal).unwrap();
    }
}
#[test]
fn formal_full_sync_grant_renewal_resolves_original_terminal_receipt_without_rewriting_pointer() {
    let fixture = GrantLifecycleFixture::new();
    let before = std::fs::read(fixture.data.join("start-action.json")).unwrap();
    require_completed_start(&fixture.data, Some(&fixture.terminal)).unwrap();
    assert_eq!(
        before,
        std::fs::read(fixture.data.join("start-action.json")).unwrap()
    );
}
#[test]
fn formal_full_sync_grant_renewal_rejects_missing_or_nonterminal_original_receipt() {
    for missing in [true, false] {
        let fixture = GrantLifecycleFixture::new();
        let receipt = process::action_path(&fixture.data, &fixture.pointer.action_id).unwrap();
        if missing {
            std::fs::remove_file(receipt).unwrap();
        } else {
            local_io::write(&receipt, &fixture.pointer).unwrap();
        }
        assert!(
            require_completed_start(&fixture.data, Some(&fixture.terminal)).is_err(),
            "current stopped alone cannot replace original receipt"
        );
    }
}
#[test]
fn formal_full_sync_grant_renewal_rejects_foreign_instance_or_tampered_action_binding() {
    let fixture = GrantLifecycleFixture::new();
    for altered in [
        {
            let mut x = fixture.terminal.clone();
            x.incarnation = local_io::random_id().unwrap();
            x
        },
        {
            let mut x = fixture.terminal.clone();
            x.generation = 2;
            x
        },
        {
            let mut x = fixture.terminal.clone();
            x.config_digest = test_hash(78);
            x
        },
        {
            let mut x = fixture.terminal.clone();
            x.action_id = "foreign-start".into();
            x
        },
        {
            let mut x = fixture.terminal.clone();
            x.state = "running".into();
            x
        },
    ] {
        assert!(require_completed_start(&fixture.data, Some(&altered)).is_err());
    }
    let mut canonical = fixture.pointer.clone();
    canonical.target = fixture.terminal.clone();
    canonical.payload_digest = test_hash(79);
    local_io::write(
        &process::action_path(&fixture.data, &canonical.action_id).unwrap(),
        &canonical,
    )
    .unwrap();
    assert!(require_completed_start(&fixture.data, Some(&fixture.terminal)).is_err());
    // Even equal pointer/receipt forgery cannot bypass ordinary indexed-query
    // payload/generation validation.
    local_io::write(&fixture.data.join("start-action.json"), &canonical).unwrap();
    assert!(require_completed_start(&fixture.data, Some(&fixture.terminal)).is_err());
}

#[test]
fn formal_full_sync_grant_renewal_missing_pointer_is_not_initial_admission() {
    let fixture = GrantLifecycleFixture::new();
    std::fs::remove_file(fixture.data.join("start-action.json")).unwrap();
    assert!(require_completed_start(&fixture.data, Some(&fixture.terminal)).is_err());
    std::fs::remove_file(fixture.data.join("instance.json")).unwrap();
    assert!(require_completed_start(&fixture.data, None).is_err());
    let virgin = fixture.data.join("virgin");
    std::fs::create_dir(&virgin).unwrap();
    require_completed_start(&virgin, None).unwrap();
}

#[cfg(unix)]
#[test]
fn formal_full_sync_grant_renewal_virgin_rejects_symlink_metadata() {
    use std::os::unix::fs::symlink;
    let root = TestPrivateDir::new();
    let data = root.0.join("virgin");
    local_io::private_dir(&data).unwrap();
    let empty = root.0.join("empty");
    local_io::private_dir(&empty).unwrap();
    symlink(&empty, data.join("actions")).unwrap();
    assert!(require_completed_start(&data, None).is_err());
    std::fs::remove_file(data.join("actions")).unwrap();
    symlink(data.join("missing-instance"), data.join("instance.json")).unwrap();
    assert!(require_completed_start(&data, None).is_err());
}
