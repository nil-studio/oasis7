//! A completed response must not replace actual consumption of the stream.
use super::*;

const INITIAL: &[u8] = b"{\"type\":\"hello_ack\"}\n{\"type\":\"snapshot\",\"snapshot\":{\"runtime_snapshot\":{}}}\n{\"type\":\"authoritative_recovery_ack\"}\n";
const SNAPSHOT: &[u8] = b"{\"type\":\"snapshot\",\"snapshot\":{\"runtime_snapshot\":{}}}\n";

#[test]
fn completed_exchange_drains_new_frames_with_one_total_read_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let accepted_probe = accepted.try_clone().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        accepted.write_all(INITIAL)?;
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        accepted.write_all(SNAPSHOT)?;
        ready_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let batch = SNAPSHOT.repeat(256);
        while !stopping.load(Ordering::SeqCst) && Instant::now() < deadline {
            accepted.write_all(&batch)?;
        }
        let mut requests = Vec::new();
        accepted.read_to_end(&mut requests)?;
        assert!(
            requests.is_empty(),
            "draining must not issue another request"
        );
        Ok(())
    });
    let mut session = Session {
        reader: BufReader::new(socket),
        partial_line: Vec::new(),
        ordered_response: OrderedResponseProgress::default(),
        diagnostic: false,
        created: Instant::now(),
        connection: 0,
        accepted_probe,
        worker,
    };
    assert!(session.snapshot_ordered(Duration::from_secs(2), true));
    release_tx.send(()).unwrap();
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(session.snapshot_ordered(Duration::ZERO, false));
    let budget = Duration::from_millis(80);
    let started = Instant::now();
    let frames = session.drain_for(budget);
    let elapsed = started.elapsed();
    stop.store(true, Ordering::SeqCst);
    let same_exchange_complete = session.snapshot_ordered(Duration::ZERO, false);
    let joined = session.close();
    assert!(joined.is_ok());
    assert!(
        frames > 0,
        "completed exchange must still consume new snapshots"
    );
    assert!(
        same_exchange_complete,
        "drain must preserve verified ordering"
    );
    assert!(
        elapsed >= budget,
        "cached completion cannot replace a read window"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "continuous readable frames cannot renew the total read budget"
    );
}

#[test]
fn completed_exchange_drain_retains_partial_frame_and_counts_it_once() {
    let snapshot: &'static [u8] =
        b"{\"type\":\"snapshot\",\"snapshot\":{\"runtime_snapshot\":{\"label\":\"\xc3\xa9\"}}}\n";
    let split = snapshot.iter().position(|byte| *byte == 0xc3).unwrap() + 1;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let accepted_probe = accepted.try_clone().unwrap();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        accepted.write_all(INITIAL)?;
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        accepted.write_all(&snapshot[..split])?;
        ready_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        accepted.write_all(&snapshot[split..])?;
        ready_tx.send(()).unwrap();
        let mut requests = Vec::new();
        accepted.read_to_end(&mut requests)?;
        assert!(requests.is_empty());
        Ok(())
    });
    let mut session = Session {
        reader: BufReader::new(socket),
        partial_line: Vec::new(),
        ordered_response: OrderedResponseProgress::default(),
        diagnostic: false,
        created: Instant::now(),
        connection: 0,
        accepted_probe,
        worker,
    };
    assert!(session.snapshot_ordered(Duration::from_secs(2), true));
    release_tx.send(()).unwrap();
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(session.drain_for(Duration::from_millis(25)), 0);
    assert_eq!(session.partial_line.as_slice(), &snapshot[..split]);
    release_tx.send(()).unwrap();
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(session.drain_for(Duration::from_millis(25)), 1);
    assert!(session.partial_line.is_empty());
    assert_eq!(session.drain_for(Duration::from_millis(25)), 0);
    assert!(session.snapshot_ordered(Duration::ZERO, false));
    assert!(session.close().is_ok());
}

#[test]
fn continuous_fragments_cannot_extend_one_frame_past_the_read_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    accepted
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let accepted_probe = accepted.try_clone().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let worker = thread::spawn(move || {
        accepted.write_all(INITIAL)?;
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        accepted.write_all(
            b"{\"type\":\"snapshot\",\"snapshot\":{\"runtime_snapshot\":{\"label\":\"",
        )?;
        ready_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !stopping.load(Ordering::SeqCst) && Instant::now() < deadline {
            accepted.write_all(&[b'x'; 256])?;
            thread::sleep(Duration::from_millis(5));
        }
        accepted.write_all(b"\"}}}\n")?;
        finished_tx.send(()).unwrap();
        let mut requests = Vec::new();
        accepted.read_to_end(&mut requests)?;
        assert!(requests.is_empty());
        Ok(())
    });
    let mut session = Session {
        reader: BufReader::new(socket),
        partial_line: Vec::new(),
        ordered_response: OrderedResponseProgress::default(),
        diagnostic: false,
        created: Instant::now(),
        connection: 0,
        accepted_probe,
        worker,
    };
    assert!(session.snapshot_ordered(Duration::from_secs(2), true));
    release_tx.send(()).unwrap();
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let budget = Duration::from_millis(80);
    let started = Instant::now();
    let initial_frames = session.drain_for(budget);
    let elapsed = started.elapsed();
    let partial_retained = !session.partial_line.is_empty();
    stop.store(true, Ordering::SeqCst);
    finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let completed_frames = session.drain_for(Duration::from_millis(25));
    let duplicate_frames = session.drain_for(Duration::from_millis(25));
    let joined = session.close();
    assert!(joined.is_ok());
    assert_eq!(
        initial_frames, 0,
        "read window must end before the frame completes"
    );
    assert!(partial_retained);
    assert!(elapsed >= budget && elapsed < Duration::from_secs(1));
    assert_eq!(
        completed_frames, 1,
        "later window must finish the original frame"
    );
    assert_eq!(duplicate_frames, 0);
}
