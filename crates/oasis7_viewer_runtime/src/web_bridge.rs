use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use oasis7::world_service::client::RemoteWorldServiceClient;
use tracing::Level;
use tungstenite::error::ProtocolError;
use tungstenite::handshake::HandshakeError;
use tungstenite::protocol::Message;
#[cfg(test)]
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Error as WsError, accept};

use oasis7::observability::emit_stderr_or_event;

#[derive(Debug, Clone)]
pub struct ViewerWebBridgeConfig {
    pub bind_addr: String,
    pub upstream_addr: String,
}

impl ViewerWebBridgeConfig {
    pub fn new(bind_addr: impl Into<String>, upstream_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            upstream_addr: upstream_addr.into(),
        }
    }
}

#[derive(Debug)]
pub enum ViewerWebBridgeError {
    Io(io::Error),
    WebSocket(WsError),
}

impl From<io::Error> for ViewerWebBridgeError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<WsError> for ViewerWebBridgeError {
    fn from(err: WsError) -> Self {
        Self::WebSocket(err)
    }
}

pub struct ViewerWebBridge {
    config: ViewerWebBridgeConfig,
    game_read_client: Option<RemoteWorldServiceClient>,
    game_read_enabled: bool,
}

impl ViewerWebBridge {
    pub fn new(config: ViewerWebBridgeConfig) -> Self {
        Self {
            config,
            game_read_client: None,
            game_read_enabled: false,
        }
    }

    pub fn with_game_read_api(mut self, client: RemoteWorldServiceClient) -> Result<Self, String> {
        let bind: std::net::SocketAddr = self
            .config
            .bind_addr
            .parse()
            .map_err(|_| "Game API bootstrap requires a loopback IP bind")?;
        if !bind.ip().is_loopback() {
            return Err("Game API bootstrap requires a loopback IP bind".into());
        }
        let endpoint = url::Url::parse(&client.config().endpoint)
            .map_err(|_| "invalid World Service endpoint")?;
        if endpoint.scheme() != "https"
            && !endpoint.host_str().is_some_and(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
            })
        {
            return Err("Game API World Service requires HTTPS or a loopback IP endpoint".into());
        }
        let safe_id = |id: &str| {
            !id.is_empty()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.~".contains(&b))
        };
        if !safe_id(&client.config().expected_world.world_id)
            || !client
                .config()
                .scope_id
                .strip_prefix("agent:")
                .is_some_and(safe_id)
        {
            return Err(
                "Game API bootstrap requires URI-safe world/Agent identifiers and an agent scope"
                    .into(),
            );
        }
        self.game_read_client = Some(client);
        self.game_read_enabled = true;
        Ok(self)
    }

    pub fn run(&self) -> Result<(), ViewerWebBridgeError> {
        let listener = TcpListener::bind(&self.config.bind_addr)?;
        for incoming in listener.incoming() {
            let stream = match incoming {
                Ok(stream) => stream,
                Err(err) => {
                    let stderr_message = format!("viewer web bridge accept error: {err:?}");
                    emit_stderr_or_event(
                        Level::WARN,
                        stderr_message.as_str(),
                        "viewer web bridge accept failed",
                    );
                    continue;
                }
            };
            let config = self.config.clone();
            let game_read_client = self.game_read_client.clone();
            let game_read_enabled = self.game_read_enabled;
            thread::spawn(move || {
                let bridge = ViewerWebBridge {
                    config,
                    game_read_client,
                    game_read_enabled,
                };
                if let Err(err) = bridge.serve_stream(stream)
                    && !is_expected_bridge_disconnect(&err)
                {
                    let stderr_message = format!("viewer web bridge error: {err:?}");
                    emit_stderr_or_event(
                        Level::WARN,
                        stderr_message.as_str(),
                        "viewer web bridge session failed",
                    );
                }
            });
        }
        Ok(())
    }

    fn serve_stream(&self, mut stream: TcpStream) -> Result<(), ViewerWebBridgeError> {
        const WS_READ_TIMEOUT: Duration = Duration::from_millis(20);

        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        if self.game_read_enabled
            && super::game_read_api::serve_if_game_request(
                &mut stream,
                self.game_read_client.as_ref(),
            )?
        {
            return Ok(());
        }
        let mut websocket = accept(stream).map_err(map_handshake_error)?;
        websocket
            .get_mut()
            .set_read_timeout(Some(WS_READ_TIMEOUT))?;

        let upstream = TcpStream::connect(&self.config.upstream_addr)?;
        upstream.set_nodelay(true)?;
        let upstream_reader = upstream.try_clone()?;
        upstream_reader.set_nonblocking(true)?;
        let upstream_shutdown = upstream.try_clone()?;
        let mut upstream_writer = BufWriter::new(upstream);
        let mut upstream_reader = BufReader::new(upstream_reader);
        let mut upstream_line = String::new();

        let session_result = (|| -> Result<(), ViewerWebBridgeError> {
            loop {
                match websocket.read() {
                    Ok(message) => {
                        if !handle_ws_message(message, &mut upstream_writer, &mut websocket)? {
                            break;
                        }
                        if !drain_upstream_lines(
                            &mut upstream_reader,
                            &mut upstream_line,
                            &mut websocket,
                        )? {
                            break;
                        }
                    }
                    Err(WsError::Io(err)) if err.kind() == io::ErrorKind::WouldBlock => {}
                    Err(WsError::Io(err)) if err.kind() == io::ErrorKind::TimedOut => {}
                    Err(WsError::ConnectionClosed) | Err(WsError::AlreadyClosed) => break,
                    Err(err) => return Err(err.into()),
                }

                if !drain_upstream_lines(&mut upstream_reader, &mut upstream_line, &mut websocket)?
                {
                    break;
                }
            }
            Ok(())
        })();

        // Ensure the cloned reader side is also released; otherwise upstream may keep the
        // first session alive and block subsequent reconnects.
        let _ = upstream_shutdown.shutdown(Shutdown::Both);

        session_result
    }
}

