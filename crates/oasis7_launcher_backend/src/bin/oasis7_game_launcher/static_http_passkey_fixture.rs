//! Disposable local HTTP fixture; TLS and virtual authenticators supplied by smoke harness.
use super::*;
#[test]
#[ignore = "manual Chromium virtual-authenticator fixture; stops when fixture stop file exists"]
fn hosted_passkey_virtual_authenticator_fixture() {
    use super::super::hosted_identity::{
        IdentityConfig, IdentityService,
        model::{IdentityNamespace, RpConfig},
    };
    let root = std::env::var_os("OASIS7_PASSKEY_FIXTURE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("oasis7-passkey-virtual-fixture"));
    std::fs::create_dir_all(&root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let origin = std::env::var("OASIS7_PASSKEY_FIXTURE_ORIGIN")
        .unwrap_or_else(|_| "https://localhost:8449".into());
    let origin_url = url::Url::parse(&origin).unwrap();
    assert_eq!(origin_url.scheme(), "https");
    assert_eq!(origin_url.host_str(), Some("localhost"));
    assert_eq!(origin_url.origin().ascii_serialization(), origin);
    let live_bind = std::env::var("OASIS7_PASSKEY_FIXTURE_RUNTIME_BIND")
        .unwrap_or_else(|_| "127.0.0.1:5023".into());
    let http_bind = std::env::var("OASIS7_PASSKEY_FIXTURE_HTTP_BIND")
        .unwrap_or_else(|_| "127.0.0.1:4179".into());
    for bind in [&live_bind, &http_bind] {
        assert!(
            bind.parse::<std::net::SocketAddr>()
                .unwrap()
                .ip()
                .is_loopback()
        );
    }
    let session_key = std::env::var("OASIS7_PASSKEY_FIXTURE_SESSION_KEY")
        .map(|value| hex::decode(value).unwrap().try_into().unwrap())
        .unwrap_or([91; 32]);
    let db = root.join(format!("{}.sqlite", std::process::id()));
    let config = IdentityConfig {
        namespace: IdentityNamespace {
            identity_service_id: "virtual-fixture".into(),
            environment_id: "test".into(),
        },
        rp: RpConfig {
            rp_id: "localhost".into(),
            allowed_origins: vec![origin],
            version: 1,
            identity_instance_epoch: 1,
        },
        sqlite_path: db,
        tablestore: None,
        trusted_proxy_loopback: false,
        registration_enabled: true,
        machine_registration_enabled: false,
        session_key,
        world_scope: "fixture-world".into(),
        runtime_live_bind: live_bind.clone(),
    };
    let viewer_runtime_config_json = serde_json::json!({
        "viewerWsEndpoint": format!("ws://{live_bind}/"),
        "deploymentMode": "hosted_public_join",
    })
    .to_string();
    let runtime = StaticHttpRuntimeConfig {
        live_bind,
        viewer_runtime_config_json,
        identity: Arc::new(Mutex::new(Some(IdentityService::new(config).unwrap()))),
        model_client: None,
    };
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../oasis7_viewer/software_safe_src");
    for name in [
        "viewer_auth_crypto.js",
        "software_safe_constants.js",
        "viewer_imported_key.js",
    ] {
        std::fs::copy(source.join(name), root.join(name)).unwrap();
    }
    std::fs::copy(
        source.join("viewer_passkey_account_module.js"),
        root.join("viewer_passkey_account_module.js"),
    )
    .unwrap();
    std::fs::copy(
        source.join("viewer_account_email_recovery.js"),
        root.join("viewer_account_email_recovery.js"),
    )
    .unwrap();
    let viewer_html = std::fs::read_to_string(source.join("../viewer.html")).unwrap();
    let style = viewer_html
        .split_once("<style>")
        .unwrap()
        .1
        .split_once("</style>")
        .unwrap()
        .0;
    std::fs::write(root.join("fixture.css"), style).unwrap();
    std::fs::write(root.join("index.html"),"<!doctype html><meta charset=utf-8><link rel=stylesheet href=/fixture.css><main style=\"padding:16px;max-width:640px;margin:auto\"><div id=account class=stack></div></main><script type=module>import {mountPasskeyAccount} from '/viewer_passkey_account_module.js';mountPasskeyAccount(document.querySelector('#account'));</script>").unwrap();
    let issuer = Arc::new(Mutex::new(
        HostedPlayerSessionIssuer::with_ledger_path(
            root.join(format!("{}.sessions.json", std::process::id())),
        )
        .unwrap(),
    ));
    let broker = Arc::new(Mutex::new(HostedAccountIdentityBroker::disabled()));
    let listener = std::net::TcpListener::bind(http_bind).unwrap();
    listener.set_nonblocking(true).unwrap();
    eprintln!("virtual_fixture_ready");
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(600) && !root.join("stop").exists() {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                handle_http_connection(
                    stream,
                    &root,
                    &runtime,
                    false,
                    None,
                    DeploymentMode::HostedPublicJoin,
                    &issuer,
                    &broker,
                )
                .unwrap();
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(e) => panic!("{e}"),
        }
    }
}
