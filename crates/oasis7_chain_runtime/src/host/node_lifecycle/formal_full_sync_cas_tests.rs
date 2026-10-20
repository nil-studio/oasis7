use super::*;
use oasis7_runtime::BlobStore;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

struct CasTestRoot(PathBuf);

impl CasTestRoot {
    fn new() -> Self {
        let root = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temp directory")
            .join(format!(
                "oasis7-full-sync-cas-test-{}-{}",
                std::process::id(),
                super::super::super::local_io::random_id().expect("unique test root id")
            ));
        super::super::super::local_io::private_dir(&root).expect("create private test root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for CasTestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn put(store: &LocalCasStore, bytes: &[u8]) -> String {
    store.put_bytes(bytes).expect("put CAS blob")
}

#[test]
fn read_only_cas_probe_does_not_create_roots_or_directories() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path().join("absent-cas"));

    let summary = verify_read_only_cas(&store).expect("inspect absent read-only CAS");

    assert_eq!(summary["object_count"], 0);
    assert_eq!(summary["pin_count"], 0);
    assert_eq!(summary["total_bytes"], 0);
    assert!(!store.root().exists());
    assert!(!store.blobs_dir().exists());
}

#[test]
fn read_only_cas_probe_rejects_missing_durable_root_and_corrupt_blob() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let missing = put(&store, b"pinned full-sync object");
    store.pin(&missing).expect("publish legacy pin");
    std::fs::remove_file(store.blobs_dir().join(format!("{missing}.blob")))
        .expect("remove pinned object");
    assert!(verify_read_only_cas(&store).is_err());

    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let digest = put(&store, b"content-addressed object");
    std::fs::write(
        store.blobs_dir().join(format!("{digest}.blob")),
        b"different bytes",
    )
    .expect("corrupt object bytes");
    let error = verify_read_only_cas(&store).expect_err("reject corrupt CAS content");
    assert!(error.contains("content digest"), "{error}");
}

#[test]
fn gc_keeps_history_and_new_loaded_closure_and_removes_old_orphans() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let history_page = put(&store, b"proof-page");
    let history_manifest = put(&store, b"proof-manifest");
    let old_world = put(&store, b"old-world-closure");
    let stale = put(&store, b"stale-interrupted-transfer");
    let new_snapshot = put(&store, b"new-snapshot");
    let new_journal = put(&store, b"new-journal");

    let history = BTreeSet::from([history_page.clone(), history_manifest.clone()]);
    replace_history_pins(&store, &history).expect("publish verified proof history");
    replace_loaded_world_pins(&store, &BTreeSet::from([old_world.clone()]))
        .expect("publish current world closure");
    replace_loaded_world_pins(
        &store,
        &BTreeSet::from([new_snapshot.clone(), new_journal.clone()]),
    )
    .expect("atomically rotate loaded-world closure");

    prune_unpinned_with_limit(&store, 1024).expect("prune unpinned blobs");

    for retained in [
        &history_page,
        &history_manifest,
        &new_snapshot,
        &new_journal,
    ] {
        assert!(
            store.has(retained).expect("check retained blob"),
            "{retained}"
        );
    }
    assert!(!store.has(&old_world).expect("check old blob"));
    assert!(!store.has(&stale).expect("check stale transfer blob"));
    let pins = store
        .list_effective_pins()
        .expect("read durable pin roots")
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        pins,
        BTreeSet::from([history_page, history_manifest, new_snapshot, new_journal])
    );
    let reopened = LocalCasStore::new(root.path());
    assert_eq!(
        reopened
            .list_effective_pins()
            .expect("reopen durable pin roots")
            .into_iter()
            .collect::<BTreeSet<_>>(),
        pins
    );
}

#[test]
fn over_capacity_candidate_keeps_old_world_and_history_pins() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let history = put(&store, b"history-root-material");
    let old_world = put(&store, b"old-world-material");
    let candidate = put(&store, b"candidate-world-material-is-too-large");

    replace_history_pins(&store, &BTreeSet::from([history.clone()])).expect("publish history root");
    replace_loaded_world_pins(&store, &BTreeSet::from([old_world.clone()]))
        .expect("publish old world root");
    let before = store
        .list_effective_pins()
        .expect("read roots before candidate");

    let error =
        replace_loaded_world_pins_with_limit(&store, &BTreeSet::from([candidate.clone()]), 40)
            .expect_err("candidate must exceed protected-set budget");

    assert!(
        error.to_string().starts_with("StorageCapacityExceeded:"),
        "{error}"
    );
    assert_eq!(
        store
            .list_effective_pins()
            .expect("read roots after candidate"),
        before
    );
    assert!(store.has(&history).expect("history remains"));
    assert!(store.has(&old_world).expect("old world remains"));
    assert!(
        store
            .has(&candidate)
            .expect("uncommitted candidate remains until safe GC")
    );
    prune_unpinned_with_limit(&store, 40).expect("remove uncommitted candidate");
    assert!(!store.has(&candidate).expect("candidate was pruned"));
    assert_eq!(
        store
            .list_effective_pins()
            .expect("old pins remain after failed-candidate cleanup"),
        before
    );
}

