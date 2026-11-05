//! Decoder evidence only: a real TCP timeout cannot terminate a JSON frame.
//! This fixture supplies no Viewer/World authorization or 150ms fairness proof.
use super::*;

#[test]
fn real_tcp_snapshot_response_prefix_survives_receive_call_timeout() {
    fragmented_snapshot(
        br#"{"type":"snapshot","fixture":"prefix"#.to_vec(),
        b"suffix\"}\n".to_vec(),
    );
}

#[test]
fn real_tcp_snapshot_response_utf8_prefix_survives_receive_call_timeout() {
    let frame = "{\"type\":\"snapshot\",\"fixture\":\"中\"}\n".as_bytes();
    let split = frame.iter().position(|byte| *byte == 0xe4).unwrap() + 1;
    fragmented_snapshot(frame[..split].to_vec(), frame[split..].to_vec());
}

fn fragmented_snapshot(prefix: Vec<u8>, suffix: Vec<u8>) {
    let prefix_len = prefix.len();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut writer, _) = listener.accept().unwrap();
    let accepted_probe = writer.try_clone().unwrap();
    let (prefix_sent, prefix_ready) = std::sync::mpsc::channel();
    let (continue_frame, continuation) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        writer.write_all(&prefix)?;
        prefix_sent.send(()).unwrap();
        // The suffix is released only after the first real receive returned.
        // No guessed sleep decides where the TCP frame is fragmented.
        continuation.recv_timeout(Duration::from_secs(3)).unwrap();
        writer.write_all(&suffix)?;
        Ok(())
    });
    let mut session = Session {
        reader: BufReader::new(socket),
        receive_frame: Vec::new(),
        diagnostic: true,
        last_receive_diagnostic: None,
        accepted_probe,
        normal_send_buffer: None,
        worker: Some(worker),
    };
    prefix_ready.recv_timeout(Duration::from_secs(3)).unwrap();
    let first = session.receive_snapshot(Duration::from_millis(150), true);
    let first_diagnostic = session.last_receive_diagnostic.take().unwrap();
    continue_frame.send(()).unwrap();
    // This call waits for the same frame, without requesting a new Hello/ACK.
    // Keeping a session-global ACK would weaken the real per-phase test.
    let second = session.receive_snapshot(Duration::from_millis(150), false);
    let joined = session.finish();
    assert!(!first, "an unterminated frame is not a complete snapshot");
    assert!(!first_diagnostic.eof);
    assert!(first_diagnostic.timeouts > 0);

    assert!(
        joined.is_ok(),
        "TCP fragment writer must complete: {joined:?}"
    );
    assert!(
        second,
        "the continuation must finish the original snapshot frame"
    );
    assert_eq!(first_diagnostic.timeout_partial_bytes, prefix_len);
}
