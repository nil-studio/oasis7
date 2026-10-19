use super::*;
use std::{net::TcpStream, sync::mpsc, thread, time::Duration};

#[test]
fn listener_readiness_wakes_for_loopback_connection_and_rearms() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let mut readiness = ListenerReadiness::new(&listener).unwrap();
    let (send, receive) = mpsc::channel();
    let worker = thread::spawn(move || {
        for _ in 0..3 {
            receive.recv_timeout(Duration::from_secs(5)).unwrap();
            let stream = TcpStream::connect(address).unwrap();
            // Keep each connection live until the accepting side acknowledges.
            receive.recv_timeout(Duration::from_secs(5)).unwrap();
            drop(stream);
        }
    });
    for _ in 0..3 {
        send.send(()).unwrap();
        assert!(
            readiness
                .wait_until(Instant::now() + Duration::from_secs(5))
                .unwrap()
        );
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
        send.send(()).unwrap();
    }
    readiness.close().unwrap();
    worker.join().unwrap();
}

#[test]
fn listener_readiness_reports_only_actual_idle_timeout_and_unregisters() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_millis(20);
    let mut readiness = ListenerReadiness::new(&listener).unwrap();
    assert!(!readiness.wait_until(deadline).unwrap());
    assert!(Instant::now() >= deadline);
    readiness.close().unwrap();
    // The same live listener can be registered by a fresh owner after close.
    let mut reopened = ListenerReadiness::new(&listener).unwrap();
    assert!(!reopened.wait_until(Instant::now()).unwrap());
    drop(reopened);
    ListenerReadiness::new(&listener).unwrap().close().unwrap();
}
