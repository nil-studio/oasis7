//! Preserve the observation reader, then half-close and drain to actual peer EOF.
use super::*;
use std::io::{self, Read};

type Observer = thread::JoinHandle<(BufReader<TcpStream>, io::Result<()>)>;
type CloseResult<T> = (thread::Result<T>, thread::Result<io::Result<()>>);

pub(super) fn close<T>(
    socket: &TcpStream,
    stop: &AtomicBool,
    observer: Observer,
    worker: thread::JoinHandle<T>,
) -> CloseResult<T> {
    close_with_drain(socket, stop, observer, worker, |reader| {
        application_stream_boundaries::drain_until_eof(reader, Duration::from_secs(2))
    })
}

fn close_with_drain<T>(
    socket: &TcpStream,
    stop: &AtomicBool,
    observer: Observer,
    worker: thread::JoinHandle<T>,
    drain: impl FnOnce(&mut BufReader<TcpStream>) -> io::Result<()>,
) -> CloseResult<T> {
    let cleanup = cleanup_deadline::CleanupDeadline::start("admission_stream_close");
    stop.store(true, Ordering::SeqCst);
    let drained = match observer.join() {
        Ok((mut reader, observed)) => Ok(observed.and_then(|()| {
            // Both would discard responses queued by a still-successful peer.
            // Keep the same reader, including bytes already in its buffer.
            socket.shutdown(std::net::Shutdown::Write)?;
            drain(&mut reader)
        })),
        Err(panic) => Err(panic),
    };
    if !matches!(&drained, Ok(Ok(()))) {
        // Force cleanup only after recording the failure. Never turn a reset,
        // timeout or observer panic into successful EOF, or detach either worker.
        let _ = socket.shutdown(std::net::Shutdown::Both);
    }
    let served = worker.join();
    cleanup.complete();
    (served, drained)
}

#[test]
fn admission_close_receives_response_sent_after_write_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    for stream in [&socket, &accepted] {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
    }
    let request = b"already queued request\n";
    let payload = vec![b'x'; 8192];
    socket.write_all(request).unwrap();
    let peer_payload = payload.clone();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let result = (|| -> io::Result<Vec<u8>> {
            accepted.write_all(&peer_payload[..64])?;
            let mut received = Vec::new();
            accepted.read_to_end(&mut received)?;
            // The remaining response follows the client's write EOF. It fits in the
            // socket buffer, so the peer can finish before the reader resumes.
            accepted.write_all(&peer_payload[64..])?;
            Ok(received)
        })();
        drop(accepted);
        let _ = finished_tx.send(());
        result
    });
    let mut reader = BufReader::new(socket.try_clone().unwrap());
    let buffered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed_buffer = buffered.clone();
    let observer = thread::spawn(move || {
        let observed = reader.fill_buf().map(|bytes| {
            observed_buffer.store(bytes.len(), Ordering::SeqCst);
        });
        (reader, observed)
    });
    let stop = AtomicBool::new(false);
    let mut received = vec![0; payload.len()];
    let (served, drained) = close_with_drain(&socket, &stop, observer, worker, |reader| {
        finished_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| io::Error::new(io::ErrorKind::TimedOut, error))?;
        // This ordering reproduces the old Both -> successful peer -> reset
        // race deterministically, rather than accepting an early local EOF.
        reader.read_exact(&mut received)?;
        application_stream_boundaries::drain_until_eof(reader, Duration::from_secs(2))
    });
    assert_eq!(served.unwrap().unwrap().as_slice(), request);
    drained
        .unwrap()
        .expect("queued response and actual peer EOF required");
    assert_eq!(received, payload);
    assert!(
        buffered.load(Ordering::SeqCst) > 0,
        "handoff must preserve buffered bytes"
    );
    assert!(stop.load(Ordering::SeqCst));
}

