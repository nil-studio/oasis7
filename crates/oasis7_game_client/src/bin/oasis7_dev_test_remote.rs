//! Loopback-only MCP endpoint for a fixed trusted TLS reverse proxy.
#[cfg(unix)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use oasis7_game_client::{
        dev_test_accounts_remote::{handle_http, HttpRequest},
        dev_test_service::{call_service, Config},
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: oasis7_dev_test_remote <private-service-config> <https-origin> <127.0.0.1:port> <max-connections>".into());
    }
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = std::fs::symlink_metadata(&args[1])?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("private_configuration_required".into());
    }
    let config: Config = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    config.validate()?;
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
        let timeout = Some(std::time::Duration::from_millis(config.timeout_ms));
        stream.set_read_timeout(timeout)?;
        stream.set_write_timeout(timeout)?;
        let result = (|| -> Result<(u16, Option<serde_json::Value>), ()> {
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                if header.len() >= 8192 {
                    return Ok((431, None));
                }
                let mut byte = [0];
                stream.read_exact(&mut byte).map_err(|_| ())?;
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
            if size > config.max_request_bytes {
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
            stream.read_exact(&mut body).map_err(|_| ())?;
            Ok(handle_http(
                &config,
                &args[2],
                HttpRequest {
                    method: parts[0],
                    host: fields.get("host").copied().unwrap_or(""),
                    origin: fields.get("origin").copied(),
                    authorization: fields.get("authorization").copied(),
                    protocol_version: fields.get("mcp-protocol-version").copied(),
                    body: &body,
                },
                |request| call_service(&config.socket, &request).map_err(|_| ()),
            ))
        })()
        .unwrap_or((400, None));
        let body = result.1.map(|v| v.to_string()).unwrap_or_default();
        let reply=format!("HTTP/1.1 {} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{}",result.0,body.len(),body);
        let _ = stream.write_all(reply.as_bytes());
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
