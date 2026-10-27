use super::*;
use std::{io::Read, net::TcpStream};
pub(super) fn serve(
    listener: TcpListener,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    root: &Path,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    stop: &AtomicBool,
    dropped: &AtomicBool,
) {
    serve_with_private(listener, node, root, signer, stop, dropped, None)
}
pub(super) fn serve_with_private(
    listener: TcpListener,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    root: &Path,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    stop: &AtomicBool,
    dropped: &AtomicBool,
    private: Option<&oasis7::world_service::private_goal_store::PrivateGoalStore>,
) {
    while !stop.load(Ordering::SeqCst) {
        let (mut stream, _) = match listener.accept() {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => panic!("{e}"),
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            if stream.read_exact(&mut byte).is_err() {
                break;
            }
            bytes.push(byte[0]);
            assert!(bytes.len() < 65536);
        }
        let headers = std::str::from_utf8(&bytes).unwrap();
        let mut first = headers.split_whitespace();
        let method = first.next().unwrap().to_string();
        let path = first.next().unwrap().to_string();
        let length = headers
            .split("\r\n")
            .filter_map(|line| line.split_once(':'))
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
            .unwrap_or(0);
        assert!(length <= 256 * 1024);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).unwrap();
        bytes.extend(body);
        if crate::world_service_api::private_goal::maybe_handle(
            &mut stream,
            &bytes,
            node,
            &method,
            &path,
            "w1",
            &root.join("world"),
            &root.join("records"),
            &root.join("store"),
            signer,
            private,
        )
        .unwrap()
        {
            continue;
        }
        if path == SUBMIT_PATH && !dropped.swap(true, Ordering::SeqCst) {
            // Run the genuine handler/admission, but discard its response at the transport boundary.
            let sink = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut writer = TcpStream::connect(sink.local_addr().unwrap()).unwrap();
            let (_reader, _) = sink.accept().unwrap();
            assert!(
                crate::world_service_api::maybe_handle(
                    &mut writer,
                    &bytes,
                    node,
                    &method,
                    &path,
                    "w1",
                    &root.join("world"),
                    &root.join("records"),
                    &root.join("store"),
                    signer
                )
                .unwrap()
            );
            drop(stream);
            continue;
        }
        assert!(
            crate::world_service_api::maybe_handle(
                &mut stream,
                &bytes,
                node,
                &method,
                &path,
                "w1",
                &root.join("world"),
                &root.join("records"),
                &root.join("store"),
                signer
            )
            .unwrap()
        );
    }
}
