//! Small real-framing experiment; no formal world or protected endpoint fixture.
use super::*;
use serde::ser::Error as _;
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const BOUND: Duration = Duration::from_secs(2);
const PEER_READ: Duration = Duration::from_millis(80);

struct ControlledEncoding {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl Serialize for ControlledEncoding {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.entered.send(()).map_err(S::Error::custom)?;
        self.release.recv_timeout(BOUND).map_err(S::Error::custom)?;
        serializer.serialize_u8(7)
    }
}

fn accept_before(listener: &TcpListener, deadline: Instant) -> Result<TcpStream, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("bounded accept expired".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

#[test]
#[cfg(unix)]
fn formal_rpc_connected_encoding_can_exhaust_peer_first_read() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let deadline = Instant::now() + BOUND;
    let (accepted_tx, accepted_rx) = mpsc::channel();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (closed_tx, closed_rx) = mpsc::channel();
    let server = std::thread::spawn(move || -> Result<(), String> {
        let mut stream = accept_before(&listener, deadline)?;
        configure(&stream, PEER_READ).map_err(|e| e.to_string())?;
        accepted_tx.send(()).map_err(|e| e.to_string())?;
        entered_rx.recv_timeout(BOUND).map_err(|e| e.to_string())?;
        let failure = read::<u8>(&mut stream).expect_err("encoding has sent no header");
        // FormalError stores IO text; compare the exact platform OS mappings,
        // rather than accepting any framing/authentication failure as timeout.
        let timeout = match failure {
            FormalError::Io(message) => [libc::EAGAIN, libc::EWOULDBLOCK, libc::ETIMEDOUT]
                .into_iter()
                .any(|code| message == std::io::Error::from_raw_os_error(code).to_string()),
            _ => false,
        };
        drop(stream);
        closed_tx.send(()).map_err(|e| e.to_string())?;
        if !timeout {
            return Err("peer failure was not a mapped socket read timeout".into());
        }
        Ok(())
    });
    let client = (|| -> Result<(), String> {
        let mut stream = TcpStream::connect_timeout(&address, BOUND).map_err(|e| e.to_string())?;
        configure(&stream, BOUND).map_err(|e| e.to_string())?;
        accepted_rx.recv_timeout(BOUND).map_err(|e| e.to_string())?;
        let value = ControlledEncoding {
            entered: entered_tx,
            release: closed_rx,
        };
        write_before_deadline(&mut stream, &value, deadline).map_err(|e| e.to_string())?;
        read_before_deadline::<u8>(&mut stream, deadline).map_err(|e| e.to_string())?;
        Ok(())
    })();
    let peer = server.join().unwrap();
    peer.unwrap();
    assert!(
        client.is_err(),
        "a connected encoder must observe the peer's closed exchange"
    );
}

