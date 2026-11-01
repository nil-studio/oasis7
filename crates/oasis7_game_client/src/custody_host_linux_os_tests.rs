//! Real Linux process/UID fixture. No Native authorization, credentials or HTTP.
use super::*;
use oasis7_custody_core::client_enrollment::*;
use std::path::PathBuf;
const ROOT: &str = "/run/custody-os";
const H: u32 = 21001;
const M: u32 = 21002;
const N: u32 = 21003;
fn root() -> PathBuf {
    assert_eq!(std::env::var("OASIS7_CUSTODY_OS_ROOT").unwrap(), ROOT);
    assert!(
        Path::new("/.dockerenv").exists(),
        "disposable container only"
    );
    PathBuf::from(ROOT)
}
fn child_state(uid: u32) {
    assert_eq!(vault::actual_uid(), uid);
    // SAFETY: current Linux fixture process only; applied after exec, before secrets.
    assert_eq!(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) }, 0);
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    for key in ["CapInh:", "CapPrm:", "CapEff:", "CapAmb:"] {
        let value = status.lines().find(|line| line.starts_with(key)).unwrap();
        assert_eq!(
            u64::from_str_radix(value.split_whitespace().nth(1).unwrap(), 16).unwrap(),
            0
        );
    }
    assert!(
        status
            .lines()
            .any(|line| line.split_whitespace().collect::<Vec<_>>() == ["NoNewPrivs:", "1"])
    );
}
fn identity() -> IdentityBinding {
    IdentityBinding {
        api_version: 1,
        enabled: true,
        service_origin: "https://custody.example".into(),
        skill_manifest_path: root().join("skill.json").to_str().unwrap().into(),
        descriptor: HostDeploymentDescriptorV1 {
            api_version: 1,
            environment: "fixture".into(),
            transport_profile: HostTransportProfileV1::NativeBrokerCustodyV1,
            tool_profile: "custody-native-v1".into(),
            host_instance_id: "linux-os-component".into(),
            client_public_key: String::new(),
            build_digest: String::new(),
            skill_manifest_digest: String::new(),
            tool_manifest_digest: String::new(),
            https_origin: "https://host.example".into(),
            restricted_runtime_identity: "linux-m-component".into(),
            isolation_mechanism: HostIsolationMechanismV1::SeparateOsPrincipal,
            isolation_acceptance_refs: vec!["component-only-no-native-grant".into()],
        },
        target: PairingInviteTargetRefV1 {
            custody_account_id: "fixture-account".into(),
            provision_id: "fixture-provision".into(),
            invite_id: "fixture-invite".into(),
            invite_digest: "01".repeat(32),
        },
        isolation: Isolation {
            host_uid: H,
            model_uid: M,
            native: NativePlacement::Local { uid: N },
        },
    }
}
fn public_file(path: &Path, value: &impl Serialize) {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(path)
        .unwrap();
    // Fixture supervision handoff is public metadata, separate from the actual
    // production sidecar under test (whose umask behavior must remain unchanged).
    file.set_permissions(std::fs::Permissions::from_mode(0o644))
        .unwrap();
    file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
    file.sync_all().unwrap();
}
fn denied_read(path: &Path) {
    assert!(
        std::fs::File::open(path).is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
    );
}
fn leaf(started: Instant, stage: &'static str) {
    println!(
        "CUSTODY_OS_LEAF {}",
        serde_json::json!({"stage":stage,"elapsed_ms":started.elapsed().as_millis()})
    );
    std::io::stdout().flush().unwrap();
}
fn host() {
    let started = Instant::now();
    leaf(started, "host.entry");
    child_state(H);
    let r = root();
    let mut key = Zeroizing::new([0u8; 32]);
    std::io::stdin().read_exact(&mut *key).unwrap();
    leaf(started, "host.key_read");
    leaf(started, "host.vault_initialize_begin");
    let v = vault::Vault::initialize(
        &r.join("host-private/vault"),
        key.clone(),
        identity(),
        "fixture-create".into(),
    )
    .unwrap();
    leaf(started, "host.vault_initialized");
    leaf(started, "host.binding_read_begin");
    let b = v
        .read(|s| {
            Ok(PublicBinding {
                api_version: 1,
                transport_profile: "native-broker-custody-v1".into(),
                socket_path: r.join("socket/host.sock").to_str().unwrap().into(),
                identity_digest: s.identity.digest()?,
                descriptor_digest: s.identity.descriptor.digest().map_err(|_| denied())?,
                // A public fixture handle, deliberately not a real Native-approved session.
                session_digest: digest(
                    "linux-os-component-no-native-session/v1",
                    &"metadata-only",
                )?,
                resource_id: "fixture-resource".into(),
                restricted_runtime_identity: s
                    .identity
                    .descriptor
                    .restricted_runtime_identity
                    .clone(),
                isolation: s.identity.isolation.clone(),
            })
        })
        .unwrap();
    leaf(started, "host.binding_read_finished");
    leaf(started, "host.vault_reopen_begin");
    let restored = vault::Vault::open(&v.root, key).unwrap();
    leaf(started, "host.vault_reopened");
    assert!(
        restored
            .read(|s| Ok(s.identity.isolation.host_uid == H))
            .unwrap()
    );
    for file in ["host.state", ".key-verifier-v1"] {
        let m = std::fs::metadata(v.root.join(file)).unwrap();
        assert!(m.uid() == H && m.mode() & 0o7777 == 0o600);
    }
    assert_eq!(std::fs::metadata(&v.root).unwrap().mode() & 0o7777, 0o700);
    denied_read(&r.join("native-private/sentinel"));
    public_file(&r.join("host-public/binding.json"), &b);
    let until = Instant::now() + Duration::from_secs(30);
    while super::super::registry::check(&b).is_err() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    let socket = Path::new(&b.socket_path);
    let held = directory(socket.parent().unwrap(), H).unwrap();
    publish_binding(&held, socket, &b).unwrap();
    let listener = UnixListener::bind(socket).unwrap();
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o660)).unwrap();
    let m = socket_metadata(socket, H).unwrap();
    assert_eq!(m.gid(), 22000);
    listener.set_nonblocking(true).unwrap();
    public_file(
        &r.join("host-public/ready.json"),
        &serde_json::json!({"ready":true,"actual_uid":H,"socket_gid_inherited":true}),
    );
    let (mut metadata, mut wrong_peer) = (0u64, 0u64);
    let end = Instant::now() + Duration::from_secs(90);
    while !r.join("stop").exists() && Instant::now() < end {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if peer_uid(&stream).unwrap() != M {
                    wrong_peer += 1;
                    continue;
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                super::super::registry::check(&b).unwrap();
                let q: control::ModelRequest = read_frame(&mut stream).unwrap();
                assert!(
                    q.api_version == 1
                        && q.resource_id == b.resource_id
                        && matches!(q.method, control::Method::ToolsList)
                );
                metadata += 1;
                // Wire/OS acknowledgement only. Never run Host.control or fabricate acceptance.
                write_frame(&mut stream, &control::ModelReply::unavailable(q.request_id)).unwrap();
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(_) => panic!("fixture listener failed"),
        }
    }
    public_file(
        &r.join("host-public/facts.json"),
        &serde_json::json!({"metadata_requests":metadata,"wrong_kernel_peer_rejected":wrong_peer,"native_http_calls":0,"native_authorization_tested":false,"world_closed":false}),
    );
}
fn native_hold() {
    child_state(N);
    let r = root();
    use std::os::unix::fs::OpenOptionsExt;
    let mut secret = Zeroizing::new([0; 32]);
    getrandom::fill(&mut *secret).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(r.join("native-private/sentinel"))
        .unwrap();
    file.write_all(&*secret).unwrap();
    file.sync_all().unwrap();
    drop(file);
    public_file(
        &r.join("native-public/ready.json"),
        &serde_json::json!({"ready":true}),
    );
    let end = Instant::now() + Duration::from_secs(90);
    while !r.join("stop").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn probe() {
    let uid = vault::actual_uid();
    assert!([M, N].contains(&uid));
    child_state(uid);
    let r = root();
    for file in ["host.state", ".key-verifier-v1"] {
        denied_read(&r.join("host-private/vault").join(file));
    }
    let pid = std::fs::read_to_string(r.join("host.pid")).unwrap();
    let pid: i32 = pid.trim().parse().unwrap();
    denied_read(Path::new(&format!("/proc/{pid}/environ")));
    assert!(std::fs::read_link(format!("/proc/{pid}/fd/0")).is_err());
    // SAFETY: this negative attempts only Linux ptrace attach; success is a fixture failure.
    let result = unsafe { libc::ptrace(libc::PTRACE_ATTACH, pid, 0, 0) };
    assert_eq!(result, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    );
    let result = std::fs::File::open(r.join("socket/host.sock.custody.json"));
    let (readable, permission) = match result {
        Ok(_) => (true, false),
        Err(e) => (false, e.kind() == std::io::ErrorKind::PermissionDenied),
    };
    println!(
        "CUSTODY_OS_FACTS {}",
        serde_json::json!({"uid":uid,"vault_read_denied":true,"host_environment_denied":true,"host_fd_denied":true,"ptrace_denied":true,"public_sidecar_readable":readable,"public_sidecar_eacces":permission})
    );
}
fn wrong_peer() {
    child_state(21004);
    let socket = root().join("socket/host.sock");
    let mut stream = UnixStream::connect(socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let q = control::ModelRequest {
        api_version: 1,
        resource_id: "fixture-resource".into(),
        request_id: "fixture-wrong-peer".into(),
        method: control::Method::ToolsList,
    };
    let _ = write_frame(&mut stream, &q);
    assert!(read_frame::<control::ModelReply>(&mut stream).is_err());
}
#[test]
#[ignore = "explicit disposable Linux container only; run repository OS fixture supervisor"]
fn linux_os_component_worker() {
    match std::env::var("OASIS7_CUSTODY_OS_ROLE").unwrap().as_str() {
        "host" => host(),
        "native" => native_hold(),
        "probe" => probe(),
        "wrong-peer" => wrong_peer(),
        _ => panic!("closed fixture role"),
    }
}
