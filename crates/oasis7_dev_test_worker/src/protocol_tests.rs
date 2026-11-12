use crate::protocol::{self, Request};
use serde_json::{Value, json};
fn call(arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"dev_test_accounts_call","arguments":arguments}})
}
fn rejected(value: Value, expected: i64) {
    let Request::Metadata(Some(reply)) = protocol::parse(&value.to_string()) else {
        panic!("expected rejection");
    };
    assert_eq!(reply["error"]["code"], expected);
}
#[test]
fn metadata_and_notification_never_become_business() {
    for method in ["ping", "tools/list"] {
        assert!(matches!(
            protocol::parse(&json!({"jsonrpc":"2.0","id":"id","method":method}).to_string()),
            Request::Metadata(Some(_))
        ));
    }
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":protocol::VERSION,"capabilities":{},"clientInfo":{"name":"tester","version":"1"}}});
    assert!(matches!(
        protocol::parse(&init.to_string()),
        Request::Metadata(Some(_))
    ));
    assert!(matches!(
        protocol::parse(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
        Request::Metadata(None)
    ));
    rejected(
        json!({"jsonrpc":"2.0","method":"tools/call","params":{}}),
        -32600,
    );
    let mut bad = init;
    bad["params"]["protocolVersion"] = json!("old");
    rejected(bad, -32602);
}
#[test]
fn closed_business_rejects_credentials_paths_and_extra_context() {
    let baseline = json!({"method":"accounts"});
    assert!(matches!(
        protocol::parse(&call(baseline.clone()).to_string()),
        Request::Business { .. }
    ));
    for field in [
        "credential",
        "token",
        "host_path",
        "socket",
        "endpoint",
        "extraContext",
    ] {
        let mut args = baseline.clone();
        args[field] = json!("/private/host");
        rejected(call(args), -32602);
    }
    for field in ["extraContext", "credential", "host_path"] {
        let mut envelope = call(baseline.clone());
        envelope["params"][field] = json!("x");
        rejected(envelope, -32602);
    }
    let mut unknown = call(baseline);
    unknown["unknown"] = json!(true);
    rejected(unknown, -32600);
    rejected(call(json!({"method":"shell"})), -32602);
}
#[test]
fn metadata_is_not_forwarded_to_model_arguments() {
    let mut envelope = call(json!({"method":"accounts"}));
    envelope["params"]["_meta"] =
        json!({"progressToken":"progress","extraContext":{"credential":"ignored"}});
    let Request::Business { id, body } = protocol::parse(&envelope.to_string()) else {
        panic!("business");
    };
    assert_eq!(id, json!(7));
    assert_eq!(
        serde_json::to_value(body).unwrap(),
        json!({"method":"accounts"})
    );
    envelope["params"]["_meta"]["progressToken"] = json!({});
    rejected(envelope, -32602);
}
#[test]
fn malformed_ids_and_unknown_envelopes_fail_closed() {
    for id in [Value::Null, json!(true), json!([]), json!({}), json!(1.5)] {
        let mut v = call(json!({"method":"accounts"}));
        v["id"] = id;
        rejected(v, -32600);
    }
    for raw in ["{", "[] trailing", ""] {
        let Request::Metadata(Some(reply)) = protocol::parse(raw) else {
            panic!("parse error")
        };
        assert_eq!(reply["error"]["code"], -32700);
    }
    rejected(json!({"jsonrpc":"2.0","id":1,"method":"unknown"}), -32601);
}
#[test]
fn business_reply_preserves_original_and_error_status() {
    for status in ["ok", "unknown", "denied"] {
        let original = json!({"status":status,"request_id":"original"});
        let reply = protocol::business_reply(json!(9), original.clone());
        assert_eq!(reply["result"]["structuredContent"], original);
        assert_eq!(reply["result"]["isError"], status != "ok");
    }
}

#[test]
fn duplicate_json_keys_are_rejected_before_business_decoding() {
    for raw in [
        r#"{"jsonrpc":"2.0","id":1,"method":"ping","method":"tools/call"}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"dev_test_accounts_call","arguments":{"method":"accounts","method":"move"}}}"#,
    ] {
        let Request::Metadata(Some(reply)) = protocol::parse(raw) else {
            panic!("duplicate accepted")
        };
        assert!(reply.get("error").is_some());
    }
}
