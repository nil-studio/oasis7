#[cfg(target_os = "linux")]
use std::io::Read as _;

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("dev-test-local-serve") {
        #[cfg(target_os = "linux")]
        {
            if args.len() != 9
                || args[1] != "--socket"
                || args[3] != "--profile"
                || args[5] != "--caller-uid"
                || args[7] != "--caller-id"
            {
                return Err("dev-test-local-serve --socket PATH --profile PRIVATE_CONFIG --caller-uid UID --caller-id ID".into());
            }
            let uid = args[6].parse::<u32>().map_err(|_| "invalid caller uid")?;
            return crate::dev_test_local_adapter::serve(
                std::path::Path::new(&args[2]),
                std::path::Path::new(&args[4]),
                uid,
                &args[8],
            );
        }
        #[cfg(not(target_os = "linux"))]
        return Err("local peer authenticated service requires Linux".into());
    }
    if args.len() != 3 {
        return Err(
            "dev-test-serve --profile <private-config> | dev-test-call --socket <approved-socket>"
                .into(),
        );
    }
    match (args[0].as_str(), args[1].as_str()) {
        #[cfg(target_os = "linux")]
        ("dev-test-local-call", "--socket") => {
            let mut body = Vec::new();
            std::io::stdin()
                .take(65537)
                .read_to_end(&mut body)
                .map_err(|_| "local request unavailable")?;
            if body.len() > 65536 {
                return Err("local request too large".into());
            }
            let request: crate::dev_test_local_adapter::LocalRequest =
                serde_json::from_slice(&body).map_err(|_| "invalid local request")?;
            let response = crate::dev_test_local_adapter::call_local(
                std::path::Path::new(&args[2]),
                &request,
            )?;
            crate::device_client::print(&response)
        }
        ("dev-test-serve", "--profile") => {
            let config = crate::dev_test_service::read_private_cli_config(&args[2])?;
            crate::dev_test_service::serve(config)
        }
        ("dev-test-call", "--socket") => {
            let request = crate::dev_test_service::read_cli_request(std::io::stdin())?;
            let response =
                crate::dev_test_service::call_service(std::path::Path::new(&args[2]), &request)?;
            crate::device_client::print(&response)
        }
        _ => Err("unsupported_service_command".into()),
    }
}
