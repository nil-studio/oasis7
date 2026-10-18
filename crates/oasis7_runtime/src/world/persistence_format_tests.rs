use super::{
    CURRENT_FORMAT, LEGACY_FORMAT, SELECTOR_API, SELECTOR_FILE, WorldPersistenceSelectorV1,
};
use crate::World;
use std::{fs, path::PathBuf, time::SystemTime};

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "oasis7-world-format-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create test directory");
    path
}

#[test]
fn saved_world_publishes_decoder_selector_and_roundtrips() {
    let dir = temp_dir("roundtrip");
    let original = World::new();
    original.save_to_dir(&dir).expect("save world");

    let selector: WorldPersistenceSelectorV1 =
        crate::util::read_json_from_path(&dir.join(SELECTOR_FILE))
            .expect("read persistence selector");
    assert_eq!(selector.api_version, SELECTOR_API);
    assert_eq!(selector.format_version, CURRENT_FORMAT);
    assert_eq!(World::current_persistence_format_version(), CURRENT_FORMAT);
    assert_eq!(World::legacy_persistence_format_version(), LEGACY_FORMAT);
    assert_eq!(World::persistence_selector_api(), SELECTOR_API);

    let restored = World::load_from_dir(&dir).expect("load saved world");
    assert_eq!(restored.state().time, original.state().time);
    fs::remove_dir_all(dir).expect("remove test directory");
}

#[test]
fn unknown_or_lowered_selector_fails_before_world_files_change() {
    let dir = temp_dir("unknown-selector");
    World::new().save_to_dir(&dir).expect("save world");
    let snapshot_path = dir.join("snapshot.json");
    let before = fs::read(&snapshot_path).expect("read snapshot before probe");

    let mut selector: serde_json::Value =
        crate::util::read_json_from_path(&dir.join(SELECTOR_FILE))
            .expect("read persistence selector");
    selector["format_version"] = serde_json::json!(CURRENT_FORMAT + 1);
    fs::write(
        dir.join(SELECTOR_FILE),
        serde_json::to_vec(&selector).expect("serialize unknown selector"),
    )
    .expect("write unknown selector");
    let error = World::load_from_dir(&dir).expect_err("unknown selector must fail closed");
    assert!(format!("{error:?}").contains("unsupported World persistence selector"));
    assert_eq!(
        fs::read(&snapshot_path).expect("snapshot remains unchanged"),
        before
    );

    selector["format_version"] = serde_json::json!(LEGACY_FORMAT);
    fs::write(
        dir.join(SELECTOR_FILE),
        serde_json::to_vec(&selector).expect("serialize lowered selector"),
    )
    .expect("write lowered selector");
    assert!(
        World::load_from_dir(&dir).is_err(),
        "marked data cannot be downgraded to the unmarked legacy selector"
    );
    assert_eq!(
        fs::read(&snapshot_path).expect("snapshot remains unchanged"),
        before
    );
    fs::remove_dir_all(dir).expect("remove test directory");
}

#[test]
fn unmarked_legacy_layout_uses_the_legacy_decoder_path() {
    let dir = temp_dir("legacy");
    World::new().save_to_dir(&dir).expect("save world");
    fs::remove_file(dir.join(SELECTOR_FILE)).expect("remove selector for legacy fixture");
    let restored = World::load_from_dir(&dir).expect("read unmarked legacy layout");
    assert_eq!(restored.state().time, 0);
    fs::remove_dir_all(dir).expect("remove test directory");
}
