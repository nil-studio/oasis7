use oasis7_command_center_core::{CommandCenter, Error, context::*};
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap()
            .success()
    );
}
fn project(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q"]);
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "--allow-empty",
            "-qm",
            "Initial",
        ],
    );
}
fn raw(id: &str) -> String {
    let metadata = serde_json::json!({"schema_version":"oasis7.context_entry.v1","project_id":"fixture","id":id,"kind":"machine","title":"Machine","environment":"development","owner":"test","tags":["build"],"state":"active","updated_at":"2026-01-01T00:00:00Z","updated_by":"fixture","verification":null,"credential_refs":[{"id":"ssh","provider":"external","locator":"private/reference","usage":"test"}],"links":[]});
    format!(
        "---\n{}\n---\nPrivate body\n",
        serde_json::to_string_pretty(&metadata).unwrap()
    )
}
#[test]
fn conditional_save_keeps_metadata_and_conflicts_with_external_writer() {
    let temp = tempfile::tempdir().unwrap();
    let service = ContextService::open(temp.path().join("host.db")).unwrap();
    let binding = service
        .bind(&temp.path().join("private"), "fixture", "Test", true)
        .unwrap();
    let mut request = ContextSaveRequest {
        binding_id: binding.id.clone(),
        binding_version: binding.binding_version,
        id: "machine".into(),
        raw_text: raw("machine"),
        expected_revision: None,
        actor: "desktop:test".into(),
    };
    service.save(&request).unwrap();
    let entry = service
        .get_bound(&binding.id, binding.binding_version, "machine")
        .unwrap();
    assert!(entry.metadata_json.contains("private/reference"));
    request.expected_revision = Some(entry.revision.clone());
    request.raw_text = entry.raw_text.replace("Private body", "Desktop edit");
    let store = oasis7_context::store::Store::open(Path::new(&binding.root)).unwrap();
    store
        .put(
            "machine",
            entry
                .raw_text
                .replace("Private body", "CLI edit")
                .as_bytes(),
            oasis7_context::model::WriteCondition::Revision(entry.revision),
            "cli:test",
        )
        .unwrap();
    match service.save(&request).unwrap_err() {
        Error::Context(e) => {
            assert_eq!(e.code, "REVISION_CONFLICT");
            assert!(e.current_revision.is_some());
        }
        other => panic!("{other}"),
    }
    assert!(
        service
            .get_bound(&binding.id, binding.binding_version, "machine")
            .unwrap()
            .raw_text
            .contains("CLI edit")
    );
}
#[test]
fn projects_and_private_libraries_are_isolated_in_both_directions() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("host.db");
    let root = temp.path().join("project");
    project(&root);
    let mut center = CommandCenter::open(&db).unwrap();
    center.open_project(&root).unwrap();
    let service = ContextService::open(&db).unwrap();
    assert!(
        service
            .bind(&root.join("private"), "fixture", "Nested", true)
            .is_err()
    );
    assert!(
        service
            .bind(temp.path(), "fixture", "Parent", true)
            .is_err()
    );
    let private = temp.path().join("private");
    let binding = service.bind(&private, "fixture", "Separate", true).unwrap();
    project(&private);
    assert!(center.open_project(&private).is_err());
    let renamed = temp.path().join("original-private");
    std::fs::rename(&private, &renamed).unwrap();
    oasis7_context::store::Store::init(&private, "fixture", "Replacement").unwrap();
    assert!(
        service
            .validate_binding(&binding.id, binding.binding_version)
            .is_err()
    );
}
#[test]
fn binding_version_invalidates_previous_operation_targets() {
    let temp = tempfile::tempdir().unwrap();
    let service = ContextService::open(temp.path().join("host.db")).unwrap();
    let root = temp.path().join("private");
    let first = service.bind(&root, "fixture", "Test", true).unwrap();
    let second = service.bind(&root, "fixture", "Test", false).unwrap();
    assert!(second.binding_version > first.binding_version);
    assert!(
        service
            .validate_binding(&first.id, first.binding_version)
            .is_err()
    );
    assert!(
        service
            .validate_binding(&second.id, second.binding_version)
            .is_ok()
    );
}

