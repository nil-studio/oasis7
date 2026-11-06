use std::io::{Read, Write};
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err(
            "dev-test-serve --profile <private-config> | dev-test-call --socket <approved-socket>"
                .into(),
        );
    }
    match (args[0].as_str(), args[1].as_str()) {
        ("dev-test-serve", "--profile") => {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata =
                std::fs::symlink_metadata(&args[2]).map_err(|_| "configuration_unavailable")?;
            if !metadata.is_file()
                || metadata.permissions().mode() & 0o077 != 0
                || metadata.uid() != unsafe { libc::geteuid() }
            {
                return Err("private_configuration_required".into());
            }
            let body = std::fs::read(&args[2]).map_err(|_| "configuration_unavailable")?;
            let config =
                serde_json::from_slice(&body).map_err(|_| "invalid_service_configuration")?;
            crate::dev_test_service::serve(config)
        }
        ("dev-test-call", "--socket") => {
            let mut body = Vec::new();
            std::io::stdin()
                .take(65537)
                .read_to_end(&mut body)
                .map_err(|_| "request_unavailable")?;
            if body.len() > 65536 {
                return Err("request_too_large".into());
            }
            let mut stream = std::os::unix::net::UnixStream::connect(&args[2])
                .map_err(|_| "service_unavailable")?;
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(30)))
                .map_err(|_| "timeout_configuration_failed")?;
            stream.write_all(&body).map_err(|_| "request_failed")?;
            stream
                .shutdown(std::net::Shutdown::Write)
                .map_err(|_| "request_failed")?;
            let mut response = Vec::new();
            stream
                .take(65537)
                .read_to_end(&mut response)
                .map_err(|_| "response_unavailable")?;
            let response: crate::dev_test_service::Response =
                serde_json::from_slice(&response).map_err(|_| "invalid_response")?;
            crate::device_client::print(&response)
        }
        _ => Err("unsupported_service_command".into()),
    }
}
