//! Actual TCP receipts are counted before any semantic parsing/assertion.
use super::*;
pub(super) struct Server {
    pub endpoint: String,
    pub count: Arc<AtomicUsize>,
    pub probe: LockProbe,
    pub legacy_weak_preflight: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    join: std::thread::JoinHandle<()>,
}
impl Server {
    pub fn start(data: world_fixture::Data, weak: bool, protocol_failure: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let probe: LockProbe = Arc::new(std::sync::Mutex::new(None));
        let probing = probe.clone();
        let legacy_weak_preflight = Arc::new(AtomicBool::new(false));
        let weak_observed = legacy_weak_preflight.clone();
        let join = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while !stopping.load(Ordering::SeqCst) && Instant::now() < until {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => panic!("resource accept {:?}", e.kind()),
                };
                observed.fetch_add(1, Ordering::SeqCst);
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let (head, length) = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() <= 65536);
                    if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let h = std::str::from_utf8(&bytes[..i]).unwrap();
                        let length = h
                            .lines()
                            .find_map(|l| {
                                l.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        break (i + 4, length);
                    }
                };
                while bytes.len() < head + length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() <= 65536);
                }
                assert!(
                    probing.lock().unwrap().as_ref().unwrap()(),
                    "both World sends must release Native identity mutex"
                );
                let h = std::str::from_utf8(&bytes[..head]).unwrap();
                let path = h.lines().next().unwrap().split_whitespace().nth(1).unwrap();
                let header = h
                    .lines()
                    .find_map(|l| {
                        l.split_once(':')
                            .filter(|(k, _)| k.eq_ignore_ascii_case("Oasis7-Custody-Resource-Read"))
                            .map(|(_, v)| v.trim())
                    })
                    .unwrap();
                let proof: PaidResourceReadProofV1 =
                    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).unwrap()).unwrap();
                let config =
                    identity::custody::resource_final_config::ResourceFinalConfigV2::from_env()
                        .unwrap();
                proof
                    .verify_issuer(
                        &config.paid_pin(&proof.payload.world_audience),
                        &PaidResourceReadIssuerFloorV1 {
                            issuer_epoch: config.issuer_floor.issuer_epoch,
                            config_generation: config.issuer_floor.config_generation,
                        },
                        now_ms(),
                    )
                    .unwrap();
                let body = if path == resource::RESOURCE_GOAL_PATH_V2 {
                    let q: oasis7_custody_core::provisioning::OwnerSetupResourceWorldBodyV2 =
                        serde_json::from_slice(&bytes[head..head + length]).unwrap();
                    q.validate_bindings(&proof.payload).unwrap();
                    serde_json::to_vec(q.resource()).unwrap()
                } else {
                    assert_eq!(path, ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH);
                    bytes[head..head + length].to_vec()
                };
                let body = data.body(path, &body);
                if protocol_failure {
                    // Deliberately truncated response after a real, fully received
                    // paid request. Never retry the original Native/World request.
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 64\r\nConnection: close\r\n\r\n{",
                        )
                        .unwrap();
                    stream.flush().unwrap();
                    continue;
                }

                let body = if weak && path == ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH
                {
                    let mut response: ws::SignedServiceResponse<
                        ws::participant_discovery::OwnerSetupObservationV1,
                    > = serde_json::from_slice(&body).unwrap();
                    let expected = response.request_digest.clone();
                    ws::verify_service_response(
                        path,
                        &expected,
                        &response,
                        &ws::sign_read_request("key", (), &data.world_key)
                            .unwrap()
                            .subject_public_key,
                    )
                    .unwrap();
                    response.signature_hex = format!("01{}", "00".repeat(63));
                    ws::verify_service_response(
                        path,
                        &expected,
                        &response,
                        &format!("01{}", "00".repeat(31)),
                    )
                    .unwrap();
                    weak_observed.store(true, Ordering::SeqCst);
                    serde_json::to_vec(&response).unwrap()
                } else {
                    body
                };
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
                stream.flush().unwrap();
            }
        });
        Self {
            endpoint,
            count,
            probe,
            legacy_weak_preflight,
            stop,
            join,
        }
    }
    pub fn finish(self) -> usize {
        self.stop.store(true, Ordering::SeqCst);
        self.join.join().unwrap();
        self.count.load(Ordering::SeqCst)
    }
}