#[test]
fn existing_store_domain_identity_is_separate_from_registered_project() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private");
    oasis7_context::store::Store::init(&root, "domain-project", "Existing").unwrap();
    let service = ContextService::open(temp.path().join("host.db")).unwrap();
    let binding = service
        .bind(&root, "registered-project", "Ignored name", false)
        .unwrap();
    assert_eq!(binding.project_id, "domain-project");
    assert_eq!(binding.registered_project_id, "registered-project");
    assert!(
        service
            .validate_binding(&binding.id, binding.binding_version)
            .is_ok()
    );
}

#[test]
fn legacy_private_index_is_hidden_without_destroying_reading_history() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("host.db");
    let private = temp.path().join("private");
    oasis7_context::store::Store::init(&private, "fixture", "Test").unwrap();
    project(&private);
    std::fs::create_dir(private.join("doc")).unwrap();
    std::fs::write(
        private.join("doc/secret.md"),
        "# Secret\nLegacy private text\n",
    )
    .unwrap();
    git(&private, &["add", "doc/secret.md"]);
    git(
        &private,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-qm",
            "Private fixture",
        ],
    );
    let mut center = CommandCenter::open(&db).unwrap();
    let snapshot = center.open_project(&private).unwrap();
    let inventory_query = oasis7_command_center_core::document_inventory::DocumentInventoryQuery {
        schema_version: 1,
        project_id: snapshot.project.as_ref().unwrap().id.clone(),
        snapshot_id: None,
        query: String::new(),
        read_filter: "all".into(),
        cursor: None,
    };
    assert!(
        !center
            .document_inventory(inventory_query.clone())
            .unwrap()
            .files
            .is_empty()
    );
    let section = &snapshot.sections[0];
    center
        .record_reading(&section.id, &section.revision, 1)
        .unwrap();
    // Simulate a pre-isolation registration, with only its cache/history.
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute("DELETE FROM ordinary_projects", [])
        .unwrap();
    let service = ContextService::open(&db).unwrap();
    service.bind(&private, "fixture", "Test", false).unwrap();
    assert!(center.snapshot().sections.is_empty());
    assert!(center.search("Legacy", 20).is_err());
    assert!(center.document_inventory(inventory_query.clone()).is_err());
    assert!(center.read_version(&section.id, &section.revision).is_err());
    let reopened = CommandCenter::open(&db).unwrap();
    assert!(reopened.snapshot().sections.is_empty());
    assert!(reopened.document_inventory(inventory_query).is_err());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM device_history", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn reopened_cached_project_is_registered_before_private_binding() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("host.db");
    let root = temp.path().join("project");
    project(&root);
    let mut center = CommandCenter::open(&db).unwrap();
    center.open_project(&root).unwrap();
    drop(center);
    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute("DELETE FROM ordinary_projects", [])
        .unwrap();
    let _reopened = CommandCenter::open(&db).unwrap();
    let service = ContextService::open(&db).unwrap();
    assert!(
        service
            .bind(&root.join("private"), "fixture", "Nested", true)
            .is_err()
    );
}

#[test]
fn simultaneous_project_registration_and_binding_cannot_both_succeed() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("host.db");
    let root = temp.path().join("library");
    oasis7_context::store::Store::init(&root, "fixture", "Private").unwrap();
    project(&root);
    let mut center = CommandCenter::open(&db).unwrap();
    let service = ContextService::open(&db).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let private_root = root.clone();
    let start = barrier.clone();
    let bind = std::thread::spawn(move || {
        start.wait();
        service.bind(&private_root, "fixture", "Private", false)
    });
    barrier.wait();
    let ordinary = center.open_project(&root);
    let private = bind.join().unwrap();
    assert!(!(private.is_ok() && ordinary.is_ok()));
    if private.is_ok() {
        assert!(center.snapshot().sections.is_empty());
        assert!(ordinary.is_err());
    }
}
