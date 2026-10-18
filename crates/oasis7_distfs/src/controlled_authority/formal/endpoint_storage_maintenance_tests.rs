use super::*;
use crate::controlled_authority::formal::{physical, transport::FormalEndpointServer};
use std::time::{Duration, Instant};
#[test]
fn default_server_maintenance_converges_and_stays_quiet_without_new_work() {
    let fixture = Fixture::new();
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    let root = fixture.pdir.join("formal-objects-v1");
    let bytes = vec![97; 131072];
    let id = blake3::hash(&bytes).to_hex().to_string();
    fs::write(root.join(&id), &bytes).unwrap();
    physical::roots::retain(&root, std::slice::from_ref(&id)).unwrap();
    let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    for tick in 0..100 {
        server.maintenance_for_test(start + Duration::from_secs(tick));
    }
    let selected = fs::read(root.join(".generation-current-v1"))
        .expect("default endpoint owner must select and retire a physical generation");
    assert!(!root.join(".generation-build-v1.cbor").exists());
    assert!(!root.join(&id).exists());
    assert_eq!(
        super::super::super::business_pack::read_object(&root.join(&id), &id)
            .unwrap()
            .unwrap(),
        bytes
    );
    for tick in 100..1000 {
        server.maintenance_for_test(start + Duration::from_secs(tick));
    }
    assert_eq!(
        fs::read(root.join(".generation-current-v1")).unwrap(),
        selected
    );
    assert!(
        !root.join(".generation-build-v1.cbor").exists(),
        "quiet endpoint must not recompact unchanged history"
    );
}

