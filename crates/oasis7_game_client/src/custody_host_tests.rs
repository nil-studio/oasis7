//! Local structural tests. No assertion here substitutes for distinct-UID deployment acceptance.
use super::*;
use oasis7_custody_core::client_enrollment::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

struct Fixture {
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-custody-host-test-{}",
            vault::random_id().unwrap()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { path }
    }
    fn identity(&self) -> IdentityBinding {
        let host = vault::actual_uid();
        // Tests run unprivileged; root cannot stand in for an isolated H principal.
        assert_ne!(host, 0, "run custody Host tests as an unprivileged account");
        let model = if host == 60001 { 60002 } else { 60001 };
        let native = if host == 60003 { 60004 } else { 60003 };
        let skill = self.path.join("readonly-skill.json");
        if !skill.exists() {
            std::fs::write(&skill, b"{\"fixture\":\"structural-only\"}").unwrap();
            std::fs::set_permissions(&skill, std::fs::Permissions::from_mode(0o400)).unwrap();
        }
        IdentityBinding {
            api_version: 1,
            enabled: true,
            service_origin: "https://custody.example".into(),
            skill_manifest_path: skill.to_str().unwrap().into(),
            descriptor: HostDeploymentDescriptorV1 {
                api_version: 1,
                environment: "fixture".into(),
                transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
                tool_profile: "custody-native-v1".into(),
                host_instance_id: "host-fixture".into(),
                client_public_key: String::new(),
                build_digest: String::new(),
                skill_manifest_digest: String::new(),
                tool_manifest_digest: String::new(),
                https_origin: "https://host.example".into(),
                restricted_runtime_identity: "model-fixture".into(),
                isolation_mechanism: HostIsolationMechanismV1::SeparateOsPrincipal,
                isolation_acceptance_refs: vec![
                    "structural-fixture-not-deployment-evidence".into(),
                ],
            },
            target: PairingInviteTargetRefV1 {
                custody_account_id: "account".into(),
                provision_id: "provision".into(),
                invite_id: "invite".into(),
                invite_digest: "01".repeat(32),
            },
            isolation: Isolation {
                host_uid: host,
                model_uid: model,
                native: NativePlacement::Local { uid: native },
            },
        }
    }
    fn vault(&self) -> vault::Vault {
        vault::Vault::initialize(
            &self.path.join("vault"),
            Zeroizing::new([7; 32]),
            self.identity(),
            "create".into(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn host_vault_is_private_independent_and_requires_original_state() {
    let a = Fixture::new();
    let b = Fixture::new();
    let av = a.vault();
    let bv = b.vault();
    let (public, secret) = av
        .read(|s| {
            Ok((
                s.identity.descriptor.client_public_key.clone(),
                s.signing_seed.clone(),
            ))
        })
        .unwrap();
    assert_ne!(
        public,
        bv.read(|s| Ok(s.identity.descriptor.client_public_key.clone()))
            .unwrap()
    );
    let ciphertext = std::fs::read(av.root.join("host.state")).unwrap();
    assert!(
        !ciphertext
            .windows(secret.len())
            .any(|w| w == secret.as_bytes())
    );
    let path = av.root.clone();
    drop(av);
    let restored = vault::Vault::open(&path, Zeroizing::new([7; 32])).unwrap();
    assert_eq!(
        public,
        restored
            .read(|s| Ok(s.identity.descriptor.client_public_key.clone()))
            .unwrap()
    );
    assert!(vault::Vault::open(&path, Zeroizing::new([8; 32])).is_err());
    drop(restored);
    std::fs::remove_file(path.join("host.state")).unwrap();
    assert!(vault::Vault::open(&path, Zeroizing::new([7; 32])).is_err());
    assert!(
        vault::Vault::initialize(
            &path,
            Zeroizing::new([7; 32]),
            a.identity(),
            "create-again".into()
        )
        .is_err()
    );
}

#[test]
fn unknown_bootstrap_attempts_and_clock_floor_survive_restart_without_refund() {
    let f = Fixture::new();
    let v = f.vault();
    for _ in 0..8 {
        v.reserve_create("create", b"same original").unwrap();
    }
    assert!(v.reserve_create("create", b"same original").is_err());
    let path = v.root.clone();
    drop(v);
    let v = vault::Vault::open(&path, Zeroizing::new([7; 32])).unwrap();
    assert_eq!(v.read(|s| Ok(s.attempts.len())).unwrap(), 8);
    assert!(v.reserve_create("another", b"new original").is_err());
    let before = v
        .read(|s| serde_json::to_vec(s).map_err(|_| denied()))
        .unwrap();
    assert!(
        v.update(|s| {
            s.time_floor = u64::MAX;
            Err::<(), _>(denied())
        })
        .is_err()
    );
    assert_eq!(
        before,
        v.read(|s| serde_json::to_vec(s).map_err(|_| denied()))
            .unwrap()
    );
}

#[test]
fn missing_private_fields_and_noncanonical_seeds_fail_closed() {
    let f = Fixture::new();
    let v = f.vault();
    let bytes = v
        .read(|s| serde_json::to_vec(s).map_err(|_| denied()))
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json.as_object_mut().unwrap().remove("session");
    assert!(serde_json::from_value::<State>(json).is_err());
    assert!(
        v.update(|s| {
            s.signing_seed = "AB".repeat(32);
            Ok(())
        })
        .is_err()
    );
    assert!(vault::secret32(&"AB".repeat(32)).is_err());
}

#[test]
fn model_interface_rejects_authority_and_arbitrary_transport_fields() {
    let good = serde_json::json!({"api_version":1,"resource_id":"resource","request_id":"request","method":"tools_list"});
    assert!(serde_json::from_value::<control::ModelRequest>(good.clone()).is_ok());
    for key in [
        "role",
        "vault",
        "session_id",
        "url",
        "token",
        "body",
        "expected_generation",
    ] {
        let mut injected = good.clone();
        injected[key] = "caller-selected".into();
        assert!(serde_json::from_value::<control::ModelRequest>(injected).is_err());
    }
    for method in [
        "raw_sign",
        "claim",
        "submit_action",
        "world_closed",
        "bootstrap",
    ] {
        let mut injected = good.clone();
        injected["method"] = method.into();
        assert!(serde_json::from_value::<control::ModelRequest>(injected).is_err());
    }
    let reply = control::ModelReply::unavailable("request".into());
    let safe = serde_json::to_value(reply).unwrap();
    assert_eq!(safe["close_requested"], false);
    assert!(safe.get("token").is_none());
    assert!(safe.get("world_closed").is_none());
}

#[test]
fn uid_policy_rejects_root_and_same_principal_without_cross_machine_uid_comparison() {
    let mut i = Isolation {
        host_uid: 501,
        model_uid: 502,
        native: NativePlacement::Local { uid: 503 },
    };
    assert!(i.validate().is_ok());
    i.model_uid = 501;
    assert!(i.validate().is_err());
    i.model_uid = 0;
    assert!(i.validate().is_err());
    i.model_uid = 502;
    i.host_uid = 0;
    assert!(i.validate().is_err());
    i.host_uid = 501;
    i.native = NativePlacement::Local { uid: 0 };
    assert!(i.validate().is_err());
    i.native = NativePlacement::Local { uid: 502 };
    assert!(i.validate().is_err());
    i.native = NativePlacement::Remote {
        https_origin: "https://native.example".into(),
    };
    assert!(i.validate().is_ok());
}
