//! Actual TCP framing/budget evidence. These bodies test transport capacity;
//! the separate shipping fixture supplies real BFT proof/World semantics.
use super::*;
use std::io::Read;

#[test]
fn v3_http_writer_transmits_response_larger_than_the_v2_limit() {
    let mut body = br#"{"padding":""#.to_vec();
    body.extend(std::iter::repeat_n(b'x', 9 * 1024 * 1024));
    body.extend_from_slice(br#""}"#);
    let expected = body.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let sender = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write_http_reply(&mut stream, 200, &body)
    });
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut received = Vec::new();
    stream.read_to_end(&mut received).unwrap();
    sender
        .join()
        .unwrap()
        .expect("legal V3 body above 8 MiB must be delivered");
    let boundary = received.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let header = std::str::from_utf8(&received[..boundary]).unwrap();
    assert!(header.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(header.contains(&format!("Content-Length: {}\r\n", expected.len())));
    assert_eq!(&received[boundary..], expected);
}

#[test]
fn v3_http_writer_expires_a_nonreading_peer_at_the_absolute_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let sender = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let body = vec![b'x'; 32 * 1024 * 1024];
        let started = Instant::now();
        let result = super::super::formal_bft_prefix_transport::write_http_reply_until(
            &mut stream,
            200,
            &body,
            started + Duration::from_millis(250),
        );
        (result, started.elapsed())
    });
    let _nonreading_peer = TcpStream::connect(address).unwrap();
    let (result, elapsed) = sender.join().unwrap();
    assert!(result.unwrap_err().contains("absolute write deadline"));
    assert!(
        elapsed < Duration::from_secs(2),
        "absolute budget was extended by progress: {elapsed:?}"
    );
}

#[test]
fn v3_http_writer_rejects_oversize_before_emitting_any_header() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let sender = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        write_http_reply(&mut stream, 200, &vec![b'x'; MAX_SIGNED_RESPONSE_BYTES + 1])
    });
    let mut peer = TcpStream::connect(address).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut received = Vec::new();
    peer.read_to_end(&mut received).unwrap();
    assert!(sender.join().unwrap().unwrap_err().contains("40 MiB"));
    assert!(received.is_empty());
}
