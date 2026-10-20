//! Explicitly requested, bounded loopback backend for the shipped release Web regression.
//! This helper remains inside the test executable; it is never a production bootstrap.
use super::*;

#[test]
#[ignore = "owned release Web harness only; requires private rendezvous and stop paths"]
fn serve_release_web_fixture() {
    use std::fs::OpenOptions;
    let timeout_secs = std::env::var("OASIS7_RELEASE_WEB_FIXTURE_TIMEOUT_SECS")
        .unwrap_or_else(|_| "1200".into())
        .parse::<u64>()
        .expect("fixture timeout must be integer seconds");
    assert!(
        (1..=1200).contains(&timeout_secs),
        "fixture timeout must be 1..=1200 seconds"
    );
    let config_path = std::path::PathBuf::from(
        std::env::var_os("OASIS7_RELEASE_WEB_FIXTURE_CONFIG")
            .expect("release Web harness must supply private configuration path"),
    );
    let stop_path = std::path::PathBuf::from(
        std::env::var_os("OASIS7_RELEASE_WEB_FIXTURE_STOP")
            .expect("release Web harness must supply owned stop path"),
    );
    assert_ne!(config_path, stop_path);
    let parent = config_path.parent().expect("configuration parent");
    assert_eq!(stop_path.parent(), Some(parent));
    let metadata = fs::symlink_metadata(parent).expect("private rendezvous directory");
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(metadata.permissions().mode() & 0o077, 0);
    }
    assert!(
        matches!(fs::symlink_metadata(&stop_path), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "stop path must be fresh"
    );
    let fixture = Fixture::new();
    // Verify the actual signed transport before publishing consumer configuration.
    fixture.client.describe().unwrap();
    let view = fixture.client.read_view(fixture.view(None)).unwrap();
    let config = fixture.client.config();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "OASIS7_WORLD_SERVICE_ENDPOINT": config.endpoint,
        "OASIS7_WORLD_SERVICE_PUBLIC_KEY": config.trusted_service_public_key,
        "OASIS7_WORLD_SERVICE_WORLD_ID": config.expected_world.world_id,
        "OASIS7_WORLD_SERVICE_GENESIS_DIGEST": config.expected_world.genesis_digest,
        "OASIS7_WORLD_SERVICE_SCOPE": config.scope_id,
        "OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY": config.read_private_key_hex,
    }))
    .unwrap();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let staging_path = config_path.with_extension("pending");
    let mut file = options
        .open(&staging_path)
        .expect("fresh private configuration");
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
    fs::hard_link(&staging_path, &config_path)
        .expect("publish complete configuration without replacement");
    fs::remove_file(staging_path).unwrap();
    println!(
        "RELEASE_WEB_SIGNED_FIXTURE_READY commit_position={} agent_id=agent-a scope=public",
        view.version().commit.position
    );
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while !stop_path.exists() {
        assert!(
            Instant::now() < deadline,
            "owned release Web fixture deadline exceeded"
        );
        thread::sleep(Duration::from_millis(100));
    }
    fs::remove_file(config_path).unwrap();
    fixture.finish_http_workers().unwrap();
}
