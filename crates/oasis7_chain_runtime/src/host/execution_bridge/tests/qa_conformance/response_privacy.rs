//! Fault real fixed-generation records; public failures must not reveal node paths.
use super::*;
#[test]
fn real_tcp_missing_or_tampered_fixed_record_has_private_stable_failure() {
    let mut leaked = Vec::new();
    for fault in ["missing", "tampered"] {
        let fixture = Fixture::with_controlled_commits(true);
        let baseline = fixture.client.read_view(fixture.view(None)).unwrap();
        let mut request = fixture.view(None);
        request.fixed_commit = Some(baseline.version().commit.clone());
        let path = execution_bridge_record_path(
            &fixture.root.join("records"),
            baseline.version().commit.position,
        );
        let original = fs::read(&path).unwrap();
        if fault == "missing" {
            fs::remove_file(&path).unwrap();
        } else {
            fs::write(&path, b"{tampered-record").unwrap();
        }
        let signed = sign_read_request("/v1/world/view", request, &fixture.owner).unwrap();
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap()
            .post(format!(
                "{}/v1/world/view",
                fixture.client.config().endpoint
            ))
            .json(&signed)
            .send()
            .unwrap();
        let status = response.status().as_u16();
        let retry = response.headers().contains_key("retry-after");
        let error: serde_json::Value = response.json().unwrap();
        fs::write(&path, &original).unwrap();
        assert_eq!(status, 503);
        assert!(!retry);
        assert_eq!(error["error"], "world_service_unavailable");
        assert!(error.get("signature_hex").is_none() && error.get("payload").is_none());
        let text = error.to_string();
        let exposes_path =
            text.contains(fixture.root.to_str().unwrap()) || text.contains(path.to_str().unwrap());
        println!(
            "fixed_record_fault_privacy fault={fault} status={status} path_exposed={exposes_path} retry_after={retry} signed_assertion=false"
        );
        if exposes_path {
            leaked.push(fault);
        }
    }
    assert!(
        leaked.is_empty(),
        "public fixed-record failures exposed internal paths for {leaked:?}"
    );
    println!("PRE2_FIXED_RECORD_FAILURE_PRIVACY_PASSED");
}