#[test]
fn post_rename_directory_sync_failure_is_classified_as_uncertain() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let old = put(&store, b"old-world");
    let candidate = put(&store, b"candidate-world");
    replace_loaded_world_pins(&store, &BTreeSet::from([old.clone()]))
        .expect("publish durable old root");
    let candidate_pins = BTreeSet::from([candidate.clone()]);
    sync_verified_blobs(&store, &candidate_pins).expect("durably flush candidate objects");
    let scope_dir = ensure_pin_scope_dir(&store).expect("create private full-sync scope");

    let error = publish_pin_shard(
        &scope_dir.join("loaded-world.json"),
        &candidate_pins,
        |_| Err("simulated directory sync failure".into()),
    )
    .expect_err("post-rename sync failure has an unknown publication outcome");

    assert!(error.outcome_unknown(), "{error}");
    assert!(error.to_string().contains("rename succeeded"), "{error}");
    let rename_error = publish_pin_shard_with(
        &scope_dir.join("loaded-world.json"),
        &candidate_pins,
        |temporary, target| {
            std::fs::rename(temporary, target)?;
            Err(std::io::Error::other("simulated ambiguous rename failure"))
        },
        |_| Ok(()),
    )
    .expect_err("rename error after publication is uncertain too");
    assert!(rename_error.outcome_unknown(), "{rename_error}");
    assert_eq!(
        store
            .list_effective_pins()
            .expect("read pin shard after uncertain publication"),
        vec![candidate.clone()]
    );
    let cleanup = cleanup_after_failed_candidate(&store, rename_error.outcome_unknown())
        .expect_err("uncertain publication must block GC until restart");
    assert!(cleanup.contains("garbage collection withheld"), "{cleanup}");
    // The old closure is still physically present for restart-time root
    // reconstruction because the caller did not run the destructive sweep.
    assert!(store.has(&old).expect("old closure remains before restart"));
    assert!(
        store
            .has(&candidate)
            .expect("candidate closure remains before restart")
    );
}

#[test]
fn durable_full_sync_pin_shard_uses_private_modes() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let world = put(&store, b"loaded-world");
    replace_loaded_world_pins(&store, &BTreeSet::from([world.clone()]))
        .expect("publish durable world root");

    let scope = store.root().join("pin_scopes/formal-full-sync");
    let shard = scope.join("loaded-world.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&scope)
                .expect("scope metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&shard)
                .expect("shard metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert_eq!(
        store
            .list_effective_pins()
            .expect("read compatible scoped pin file"),
        vec![world]
    );
}

#[cfg(unix)]
#[test]
fn durable_pin_publication_tightens_legacy_scoped_directories() {
    use std::os::unix::fs::PermissionsExt;

    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let scopes = store.root().join("pin_scopes");
    let scope = scopes.join("formal-full-sync");
    std::fs::create_dir_all(&scope).expect("create legacy scoped directories");
    std::fs::set_permissions(&scopes, std::fs::Permissions::from_mode(0o755))
        .expect("make legacy pin_scopes mode permissive");
    std::fs::set_permissions(&scope, std::fs::Permissions::from_mode(0o755))
        .expect("make legacy scope mode permissive");
    let world = put(&store, b"world-from-legacy-pin-layout");

    replace_loaded_world_pins(&store, &BTreeSet::from([world.clone()]))
        .expect("harden and durably publish scoped pin shard");

    assert_eq!(
        std::fs::metadata(&scopes)
            .expect("pin_scopes metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&scope)
            .expect("scope metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        store.list_effective_pins().expect("read roots"),
        vec![world]
    );
}

#[test]
fn missing_protected_blob_fails_closed_without_sweeping_other_objects() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let protected = put(&store, b"protected-history");
    let unrelated = put(&store, b"unrelated-orphan");
    replace_history_pins(&store, &BTreeSet::from([protected.clone()]))
        .expect("publish history root");
    std::fs::remove_file(store.blobs_dir().join(format!("{protected}.blob")))
        .expect("simulate missing pinned blob");

    let error = prune_unpinned_with_limit(&store, 1024).expect_err("missing pin must fail closed");

    assert!(error.contains("HistoryUnavailable:"), "{error}");
    assert!(
        store
            .has(&unrelated)
            .expect("unrelated blob remains untouched")
    );
}

#[test]
fn oversized_local_object_is_rejected_before_pin_publication() {
    let root = CasTestRoot::new();
    let store = LocalCasStore::new(root.path());
    let old_world = put(&store, b"old-qualified-world");
    replace_loaded_world_pins(&store, &BTreeSet::from([old_world.clone()]))
        .expect("publish prior world root");

    let oversized_hash = hex::encode(blake3::hash(b"not-the-on-disk-content").as_bytes());
    let oversized_path = store.blobs_dir().join(format!("{oversized_hash}.blob"));
    let file = std::fs::File::create(&oversized_path).expect("create oversized local blob");
    file.set_len(super::super::MAX_FULL_SYNC_RECORD_BYTES as u64 + 1)
        .expect("set oversized blob length");

    let error = replace_loaded_world_pins(&store, &BTreeSet::from([oversized_hash.clone()]))
        .expect_err("oversized local object cannot become a durable root");

    assert!(
        error.to_string().contains("per-object byte bound"),
        "{error}"
    );
    assert_eq!(
        store
            .list_effective_pins()
            .expect("read roots after rejection"),
        vec![old_world.clone()]
    );
    assert!(store.has(&old_world).expect("old root remains"));
}
