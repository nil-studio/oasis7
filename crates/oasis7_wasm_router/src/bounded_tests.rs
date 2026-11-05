use super::*;
use oasis7_wasm_abi::ModuleSubscriptionStage;
use serde_json::json;
fn work() -> RouterWorkV1 {
    RouterWorkV1 {
        nodes_remaining: 10000,
        edges_remaining: 10000,
        string_bytes_remaining: 10000,
        match_steps_remaining: 100000,
        max_depth: 64,
    }
}
#[test]
fn admission_and_cancel_are_typed() {
    let value = json!({"a":1});
    let result = pre_action_matches_serialized_bounded_v1(
        &value,
        &[],
        "m",
        "move",
        &mut work(),
        &mut || false,
        |_| Err::<(), _>(42u8),
    );
    assert!(matches!(result, Err(BoundedRouterErrorV1::Admission(42))));
    let result = pre_action_matches_serialized_bounded_v1(
        &value,
        &[],
        "m",
        "move",
        &mut work(),
        &mut || true,
        |_| Ok::<_, u8>(()),
    );
    assert!(matches!(result, Err(BoundedRouterErrorV1::Cancelled)));
    let mut low = work();
    low.match_steps_remaining = 0;
    let result = pre_action_matches_serialized_bounded_v1(
        &value,
        &[],
        "m",
        "move",
        &mut low,
        &mut || false,
        |_| Ok::<_, u8>(()),
    );
    assert!(matches!(
        result,
        Err(BoundedRouterErrorV1::ResourceLimited(_))
    ));
}
#[test]
fn old_router_equivalence_and_regex_rejection() {
    let value = json!({"a/b":{"~x":[null, "escaped\n😀", 12]},"obj":{"x":true}});
    for rule in [
        json!({"path":"/a~1b/~0x/1","eq":"escaped\n😀"}),
        json!({"path":"/obj","eq":{"x":true}}),
        json!({"path":"/missing","ne":null}),
        json!({"path":"/a~1b/~0x/2","gte":10}),
        json!({"path":"/a~1b/~0x/0","eq":null}),
    ] {
        let s = ModuleSubscription {
            event_kinds: vec![],
            action_kinds: vec!["move*".into()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: Some(json!({"action":[rule]})),
        };
        let expected = crate::module_subscribes_to_action(
            std::slice::from_ref(&s),
            ModuleSubscriptionStage::PreAction,
            "move",
            &value,
        );
        let actual = pre_action_matches_serialized_bounded_v1(
            &value,
            &[s],
            "m",
            "move",
            &mut work(),
            &mut || false,
            |_| Ok::<_, u8>(()),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    let s = ModuleSubscription {
        event_kinds: vec![],
        action_kinds: vec!["*".into()],
        stage: Some(ModuleSubscriptionStage::PreAction),
        filters: Some(json!({"action":[{"path":"","re":".*"}]})),
    };
    assert!(matches!(
        pre_action_matches_serialized_bounded_v1(
            &value,
            &[s],
            "m",
            "move",
            &mut work(),
            &mut || false,
            |_| Ok::<_, u8>(())
        ),
        Err(BoundedRouterErrorV1::Unsupported(_))
    ));
}

#[test]
fn numeric_and_json_edge_cases_match_legacy() {
    for value in [
        json!(-0.0f64),
        json!(1.2345678901234567f64),
        json!(f64::MIN_POSITIVE),
        json!(f64::MAX),
        json!(u64::MAX),
        json!(i64::MIN),
        json!({"unicode":"\u{0000}\n\t😀","a/b":{"~":[true]}}),
    ] {
        for rule in [
            json!({"path":"","eq":value}),
            json!({"path":"","ne":value}),
            json!({"path":"/missing","ne":null}),
        ] {
            let sub = ModuleSubscription {
                stage: Some(ModuleSubscriptionStage::PreAction),
                event_kinds: vec![],
                action_kinds: vec!["*".into()],
                filters: Some(json!({"action":[rule]})),
            };
            let expected = crate::module_subscribes_to_action(
                std::slice::from_ref(&sub),
                ModuleSubscriptionStage::PreAction,
                "move",
                &value,
            );
            let actual = pre_action_matches_serialized_bounded_v1(
                &value,
                &[sub],
                "m",
                "move",
                &mut work(),
                &mut || false,
                |_| Ok::<_, u8>(()),
            )
            .unwrap();
            assert_eq!(expected, actual, "{value}");
        }
    }
    for rules in [json!([]), json!({}), json!({"all":[],"any":[]})] {
        let sub = ModuleSubscription {
            stage: Some(ModuleSubscriptionStage::PreAction),
            event_kinds: vec![],
            action_kinds: vec!["*".into()],
            filters: Some(json!({"action":rules})),
        };
        assert!(
            pre_action_matches_serialized_bounded_v1(
                &json!(null),
                &[sub],
                "m",
                "move",
                &mut work(),
                &mut || false,
                |_| Ok::<_, u8>(())
            )
            .unwrap()
        );
    }
}
#[test]
fn validates_non_preaction_subscriptions() {
    for sub in [
        ModuleSubscription {
            stage: Some(ModuleSubscriptionStage::Tick),
            event_kinds: vec![],
            action_kinds: vec!["move".into()],
            filters: None,
        },
        ModuleSubscription {
            stage: None,
            event_kinds: vec!["event".into()],
            action_kinds: vec!["move".into()],
            filters: None,
        },
        ModuleSubscription {
            stage: Some(ModuleSubscriptionStage::PostEvent),
            event_kinds: vec!["event".into()],
            action_kinds: vec![],
            filters: Some(json!({"event":[{"path":"bad","eq":null}]})),
        },
    ] {
        assert!(crate::prepare_subscriptions(std::slice::from_ref(&sub), "m").is_err());
        assert!(matches!(
            pre_action_matches_serialized_bounded_v1(
                &json!(null),
                &[sub],
                "m",
                "move",
                &mut work(),
                &mut || false,
                |_| Ok::<_, u8>(())
            ),
            Err(BoundedRouterErrorV1::Invalid(_))
        ));
    }
}
