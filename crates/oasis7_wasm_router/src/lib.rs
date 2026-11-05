mod filtering;
mod metrics;
pub mod bounded;

#[cfg(test)]
use filtering::ruleset_matches;
use filtering::{
    parsed_subscription_filters_types::*, prepared_subscription_filters_match,
    subscription_filters_match, subscription_match, validate_rule_shape,
};
pub use filtering::{validate_subscription_filters, validate_subscription_stage};
pub use metrics::WasmRouterMetricsSnapshot;
pub use metrics::snapshot_global_wasm_router_metrics;
pub use bounded::{BoundedRouterErrorV1, RouterWorkV1, pre_action_matches_serialized_bounded_v1};

use oasis7_wasm_abi::{BoundedFifoCache, ModuleSubscription, ModuleSubscriptionStage};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

const MAX_CACHE_ENTRIES: usize = 1024;
type RegexCache = Mutex<BoundedFifoCache<Arc<regex::Regex>>>;

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn regex_cache() -> &'static RegexCache {
    static CACHE: OnceLock<RegexCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BoundedFifoCache::new(MAX_CACHE_ENTRIES)))
}

fn parsed_subscription_filters(
    filters_value: &JsonValue,
) -> Result<SubscriptionFilters, serde_json::Error> {
    SubscriptionFilters::deserialize(filters_value)
}

fn cached_regex(pattern: &str) -> Result<Arc<regex::Regex>, regex::Error> {
    if let Some(cached) = lock_recover(regex_cache()).get_cloned(pattern) {
        return Ok(cached);
    }
    let compile_started = Instant::now();
    let compiled = Arc::new(regex::Regex::new(pattern)?);
    metrics::observe_wasm_router_regex_compile(elapsed_ms(compile_started));
    lock_recover(regex_cache()).insert(pattern.to_string(), compiled.clone());
    Ok(compiled)
}

#[derive(Debug, Clone)]
pub struct PreparedSubscription {
    resolved_stage: ModuleSubscriptionStage,
    event_kinds: Vec<String>,
    action_kinds: Vec<String>,
    filters: Option<PreparedSubscriptionFilters>,
}

#[derive(Debug, Clone)]
struct PreparedSubscriptionFilters {
    event: Option<PreparedRuleSet>,
    action: Option<PreparedRuleSet>,
}

#[derive(Debug, Clone)]
enum PreparedRuleSet {
    List(Vec<PreparedMatchRule>),
    Group(PreparedRuleGroup),
}

#[derive(Debug, Clone)]
struct PreparedRuleGroup {
    all: Vec<PreparedMatchRule>,
    any: Vec<PreparedMatchRule>,
}

#[derive(Debug, Clone)]
struct PreparedMatchRule {
    path: String,
    operator: PreparedMatchOperator,
}

#[derive(Debug, Clone)]
enum PreparedMatchOperator {
    Eq(JsonValue),
    Ne(JsonValue),
    Gt(f64),
    Gte(f64),
    Lt(f64),
    Lte(f64),
    Re(Arc<regex::Regex>),
}

pub fn prepare_subscriptions(
    subscriptions: &[ModuleSubscription],
    module_id: &str,
) -> Result<Arc<[PreparedSubscription]>, String> {
    let started = Instant::now();
    let prepared = subscriptions
        .iter()
        .map(|subscription| prepare_subscription(subscription, module_id))
        .collect::<Result<Vec<_>, _>>();
    metrics::observe_wasm_router_prepare(elapsed_ms(started));
    let prepared = prepared?;
    Ok(Arc::from(prepared))
}

fn prepare_subscription(
    subscription: &ModuleSubscription,
    module_id: &str,
) -> Result<PreparedSubscription, String> {
    validate_subscription_stage(subscription, module_id)?;
    let filters = prepare_subscription_filters(&subscription.filters, module_id)?;
    Ok(PreparedSubscription {
        resolved_stage: subscription.resolved_stage(),
        event_kinds: subscription.event_kinds.clone(),
        action_kinds: subscription.action_kinds.clone(),
        filters,
    })
}