fn handle_ws_message(
    message: Message,
    upstream_writer: &mut BufWriter<TcpStream>,
    websocket: &mut tungstenite::WebSocket<TcpStream>,
) -> Result<bool, ViewerWebBridgeError> {
    match message {
        Message::Text(text) => {
            upstream_writer.write_all(text.as_bytes())?;
            upstream_writer.write_all(b"\n")?;
            upstream_writer.flush()?;
            Ok(true)
        }
        Message::Binary(binary) => {
            if let Ok(text) = String::from_utf8(binary.to_vec()) {
                upstream_writer.write_all(text.as_bytes())?;
                upstream_writer.write_all(b"\n")?;
                upstream_writer.flush()?;
            }
            Ok(true)
        }
        Message::Ping(payload) => {
            websocket.send(Message::Pong(payload))?;
            Ok(true)
        }
        Message::Close(frame) => {
            match websocket.close(frame) {
                Ok(()) | Err(WsError::ConnectionClosed) | Err(WsError::AlreadyClosed) => {}
                Err(err) => return Err(err.into()),
            }
            Ok(false)
        }
        Message::Pong(_) => Ok(true),
        Message::Frame(_) => Ok(true),
    }
}

fn map_handshake_error(
    err: HandshakeError<
        tungstenite::ServerHandshake<TcpStream, tungstenite::handshake::server::NoCallback>,
    >,
) -> ViewerWebBridgeError {
    match err {
        HandshakeError::Failure(error) => ViewerWebBridgeError::WebSocket(error),
        HandshakeError::Interrupted(_) => ViewerWebBridgeError::Io(io::Error::new(
            io::ErrorKind::Interrupted,
            "websocket handshake interrupted",
        )),
    }
}

