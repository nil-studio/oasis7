//! Usage: cargo run -p oasis7_distfs --example storage_inventory -- primary=/path replica=/path
use oasis7_distfs::storage_inventory::inventory_storage;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
fn main() {
    let mut domains = BTreeMap::new();
    for argument in std::env::args().skip(1) {
        let Some((domain, path)) = argument.split_once('=') else {
            eprintln!("expected DOMAIN=PATH");
            std::process::exit(2);
        };
        if domain.is_empty() || path.is_empty() || domains.contains_key(domain) {
            eprintln!("expected a unique nonempty DOMAIN and PATH");
            std::process::exit(2);
        }
        domains.insert(
            domain.to_string(),
            json!({"root": path, "measurement": inventory_storage(Path::new(path))}),
        );
    }
    if domains.is_empty() {
        eprintln!("usage: storage_inventory DOMAIN=PATH [DOMAIN=PATH ...]");
        std::process::exit(2);
    }
    let complete = domains
        .values()
        .all(|value| value["measurement"]["complete"] == true);
    println!("{}", serde_json::to_string_pretty(&json!({
        "version": 1, "complete": complete, "domains": domains,
        "scope": "quiescent local filesystem; domain independence and history completeness unverified; inode blocks do not resolve shared extents or remote billing"
    })).unwrap());
    if !complete {
        std::process::exit(1);
    }
}
