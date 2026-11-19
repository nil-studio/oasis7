use super::*;
use std::io::Cursor;

fn action(wire: &str, limit: usize) -> Result<ActionV1, String> {
    let command = serde_json::from_str::<CommandV1<'_>>(wire).map_err(|e| e.to_string())?;
    ActionV1::from_command(command, limit)
}

#[test]
fn completed_command_reply_failure_is_terminal_not_a_business_refusal() {
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    let mut io = LocalIoV1::new(&memory, 64, 100, 10_000, 10_000).unwrap();
    io.encode(&Reply {
        ok: true,
        kind: "local_service_started",
        data: (),
    })
    .unwrap();
    let record = [3; 32];
    let reply_error = io
        .encode(&Reply {
            ok: true,
            kind: "clean_checkpoint",
            data: HexIdV1(&record),
        })
        .unwrap_err();
    // A small refusal would fit, but it must not replace the completed action's
    // failed response or permit the input loop to continue on that basis.
    assert!(finish_command_v1(Err(CommandFailureV1::Reply(reply_error.clone())), &mut io).is_err());
    assert_eq!(
        finish_command_v1(Err(CommandFailureV1::Reply(reply_error.clone())), &mut io).unwrap_err(),
        reply_error
    );
    assert!(
        !finish_command_v1(
            Err(CommandFailureV1::Business("invalid input".into())),
            &mut io
        )
        .unwrap()
    );
    assert!(finish_command_v1(Ok(true), &mut io).unwrap());
}