#[test]
fn admission_close_preserves_observation_and_drain_resets_after_join() {
    for observation_failed in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut accepted, _) = listener.accept().unwrap();
        accepted
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let worker = thread::spawn(move || {
            let mut received = Vec::new();
            accepted.read_to_end(&mut received)
        });
        let reader = BufReader::new(socket.try_clone().unwrap());
        let observer = thread::spawn(move || {
            let observed = if observation_failed {
                Err(io::Error::from(io::ErrorKind::ConnectionReset))
            } else {
                Ok(())
            };
            (reader, observed)
        });
        let stop = AtomicBool::new(false);
        let mut drain_called = false;
        let (served, drained) = close_with_drain(&socket, &stop, observer, worker, |_| {
            drain_called = true;
            Err(io::Error::from(io::ErrorKind::ConnectionReset))
        });
        served.unwrap().unwrap();
        assert_eq!(drain_called, !observation_failed);
        assert_eq!(
            drained.unwrap().unwrap_err().kind(),
            io::ErrorKind::ConnectionReset
        );
        assert!(stop.load(Ordering::SeqCst));
    }
}

#[test]
fn admission_close_joins_peer_before_returning_observer_panic() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let worker = thread::spawn(move || {
        let mut received = Vec::new();
        accepted.read_to_end(&mut received)
    });
    let observer: Observer = thread::spawn(|| panic!("injected observation panic"));
    let (served, drained) = close(&socket, &AtomicBool::new(false), observer, worker);
    served.unwrap().unwrap();
    assert!(
        drained.is_err(),
        "observation panic must survive joined cleanup"
    );
}

#[test]
fn admission_close_bounds_continuously_readable_peer_and_joins_it() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    accepted
        .set_write_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let worker = thread::spawn(move || -> io::Result<usize> {
        let mut requests = Vec::new();
        accepted.read_to_end(&mut requests)?;
        let mut writes = 0;
        // Model an active peer without saturating the runner. Each write is
        // sooner than the read timeout; only the total EOF budget can finish.
        while accepted.write_all(b"x").is_ok() {
            writes += 1;
            thread::sleep(Duration::from_millis(10));
        }
        Ok(writes)
    });
    let reader = BufReader::new(socket.try_clone().unwrap());
    let observer = thread::spawn(move || (reader, Ok(())));
    let started = Instant::now();
    let (served, drained) = close(&socket, &AtomicBool::new(false), observer, worker);
    assert!(
        served.unwrap().unwrap() > 1,
        "peer must actually keep writing"
    );
    assert!(matches!(
        drained.unwrap().unwrap_err().kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ));
    assert!(started.elapsed() >= Duration::from_millis(1500));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn admission_close_rejects_real_peer_reset_after_write_eof() {
    use std::os::fd::AsRawFd;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let linger = libc::linger {
        l_onoff: 1,
        l_linger: 0,
    };
    // SAFETY: accepted owns a live socket; linger has the platform's libc layout,
    // and its correctly sized pointer remains valid for this synchronous call.
    let configured = unsafe {
        libc::setsockopt(
            accepted.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_LINGER,
            (&linger as *const libc::linger).cast(),
            std::mem::size_of_val(&linger) as libc::socklen_t,
        )
    };
    assert_eq!(configured, 0, "configure actual abortive peer close");
    let (closed_tx, closed_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        let result = accepted.read_to_end(&mut requests);
        drop(accepted);
        let _ = closed_tx.send(());
        result
    });
    let reader = BufReader::new(socket.try_clone().unwrap());
    let observer = thread::spawn(move || (reader, Ok(())));
    let (served, drained) = close_with_drain(
        &socket,
        &AtomicBool::new(false),
        observer,
        worker,
        |reader| {
            closed_rx
                .recv_timeout(Duration::from_secs(2))
                .map_err(|error| io::Error::new(io::ErrorKind::TimedOut, error))?;
            application_stream_boundaries::drain_until_eof(reader, Duration::from_secs(2))
        },
    );
    served.unwrap().unwrap();
    assert_eq!(
        drained.unwrap().unwrap_err().kind(),
        io::ErrorKind::ConnectionReset
    );
}
