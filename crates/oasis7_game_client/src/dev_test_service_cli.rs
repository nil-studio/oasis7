pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err(
            "dev-test-serve --profile <private-config> | dev-test-call --socket <approved-socket>"
                .into(),
        );
    }
    match (args[0].as_str(), args[1].as_str()) {
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