fn is_expected_bridge_disconnect(err: &ViewerWebBridgeError) -> bool {
    match err {
        ViewerWebBridgeError::Io(io_err) => matches!(
            io_err.kind(),
            io::ErrorKind::ConnectionReset
                | io::ErrorKind::ConnectionAborted
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::UnexpectedEof
                | io::ErrorKind::NotConnected
        ),
        ViewerWebBridgeError::WebSocket(ws_err) => match ws_err {
            WsError::ConnectionClosed | WsError::AlreadyClosed => true,
            WsError::Protocol(ProtocolError::HandshakeIncomplete) => true,
            WsError::Io(io_err) => matches!(
                io_err.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::UnexpectedEof
                    | io::ErrorKind::NotConnected
            ),
            _ => false,
        },
    }
}

fn drain_upstream_lines(
    reader: &mut BufReader<TcpStream>,
    line: &mut String,
    websocket: &mut tungstenite::WebSocket<TcpStream>,
) -> Result<bool, ViewerWebBridgeError> {
    loop {
        match reader.read_line(line) {
            Ok(0) => return Ok(false),
            Ok(_) => {
                let text = line.trim();
                if text.is_empty() {
                    line.clear();
                    continue;
                }
                websocket.send(Message::Text(text.to_string().into()))?;
                line.clear();
            }
            Err(err)
                if err.kind() == io::ErrorKind::WouldBlock
                    || err.kind() == io::ErrorKind::TimedOut =>
            {
                return Ok(true);
            }
            Err(err) => return Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7::viewer::{
        VIEWER_PROTOCOL_VERSION, ViewerRequest, ViewerResponse, ViewerRuntimeLiveServer,
        ViewerRuntimeLiveServerConfig, ViewerStream,
    };
    use oasis7_simulator::WorldScenario;
    use std::sync::mpsc;
    use std::time::Instant;
    use tungstenite::connect;

    #[test]
    fn bridge_config_new_sets_defaults() {
        let config = ViewerWebBridgeConfig::new("127.0.0.1:5011", "127.0.0.1:5010");
        assert_eq!(config.bind_addr, "127.0.0.1:5011");
        assert_eq!(config.upstream_addr, "127.0.0.1:5010");
    }

    #[test]
    fn bridge_allows_reconnect_after_websocket_refresh() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
        let upstream_addr = upstream_listener.local_addr().expect("upstream addr");
        let (upstream_tx, upstream_rx) = mpsc::channel::<String>();

        let upstream_thread = thread::spawn(move || {
            let stream_one = accept_with_timeout(&upstream_listener, Duration::from_secs(2))
                .expect("accept first upstream session");
            let mut reader_one = BufReader::new(stream_one);
            let mut line = String::new();
            reader_one
                .read_line(&mut line)
                .expect("read first line from first session");
            upstream_tx
                .send(format!("session1:{}", line.trim()))
                .expect("send session1 line");

            reader_one
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(50)))
                .expect("set timeout on first session");
            let close_wait_start = Instant::now();
            let first_closed = loop {
                line.clear();
                match reader_one.read_line(&mut line) {
                    Ok(0) => break true,
                    Ok(_) => break false,
                    Err(err)
                        if err.kind() == io::ErrorKind::WouldBlock
                            || err.kind() == io::ErrorKind::TimedOut =>
                    {
                        if close_wait_start.elapsed() >= Duration::from_secs(2) {
                            break false;
                        }
                    }
                    Err(_) => break false,
                }
            };
            upstream_tx
                .send(format!("session1_closed:{first_closed}"))
                .expect("send close state");

            let stream_two = accept_with_timeout(&upstream_listener, Duration::from_secs(2))
                .expect("accept second upstream session");
            let mut reader_two = BufReader::new(stream_two);
            line.clear();
            reader_two
                .read_line(&mut line)
                .expect("read first line from second session");
            upstream_tx
                .send(format!("session2:{}", line.trim()))
                .expect("send session2 line");
        });

        let bridge = ViewerWebBridge::new(ViewerWebBridgeConfig::new(
            "127.0.0.1:0",
            upstream_addr.to_string(),
        ));

        run_ws_session(&bridge, "first-request");
        assert_eq!(
            upstream_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("session1 message"),
            "session1:first-request"
        );
        assert_eq!(
            upstream_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("session1 close state"),
            "session1_closed:true"
        );

        run_ws_session(&bridge, "second-request");
        assert_eq!(
            upstream_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("session2 message"),
            "session2:second-request"
        );

        upstream_thread.join().expect("join upstream thread");
    }

    #[test]
    fn expected_bridge_disconnect_classifies_handshake_and_reset_noise() {
        assert!(is_expected_bridge_disconnect(
            &ViewerWebBridgeError::WebSocket(WsError::Protocol(ProtocolError::HandshakeIncomplete),)
        ));
        assert!(is_expected_bridge_disconnect(&ViewerWebBridgeError::Io(
            io::Error::new(io::ErrorKind::ConnectionReset, "reset",)
        )));
        assert!(!is_expected_bridge_disconnect(&ViewerWebBridgeError::Io(
            io::Error::new(io::ErrorKind::AddrInUse, "real failure",)
        )));
    }

    #[test]
    fn bridge_run_accepts_second_websocket_while_first_stays_open() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
        let upstream_addr = upstream_listener.local_addr().expect("upstream addr");
        let bridge_listener = TcpListener::bind("127.0.0.1:0").expect("bind bridge");
        let bridge_addr = bridge_listener.local_addr().expect("bridge addr");
        drop(bridge_listener);

        let (upstream_tx, upstream_rx) = mpsc::channel::<String>();
        let (release_first_tx, release_first_rx) = mpsc::channel::<()>();

        let upstream_thread = thread::spawn(move || {
            let stream_one = accept_with_timeout(&upstream_listener, Duration::from_secs(2))
                .expect("accept first upstream session");
            let mut reader_one = BufReader::new(stream_one);
            let mut line = String::new();
            reader_one
                .read_line(&mut line)
                .expect("read first line from first session");
            upstream_tx
                .send(format!("session1:{}", line.trim()))
                .expect("send session1 line");

            let stream_two = accept_with_timeout(&upstream_listener, Duration::from_secs(2))
                .expect("accept second upstream session");
            let mut reader_two = BufReader::new(stream_two);
            line.clear();
            reader_two
                .read_line(&mut line)
                .expect("read first line from second session");
            upstream_tx
                .send(format!("session2:{}", line.trim()))
                .expect("send session2 line");
        });

        let bridge_addr_string = bridge_addr.to_string();
        let upstream_addr_string = upstream_addr.to_string();
        thread::spawn(move || {
            let bridge = ViewerWebBridge::new(ViewerWebBridgeConfig::new(
                bridge_addr_string,
                upstream_addr_string,
            ));
            bridge.run().expect("run bridge");
        });
        wait_for_listener(bridge_addr);

        let first_client_release_rx = release_first_rx;
        let first_client = thread::spawn(move || {
            let url = format!("ws://{bridge_addr}");
            let (mut client, _) = connect(url.as_str()).expect("connect first ws client");
            client
                .send(Message::Text("first-request".to_string().into()))
                .expect("send first ws payload");
            first_client_release_rx
                .recv()
                .expect("release first ws client");
            client.close(None).expect("close first ws client");
        });

        assert_eq!(
            upstream_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("session1 message"),
            "session1:first-request"
        );

        let second_client = thread::spawn(move || {
            let url = format!("ws://{bridge_addr}");
            let (mut client, _) = connect(url.as_str()).expect("connect second ws client");
            client
                .send(Message::Text("second-request".to_string().into()))
                .expect("send second ws payload");
            client.close(None).expect("close second ws client");
        });

        assert_eq!(
            upstream_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("session2 message"),
            "session2:second-request"
        );

        release_first_tx.send(()).expect("release first client");
        first_client.join().expect("join first client");
        second_client.join().expect("join second client");
        upstream_thread.join().expect("join upstream thread");
    }

    #[test]
    fn bridge_forwards_viewer_protocol_to_live_runtime_entrypoint() {
        // PG5 boundary contract: the Viewer-facing WebSocket entrypoint must carry
        // the same protocol request into the production live runtime handler. This
        // test deliberately starts both production servers and drives the wire DTOs
        // through the bridge; no test-only runtime model or fake upstream is involved.
        let runtime_listener = TcpListener::bind("127.0.0.1:0").expect("bind runtime listener");
        let runtime_addr = runtime_listener.local_addr().expect("runtime address");
        drop(runtime_listener);
        let runtime_addr_string = runtime_addr.to_string();
        thread::spawn(move || {
            let server = ViewerRuntimeLiveServer::new(
                ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                    .with_bind_addr(runtime_addr_string),
            )
            .expect("create live runtime server");
            server.run().expect("run live runtime server");
        });
        wait_for_listener(runtime_addr);

        let bridge_listener = TcpListener::bind("127.0.0.1:0").expect("bind bridge listener");
        let bridge_addr = bridge_listener.local_addr().expect("bridge address");
        drop(bridge_listener);
        let bridge_addr_string = bridge_addr.to_string();
        let runtime_addr_string = runtime_addr.to_string();
        thread::spawn(move || {
            let bridge = ViewerWebBridge::new(ViewerWebBridgeConfig::new(
                bridge_addr_string,
                runtime_addr_string,
            ));
            bridge.run().expect("run viewer web bridge");
        });
        wait_for_listener(bridge_addr);

        let url = format!("ws://{bridge_addr}");
        let (mut client, _) = connect(url.as_str()).expect("connect viewer websocket");
        match client.get_mut() {
            MaybeTlsStream::Plain(stream) => stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("set websocket read timeout"),
            _ => panic!("test uses a plain ws:// client"),
        }

        let send = |client: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
                    request: ViewerRequest| {
            let payload = serde_json::to_string(&request).expect("encode viewer request");
            client
                .send(Message::Text(payload.into()))
                .expect("send viewer request through bridge");
        };
        let read_response =
            |client: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>| -> ViewerResponse {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for live runtime response through bridge"
                    );
                    match client
                        .read()
                        .expect("read live runtime response through bridge")
                    {
                        Message::Text(text) => {
                            return serde_json::from_str(text.as_ref())
                                .expect("decode live runtime response through bridge");
                        }
                        Message::Binary(bytes) => {
                            return serde_json::from_slice(bytes.as_ref())
                                .expect("decode binary live runtime response through bridge");
                        }
                        Message::Ping(payload) => {
                            client
                                .send(Message::Pong(payload))
                                .expect("reply to websocket ping");
                        }
                        Message::Pong(_) => {}
                        Message::Close(frame) => {
                            panic!("live runtime bridge closed before response: {frame:?}")
                        }
                        Message::Frame(_) => {}
                    }
                }
            };

        send(
            &mut client,
            ViewerRequest::HelloV2 {
                client: "pg5-boundary-test".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
                capabilities: Vec::new(),
            },
        );
        let hello = read_response(&mut client);
        match hello {
            ViewerResponse::HelloAck {
                control_profile,
                version,
                ..
            } => {
                assert_eq!(control_profile, oasis7::viewer::ViewerControlProfile::Live);
                assert_eq!(version, VIEWER_PROTOCOL_VERSION);
            }
            other => panic!("expected live hello ack through bridge, got {other:?}"),
        }

        send(
            &mut client,
            ViewerRequest::Subscribe {
                streams: vec![ViewerStream::Snapshot],
                event_kinds: Vec::new(),
            },
        );
        send(&mut client, ViewerRequest::RequestSnapshot);
        let initial_snapshot = loop {
            match read_response(&mut client) {
                ViewerResponse::Snapshot { snapshot } => break snapshot,
                ViewerResponse::AuthoritativeRecoveryAck { .. } => continue,
                other => panic!("expected snapshot through bridge, got {other:?}"),
            }
        };
        assert!(
            initial_snapshot.runtime_snapshot.is_some(),
            "live runtime snapshot must cross the Viewer bridge"
        );
        // Snapshot sync also emits recovery metadata on the live protocol. Consume
        // that production response before closing the Viewer session so the
        // bridge exercises the complete initial sync response sequence.
        loop {
            if matches!(
                read_response(&mut client),
                ViewerResponse::AuthoritativeRecoveryAck { .. }
            ) {
                break;
            }
        }
        client.close(None).expect("close viewer websocket");
    }

    #[test]
    fn bridge_preserves_split_upstream_lines() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").expect("bind upstream");
        let upstream_addr = upstream_listener.local_addr().expect("upstream addr");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ws listener");
        let ws_addr = listener.local_addr().expect("ws addr");
        let (release_upstream_tx, release_upstream_rx) = mpsc::channel::<()>();

        let upstream_thread = thread::spawn(move || {
            let mut stream = accept_with_timeout(&upstream_listener, Duration::from_secs(2))
                .expect("accept upstream session");
            stream
                .write_all(br#"{"kind":"split""#)
                .expect("write part one");
            thread::sleep(Duration::from_millis(60));
            stream.write_all(br#","ok":true}"#).expect("write part two");
            stream.write_all(b"\n").expect("write newline");
            release_upstream_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("release upstream");
        });

        let client_thread = thread::spawn(move || {
            let url = format!("ws://{ws_addr}");
            let (mut client, _) = connect(url.as_str()).expect("connect ws client");
            match client.get_mut() {
                MaybeTlsStream::Plain(stream) => stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("set client read timeout"),
                _ => panic!("test uses a plain ws:// client"),
            }
            client
                .send(Message::Text("hello-upstream".to_string().into()))
                .expect("send ws payload");
            match client.read().expect("read split upstream line") {
                Message::Text(text) => assert_eq!(text.as_str(), r#"{"kind":"split","ok":true}"#),
                other => panic!("expected text message, got {other:?}"),
            }
            release_upstream_tx.send(()).expect("release upstream");
            client.close(None).expect("close ws client");
        });

        let bridge = ViewerWebBridge::new(ViewerWebBridgeConfig::new(
            "127.0.0.1:0",
            upstream_addr.to_string(),
        ));
        let stream = accept_with_timeout(&listener, Duration::from_secs(2))
            .expect("accept websocket stream");
        if let Err(err) = bridge.serve_stream(stream) {
            assert!(is_expected_bridge_disconnect(&err), "{err:?}");
        }
        client_thread.join().expect("join client thread");
        upstream_thread.join().expect("join upstream thread");
    }

    fn run_ws_session(bridge: &ViewerWebBridge, payload: &str) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ws listener");
        let ws_addr = listener.local_addr().expect("ws addr");
        let payload = payload.to_string();

        let client_thread = thread::spawn(move || {
            let url = format!("ws://{ws_addr}");
            let (mut client, _) = connect(url.as_str()).expect("connect ws client");
            client
                .send(Message::Text(payload.into()))
                .expect("send ws payload");
            client.close(None).expect("close ws client");
        });

        let stream = accept_with_timeout(&listener, Duration::from_secs(2))
            .expect("accept websocket stream");
        bridge.serve_stream(stream).expect("serve websocket stream");
        client_thread.join().expect("join ws client thread");
    }

    fn accept_with_timeout(listener: &TcpListener, timeout: Duration) -> io::Result<TcpStream> {
        listener.set_nonblocking(true)?;
        let start = Instant::now();
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false)?;
                    listener.set_nonblocking(false)?;
                    return Ok(stream);
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    if start.elapsed() >= timeout {
                        listener.set_nonblocking(false)?;
                        return Err(io::Error::new(io::ErrorKind::TimedOut, "accept timed out"));
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(err) => {
                    listener.set_nonblocking(false)?;
                    return Err(err);
                }
            }
        }
    }

    fn wait_for_listener(addr: std::net::SocketAddr) {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if TcpStream::connect(addr).is_ok() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("listener did not become ready at {addr}");
    }
}
