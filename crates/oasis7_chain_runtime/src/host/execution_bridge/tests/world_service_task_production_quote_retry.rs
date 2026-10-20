use oasis7::world_service::{
    SignedReadRequest, WorldIdentity,
    authority::{request_digest, sign_read_request, sign_service_response},
    client::{RemoteWorldServiceClient, WorldServiceClientConfig, WorldServiceClientError},
    task_recipe_quote::{TASK_RECIPE_QUOTE_PATH, TaskRecipeQuoteRequestV1, TaskRecipeQuoteV1},
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

pub(super) fn assert_delayed_quote_retry_reuses_original(
    proof: &SignedReadRequest<TaskRecipeQuoteRequestV1>,
    quoted: &TaskRecipeQuoteV1,
    delegate: &str,
) {
    let service_key = hex::encode([91; 32]);
    let trusted_service_public_key = sign_read_request("test-service", (), &service_key)
        .unwrap()
        .subject_public_key;
    let signed_response = sign_service_response(
        TASK_RECIPE_QUOTE_PATH,
        request_digest(TASK_RECIPE_QUOTE_PATH, proof).unwrap(),
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
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut requests = Vec::new();
        for attempt in 0..2 {
            let mut stream = accept_until(&listener, deadline).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            requests.push(request_body(&mut stream).unwrap());
            if attempt == 0 {
                request_seen.send(()).unwrap();
                release_first_rx
                    .recv_timeout(Duration::from_secs(2))
                    .expect("first response waits for client timeout signal");
                let _ = response(&mut stream, &server_response);
            } else {
                response(&mut stream, &server_response).unwrap();
            }
        }
        requests
    });
    let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        endpoint,
        formal_read_trust: None,
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
    let repeated = super::retry_same_signed_private_quote_until(proof, deadline, |original| {
        attempts += 1;
        let result = client.quote_task_recipe(original);
        if attempts == 1 {
            first_request_timed_out = matches!(
                &result,
                Err(WorldServiceClientError::Transport(reason))
                    if reason == "private transport timed out"
            );
            request_seen_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("server accepted first request before timeout handling");
            release_first.send(()).unwrap();
        }
        result
    })
    .unwrap();
    assert!(first_request_timed_out, "first signed read should time out");
    assert_eq!(attempts, 2);
    assert!(
        serde_json::to_value(repeated).unwrap() == serde_json::to_value(quoted).unwrap(),
        "same signed request retry must return the signed quote"
    );
    assert!(
        Instant::now() < deadline,
        "retry must retain the original deadline"
    );
    let requests = server.join().unwrap();
    assert!(
        requests.len() == 2 && requests[0] == expected_request && requests[1] == expected_request,
        "timeout retry must send the exact same signed request twice"
    );
}
