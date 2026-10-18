use super::*;
use std::fs;

#[test]
fn load_from_dir_does_not_mix_inline_module_artifacts_with_newer_store() {
    let mut historical_world = World::new();
    let historical_hash = install_test_module(
        &mut historical_world,
        "m.persistence.historical",
        b"historical-module",
    );
    let historical_dir = temp_dir("persist-module-store-historical");
    historical_world
        .save_to_dir(&historical_dir)
        .expect("save historical world");

    let mut newer_world = World::new();
    let newer_hash = install_test_module(&mut newer_world, "m.persistence.newer", b"newer-module");
    let newer_dir = temp_dir("persist-module-store-newer");
    newer_world
        .save_to_dir(&newer_dir)
        .expect("save newer world");

    fs::copy(
        newer_dir.join("module_registry.json"),
        historical_dir.join("module_registry.json"),
    )
    .expect("replace mutable module registry cache");
    fs::remove_dir_all(historical_dir.join("modules")).expect("remove historical module cache");
    fs::create_dir_all(historical_dir.join("modules")).expect("create newer module cache");
    fs::copy(
        newer_dir.join("modules").join(format!("{newer_hash}.wasm")),
        historical_dir
            .join("modules")
            .join(format!("{newer_hash}.wasm")),
    )
    .expect("replace mutable module artifact cache");

    let mut restored = World::load_from_dir(&historical_dir).expect("load historical snapshot");
    assert!(
        restored
            .module_registry()
            .records
            .values()
            .any(|record| record.manifest.wasm_hash == historical_hash)
    );
    assert!(
        !restored
            .module_registry()
            .records
            .values()
            .any(|record| record.manifest.wasm_hash == newer_hash)
    );
    let artifact = restored
        .load_module(&historical_hash)
        .expect("historical inline module bytes");
    assert_eq!(artifact.bytes, b"historical-module".to_vec().into());

    let _ = fs::remove_dir_all(&historical_dir);
    let _ = fs::remove_dir_all(&newer_dir);
}
