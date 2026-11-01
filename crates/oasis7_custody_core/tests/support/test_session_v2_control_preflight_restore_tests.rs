use super::*;
#[test]
fn session_v2_control_preflight_reservation_is_exact_and_not_an_orphan_escape() {
    let f = activation_tests::activated(9000);
    let original = serde_json::to_value(&f.state).unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(original.clone()).is_ok());
    for case in 0..4 {
        let mut raw = original.clone();
        let s = &mut raw["test_sessions_v2"][&f.session_id];
        let logical = s["control_logicals"]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        match case {
            0 => s["control_logicals"][&logical] = serde_json::json!("ff".repeat(32)),
            1 => {
                let body = s["control_logicals"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&logical)
                    .unwrap();
                s["control_logicals"]["wrong-preflight-id"] = body;
                let amount = s["control"]["requests"]["reservations"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&logical)
                    .unwrap();
                s["control"]["requests"]["reservations"]["wrong-preflight-id"] = amount;
            }
            2 => {
                s["control_logicals"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&logical)
                    .unwrap();
                s["control"]["requests"]["reservations"]
                    .as_object_mut()
                    .unwrap()
                    .remove(&logical)
                    .unwrap();
            }
            3 => {
                s["preflight_steps"]
                    .as_object_mut()
                    .unwrap()
                    .remove("owner_read")
                    .unwrap();
                let spent = s["identity_transport"]["spent"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                s["identity_transport"]["spent"] = serde_json::json!((spent - 1).to_string());
            }
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(raw).is_err(),
            "preflight case {case}"
        );
    }
}
