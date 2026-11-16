//! Loopback-only MCP endpoint for a fixed trusted TLS reverse proxy.
#[cfg(unix)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use oasis7_game_client::{
        dev_test_accounts_remote::HttpRequest, dev_test_remote_business::handle_http,
        dev_test_sessions::ServiceConfig,
    };
    use std::net::TcpListener;
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: oasis7_dev_test_remote <private-service-config> <https-origin> <127.0.0.1:port> <max-connections>".into());
    }
    let config: ServiceConfig = serde_json::from_slice(
        &oasis7_game_client::dev_test_private_config::read_private_config(std::path::Path::new(
            &args[1],
        ))?,
    )?;
    config.service.validate()?;
    let address: std::net::SocketAddr = args[3].parse()?;
    if !address.ip().is_loopback() {
        return Err("loopback_required".into());
    }
    let limit: usize = args[4].parse()?;
    if limit == 0 || limit > 10000 {
        return Err("finite_connection_limit_required".into());
    }
    let listener = TcpListener::bind(address)?;
    eprintln!("remote_endpoint_listening {}", listener.local_addr()?);
    for connection in listener.incoming().take(limit) {
        let mut stream = connection?;
        let config: ServiceConfig = serde_json::from_slice(
            &oasis7_game_client::dev_test_private_config::read_private_config(
                std::path::Path::new(&args[1]),
            )?,
        )?;
        config.service.validate()?;
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(config.service.timeout_ms);
        let result = (|| -> Result<(u16, Option<serde_json::Value>), ()> {
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                if header.len() >= 8192 {
                    return Ok((431, None));
                }
                let mut byte = [0];
                read_before(&mut stream, &mut byte, deadline).map_err(|_| ())?;
                header.push(byte[0]);
            }
            let text = std::str::from_utf8(&header).map_err(|_| ())?;
            let mut lines = text.split("\r\n");
            let parts: Vec<_> = lines.next().ok_or(())?.split(' ').collect();
            if parts.len() != 3 || parts[1] != "/mcp" || parts[2] != "HTTP/1.1" {
                return Ok((400, None));
            }
            let mut fields = std::collections::HashMap::new();
            for line in lines.filter(|line| !line.is_empty()) {
                let (key, value) = line.split_once(':').ok_or(())?;
                let key = key.to_ascii_lowercase();
                if fields.insert(key, value.trim()).is_some() {
                    return Ok((400, None));
                }
            }
            if fields.contains_key("transfer-encoding") {
                return Ok((400, None));
            }
            let size: usize = fields
                .get("content-length")
                .unwrap_or(&"0")
                .parse()
                .map_err(|_| ())?;
            if size > config.service.max_request_bytes {
                return Ok((413, None));
            }
            if parts[0] == "POST"
                && (fields.get("content-type") != Some(&"application/json")
                    || !fields.get("accept").is_some_and(|v| {
                        v.contains("application/json") && v.contains("text/event-stream")
                    }))
            {
                return Ok((400, None));
            }
            let mut body = vec![0; size];
            read_before(&mut stream, &mut body, deadline).map_err(|_| ())?;
            Ok(handle_http(
                &config.service,
                &args[2],
                HttpRequest {
                    method: parts[0],
                    host: fields.get("host").copied().unwrap_or(""),
                    origin: fields.get("origin").copied(),
                    authorization: fields.get("authorization").copied(),
                    protocol_version: fields.get("mcp-protocol-version").copied(),
                    body: &body,
                },
                |body| {
                    oasis7_game_client::dev_test_remote_business::call_backend(
                        &config.service.socket,
                        body,
                        deadline,
                    )
                    .map_err(|_| ())
                },
            ))
        })()
        .unwrap_or((400, None));
        let body = result.1.map(|v| v.to_string()).unwrap_or_default();
        let reply = format!(
            "HTTP/1.1 {} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{}",
            result.0,
            body.len(),
            body
        );
        let _ = write_before(&mut stream, reply.as_bytes(), deadline);
    }
    Ok(())
}
fn main() {
    #[cfg(unix)]
    if run().is_err() {
        eprintln!("remote_endpoint_failed");
        std::process::exit(1)
    }
    #[cfg(not(unix))]
    {
        eprintln!("unix_required");
        std::process::exit(1)
    }
}

#[cfg(unix)]
fn remaining(deadline: std::time::Instant) -> std::io::Result<std::time::Duration> {
    deadline
        .checked_duration_since(std::time::Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "request deadline"))
}
#[cfg(unix)]
fn read_before(
    stream: &mut std::net::TcpStream,
    mut body: &mut [u8],
    deadline: std::time::Instant,
) -> std::io::Result<()> {
    use std::io::Read;
    while !body.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        let n = stream.read(body)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        body = &mut body[n..];
    }
    remaining(deadline)?;
    Ok(())
}
#[cfg(unix)]
fn write_before(
    stream: &mut std::net::TcpStream,
    mut body: &[u8],
    deadline: std::time::Instant,
) -> std::io::Result<()> {
    use std::io::Write;
    while !body.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        let n = stream.write(body)?;
        if n == 0 {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        body = &body[n..];
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod deadline_tests {
    use super::*;
    #[test]
    fn slow_drip_cannot_refresh_absolute_deadline() {
        use std::{
            io::Write,
            net::{TcpListener, TcpStream},
            time::{Duration, Instant},
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let writer = std::thread::spawn(move || {
            for _ in 0..20 {
                if client.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let start = Instant::now();
        let mut body = [0; 20];
        assert!(read_before(&mut server, &mut body, start + Duration::from_millis(100)).is_err());
        assert!(start.elapsed() < Duration::from_millis(300));
        drop(server);
        writer.join().unwrap();
    }
}
