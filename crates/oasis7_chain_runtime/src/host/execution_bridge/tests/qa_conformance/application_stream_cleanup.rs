//! Total EOF-drain budget and real continuously-readable peer regression.
use super::*;

pub(super) fn drain_until_eof(
    reader: &mut BufReader<TcpStream>,
    budget: Duration,
) -> std::io::Result<()> {
    let deadline = Instant::now() + budget;
    let mut buffer = [0_u8; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "conformance stream drain deadline exceeded",
            ));
        }
        reader.get_ref().set_read_timeout(Some(remaining))?;
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn real_tcp_session_close_bounds_continuously_readable_peer_and_joins_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let accepted_probe = accepted.try_clone().unwrap();
    let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peer_writes = writes.clone();
    let worker = thread::spawn(move || {
        // Every write is sooner than the old per-read timeout; only a total
        // drain deadline can stop this peer without waiting for an idle gap.
        while accepted.write_all(b"x").is_ok() {
            peer_writes.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    });
    let session = Session {
        reader: BufReader::new(socket),
        partial_line: String::new(),
        ordered_response: OrderedResponseProgress::default(),
        diagnostic: false,
        created: Instant::now(),
        connection: 0,
        accepted_probe,
        worker,
    };
    let started = Instant::now();
    let error = session.close().expect_err("continuous peer must reach the drain deadline");
    let oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServerError::Io(error) = error else {
        panic!("continuous peer must fail at the I/O drain boundary");
    };
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ));
    assert!(started.elapsed() >= Duration::from_millis(1500));
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(
        writes.load(Ordering::SeqCst) > 1,
        "peer must actually keep sending"
    );
}
