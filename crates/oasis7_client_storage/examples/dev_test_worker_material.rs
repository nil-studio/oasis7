//! Trusted operator handoff only. Never called by MCP or exported into the Worker.
//! Reads existing SDK originals; writes one new private file, never stdout secrets.
#[cfg(unix)]
mod operator {
    use oasis7_client_storage::identity_store::{ExecutorControlStore, IdentityStore};
    use serde::{Deserialize, Serialize};
    use std::{
        collections::BTreeMap,
        fs,
        io::Write,
        os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        path::{Path, PathBuf},
    };
    use zeroize::Zeroizing;
    #[derive(Deserialize, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Identity {
        endpoint: String,
        identity_service_id: String,
        environment_id: String,
        audience: String,
        state_directory: PathBuf,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Session {
        account_id: String,
        cookie: String,
    }
    fn private(path: &Path, directory: bool) -> Result<fs::Metadata, &'static str> {
        if !path.is_absolute()
            || path
                .canonicalize()
                .map_err(|_| "private_path_unavailable")?
                != path
        {
            return Err("private_path_required");
        }
        let meta = fs::symlink_metadata(path).map_err(|_| "private_path_unavailable")?;
        if meta.file_type().is_symlink()
            || (directory && !meta.is_dir())
            || (!directory && !meta.is_file())
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err("private_path_required");
        }
        if !directory && meta.len() > 65536 {
            return Err("private_input_bound");
        }
        Ok(meta)
    }
    fn public(seed: &[u8; 32]) -> Result<String, &'static str> {
        let key = Zeroizing::new(hex::encode(seed));
        oasis7_client_api::world_service::sign_read_request("key", (), &key)
            .map(|p| p.subject_public_key)
            .map_err(|_| "key_unavailable")
    }
    pub fn export(args: &[String]) -> Result<(), &'static str> {
        let names = [
            "--identity-config",
            "--login-operation",
            "--executor-directory",
            "--unlock-file",
            "--expected-account",
            "--expected-executor",
            "--output",
        ];
        if args.len() != names.len() * 2 {
            return Err("invalid_operator_arguments");
        }
        let mut values = BTreeMap::new();
        for pair in args.chunks_exact(2) {
            if !names.contains(&pair[0].as_str())
                || values.insert(pair[0].as_str(), pair[1].as_str()).is_some()
            {
                return Err("invalid_operator_arguments");
            }
        }
        let cfg = Path::new(values["--identity-config"]);
        let unlock = Path::new(values["--unlock-file"]);
        let executor = Path::new(values["--executor-directory"]);
        let output = Path::new(values["--output"]);
        let parent = output.parent().ok_or("private_output_required")?;
        let owner = private(parent, true)?.uid();
        for (path, dir) in [(cfg, false), (unlock, false), (executor, true)] {
            if private(path, dir)?.uid() != owner {
                return Err("private_owner_mismatch");
            }
        }
        if output.file_name().is_none() || output.exists() {
            return Err("new_private_output_required");
        }
        let raw = fs::read(cfg).map_err(|_| "identity_configuration_unavailable")?;
        let identity: Identity = oasis7_agent_api::game::parse_bounded_json(&raw, 65536)
            .map_err(|_| "identity_configuration_unavailable")?;
        if private(&identity.state_directory, true)?.uid() != owner
            || identity.endpoint != identity.audience
            || !identity.endpoint.starts_with("https://")
            || identity.identity_service_id.is_empty()
            || identity.environment_id.is_empty()
        {
            return Err("identity_configuration_mismatch");
        }
        let operation = values["--login-operation"];
        if !oasis7_custody_id(operation) {
            return Err("invalid_login_operation");
        }
        let password = Zeroizing::new(fs::read(unlock).map_err(|_| "private_unlock_unavailable")?);
        if password.is_empty() || password.len() > 4096 {
            return Err("private_unlock_unavailable");
        }
        let store = IdentityStore::open(&identity.state_directory, &password)
            .map_err(|_| "original_identity_unavailable")?;
        let vault = ExecutorControlStore::open(executor, &password)
            .map_err(|_| "original_executor_unavailable")?;
        let executor_public = public(vault.executor_seed())?;
        if executor_public != values["--expected-executor"]
            || executor_public == public(store.login_seed())?
        {
            return Err("independent_executor_mismatch");
        }
        let scope = blake3::hash(
            &serde_json::to_vec(&(
                &identity.endpoint,
                &identity.identity_service_id,
                &identity.environment_id,
                &identity.audience,
            ))
            .map_err(|_| "identity_scope_unavailable")?,
        )
        .to_hex()
        .to_string();
        let session: Session = store
            .read_original(&scope, operation, "login-session")
            .map_err(|_| "original_session_unavailable")?
            .ok_or("original_session_unavailable")?;
        let cookie = Zeroizing::new(session.cookie);
        if session.account_id != values["--expected-account"]
            || !cookie.starts_with("__Host-oasis7_sid=")
            || cookie.len() > 4096
            || cookie.chars().any(char::is_control)
            || cookie.contains(';')
        {
            return Err("original_session_mismatch");
        }
        #[derive(Serialize)]
        struct Material<'a> {
            executor_seed: &'a str,
            identity_cookie: &'a str,
        }
        let seed = Zeroizing::new(hex::encode(vault.executor_seed()));
        let material = Zeroizing::new(
            serde_json::to_vec(&Material {
                executor_seed: &seed,
                identity_cookie: &cookie,
            })
            .map_err(|_| "material_encoding_failed")?,
        );
        // O_EXCL prevents overwrite/final symlink following. The parent is private,
        // canonical and owned by the same operator as both existing SDK vaults.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(output)
            .map_err(|_| "new_private_output_required")?;
        file.write_all(&material)
            .and_then(|_| file.sync_all())
            .map_err(|_| "private_handoff_unknown")?;
        fs::File::open(parent)
            .and_then(|d| d.sync_all())
            .map_err(|_| "private_handoff_unknown")?;
        Ok(())
    }
    fn oasis7_custody_id(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn export_is_private_exact_non_overwriting_and_reads_only_originals() {
            let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "worker-handoff-{}",
                hex::encode(getrandom::u64().unwrap().to_le_bytes())
            ));
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            let password = b"test-private-passphrase";
            let identity = Identity {
                endpoint: "https://native.example.test".into(),
                identity_service_id: "service".into(),
                environment_id: "env".into(),
                audience: "https://native.example.test".into(),
                state_directory: root.join("identity"),
            };
            let store = IdentityStore::create(&identity.state_directory, password).unwrap();
            let executor = root.join("executor");
            let vault = ExecutorControlStore::create(&executor, password).unwrap();
            let scope = blake3::hash(
                &serde_json::to_vec(&(
                    &identity.endpoint,
                    &identity.identity_service_id,
                    &identity.environment_id,
                    &identity.audience,
                ))
                .unwrap(),
            )
            .to_hex()
            .to_string();
            #[derive(Serialize, Deserialize)]
            struct Saved {
                account_id: String,
                cookie: String,
            }
            store
                .put_original(
                    &scope,
                    "login",
                    "login-session",
                    &Saved {
                        account_id: "account".into(),
                        cookie: "__Host-oasis7_sid=private-fixture".into(),
                    },
                )
                .unwrap();
            let cfg = root.join("identity.json");
            let unlock = root.join("unlock");
            for (path, bytes) in [
                (&cfg, serde_json::to_vec(&identity).unwrap()),
                (&unlock, password.to_vec()),
            ] {
                fs::write(path, bytes).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            let output = root.join("material");
            let expected = public(vault.executor_seed()).unwrap();
            let args = vec![
                "--identity-config".into(),
                cfg.display().to_string(),
                "--login-operation".into(),
                "login".into(),
                "--executor-directory".into(),
                executor.display().to_string(),
                "--unlock-file".into(),
                unlock.display().to_string(),
                "--expected-account".into(),
                "account".into(),
                "--expected-executor".into(),
                expected,
                "--output".into(),
                output.display().to_string(),
            ];
            let mut wrong = args.clone();
            wrong[11] = "wrong".into();
            assert!(export(&wrong).is_err());
            assert!(!output.exists());
            for index in [3usize, 9] {
                let mut invalid = args.clone();
                invalid[index] = "missing-original".into();
                assert!(export(&invalid).is_err());
                assert!(!output.exists());
            }
            let fifo = root.join("fifo-unlock");
            assert!(
                std::process::Command::new("mkfifo")
                    .arg("-m")
                    .arg("600")
                    .arg(&fifo)
                    .status()
                    .unwrap()
                    .success()
            );
            let mut pipe = args.clone();
            pipe[7] = fifo.display().to_string();
            assert!(export(&pipe).is_err());
            assert!(!output.exists());
            fs::set_permissions(&unlock, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(export(&args).is_err());
            assert!(!output.exists());
            fs::set_permissions(&unlock, fs::Permissions::from_mode(0o600)).unwrap();
            let mut missing = args.clone();
            missing[5] = root.join("missing-vault").display().to_string();
            assert!(export(&missing).is_err());
            assert!(!root.join("missing-vault").exists());
            let snapshots = |path: &Path| -> BTreeMap<PathBuf, Vec<u8>> {
                fs::read_dir(path)
                    .unwrap()
                    .map(|entry| {
                        let path = entry.unwrap().path();
                        (path.clone(), fs::read(path).unwrap())
                    })
                    .collect()
            };
            let identity_before = snapshots(&identity.state_directory);
            let executor_before = snapshots(&executor);
            export(&args).unwrap();
            assert_eq!(snapshots(&identity.state_directory), identity_before);
            assert_eq!(snapshots(&executor), executor_before);
            let original = fs::read(&output).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&original).unwrap();
            assert_eq!(value["executor_seed"], hex::encode(vault.executor_seed()));
            assert_eq!(
                value["identity_cookie"],
                "__Host-oasis7_sid=private-fixture"
            );
            assert_eq!(
                fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert!(export(&args).is_err());
            assert_eq!(fs::read(&output).unwrap(), original);
            fs::remove_file(&output).unwrap();
            std::os::unix::fs::symlink(root.join("elsewhere"), &output).unwrap();
            assert!(export(&args).is_err());
            assert!(!root.join("elsewhere").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }
}
#[cfg(unix)]
fn main() {
    if operator::export(&std::env::args().skip(1).collect::<Vec<_>>()).is_err() {
        eprintln!("trusted_worker_handoff_failed");
        std::process::exit(1);
    }
    println!("trusted_worker_handoff_written");
}
#[cfg(not(unix))]
fn main() {
    eprintln!("trusted_worker_handoff_requires_unix");
    std::process::exit(1);
}
