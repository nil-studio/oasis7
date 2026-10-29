use super::*;
use std::io::Write;
use std::time::Duration;

#[test]
fn generic_transport_refuses_archive_payload_before_any_write() {
    let mut bytes = Vec::new();
    assert!(
        write_response(
            &mut bytes,
            b"archive payload",
            Some(Instant::now() + Duration::from_secs(1)),
            &mut None
        )
        .is_err()
    );
    assert!(bytes.is_empty());
    write_response(&mut bytes, b"legacy response", None, &mut None).unwrap();
    assert_eq!(bytes, b"legacy response");
}
#[test]
fn tcp_archive_write_uses_one_deadline_and_restores_timeout() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let reader = std::net::TcpStream::connect(address).unwrap();
    let (mut writer, _) = listener.accept().unwrap();
    let original = Some(Duration::from_secs(2));
    writer.set_write_timeout(original).unwrap();
    let started = Instant::now();
    let mut policy = |stream: &mut std::net::TcpStream, deadline: Option<Instant>| {
        let remaining = match deadline {
            Some(deadline) => Some(
                deadline
                    .checked_duration_since(Instant::now())
                    .filter(|d| !d.is_zero())
                    .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "expired"))?,
            ),
            None => original,
        };
        stream.set_write_timeout(remaining)
    };
    // The peer deliberately never drains its receive buffer. This bounded frame
    // cannot all fit in the kernel buffers and therefore exercises blocked I/O.
    let payload = vec![0u8; 16 * 1024 * 1024];
    let result = write_response(
        &mut writer,
        &payload,
        Some(started + Duration::from_millis(35)),
        &mut Some(&mut policy),
    );
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(writer.write_timeout().unwrap(), original);
    drop(reader);
    writer.flush().unwrap();
}
