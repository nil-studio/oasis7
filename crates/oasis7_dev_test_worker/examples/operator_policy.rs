//! Public policy only; never accepts a material/credential value schema.
fn main() {
    let result = (|| {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        if args.len() != 1 && !(args.len() == 2 && args[1] == "--bindings") {
            return Err("policy file and optional --bindings required");
        }
        use std::io::Read;
        let metadata = std::fs::symlink_metadata(&args[0]).map_err(|_| "policy unavailable")?;
        if !metadata.is_file() || metadata.len() > 60 * 1024 {
            return Err("policy unavailable");
        }
        let mut raw = String::new();
        std::fs::File::open(&args[0])
            .map_err(|_| "policy unavailable")?
            .take(60 * 1024 + 1)
            .read_to_string(&mut raw)
            .map_err(|_| "policy unavailable")?;
        if raw.len() > 60 * 1024 {
            return Err("policy unavailable");
        }
        if args.len() == 2 {
            let bindings = oasis7_dev_test_worker::prepare_operator_policy_bindings(&raw)?;
            serde_json::to_string(&bindings).map_err(|_| "policy unavailable")
        } else {
            oasis7_dev_test_worker::prepare_operator_policy(&raw)
        }
    })();
    match result {
        Ok(policy) => println!("{policy}"),
        Err(_) => {
            eprintln!("operator_policy_rejected");
            std::process::exit(1);
        }
    }
}
