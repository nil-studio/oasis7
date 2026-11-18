use std::time::Duration;

/// Direct HTTP policy shared by formal view and full-sync material readers.
///
/// This preserves encoded response bytes and leaves attempts and provider
/// selection to the owning operation. It does not supply authorization, a
/// cumulative operation budget, or a bound on response/header bytes.
pub fn formal_read_http_client(
    timeout: Duration,
) -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        process::Command,
        thread,
        time::Instant,
    };

    const CHILD: &str = "OASIS7_FORMAL_HTTP_PROXY_TEST_CHILD";
    const TIMEOUT: Duration = Duration::from_secs(10);

    fn respond(listener: TcpListener, headers: String, body: Vec<u8>) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            let deadline = Instant::now() + TIMEOUT;
            let mut stream: TcpStream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "HTTP fixture was not contacted");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            };
            stream.set_read_timeout(Some(TIMEOUT)).unwrap();
            stream.set_write_timeout(Some(TIMEOUT)).unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                assert!(request.len() < 16 * 1024);
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let headers_text = std::str::from_utf8(&request).unwrap();
            let length = headers_text
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            assert!(length <= 64 * 1024);
            stream.read_exact(&mut vec![0; length]).unwrap();
            write!(
                stream,
                "{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        })
    }

    #[test]
    fn formal_http_ignores_environment_proxy() {
        with_environment_proxy(
            "formal_http::tests::formal_http_ignores_environment_proxy",
            "HTTP/1.1 200 OK\r\n",
            |endpoint| {
                let body = formal_read_http_client(TIMEOUT)
                    .unwrap()
                    .get(endpoint)
                    .send()
                    .unwrap()
                    .bytes()
                    .unwrap();
                assert_eq!(body.as_ref(), b"direct");
            },
        );
    }

    pub(crate) fn with_environment_proxy(
        test_name: &str,
        status_line: &str,
        exercise: impl FnOnce(String),
    ) {
        if std::env::var_os(CHILD).is_some() {
            exercise(std::env::var("OASIS7_FORMAL_HTTP_TEST_ENDPOINT").unwrap());
            return;
        }
        let origin = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", origin.local_addr().unwrap());
        let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
        let origin_task = respond(origin, status_line.into(), b"direct".to_vec());
        // Only the child's environment changes, avoiding unsafe process-global
        // environment mutation while other tests may be performing HTTP I/O.
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test_name])
            .env(CHILD, "1")
            .env("OASIS7_FORMAL_HTTP_TEST_ENDPOINT", endpoint)
            .env("HTTP_PROXY", &proxy_url)
            .env("http_proxy", &proxy_url)
            .env("HTTPS_PROXY", &proxy_url)
            .env("https_proxy", &proxy_url)
            .env("ALL_PROXY", &proxy_url)
            .env("all_proxy", &proxy_url)
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .status()
            .unwrap();
        origin_task.join().unwrap();
        proxy.set_nonblocking(true).unwrap();
        assert!(
            matches!(proxy.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        assert!(status.success());
    }

    #[test]
    fn formal_http_preserves_encoded_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        // Not a gzip stream: automatic decoding must not even be attempted.
        let encoded = b"encoded-not-gzip";
        let task = respond(
            listener,
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n".into(),
            encoded.to_vec(),
        );
        let response = formal_read_http_client(TIMEOUT)
            .unwrap()
            .get(endpoint)
            .send()
            .unwrap();
        assert_eq!(response.headers()["content-encoding"], "gzip");
        assert_eq!(response.bytes().unwrap().as_ref(), encoded);
        task.join().unwrap();
    }

    #[test]
    fn formal_http_does_not_follow_redirect() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/", listener.local_addr().unwrap());
        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        let task = respond(
            listener,
            format!(
                "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/\r\n",
                target.local_addr().unwrap()
            ),
            Vec::new(),
        );
        let response = formal_read_http_client(TIMEOUT)
            .unwrap()
            .get(endpoint)
            .send()
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        task.join().unwrap();
        target.set_nonblocking(true).unwrap();
        assert!(
            matches!(target.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
}
