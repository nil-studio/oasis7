//! Actual Native HTTP handler and SQLite; upstream is a signed synthetic transport fixture.
//! No test here claims real World quotes, custody MFA consumption or paid-once authority.
use super::*;
use oasis7_client_api::world_service::{entry_control::*, resource_basis as wire};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
#[path = "resource_component_fixture.rs"]
mod fixture;
type Fixture = crate::hosted_identity::model_session::model_session_tests::Fixture;
fn service(endpoint: String, data: &fixture::Data) -> Fixture {
    let mut f = IdentityService::model_executor_test_fixture();
    let mut entry = ParticipantEntry::new(
        EntryTrust {
            endpoint,
            service_public_key: sign_read_request("key", (), &data.world_key)
                .unwrap()
                .subject_public_key,
            world: data.world.clone(),
        },
        Zeroizing::new(data.issuer.clone()),
    )
    .unwrap();
    entry.owner_control_enabled = true;
    f.service.participant_entry = Some(entry);
    f
}
fn call<T: Serialize>(
    f: &mut Fixture,
    path: &str,
    q: &T,
    bad_csrf: bool,
) -> super::super::super::service::IdentityReply {
    let csrf = super::super::super::digest(&f.service.config.session_key, "csrf", &f.token);
    let csrf = if bad_csrf { "00".repeat(32) } else { csrf };
    let headers = format!(
        "POST / HTTP/1.1\r\nCookie: __Host-oasis7_sid={}\r\nOrigin: https://localhost\r\nSec-Fetch-Site: same-origin\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\n\r\n",
        f.token
    );
    f.service.handle(
        "POST",
        &format!("{}{path}", super::super::super::service::PREFIX),
        &headers,
        serde_json::to_value(q).unwrap(),
        "resource-local-component",
        &mut crate::hosted_player_session::HostedPlayerSessionIssuer::default(),
    )
}
fn request(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut all = Vec::new();
    loop {
        let mut chunk = [0; 2048];
        let n = stream.read(&mut chunk).unwrap();
        assert!(n > 0);
        all.extend_from_slice(&chunk[..n]);
        assert!(all.len() <= wire::MAX_RESOURCE_ENVELOPE_BYTES + 8192);
        if let Some(end) = all.windows(4).position(|v| v == b"\r\n\r\n") {
            let header = String::from_utf8(all[..end].to_vec()).unwrap();
            let length: usize = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap();
            if all.len() >= end + 4 + length {
                return (
                    header
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .to_owned(),
                    all[end + 4..end + 4 + length].to_vec(),
                );
            }
        }
    }
}
fn serve(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    before: impl FnOnce() + Send + 'static,
    corrupt: bool,
) -> thread::JoinHandle<Vec<String>> {
    thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let data = fixture::Data::new();
        let mut before = Some(before);
        let mut paths = Vec::new();
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let (path, body) = request(&mut stream);
                    let mut bytes = data.body(&path, &body);
                    if corrupt && path == wire::OWNER_RESOURCE_VIEW_PATH_V1 {
                        let mut reply: wire::SignedResourceResponseV1<
                            wire::ParticipantOwnerResourceObservationV1,
                        > = serde_json::from_slice(&bytes).unwrap();
                        reply.signature_hex = "00".repeat(64);
                        bytes = serde_json::to_vec(&reply).unwrap();
                    }
                    if let Some(run) = before.take() {
                        run();
                    }
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).unwrap();
                    stream.write_all(&bytes).unwrap();
                    paths.push(path);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5))
                }
                Err(e) => panic!("component accept: {e}"),
            }
        }
        paths
    })
}
#[test]
fn resource_basis_native_component_has_one_direct_world_read_and_unchanged_two_read_final_path() {
    for goal in [false, true] {
        let data = fixture::Data::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut f = service(format!("http://{}", listener.local_addr().unwrap()), &data);
        let stop = Arc::new(AtomicBool::new(false));
        let server = serve(listener, stop.clone(), || {}, false);
        let reply = if goal {
            call(
                &mut f,
                "/_fixture/resource-basis/owner-control",
                &data.goal_request(),
                false,
            )
        } else {
            call(
                &mut f,
                "/_fixture/resource-basis/owner-view",
                &data.owner_request(),
                false,
            )
        };
        stop.store(true, Ordering::SeqCst);
        let paths = server.join().unwrap();
        assert_eq!(reply.status, 200);
        if goal {
            assert_eq!(
                paths,
                vec![PARTICIPANT_OWNER_VIEW_PATH, wire::RESOURCE_GOAL_PATH_V2]
            );
            let response: wire::NativeResourceEnvelopeV1<
                wire::OwnerSetupFinalResourceObservationV2,
            > = serde_json::from_slice(&serde_json::to_vec(&reply.body).unwrap()).unwrap();
            assert!(matches!(
                response.world_response.payload.resource,
                wire::ResourceBasisResolutionV1::Blocked { .. }
            ));
        } else {
            assert_eq!(paths, vec![wire::OWNER_RESOURCE_VIEW_PATH_V1]);
            let response: wire::NativeResourceEnvelopeV1<
                wire::ParticipantOwnerResourceObservationV1,
            > = serde_json::from_slice(&serde_json::to_vec(&reply.body).unwrap()).unwrap();
            wire::verify_owner_resource_response(
                &data.owner_request(),
                &response.world_response,
                &sign_read_request("key", (), &data.world_key)
                    .unwrap()
                    .subject_public_key,
            )
            .unwrap();
        }
    }
}
#[test]
fn resource_basis_native_production_variants_remain_zero_outbound_even_with_valid_sid_owner_and_old_flag()
 {
    let data = fixture::Data::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut f = service(format!("http://{}", listener.local_addr().unwrap()), &data);
    assert_ne!(
        call(
            &mut f,
            "/participant/owner-resource-view",
            &data.owner_request(),
            false
        )
        .status,
        200
    );
    let q = data.goal_request();
    let EntryOwnerOperationV1::ReadResourceGoal(read) = &q.operation else {
        unreachable!()
    };
    wire::verify_resource_goal_request(read).unwrap();
    assert_ne!(call(&mut f, "/entry-owner/control", &q, false).status, 200);
    assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn resource_basis_native_component_rejects_wrong_issuer_owner_signature_and_csrf_before_world_io() {
    let data = fixture::Data::new();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut f = service(format!("http://{}", listener.local_addr().unwrap()), &data);
    assert_ne!(
        call(
            &mut f,
            "/_fixture/resource-basis/owner-view",
            &data.owner_request(),
            true
        )
        .status,
        200
    );
    let mut q = data.owner_request();
    q.read.signature_hex = "00".repeat(64);
    assert_ne!(
        call(&mut f, "/_fixture/resource-basis/owner-view", &q, false).status,
        200
    );
    let mut q = data.owner_request();
    q.admission = sign_read_request(
        GAME_PARTICIPANT_ADMISSION_DOMAIN,
        q.admission.request,
        &hex::encode([90; 32]),
    )
    .unwrap()
    .into();
    assert_ne!(
        call(&mut f, "/_fixture/resource-basis/owner-view", &q, false).status,
        200
    );
    assert!(matches!(listener.accept(),Err(e)if e.kind()==std::io::ErrorKind::WouldBlock));
}
#[test]
fn resource_basis_native_component_rejects_bad_world_signature_and_actual_session_revocation_before_delivery()
 {
    for revoke in [false, true] {
        let data = fixture::Data::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut f = service(format!("http://{}", listener.local_addr().unwrap()), &data);
        let path = f.service.config.sqlite_path.clone();
        let namespace = f.service.config.namespace.clone();
        let rp = f.service.config.rp.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let server = serve(
            listener,
            stop.clone(),
            move || {
                if revoke {
                    let mut store =
                        crate::hosted_identity::store::IdentityStore::open(&path, namespace, rp)
                            .unwrap();
                    store
                        .transaction(|tx| {
                            let mut row = tx
                                .get::<crate::hosted_identity::model::AccountRecord>("account:a")?
                                .unwrap();
                            row.value.security_epoch += 1;
                            tx.replace(
                                "account:a",
                                row.row_version,
                                "account",
                                Some("a"),
                                None,
                                &row.value,
                            )
                        })
                        .unwrap();
                }
            },
            !revoke,
        );
        let reply = call(
            &mut f,
            "/_fixture/resource-basis/owner-view",
            &data.owner_request(),
            false,
        );
        stop.store(true, Ordering::SeqCst);
        assert_ne!(reply.status, 200);
        assert_eq!(
            server.join().unwrap(),
            vec![wire::OWNER_RESOURCE_VIEW_PATH_V1]
        );
    }
}
