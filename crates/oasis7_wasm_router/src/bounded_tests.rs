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

// Data-logic reference provider only; this grants no production activation authority.
struct ReferenceRegex {
    calls: usize,
    compile_only: usize,
    truncate: bool,
}
struct ReferenceOutput(Vec<bool>);
impl RegexBatchOutputV1 for ReferenceOutput {
    fn values(&self) -> &[bool] {
        &self.0
    }
}
impl RegexBatchBackendV1 for ReferenceRegex {
    type Error = usize;
    type Output = ReferenceOutput;
    fn match_batch<C: FnMut() -> bool>(
        &mut self,
        probes: &[RegexProbeV1<'_>],
        c: &mut C,
    ) -> Result<Self::Output, usize> {
        self.calls += 1;
        let mut values = Vec::new();
        for (i, p) in probes.iter().enumerate() {
            if c() {
                return Err(usize::MAX);
            }
            let re = regex::Regex::new(p.pattern).map_err(|_| i)?;
            self.compile_only += usize::from(p.text.is_none());
            values.push(p.text.is_none_or(|text| re.is_match(text)));
        }
        if self.truncate {
            values.pop();
        }
        Ok(ReferenceOutput(values))
    }
}
fn reference_backend() -> ReferenceRegex {
    ReferenceRegex {
        calls: 0,
        compile_only: 0,
        truncate: false,
    }
}
fn regex_subscription(rules: serde_json::Value) -> ModuleSubscription {
    ModuleSubscription {
        event_kinds: vec![],
        action_kinds: vec!["move*".into()],
        stage: Some(ModuleSubscriptionStage::PreAction),
        filters: Some(json!({"action":rules})),
    }
}
#[test]
fn batch_regex_matches_legacy_real_engine_cases() {
    let value = json!({"s":"hello world", "n":5});
    for rules in [
        json!([{"path":"/s","re":"^hello"}]),
        json!([{"path":"/s","re":"^world"}]),
        json!([{"path":"/missing","re":".*"}]),
        json!([{"path":"/n","re":".*"}]),
        json!({"all":[{"path":"/s","re":"hello"},{"path":"/n","eq":5}],"any":[{"path":"/s","re":"world$"},{"path":"/s","re":"no"}]}),
        json!({"all":[],"any":[]}),
    ] {
        let subs = [regex_subscription(rules)];
        let prepared = crate::prepare_subscriptions(&subs, "m").unwrap();
        let expected = crate::module_subscribes_to_action(
            &subs,
            ModuleSubscriptionStage::PreAction,
            "move",
            &value,
        );
        drop(prepared);
        let mut backend = reference_backend();
        let actual = pre_action_matches_serialized_bounded_with_regex_v1(
            &value,
            &subs,
            "m",
            "move",
            &mut work(),
            &mut || false,
            |_| Ok::<_, u8>(()),
            &mut backend,
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
}
#[test]
fn batch_regex_compiles_unselected_and_preserves_errors() {
    let mut other = regex_subscription(json!([{"path":"/s","re":"["}]));
    other.stage = Some(ModuleSubscriptionStage::PostAction);
    let mut backend = reference_backend();
    let result = pre_action_matches_serialized_bounded_with_regex_v1(
        &json!({"s":"x"}),
        &[other],
        "m",
        "move",
        &mut work(),
        &mut || false,
        |_| Ok::<_, u8>(()),
        &mut backend,
    );
    assert!(matches!(result, Err(BoundedRouterErrorV1::Regex(0))));
    assert_eq!(backend.calls, 1);
    let subs = [regex_subscription(
        json!([{"path":"/s","re":".*"},{"path":"/n","re":".*"},{"path":"/missing","re":".*"}]),
    )];
    let mut backend = reference_backend();
    backend.truncate = true;
    let result = pre_action_matches_serialized_bounded_with_regex_v1(
        &json!({"s":"x","n":1}),
        &subs,
        "m",
        "move",
        &mut work(),
        &mut || false,
        |_| Ok::<_, u8>(()),
        &mut backend,
    );
    assert!(matches!(
        result,
        Err(BoundedRouterErrorV1::Invalid("regex result count"))
    ));
    assert_eq!(backend.compile_only, 2);
}

#[test]
fn batch_regex_no_backend_without_rules_and_fail_closed_before_batch() {
    let value = json!({"s":"abcdefgh"});
    let mut backend = reference_backend();
    assert!(
        !pre_action_matches_serialized_bounded_with_regex_v1(
            &value,
            &[],
            "m",
            "move",
            &mut work(),
            &mut || false,
            |_| Ok::<_, u8>(()),
            &mut backend
        )
        .unwrap()
    );
    assert_eq!(backend.calls, 0);
    let subs = [regex_subscription(json!([{"path":"/s","re":"abcdefgh"}]))];
    let mut limit = work();
    limit.match_steps_remaining = 1;
    assert!(matches!(
        pre_action_matches_serialized_bounded_with_regex_v1(
            &value,
            &subs,
            "m",
            "move",
            &mut limit,
            &mut || false,
            |_| Ok::<_, u8>(()),
            &mut backend
        ),
        Err(BoundedRouterErrorV1::ResourceLimited(_))
    ));
    assert_eq!(backend.calls, 0);
    assert!(matches!(
        pre_action_matches_serialized_bounded_with_regex_v1(
            &value,
            &subs,
            "m",
            "move",
            &mut work(),
            &mut || true,
            |_| Ok::<_, u8>(()),
            &mut backend
        ),
        Err(BoundedRouterErrorV1::Cancelled)
    ));
    assert_eq!(backend.calls, 0);
    let mut reservations = 0;
    assert!(matches!(
        pre_action_matches_serialized_bounded_with_regex_v1(
            &value,
            &subs,
            "m",
            "move",
            &mut work(),
            &mut || false,
            |_| {
                reservations += 1;
                if reservations == 2 { Err(9u8) } else { Ok(()) }
            },
            &mut backend
        ),
        Err(BoundedRouterErrorV1::Admission(9))
    ));
    assert_eq!(backend.calls, 0);
}

#[test]
fn stage_aware_regex_matches_only_requested_stage_and_payload_kind() {
    use oasis7_wasm_abi::ModuleSubscriptionStage;
    for stage in [
        ModuleSubscriptionStage::PreAction,
        ModuleSubscriptionStage::PostAction,
        ModuleSubscriptionStage::PostEvent,
    ] {
        let event = stage == ModuleSubscriptionStage::PostEvent;
        let mut subscription = regex_subscription(json!([{"path":"/s","re":"^yes$"}]));
        subscription.stage = Some(stage);
        if event {
            subscription.event_kinds = subscription.action_kinds.clone();
            subscription.action_kinds.clear();
            subscription.filters = Some(json!({"event":[{"path":"/s","re":"^yes$"}]}));
        }
        for requested in [
            ModuleSubscriptionStage::PreAction,
            ModuleSubscriptionStage::PostAction,
            ModuleSubscriptionStage::PostEvent,
        ] {
            let mut backend = reference_backend();
            let actual = subscription_matches_serialized_bounded_with_regex_v1(
                &json!({"s":"yes"}),
                std::slice::from_ref(&subscription),
                "m",
                "move",
                requested,
                &mut work(),
                &mut || false,
                |_| Ok::<_, u8>(()),
                &mut backend,
            )
            .unwrap();
            assert_eq!(actual, stage == requested);
            assert_eq!(backend.calls, 1);
            assert_eq!(backend.compile_only, usize::from(stage != requested));
        }
    }
}
