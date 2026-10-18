use super::{CliOptions, run_import};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "oasis7-governance-import-writer-lock-{}-{unique}",
        std::process::id()
    ))
}

fn world_lock_dir(world_dir: &Path) -> PathBuf {
    let mut path: OsString = world_dir.as_os_str().to_owned();
    path.push(".lock");
    PathBuf::from(path)
}

fn collect_files(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let mut entries = std::fs::read_dir(dir)
        .expect("read world directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect world entries");
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            files.insert(
                path.strip_prefix(root)
                    .expect("relative world path")
                    .to_path_buf(),
                std::fs::read(path).expect("read world file"),
            );
        }
    }
}

fn world_files(world_dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect_files(world_dir, world_dir, &mut files);
    files
}

fn governed_manifest() -> serde_json::Value {
    let entry = |slot_id: &str, signer_id: &str, key_byte: &str| {
        serde_json::json!({
            "slot_id": slot_id,
            "signer_id": signer_id,
            "scheme": "ed25519",
            "threshold": 1,
            "public_key_hex": key_byte.repeat(64),
        })
    };
    serde_json::json!([
        entry("governance.finality.v1", "finality-1", "1"),
        entry("msig.genesis.v1", "genesis-1", "2"),
        entry("msig.staking_governance.v1", "staking-1", "3"),
        entry("msig.ecosystem_governance.v1", "ecosystem-1", "4"),
        entry("msig.security_council.v1", "security-1", "5"),
        entry("ops.rollback.on_call.v1", "rollback-on-call", "6"),
        entry("governance.rollback.v1", "rollback-governance", "7"),
    ])
}

#[test]
fn import_fails_closed_while_live_world_writer_lock_is_held() {
    let root = temp_dir();
    let world_dir = root.join("world");
    std::fs::create_dir_all(&root).expect("create fixture root");
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world.step().expect("seed observable world state");
    world.save_to_dir(&world_dir).expect("seed world directory");
    let manifest_path = root.join("public-manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&governed_manifest()).expect("encode manifest"),
    )
    .expect("write manifest");

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let files_before = world_files(&world_dir);
    let lock_dir = world_lock_dir(&world_dir);
    std::fs::create_dir(&lock_dir).expect("hold world writer lock");
    std::fs::write(
        lock_dir.join("owner.json"),
        serde_json::to_vec(&serde_json::json!({
            "pid": std::process::id(),
            "token": "governance-import-contention-owner"
        }))
        .expect("encode live lock owner"),
    )
    .expect("publish live lock owner");

    let error = run_import(CliOptions {
        world_dir: world_dir.clone(),
        public_manifest: manifest_path,
        finality_slot_id: "governance.finality.v1".to_string(),
        default_threshold: 1,
    })
    .expect_err("import must fail before load or mutation while a live writer owns the world");
    assert!(error.contains("lock") && error.contains("held"), "{error}");

    assert_eq!(
        world_files(&world_dir),
        files_before,
        "failed import must not mutate any world-directory byte"
    );
    let restored =
        oasis7::runtime_host::world::load_from_dir(&world_dir).expect("reload untouched world");
    assert_eq!(restored.snapshot(), snapshot_before);
    assert_eq!(restored.journal(), &journal_before);
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(lock_dir);
}
