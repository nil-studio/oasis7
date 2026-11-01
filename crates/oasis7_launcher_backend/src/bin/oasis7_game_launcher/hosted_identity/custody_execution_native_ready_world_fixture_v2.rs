//! Three actual loopback requests with typed synthetic Node responses.
use super::*;
pub(super) struct Server {
    pub endpoint: String,
    pub probe: LockProbe,
    count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    join: std::thread::JoinHandle<()>,
}
impl Server {
    pub(super) fn start(
        data: identity::participant_entry::resource_fixture::Data,
        namespace: Value,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let probe: LockProbe = Arc::new(std::sync::Mutex::new(None));
        let probing = probe.clone();
        let join = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while !stopping.load(Ordering::SeqCst)
                && Instant::now() < until
                && observed.load(Ordering::SeqCst) < 3
            {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => panic!("World accept {:?}", e.kind()),
                };
                observed.fetch_add(1, Ordering::SeqCst);
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let (head, length, path) = loop {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() <= 65536);
                    if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let header = std::str::from_utf8(&bytes[..i]).unwrap();
                        let first = header.lines().next().unwrap();
                        assert!(first.starts_with("POST "));
                        let path = first.split_whitespace().nth(1).unwrap().to_owned();
                        let length: usize = header
                            .lines()
                            .find_map(|l| {
                                l.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, v)| v.trim().parse().unwrap())
                            })
                            .unwrap();
                        assert!(i + 4 + length <= 65536);
                        break (i + 4, length, path);
                    }
                };
                while bytes.len() < head + length {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                assert!(
                    probing
                        .lock()
                        .unwrap()
                        .as_ref()
                        .expect("real runtime lock probe")(),
                    "all three World sends must release Native mutex"
                );
                let input = &bytes[head..head + length];
                let body = if path == ws::participant_discovery::PARTICIPANT_POLICY_PATH {
                    let q: ws::SignedReadRequest<
                        ws::participant_discovery::ParticipantPolicyRequestV1,
                    > = serde_json::from_slice(input).unwrap();
                    let payload = json!({"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":3,"logical_tick":3,
                        "policy":{"world_id":"world","genesis_digest":"genesis","enabled":true,"registry_complete":true,"identity_namespace":namespace,
                            "issuer_public_key":ws::sign_read_request("key",(),&data.issuer).unwrap().subject_public_key,"generation":1,"max_grant_ticks":100}});
                    serde_json::to_vec(
                        &ws::sign_service_response(
                            &path,
                            ws::request_digest(&path, &q).unwrap(),
                            payload,
                            &data.world_key,
                        )
                        .unwrap(),
                    )
                    .unwrap()
                } else {
                    assert_eq!(path, ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH);
                    data.body(&path, input)
                };
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
                stream.flush().unwrap();
            }
        });
        Self {
            endpoint,
            probe,
            count,
            stop,
            join,
        }
    }
    pub(super) fn finish(self) -> usize {
        self.stop.store(true, Ordering::SeqCst);
        self.join.join().unwrap();
        self.count.load(Ordering::SeqCst)
    }
}
