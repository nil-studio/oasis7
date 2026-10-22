//! Actual formal constructor/Node lifetime proof; endpoints remain loopback test simulations.
use super::*;

pub(super) fn exercise(mut runtime: FormalLiveRuntime, config_path: &Path) -> Result<(), String> {
    let original_key = runtime.validated.node_keypair.public_key_hex.clone();
    let original = runtime.validated.config.clone();
    let alternate_data = original
        .writer_directory
        .parent()
        .ok_or("writer parent")?
        .join("identity-duplicate-writer");
    oracle_support::private_directory(&alternate_data)?;
    let mut duplicate = original.clone();
    duplicate.writer_directory = alternate_data.clone();
    let duplicate_path = alternate_data
        .parent()
        .ok_or("duplicate parent")?
        .join("identity-duplicate-runtime.json");
    oracle_support::write_private(
        &duplicate_path,
        &serde_json::to_vec(&duplicate).map_err(|e| e.to_string())?,
    )?;
    let error = match FormalLiveRuntime::open_with_native_history(&duplicate_path) {
        Err(error) => error,
        Ok(_) => {
            return Err("same identity opened through a different config/writer directory".into());
        }
    };
    if !error.contains("runtime identity already in use") {
        return Err(format!(
            "duplicate identity rejected for unrelated reason: {error}"
        ));
    }
    if std::fs::read_dir(&alternate_data)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err(
            "duplicate identity mutated alternate writer data before lease rejection".into(),
        );
    }
    // Start the actual effect worker behind a controlled scheduler boundary.
    // Empty routes produce no external effect, while the real worker retains
    // its production signer and lease through a pending stop.
    let mut delivery = runtime
        .effect_delivery
        .take()
        .ok_or("identity fixture needs effect worker")?;
    let (entered_send, entered_receive) = std::sync::mpsc::sync_channel(1);
    let (release_send, release_receive) = std::sync::mpsc::channel();
    delivery.set_test_worker_barrier(entered_send, release_receive);
    delivery.start(runtime.state.clone())?;
    entered_receive
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "effect worker did not enter barrier")?;
    if delivery.stop(Duration::ZERO).is_ok() {
        return Err("blocked effect worker falsely reported completed stop".into());
    }
    runtime.start()?;
    let node_owner = Arc::clone(&runtime.runtime);
    drop(runtime);
    // The outer owner has gone, but the actual NodeRuntime/hook still holds
    // the signer lease. Acquiring a new formal runtime must remain blocked.
    let error = match FormalLiveRuntime::open_with_native_history(config_path) {
        Err(error) => error,
        Ok(_) => return Err("outer runtime drop released a live Node signer lease".into()),
    };
    if !error.contains("runtime identity already in use") {
        return Err(format!(
            "retained Node lease rejected for unrelated reason: {error}"
        ));
    }
    node_owner
        .lock()
        .map_err(|_| "retained Node poisoned")?
        .stop()
        .map_err(|e| format!("stop retained Node: {e:?}"))?;
    drop(node_owner);
    if FormalLiveRuntime::open_with_native_history(config_path).is_ok() {
        return Err("stopped Node released lease while effect signer still lived".into());
    }
    release_send
        .send(())
        .map_err(|_| "effect worker barrier disappeared")?;
    delivery.stop(Duration::from_secs(5))?;
    drop(delivery);
    let restored = FormalLiveRuntime::open_with_native_history(config_path)?;
    if restored.validated.node_keypair.public_key_hex != original_key {
        return Err("restarted formal runtime replaced original identity".into());
    }
    drop(restored);
    // After release, changing the data directory still fails the durable
    // binding rather than starting a second lineage under the same root key.
    if FormalLiveRuntime::open_with_native_history(&duplicate_path).is_ok() {
        return Err("released identity accepted a conflicting data-directory binding".into());
    }
    Ok(())
}
