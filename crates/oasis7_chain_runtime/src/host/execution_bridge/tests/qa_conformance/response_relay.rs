//! Relay only after the real dispatcher and request-bound signature verifier complete.
use std::io::{self, Write};
pub fn relay(output: &mut impl Write, bytes: &[u8], verified: bool) -> io::Result<bool> {
    match output.write_all(bytes) {
        Err(error)
            if verified
                && matches!(
                    error.kind(),
                    io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                ) =>
        {
            Ok(true)
        }
        result => result.map(|()| false),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn verified_capture_allows_disconnected_relay() {
        assert!(relay(&mut Closed, b"original", true).unwrap());
    }
    #[test]
    fn unverified_capture_rejects_disconnect() {
        assert!(relay(&mut Closed, b"invalid", false).is_err());
    }
    #[test]
    fn ordinary_relay_preserves_bytes() {
        let mut v = Vec::new();
        relay(&mut v, b"original", true).unwrap();
        assert_eq!(v, b"original");
    }
    fn disconnected_output() -> std::net::TcpStream {
        use std::io::Read;
        use std::net::{Shutdown, TcpListener, TcpStream};
        use std::time::{Duration, Instant};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut output, _) = listener.accept().unwrap();
        output
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        output
            .set_write_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        peer.shutdown(Shutdown::Both).unwrap();
        drop(peer);
        assert_eq!(output.read(&mut [0; 1]).unwrap(), 0);
        // FIN observation does not guarantee that the first local write fails.
        // Bound both kernel waits and probes rather than assuming a buffer size.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut accepted = 0;
        while Instant::now() < deadline && accepted < 16 * 1024 * 1024 {
            match output.write(&[b'x'; 4096]) {
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                    ) =>
                {
                    return output;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => panic!("unexpected TCP probe failure: {error}"),
                Ok(count) => accepted += count,
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("peer disconnect was not observed within finite probe limits");
    }
    #[test]
    fn actual_tcp_peer_shutdown_rejects_legacy_write_but_allows_only_verified_relay() {
        let response = b"original captured response";
        let mut verified = disconnected_output();
        // Legacy direct dispatch propagates the real closed-peer error.
        assert!(verified.write_all(response).is_err());
        assert!(relay(&mut verified, response, true).unwrap());
        let mut unverified = disconnected_output();
        assert!(relay(&mut unverified, response, false).is_err());
    }
}