fn prepare_subscription_filters(
    filters: &Option<JsonValue>,
    module_id: &str,
) -> Result<Option<PreparedSubscriptionFilters>, String> {
    let Some(filters_value) = filters else {
        return Ok(None);
    };
    if filters_value.is_null() {
        return Ok(None);
    }
    let parsed = parsed_subscription_filters(filters_value)
        .map_err(|err| format!("module {module_id} subscription filters invalid: {err}"))?;
    let event = parsed
        .event
        .as_ref()
        .map(|ruleset| prepare_ruleset(ruleset, module_id))
        .transpose()?;
    let action = parsed
        .action
        .as_ref()
        .map(|ruleset| prepare_ruleset(ruleset, module_id))
        .transpose()?;
    Ok(Some(PreparedSubscriptionFilters { event, action }))
}

fn prepare_ruleset(ruleset: &RuleSet, module_id: &str) -> Result<PreparedRuleSet, String> {
    match ruleset {
        RuleSet::List(rules) => Ok(PreparedRuleSet::List(
            rules
                .iter()
                .map(|rule| prepare_rule(rule, module_id))
                .collect::<Result<_, _>>()?,
        )),
        RuleSet::Group(group) => Ok(PreparedRuleSet::Group(PreparedRuleGroup {
            all: group
                .all
                .iter()
                .map(|rule| prepare_rule(rule, module_id))
                .collect::<Result<_, _>>()?,
            any: group
                .any
                .iter()
                .map(|rule| prepare_rule(rule, module_id))
                .collect::<Result<_, _>>()?,
        })),
    }
}

fn prepare_rule(rule: &MatchRule, module_id: &str) -> Result<PreparedMatchRule, String> {
    validate_rule_shape(rule, module_id)?;
    let operator = if let Some(expected) = &rule.eq {
        PreparedMatchOperator::Eq(expected.clone())
    } else if let Some(expected) = &rule.ne {
        PreparedMatchOperator::Ne(expected.clone())
    } else if let Some(threshold) = rule.gt {
        PreparedMatchOperator::Gt(threshold)
    } else if let Some(threshold) = rule.gte {
        PreparedMatchOperator::Gte(threshold)
    } else if let Some(threshold) = rule.lt {
        PreparedMatchOperator::Lt(threshold)
    } else if let Some(threshold) = rule.lte {
        PreparedMatchOperator::Lte(threshold)
    } else if let Some(pattern) = &rule.re {
        PreparedMatchOperator::Re(cached_regex(pattern).map_err(|err| {
            format!("module {module_id} subscription filter regex invalid: {err}")
        })?)
    } else {
        return Err(format!(
            "module {module_id} subscription filter must specify exactly one operator"
        ));
    };
    Ok(PreparedMatchRule {
        path: rule.path.clone(),
        operator,
    })
}

pub fn module_subscribes_to_event(
    subscriptions: &[ModuleSubscription],
    event_kind: &str,
    event_value: &JsonValue,
) -> bool {
    let started = Instant::now();
    let matched = subscriptions.iter().any(|subscription| {
        subscription.resolved_stage() == ModuleSubscriptionStage::PostEvent
            && subscription
                .event_kinds
                .iter()
                .any(|pattern| subscription_match(pattern, event_kind))
            && subscription_filters_match(&subscription.filters, FilterKind::Event, event_value)
    });
    metrics::observe_wasm_router_match(elapsed_ms(started), false, true);
    matched
}

pub fn prepared_module_subscribes_to_event(
    subscriptions: &[PreparedSubscription],
    event_kind: &str,
    event_value: &JsonValue,
) -> bool {
    let started = Instant::now();
    let matched = subscriptions.iter().any(|subscription| {
        subscription.resolved_stage == ModuleSubscriptionStage::PostEvent
            && subscription
                .event_kinds
                .iter()
                .any(|pattern| subscription_match(pattern, event_kind))
            && prepared_subscription_filters_match(
                subscription.filters.as_ref(),
                FilterKind::Event,
                event_value,
            )
    });
    metrics::observe_wasm_router_match(elapsed_ms(started), true, false);
    matched
}