#[test]
fn local_action_hex_preserves_bytes_and_enforces_the_exact_bound() {
    let wire = r#"{"command":"submit","action_id":7,"payload_hex":"00aF"}"#;
    let ActionV1::Submit(id, bytes) = action(wire, 2).unwrap() else {
        panic!("submit was not decoded as original payload DATA");
    };
    assert_eq!(id, 7);
    assert_eq!(bytes, [0, 0xaf]);
    assert_eq!(action(wire, 1).err().unwrap(), "action byte bound");
    for payload in ["0", "000", "gg", "0x", "é"] {
        let wire = format!(r#"{{"command":"submit","action_id":7,"payload_hex":"{payload}"}}"#);
        assert!(
            action(&wire, 2).is_err(),
            "accepted malformed hex {payload}"
        );
    }
    for id in ["-1", "18446744073709551616", "1.5", "\"1\""] {
        let wire = format!(r#"{{"command":"submit","action_id":{id},"payload_hex":"00"}}"#);
        assert!(action(&wire, 1).is_err());
    }
}

#[test]
fn local_action_wire_rejects_unknown_duplicate_and_unrelated_fields() {
    for wire in [
        r#"{"command":"status","unexpected":true}"#,
        r#"{"command":"status","command":"stop"}"#,
        r#"{"command":"status","action_id":1}"#,
        r#"{"command":"status","payload_hex":"00"}"#,
        r#"{"command":"start","sender":"a"}"#,
        r#"{"command":"stop","receiver":"b"}"#,
        r#"{"command":"submit","action_id":1}"#,
        r#"{"command":"submit","payload_hex":"00"}"#,
        r#"{"command":"submit","action_id":1,"payload_hex":"00","sender":"a"}"#,
        r#"{"command":"observe","sender":"a"}"#,
        r#"{"command":"observe","receiver":"b"}"#,
        r#"{"command":"observe","sender":"a","receiver":"b","action_id":1}"#,
        r#"{"command":"unknown"}"#,
        r#"{"command":"\u0073tatus"}"#,
    ] {
        assert!(
            action(wire, 16).is_err(),
            "accepted unrelated command fields: {wire}"
        );
    }
    assert!(matches!(
        action(r#"{"command":"status"}"#, 0),
        Ok(ActionV1::Status)
    ));
    assert!(matches!(
        action(r#"{"command":"start"}"#, 0),
        Ok(ActionV1::Start)
    ));
    assert!(matches!(
        action(r#"{"command":"stop"}"#, 0),
        Ok(ActionV1::Stop)
    ));
}

#[test]
fn local_observation_identifiers_are_bounded_without_rewriting() {
    let sender = "a".repeat(256);
    let wire = format!(r#"{{"command":"observe","sender":"{sender}","receiver":"receiver"}}"#);
    let ActionV1::Observe(actual, receiver) = action(&wire, 1).unwrap() else {
        panic!("observe was not decoded");
    };
    assert_eq!(actual, sender);
    assert_eq!(receiver, "receiver");
    for invalid in [
        String::new(),
        " leading".into(),
        "trailing ".into(),
        "a".repeat(257),
    ] {
        for (sender, receiver) in [(&invalid[..], "receiver"), ("sender", &invalid[..])] {
            let wire =
                format!(r#"{{"command":"observe","sender":"{sender}","receiver":"{receiver}"}}"#);
            assert!(action(&wire, 1).is_err());
        }
    }
    assert!(
        action(
            r#"{"command":"observe","sender":"a\nb","receiver":"receiver"}"#,
            1
        )
        .is_err()
    );
}

#[test]
fn local_wire_limit_is_inclusive_and_success_does_not_refill_cli_work() {
    let wire = b"{\"command\":\"status\"}\n";
    let memory = MemoryBudget::new(1 << 20, 0).unwrap();
    {
        let mut io = LocalIoV1::new(&memory, wire.len(), 64, 2, (wire.len() * 2) as u64).unwrap();
        assert!(io.read(&mut Cursor::new(wire)).unwrap());
        assert!(matches!(
            ActionV1::from_command(io.command().unwrap(), 0),
            Ok(ActionV1::Status)
        ));
        let error = io.read(&mut Cursor::new(wire)).unwrap_err();
        assert!(error.contains("finite CLI work exhausted"));
        assert!(memory.used() > 0);
    }
    assert_eq!(memory.used(), 0);
    let mut io = LocalIoV1::new(&memory, wire.len() - 1, 64, 2, (wire.len() * 2) as u64).unwrap();
    let error = io.read(&mut Cursor::new(wire)).unwrap_err();
    assert!(error.contains("wire bound"));
    drop(io);
    assert_eq!(memory.used(), 0);
}

#[cfg(unix)]
mod private_files {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs,
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "oasis7-iws-live-cli-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed),
                ));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
        fn config(&self, value: &serde_json::Value) -> PathBuf {
            let path = self.0.join("config.json");
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .unwrap();
            file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
            path
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<[u8; 32]>> {
        fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Option<[u8; 32]>>) {
            for item in fs::read_dir(path).unwrap() {
                let item = item.unwrap();
                let path = item.path();
                let kind = item.file_type().unwrap();
                let digest = if kind.is_dir() {
                    visit(root, &path, out);
                    None
                } else {
                    assert!(kind.is_file());
                    Some(Sha256::digest(fs::read(&path).unwrap()).into())
                };
                out.insert(path.strip_prefix(root).unwrap().to_owned(), digest);
            }
        }
        let mut files = BTreeMap::new();
        visit(root, root, &mut files);
        files
    }

    #[test]
    fn profile_two_without_runtime_mode_does_not_start_or_write_identity() {
        let root = Directory::new();
        let path = root.config(&serde_json::json!({
            "schema_version":1, "profile_version":2,
            "node_keypair_directory":root.0.join("unprovisioned-key"),
            "writer_directory":root.0.join("unopened-writer"),
        }));
        let before = snapshot(&root.0);
        assert!(!try_run(&path).unwrap());
        assert_eq!(snapshot(&root.0), before);
    }

    #[test]
    fn unknown_or_incomplete_profile_three_refuses_before_identity() {
        for mode in [
            serde_json::Value::Null,
            serde_json::json!(3),
            serde_json::json!(true),
            serde_json::json!("controlled-v2"),
            serde_json::json!("iws_clean_local_v9"),
        ] {
            let root = Directory::new();
            let path = root.config(&serde_json::json!({"runtime_mode":mode}));
            let before = snapshot(&root.0);
            assert_eq!(
                try_run(&path).unwrap_err(),
                "unknown formal live runtime_mode"
            );
            assert_eq!(snapshot(&root.0), before);
        }
        for (value, expected) in [
            (
                serde_json::json!({"schema_version":1,"runtime_mode":config::MODE}),
                "missing field",
            ),
            (
                serde_json::json!({"schema_version":1,"runtime_mode":config::MODE,"unrelated":true}),
                "unknown field",
            ),
        ] {
            let root = Directory::new();
            let path = root.config(&value);
            let before = snapshot(&root.0);
            assert!(try_run(&path).unwrap_err().contains(expected));
            assert_eq!(snapshot(&root.0), before);
        }
    }

    #[test]
    fn clean_profile_identity_reopens_but_cannot_replace_a_controlled_binding() {
        use super::super::super::node_keypair_config::{
            RuntimeIdentityBindingV1, acquire_runtime_identity_lease,
            ensure_node_keypair_in_secure_config_dir,
        };
        for original in ["iws-clean-local-v1", "controlled-v2"] {
            let root = Directory::new();
            let key_dir = root.0.join("key");
            let data_directory = root.0.join("writer");
            for path in [&key_dir, &data_directory] {
                fs::DirBuilder::new().mode(0o700).create(path).unwrap();
            }
            let (key, key_path) = ensure_node_keypair_in_secure_config_dir(&key_dir).unwrap();
            let derived =
                super::super::super::derive_node_libp2p_identity_keypair_config("node", &key)
                    .unwrap();
            let peer_id = oasis7_node::derive_libp2p_identity_keypair(&derived.private_key_hex)
                .unwrap()
                .public()
                .to_peer_id()
                .to_string();
            let binding = RuntimeIdentityBindingV1 {
                version: 1,
                key_path,
                data_directory,
                node_id: "node".into(),
                root_public_key: key.public_key_hex,
                peer_id,
                world_id: "world".into(),
                chain_id: "chain".into(),
                genesis_digest: "ab".repeat(32),
                profile: original.into(),
            };
            drop(acquire_runtime_identity_lease(&binding.key_path, &binding).unwrap());
            let before = snapshot(&root.0);
            drop(acquire_runtime_identity_lease(&binding.key_path, &binding).unwrap());
            assert_eq!(snapshot(&root.0), before);
            let mut changed = binding.clone();
            changed.profile = if original == "controlled-v2" {
                "iws-clean-local-v1"
            } else {
                "controlled-v2"
            }
            .into();
            let error = acquire_runtime_identity_lease(&changed.key_path, &changed).unwrap_err();
            assert!(error.contains("binding conflict"));
            assert_eq!(snapshot(&root.0), before);
            drop(acquire_runtime_identity_lease(&binding.key_path, &binding).unwrap());
        }
    }
}