#[test]
fn formal_rpc_prepared_encoding_connects_only_after_local_work() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + BOUND;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = std::thread::spawn(move || -> Result<(), String> {
        entered_rx.recv_timeout(BOUND).map_err(|e| e.to_string())?;
        match listener.accept() {
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            _ => return Err("a connection existed while local encoding was blocked".into()),
        }
        release_tx.send(()).map_err(|e| e.to_string())?;
        let mut stream = accept_before(&listener, deadline)?;
        configure(&stream, PEER_READ).map_err(|e| e.to_string())?;
        let request: u8 = read(&mut stream).map_err(|e| e.to_string())?;
        if request != 7 {
            return Err("prepared frame changed the value".into());
        }
        write(&mut stream, &9u8).map_err(|e| e.to_string())
    });
    let client = (|| -> Result<u8, String> {
        let value = ControlledEncoding {
            entered: entered_tx,
            release: release_rx,
        };
        let bytes = serde_cbor::to_vec(&value).map_err(|e| e.to_string())?;
        remaining(deadline).map_err(|e| e.to_string())?;
        let mut stream =
            TcpStream::connect_timeout(&address, remaining(deadline).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        configure(&stream, remaining(deadline).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        // The existing four-byte CBOR framing is decoded by actual wire::read.
        stream
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .map_err(|e| e.to_string())?;
        stream.write_all(&bytes).map_err(|e| e.to_string())?;
        read_before_deadline(&mut stream, deadline).map_err(|e| e.to_string())
    })();
    let peer = server.join().unwrap();
    peer.unwrap();
    assert_eq!(client.unwrap(), 9);
}

#[test]
fn formal_rpc_request_is_encoded_before_connect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let deadline = Instant::now() + BOUND;
    let connections = Arc::new(AtomicUsize::new(0));
    let observed = connections.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let peer = std::thread::spawn(move || -> Result<usize, String> {
        entered_rx.recv_timeout(BOUND).map_err(|e| e.to_string())?;
        let during_encoding = observed.load(Ordering::SeqCst);
        release_tx.send(()).map_err(|e| e.to_string())?;
        let mut stream = accept_before(&listener, deadline)?;
        configure(&stream, BOUND).map_err(|e| e.to_string())?;
        if read::<u8>(&mut stream).map_err(|e| e.to_string())? != 7 {
            return Err("request bytes changed".into());
        }
        write(&mut stream, &9u8).map_err(|e| e.to_string())?;
        Ok(during_encoding)
    });
    let response = (|| -> Result<u8, FormalError> {
        let value = ControlledEncoding {
            entered: entered_tx,
            release: release_rx,
        };
        let mut stream =
            connect_and_write_using(&address, &value, deadline, |address, timeout| {
                connections.fetch_add(1, Ordering::SeqCst);
                TcpStream::connect_timeout(address, timeout)
            })?;
        read_before_deadline(&mut stream, deadline)
    })();
    let during_encoding = peer.join().unwrap().unwrap();
    assert_eq!(response.unwrap(), 9);
    assert_eq!(
        during_encoding, 0,
        "local encoding must finish before the peer read window starts"
    );
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

fn rejected_preparation_does_not_connect(
    value: &impl Serialize,
    deadline: Instant,
    expected: FormalError,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let connections = AtomicUsize::new(0);
    let result = connect_and_write_using(
        &listener.local_addr().unwrap(),
        value,
        deadline,
        |address, timeout| {
            connections.fetch_add(1, Ordering::SeqCst);
            TcpStream::connect_timeout(address, timeout)
        },
    );
    assert_eq!(result.unwrap_err(), expected);
    assert_eq!(connections.load(Ordering::SeqCst), 0);
}

#[test]
fn formal_rpc_request_encoding_error_has_no_connection() {
    struct Bad;
    impl Serialize for Bad {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(S::Error::custom("controlled encoding failure"))
        }
    }
    rejected_preparation_does_not_connect(
        &Bad,
        Instant::now() + BOUND,
        FormalError::Invalid("controlled encoding failure".into()),
    );
}

#[test]
fn formal_rpc_request_oversize_has_no_connection() {
    rejected_preparation_does_not_connect(
        &serde_cbor::Value::Bytes(vec![0; LIMIT]),
        Instant::now() + BOUND,
        FormalError::Invalid("formal RPC bounded frame".into()),
    );
}

#[test]
fn formal_rpc_request_expired_during_encoding_has_no_connection() {
    struct Until<'a>(Instant, &'a AtomicUsize);
    impl Serialize for Until<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            self.1.fetch_add(1, Ordering::SeqCst);
            if let Some(left) = self.0.checked_duration_since(Instant::now()) {
                std::thread::sleep(left);
            }
            serializer.serialize_u8(7)
        }
    }
    let deadline = Instant::now() + Duration::from_millis(30);
    let entered = AtomicUsize::new(0);
    rejected_preparation_does_not_connect(
        &Until(deadline, &entered),
        deadline,
        FormalError::Io("formal RPC frame deadline expired".into()),
    );
    assert_eq!(entered.load(Ordering::SeqCst), 1);
}