#[test]
fn actual_serve_one_idle_path_runs_default_maintenance_without_an_rpc() {
    let fixture = Fixture::new();
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    let root = fixture.pdir.join("formal-objects-v1");
    let bytes = vec![71; 131072];
    let id = blake3::hash(&bytes).to_hex().to_string();
    fs::write(root.join(&id), bytes).unwrap();
    physical::roots::retain(&root, std::slice::from_ref(&id)).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(2)).unwrap();
    server.short_maintenance_quiet_for_test();
    let worker = std::thread::spawn(move || {
        let _ = server.serve_one(&listener);
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let complete = loop {
        if root.join(".generation-current-v1").exists()
            && !root.join(".generation-build-v1.cbor").exists()
        {
            break true;
        }
        if Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // Always release the blocked serve_one owner before making assertions.
    drop(std::net::TcpStream::connect(address).unwrap());
    worker.join().unwrap();
    assert!(
        complete,
        "idle production accept loop never converged physical maintenance"
    );
}
#[test]
fn failed_publication_debris_with_zero_new_logical_records_converges_automatically() {
    let fixture = Fixture::new();
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    let root = fixture.pdir.join("formal-objects-v1");
    let bytes = vec![81; 131072];
    let id = blake3::hash(&bytes).to_hex().to_string();
    fs::write(root.join(&id), &bytes).unwrap();
    physical::roots::retain(&root, std::slice::from_ref(&id)).unwrap();
    let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    for tick in 0..100 {
        server.maintenance_for_test(start + Duration::from_secs(tick));
    }
    let count = physical::roots::count(&root).unwrap();
    let staging = root.join(".physical-staging-v1");
    fs::create_dir_all(&staging).unwrap();
    let temporary = staging.join("aa".repeat(32));
    fs::write(&temporary, b"partial never-selected publication").unwrap();
    let orphan_bytes = b"unselected orphan frame";
    let orphan = root
        .join(".frames-v1")
        .join(blake3::hash(orphan_bytes).to_hex().as_str());
    fs::write(&orphan, orphan_bytes).unwrap();
    for tick in 100..700 {
        server.maintenance_for_test(start + Duration::from_secs(tick));
    }
    assert_eq!(physical::roots::count(&root).unwrap(), count);
    assert!(
        !temporary.exists(),
        "failed publication staging was never scheduled"
    );
    assert!(
        !orphan.exists(),
        "failed publication orphan frame was never scheduled"
    );
    assert!(!root.join(".generation-build-v1.cbor").exists());
    assert_eq!(
        super::super::super::business_pack::read_object(&root.join(&id), &id)
            .unwrap()
            .unwrap(),
        bytes
    );
}
#[test]
fn local_inventory_pages_are_fresh_bounded_and_bound_to_original_owner() {
    let fixture = Fixture::new();
    let mut endpoint = fixture.endpoint(EndpointRole::Primary);
    let root = fixture.pdir.join("formal-objects-v1");
    for byte in [1u8, 2, 3] {
        let bytes = vec![byte; 5 * 1024 * 1024];
        let id = blake3::hash(&bytes).to_hex().to_string();
        fs::write(root.join(&id), &bytes).unwrap();
        physical::roots::retain(&root, std::slice::from_ref(&id)).unwrap();
    }
    let first = endpoint.storage_logical_inventory_page(None).unwrap();
    assert_eq!(first.objects.len(), 1);
    assert!(!first.complete);
    let cursor = first.next.unwrap();
    let second = endpoint
        .storage_logical_inventory_page(Some(&cursor))
        .unwrap();
    assert_eq!(second.namespace_digest, first.namespace_digest);
    assert_eq!(second.objects.len(), 1);
    let corrupted = &second.objects[0].digest;
    fs::write(root.join(corrupted), b"corrupt original").unwrap();
    assert!(
        endpoint
            .storage_logical_inventory_page(Some(&cursor))
            .is_err()
    );
    drop(endpoint);
    let mut reopened = fixture.endpoint(EndpointRole::Primary);
    assert!(
        reopened
            .storage_logical_inventory_page(Some(&cursor))
            .is_err(),
        "reopened owner cannot reuse an old session cursor"
    );
}

#[test]
fn continuous_growth_debt_starts_and_advances_without_thirty_seconds_of_data_quiet() {
    let fixture = Fixture::new();
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    let root = fixture.pdir.join("formal-objects-v1");
    let bytes = b"original retained data under continuous publications";
    let id = blake3::hash(bytes).to_hex().to_string();
    fs::write(root.join(&id), bytes).unwrap();
    let append = vec![id; 32];
    let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(2)).unwrap();
    let start = Instant::now();
    server.maintenance_for_test(start + Duration::from_secs(1));
    for tick in 2..33 {
        physical::roots::retain(&root, &append).unwrap();
        server.maintenance_for_test(start + Duration::from_secs(tick));
        assert!(!root.join(".generation-build-v1.cbor").exists());
        assert!(!root.join(".generation-current-v1").exists());
    }
    // The 32nd append reaches 1024 records; every prior second changed the
    // source, so no thirty-second data-quiet interval has occurred.
    physical::roots::retain(&root, &append).unwrap();
    server.maintenance_for_test(start + Duration::from_secs(33));
    let build = root.join(".generation-build-v1.cbor");
    let before = fs::read(&build).expect("growth debt must start without data quiet");
    physical::roots::retain(&root, &append).unwrap();
    server.maintenance_for_test(start + Duration::from_secs(34));
    let after = fs::read(&build).unwrap();
    assert_ne!(
        before, after,
        "an existing job must advance even as data keeps arriving"
    );
}

#[test]
fn sequential_authenticated_head_rpcs_observe_readiness_without_poll_sleep() {
    use crate::controlled_authority::formal::transport::RemoteFormalEndpoint;
    use std::net::TcpListener;
    const REQUESTS: usize = 24;
    let fixture = Fixture::new();
    let (endpoint, _oracle) = super::protected_tests::native_rpc_endpoint(&fixture);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let mut server = FormalEndpointServer::new(endpoint, Duration::from_secs(3)).unwrap();
        for _ in 0..REQUESTS {
            server.serve_one(&listener).unwrap();
        }
        server.readiness_counts_for_test()
    });
    let head = fixture.authority.definition().world.genesis_head().unwrap();
    let mut client = RemoteFormalEndpoint::new(
        address,
        Duration::from_secs(3),
        fixture.authority.clone(),
        EndpointRole::Primary,
        key(4),
        head.clone(),
    )
    .unwrap();
    let start = Instant::now();
    for _ in 0..REQUESTS {
        assert_eq!(client.head().unwrap(), head);
    }
    let elapsed = start.elapsed();
    let (readable, timeouts) = worker.join().unwrap();
    eprintln!(
        "formal_endpoint_serial_rpc requests={REQUESTS} elapsed_ms={} readable_wakes={readable} idle_timeouts={timeouts}",
        elapsed.as_millis()
    );
    // Latency is observation evidence, not a tight wall-clock gate on shared CI.
    // The separate socket tests require actual readiness and timeout outcomes.
}