pub fn module_subscribes_to_action(
    subscriptions: &[ModuleSubscription],
    stage: ModuleSubscriptionStage,
    action_kind: &str,
    action_value: &JsonValue,
) -> bool {
    let started = Instant::now();
    let matched = subscriptions.iter().any(|subscription| {
        subscription.resolved_stage() == stage
            && subscription
                .action_kinds
                .iter()
                .any(|pattern| subscription_match(pattern, action_kind))
            && subscription_filters_match(&subscription.filters, FilterKind::Action, action_value)
    });
    metrics::observe_wasm_router_match(elapsed_ms(started), false, true);
    matched
}

pub fn prepared_module_subscribes_to_action(
    subscriptions: &[PreparedSubscription],
    stage: ModuleSubscriptionStage,
    action_kind: &str,
    action_value: &JsonValue,
) -> bool {
    let started = Instant::now();
    let matched = subscriptions.iter().any(|subscription| {
        subscription.resolved_stage == stage
            && subscription
                .action_kinds
                .iter()
                .any(|pattern| subscription_match(pattern, action_kind))
            && prepared_subscription_filters_match(
                subscription.filters.as_ref(),
                FilterKind::Action,
                action_value,
            )
    });
    metrics::observe_wasm_router_match(elapsed_ms(started), true, false);
    matched
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event_subscription(patterns: &[&str], filters: Option<JsonValue>) -> ModuleSubscription {
        ModuleSubscription {
            event_kinds: patterns.iter().map(|value| (*value).to_string()).collect(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters,
        }
    }

    fn action_subscription(
        stage: ModuleSubscriptionStage,
        patterns: &[&str],
        filters: Option<JsonValue>,
    ) -> ModuleSubscription {
        ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: patterns.iter().map(|value| (*value).to_string()).collect(),
            stage: Some(stage),
            filters,
        }
    }

    #[test]
    fn event_subscription_matches_wildcard_prefix_and_exact_patterns() {
        let payload = json!({ "kind": "world.tick", "hp": 3 });

        let wildcard = [event_subscription(&["*"], None)];
        assert!(module_subscribes_to_event(
            &wildcard,
            "world.tick",
            &payload
        ));

        let prefix = [event_subscription(&["world.*"], None)];
        assert!(module_subscribes_to_event(&prefix, "world.tick", &payload));
        assert!(!module_subscribes_to_event(
            &prefix,
            "action.move",
            &payload
        ));

        let exact = [event_subscription(&["world.tick"], None)];
        assert!(module_subscribes_to_event(&exact, "world.tick", &payload));
        assert!(!module_subscribes_to_event(&exact, "world.idle", &payload));
    }

    #[test]
    fn event_subscription_respects_filters_and_rejects_invalid_filter_schema() {
        let payload = json!({ "status": "ok", "hp": 8 });

        let match_status = [event_subscription(
            &["world.tick"],
            Some(json!({
                "event": [
                    { "path": "/status", "eq": "ok" }
                ]
            })),
        )];
        assert!(module_subscribes_to_event(
            &match_status,
            "world.tick",
            &payload
        ));

        let mismatch_status = [event_subscription(
            &["world.tick"],
            Some(json!({
                "event": [
                    { "path": "/status", "eq": "bad" }
                ]
            })),
        )];
        assert!(!module_subscribes_to_event(
            &mismatch_status,
            "world.tick",
            &payload
        ));

        let invalid_schema = [event_subscription(
            &["world.tick"],
            Some(json!({
                "event": [
                    { "path": "/status", "eq": "ok", "unknown": 1 }
                ]
            })),
        )];
        assert!(!module_subscribes_to_event(
            &invalid_schema,
            "world.tick",
            &payload
        ));
    }

    #[test]
    fn action_subscription_checks_stage_and_filters() {
        let subscription = action_subscription(
            ModuleSubscriptionStage::PreAction,
            &["action.move.*"],
            Some(json!({
                "action": {
                    "all": [
                        { "path": "/cost", "gte": 2.0 }
                    ],
                    "any": [
                        { "path": "/kind", "re": "^move\\." },
                        { "path": "/kind", "eq": "jump" }
                    ]
                }
            })),
        );

        let matched = json!({ "cost": 3.0, "kind": "move.left" });
        assert!(module_subscribes_to_action(
            std::slice::from_ref(&subscription),
            ModuleSubscriptionStage::PreAction,
            "action.move.step",
            &matched,
        ));

        assert!(!module_subscribes_to_action(
            std::slice::from_ref(&subscription),
            ModuleSubscriptionStage::PostAction,
            "action.move.step",
            &matched,
        ));

        let low_cost = json!({ "cost": 1.0, "kind": "move.left" });
        assert!(!module_subscribes_to_action(
            std::slice::from_ref(&subscription),
            ModuleSubscriptionStage::PreAction,
            "action.move.step",
            &low_cost,
        ));

        let kind_not_matched = json!({ "cost": 3.0, "kind": "scan" });
        assert!(!module_subscribes_to_action(
            &[subscription],
            ModuleSubscriptionStage::PreAction,
            "action.move.step",
            &kind_not_matched,
        ));
    }

    #[test]
    fn validate_subscription_stage_rejects_invalid_combinations() {
        let cases = vec![
            (
                ModuleSubscription {
                    event_kinds: vec!["world.tick".to_string()],
                    action_kinds: vec!["action.move".to_string()],
                    stage: None,
                    filters: None,
                },
                "cannot mix event_kinds and action_kinds",
            ),
            (
                ModuleSubscription {
                    event_kinds: Vec::new(),
                    action_kinds: Vec::new(),
                    stage: None,
                    filters: None,
                },
                "requires event_kinds or action_kinds",
            ),
            (
                ModuleSubscription {
                    event_kinds: vec!["world.tick".to_string()],
                    action_kinds: vec!["action.move".to_string()],
                    stage: Some(ModuleSubscriptionStage::PostEvent),
                    filters: None,
                },
                "post_event cannot include action_kinds",
            ),
            (
                ModuleSubscription {
                    event_kinds: vec!["world.tick".to_string()],
                    action_kinds: Vec::new(),
                    stage: Some(ModuleSubscriptionStage::PreAction),
                    filters: None,
                },
                "action stage cannot include event_kinds",
            ),
            (
                ModuleSubscription {
                    event_kinds: vec!["world.tick".to_string()],
                    action_kinds: Vec::new(),
                    stage: Some(ModuleSubscriptionStage::Tick),
                    filters: None,
                },
                "tick stage cannot include event_kinds or action_kinds",
            ),
        ];

        for (subscription, expected) in cases {
            let err = validate_subscription_stage(&subscription, "m.test").unwrap_err();
            assert!(
                err.contains(expected),
                "expected error containing `{expected}`, got `{err}`"
            );
        }

        let event_only = ModuleSubscription {
            event_kinds: vec!["world.tick".to_string()],
            action_kinds: Vec::new(),
            stage: None,
            filters: None,
        };
        validate_subscription_stage(&event_only, "m.test").unwrap();

        let action_only = ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.move".to_string()],
            stage: None,
            filters: None,
        };
        validate_subscription_stage(&action_only, "m.test").unwrap();

        let tick = ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::Tick),
            filters: None,
        };
        validate_subscription_stage(&tick, "m.test").unwrap();
    }

    #[test]
    fn validate_subscription_filters_enforces_schema_and_rules() {
        validate_subscription_filters(&None, "m.test").unwrap();
        validate_subscription_filters(&Some(JsonValue::Null), "m.test").unwrap();

        let valid = Some(json!({
            "event": [
                { "path": "/hp", "gt": 0.0 }
            ],
            "action": {
                "all": [
                    { "path": "/kind", "re": "^move\\." }
                ]
            }
        }));
        validate_subscription_filters(&valid, "m.test").unwrap();

        let invalid_path = Some(json!({
            "event": [
                { "path": "hp", "eq": 1 }
            ]
        }));
        let path_err = validate_subscription_filters(&invalid_path, "m.test").unwrap_err();
        assert!(path_err.contains("path must start with '/'"));

        let invalid_operator_count = Some(json!({
            "event": [
                { "path": "/hp", "eq": 1, "gt": 0.0 }
            ]
        }));
        let operator_err =
            validate_subscription_filters(&invalid_operator_count, "m.test").unwrap_err();
        assert!(operator_err.contains("exactly one operator"));

        let invalid_regex = Some(json!({
            "action": [
                { "path": "/kind", "re": "[" }
            ]
        }));
        let regex_err = validate_subscription_filters(&invalid_regex, "m.test").unwrap_err();
        assert!(regex_err.contains("regex invalid"));

        let invalid_schema = Some(json!({
            "event": [
                { "path": "/kind", "eq": "ok", "unexpected": true }
            ]
        }));
        let schema_err = validate_subscription_filters(&invalid_schema, "m.test").unwrap_err();
        assert!(schema_err.contains("subscription filters invalid"));
        assert!(schema_err.starts_with("module m.test subscription filters invalid:"));
    }

    #[test]
    fn bounded_cache_evicts_oldest_entries() {
        let mut cache = BoundedFifoCache::new(2);
        cache.insert("a".to_string(), Arc::new(1u32));
        cache.insert("b".to_string(), Arc::new(2u32));
        cache.insert("c".to_string(), Arc::new(3u32));

        assert!(cache.get_cloned("a").is_none());
        assert_eq!(cache.get_cloned("b").as_deref(), Some(&2u32));
        assert_eq!(cache.get_cloned("c").as_deref(), Some(&3u32));
    }

    #[test]
    fn bounded_cache_updates_existing_entry_without_refreshing_eviction_order() {
        let mut cache = BoundedFifoCache::new(2);
        cache.insert("a".to_string(), Arc::new(1u32));
        cache.insert("b".to_string(), Arc::new(2u32));
        cache.insert("a".to_string(), Arc::new(10u32));
        cache.insert("c".to_string(), Arc::new(3u32));

        assert!(cache.get_cloned("a").is_none());
        assert_eq!(cache.get_cloned("b").as_deref(), Some(&2u32));
        assert_eq!(cache.get_cloned("c").as_deref(), Some(&3u32));
    }

    #[test]
    fn prepared_subscriptions_match_equivalent_event_and_action_routes() {
        let event_subscriptions = vec![event_subscription(
            &["world.*"],
            Some(json!({
                "event": {
                    "all": [
                        { "path": "/status", "eq": "ok" },
                        { "path": "/hp", "gt": 0.0 }
                    ]
                }
            })),
        )];
        let prepared_event =
            prepare_subscriptions(&event_subscriptions, "m.test").expect("prepare event filters");
        let event_value = json!({ "status": "ok", "hp": 5.0 });
        assert_eq!(
            module_subscribes_to_event(&event_subscriptions, "world.tick", &event_value),
            prepared_module_subscribes_to_event(
                prepared_event.as_ref(),
                "world.tick",
                &event_value
            )
        );

        let action_subscriptions = vec![action_subscription(
            ModuleSubscriptionStage::PreAction,
            &["action.move.*"],
            Some(json!({
                "action": {
                    "all": [
                        { "path": "/cost", "gte": 2.0 }
                    ],
                    "any": [
                        { "path": "/kind", "eq": "move.left" },
                        { "path": "/kind", "re": "^move\\." }
                    ]
                }
            })),
        )];
        let prepared_action =
            prepare_subscriptions(&action_subscriptions, "m.test").expect("prepare action filters");
        let action_value = json!({ "cost": 3.0, "kind": "move.left" });
        assert_eq!(
            module_subscribes_to_action(
                &action_subscriptions,
                ModuleSubscriptionStage::PreAction,
                "action.move.step",
                &action_value,
            ),
            prepared_module_subscribes_to_action(
                prepared_action.as_ref(),
                ModuleSubscriptionStage::PreAction,
                "action.move.step",
                &action_value,
            )
        );
    }

    #[test]
    #[ignore = "local perf probe"]
    fn perf_probe_subscription_filter_parse_overhead() {
        use std::time::Instant;

        let no_regex_filter_json = json!({
            "event": {
                "all": [
                    { "path": "/actor/kind", "eq": "player" },
                    { "path": "/stats/hp", "gt": 0.0 },
                    { "path": "/region/id", "eq": "region-alpha" }
                ],
                "any": [
                    { "path": "/event_kind", "eq": "world.tick" },
                    { "path": "/event_kind", "eq": "world.effect" },
                    { "path": "/event_kind", "eq": "world.spawn" }
                ]
            }
        });
        let no_regex_subscription = ModuleSubscription {
            event_kinds: vec!["world.*".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: Some(no_regex_filter_json.clone()),
        };
        let subscriptions = vec![no_regex_subscription];
        let event_value = json!({
            "actor": { "kind": "player", "id": "p-1" },
            "stats": { "hp": 7.0, "energy": 3.0 },
            "region": { "id": "region-alpha" },
            "event_kind": "world.tick"
        });

        assert!(module_subscribes_to_event(
            &subscriptions,
            "world.tick",
            &event_value
        ));

        let parsed: SubscriptionFilters =
            serde_json::from_value(no_regex_filter_json).expect("parse filters once");
        let parsed_rules = parsed.event.as_ref().expect("event filters");
        let prepared =
            prepare_subscriptions(&subscriptions, "m.perf.no_regex").expect("prepare once");
        assert!(ruleset_matches(parsed_rules, &event_value));
        assert!(prepared_module_subscribes_to_event(
            prepared.as_ref(),
            "world.tick",
            &event_value
        ));

        let iterations = 200_000u32;
        let started = Instant::now();
        for _ in 0..iterations {
            assert!(module_subscribes_to_event(
                &subscriptions,
                "world.tick",
                &event_value
            ));
        }
        let parse_each_time_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            assert!(ruleset_matches(parsed_rules, &event_value));
        }
        let parsed_once_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            assert!(prepared_module_subscribes_to_event(
                prepared.as_ref(),
                "world.tick",
                &event_value
            ));
        }
        let prepared_once_elapsed = started.elapsed();

        let regex_filter_json = json!({
            "event": {
                "all": [
                    { "path": "/actor/kind", "eq": "player" },
                    { "path": "/stats/hp", "gt": 0.0 },
                    { "path": "/region/id", "re": "^region-" }
                ],
                "any": [
                    { "path": "/event_kind", "eq": "world.tick" },
                    { "path": "/event_kind", "eq": "world.effect" },
                    { "path": "/event_kind", "eq": "world.spawn" }
                ]
            }
        });
        let regex_subscription = vec![ModuleSubscription {
            event_kinds: vec!["world.*".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: Some(regex_filter_json.clone()),
        }];
        let parsed_regex: SubscriptionFilters =
            serde_json::from_value(regex_filter_json).expect("parse regex filters once");
        let parsed_regex_rules = parsed_regex.event.as_ref().expect("regex event filters");
        let prepared_regex =
            prepare_subscriptions(&regex_subscription, "m.perf.regex").expect("prepare regex");
        assert!(module_subscribes_to_event(
            &regex_subscription,
            "world.tick",
            &event_value
        ));
        assert!(ruleset_matches(parsed_regex_rules, &event_value));
        assert!(prepared_module_subscribes_to_event(
            prepared_regex.as_ref(),
            "world.tick",
            &event_value
        ));

        let started = Instant::now();
        for _ in 0..iterations {
            assert!(module_subscribes_to_event(
                &regex_subscription,
                "world.tick",
                &event_value
            ));
        }
        let regex_parse_each_time_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            assert!(ruleset_matches(parsed_regex_rules, &event_value));
        }
        let regex_parsed_once_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            assert!(prepared_module_subscribes_to_event(
                prepared_regex.as_ref(),
                "world.tick",
                &event_value
            ));
        }
        let regex_prepared_once_elapsed = started.elapsed();

        eprintln!(
            "perf_probe_subscription_filter_parse_overhead: iterations={iterations} no_regex_parse_each_time_ms={:.3} no_regex_parsed_once_ms={:.3} no_regex_prepared_once_ms={:.3} no_regex_parse_vs_prepared_ratio={:.2}x regex_parse_each_time_ms={:.3} regex_parsed_once_ms={:.3} regex_prepared_once_ms={:.3} regex_parse_vs_prepared_ratio={:.2}x",
            parse_each_time_elapsed.as_secs_f64() * 1_000.0,
            parsed_once_elapsed.as_secs_f64() * 1_000.0,
            prepared_once_elapsed.as_secs_f64() * 1_000.0,
            if prepared_once_elapsed.as_nanos() == 0 {
                0.0
            } else {
                parse_each_time_elapsed.as_secs_f64() / prepared_once_elapsed.as_secs_f64()
            },
            regex_parse_each_time_elapsed.as_secs_f64() * 1_000.0,
            regex_parsed_once_elapsed.as_secs_f64() * 1_000.0,
            regex_prepared_once_elapsed.as_secs_f64() * 1_000.0,
            if regex_prepared_once_elapsed.as_nanos() == 0 {
                0.0
            } else {
                regex_parse_each_time_elapsed.as_secs_f64()
                    / regex_prepared_once_elapsed.as_secs_f64()
            },
        );
    }

    #[test]
    fn router_metrics_track_prepare_and_match_paths() {
        let baseline = snapshot_global_wasm_router_metrics();
        let subscription = event_subscription(
            &["world.tick"],
            Some(json!({
                "event": [
                    { "path": "/status", "eq": "ok" }
                ]
            })),
        );
        let payload = json!({ "status": "ok" });
        let prepared = prepare_subscriptions(std::slice::from_ref(&subscription), "m.metrics")
            .expect("prepare metrics");
        assert!(module_subscribes_to_event(
            &[subscription],
            "world.tick",
            &payload
        ));
        assert!(prepared_module_subscribes_to_event(
            prepared.as_ref(),
            "world.tick",
            &payload
        ));

        let latest = snapshot_global_wasm_router_metrics();
        assert!(latest.prepare_calls_total > baseline.prepare_calls_total);
        assert!(latest.match_calls_total >= baseline.match_calls_total + 2);
        assert!(latest.parse_fallbacks > baseline.parse_fallbacks);
        assert!(latest.prepared_hits > baseline.prepared_hits);
    }

    #[test]
    fn router_metrics_track_regex_compile_time() {
        let baseline = snapshot_global_wasm_router_metrics();
        let unique = format!(
            "^metrics-{}$",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        );
        let subscription = event_subscription(
            &["world.tick"],
            Some(json!({
                "event": [
                    { "path": "/status", "re": unique }
                ]
            })),
        );
        let payload = json!({ "status": "metrics-ok" });
        let _ = module_subscribes_to_event(&[subscription], "world.tick", &payload);

        let latest = snapshot_global_wasm_router_metrics();
        assert!(
            latest.regex_compile_ms_total >= baseline.regex_compile_ms_total,
            "regex compile counter should be monotonic"
        );
    }

    #[test]
    fn router_metrics_track_prepared_regex_compile_count() {
        let baseline = snapshot_global_wasm_router_metrics();
        let unique = format!(
            "^prepared-metrics-{}$",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos()
        );
        let subscription = event_subscription(
            &["world.tick"],
            Some(json!({
                "event": [
                    { "path": "/status", "re": unique }
                ]
            })),
        );

        let _ = prepare_subscriptions(&[subscription], "m.prepared.metrics")
            .expect("prepare regex subscription");

        let latest = snapshot_global_wasm_router_metrics();
        assert!(
            latest.regex_compile_calls_total > baseline.regex_compile_calls_total,
            "prepared regex compilation should be counted"
        );
    }
}
