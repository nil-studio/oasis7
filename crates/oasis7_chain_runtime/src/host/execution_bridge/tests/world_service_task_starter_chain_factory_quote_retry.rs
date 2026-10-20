use oasis7::world_service::{
    SignedReadRequest, WorldIdentity,
    authority::{request_digest, sign_read_request, sign_service_response},
    client::{RemoteWorldServiceClient, WorldServiceClientConfig, WorldServiceClientError},
    task_factory_quote::{TASK_FACTORY_QUOTE_PATH, TaskFactoryQuoteRequestV1, TaskFactoryQuoteV1},
};
use std::{
    io::{Read, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

fn request_body(stream: &mut std::net::TcpStream) -> std::io::Result<Vec<u8>> {
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut byte = [0; 1];
        stream.read_exact(&mut byte)?;
        headers.push(byte[0]);
        if headers.len() > 64 * 1024 {
            return Err(std::io::Error::other("request headers exceed test bound"));
        }
    }
    let headers = std::str::from_utf8(&headers)
        .map_err(|_| std::io::Error::other("invalid test request headers"))?;
    let length = headers
        .split("\r\n")
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .ok_or_else(|| std::io::Error::other("request body length missing"))?;
    if length > 2 * 1024 * 1024 {
        return Err(std::io::Error::other("request body exceeds test bound"));
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    Ok(body)
}

fn response(stream: &mut std::net::TcpStream, body: &[u8]) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}

fn accept_until(
    listener: &std::net::TcpListener,
    deadline: Instant,
) -> std::io::Result<std::net::TcpStream> {
    loop {
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "test accept deadline elapsed",
            ));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error),
        }
    }
}

pub(super) fn assert_delayed_factory_quote_retry_reuses_original(
    proof: &SignedReadRequest<TaskFactoryQuoteRequestV1>,
    quoted: &TaskFactoryQuoteV1,
    delegate: &str,
) {
    let service_key = hex::encode([92; 32]);
    let trusted_service_public_key = sign_read_request("test-service", (), &service_key)
        .unwrap()
        .subject_public_key;
    let signed_response = sign_service_response(
        TASK_FACTORY_QUOTE_PATH,
        request_digest(TASK_FACTORY_QUOTE_PATH, proof).unwrap(),
        quoted.clone(),
        &service_key,
    )
    .unwrap();
    let response_body = serde_json::to_vec(&signed_response).unwrap();
    let expected_request = serde_json::to_vec(proof).unwrap();
    let fence = &proof.request.context_proof.request.context.fence;
    let expected_world = WorldIdentity {
        world_id: fence.world_id.clone(),
        genesis_digest: fence.genesis_digest.clone(),
    };
    let scope_id = format!("agent:{}", fence.agent_id);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (request_seen, request_seen_rx) = mpsc::sync_channel(1);
    let (release_first, release_first_rx) = mpsc::sync_channel(1);
    let server_response = response_body.clone();
    let server = std::thread::spawn(move || {
        let mut first = accept_until(&listener, Instant::now() + Duration::from_secs(3)).unwrap();
        first
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        first
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut requests = vec![request_body(&mut first).unwrap()];
        request_seen.send(()).unwrap();
        release_first_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first response waits until after the client timeout");
        let _ = response(&mut first, &server_response);

        if let Ok(mut second) = accept_until(&listener, Instant::now() + Duration::from_secs(1)) {
            second
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            second
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            requests.push(request_body(&mut second).unwrap());
            response(&mut second, &server_response).unwrap();
        }
        requests
    });
    let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint,
        trusted_service_public_key,
        expected_world,
        scope_id,
        read_private_key_hex: delegate.into(),
        timeout: Duration::from_millis(100),
        max_response_bytes: 2 * 1024 * 1024,
    })
    .unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut attempts = 0;
    let mut first_request_timed_out = false;
    let repeated = super::super::production_playtest::retry_same_signed_private_quote_until(
        proof,
        deadline,
        |original| {
            attempts += 1;
            let result = client.quote_task_factory(original);
            if attempts == 1 {
                first_request_timed_out = matches!(
                    &result,
                    Err(WorldServiceClientError::Transport(reason))
                        if reason == "private transport timed out"
                );
                request_seen_rx
                    .recv_timeout(Duration::from_secs(2))
                    .expect("server accepted the first signed factory quote request");
                release_first.send(()).unwrap();
            }
            result
        },
    );
    let requests = server.join().unwrap();
    let repeated = repeated.expect("same signed factory quote retry must recover the response");
    assert!(
        first_request_timed_out,
        "first signed factory read must time out"
    );
    assert_eq!(attempts, 2);
    assert!(
        serde_json::to_value(repeated).unwrap() == serde_json::to_value(quoted).unwrap(),
        "same signed factory request retry must return the verified quote"
    );
    assert!(
        Instant::now() < deadline,
        "retry must retain the original factory quote deadline"
    );
    assert!(
        requests.len() == 2 && requests[0] == expected_request && requests[1] == expected_request,
        "timeout retry must send the exact same signed factory request twice"
    );
}
