//! Bounded, source-bound projection of this connection's authorized snapshot.
use crate::simulator::WorldSnapshot;
use crate::viewer::world_view_authority::SourceAuthority;
use serde::Deserialize;
use serde_json::{Value, json};
#[path = "world_view_delta.rs"]
mod delta;
#[cfg(test)]
#[path = "world_view_delta_tests.rs"]
mod delta_tests;
#[path = "world_view_index.rs"]
mod index;
#[path = "world_view_projection.rs"]
mod projection;
use index::Rect;
use projection::{Prepared, prepare};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    request_generation: String,
    #[serde(default)]
    bounds: Option<Bounds>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    pinned_entities: Vec<Target>,
    page_size: usize,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    resume_cursor: Option<String>,
    #[serde(default)]
    detail_level: Option<String>,
    #[serde(default)]
    include_events: bool,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Target {
    kind: String,
    id: String,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Bounds {
    min_x_cm: String,
    max_x_cm: String,
    min_y_cm: String,
    max_y_cm: String,
}
fn validate(query: &Value) -> Result<(Query, Option<Rect>), &'static str> {
    let q: Query = serde_json::from_value(query.clone()).map_err(|_| "invalid_query")?;
    if q.page_size == 0
        || q.page_size > 200
        || q.request_generation.len() > 64
        || q.search.as_ref().is_some_and(|s| s.len() > 128)
        || (q.cursor.is_some() && q.resume_cursor.is_some())
        || q.resume_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 4096)
        || q.detail_level
            .as_ref()
            .is_some_and(|level| !matches!(level.as_str(), "auto" | "overview" | "site" | "entity"))
        || q.pinned_entities.len() > 3
        || q.pinned_entities.iter().any(|p| {
            !matches!(p.kind.as_str(), "agent" | "location" | "factory")
                || p.id.is_empty()
                || p.id.len() > 256
        })
    {
        return Err("invalid_query");
    }
    let bounds = if let Some(b) = &q.bounds {
        let [Ok(x0), Ok(x1), Ok(y0), Ok(y1)] =
            [&b.min_x_cm, &b.max_x_cm, &b.min_y_cm, &b.max_y_cm].map(|v| v.parse::<i64>())
        else {
            return Err("invalid_bounds");
        };
        if x0 > x1 || y0 > y1 {
            return Err("invalid_bounds");
        }
        Some(Rect(x0, x1, y0, y1))
    } else {
        None
    };
    Ok((q, bounds))
}
fn unavailable(world: &str, query: &Value, reason: &str) -> Value {
    json!({"world_id":world,"request_generation":query.get("request_generation"),"completeness":"unknown","entities":[],"relations":[],"events":[],"unavailable_reasons":[reason]})
}
pub fn build(
    snapshot: &WorldSnapshot,
    world: &str,
    epoch: u64,
    audience: &str,
    query: Value,
) -> Value {
    build_with_authority(snapshot, world, epoch, audience, query, false)
}
pub(super) fn build_with_authority(
    snapshot: &WorldSnapshot,
    world: &str,
    epoch: u64,
    audience: &str,
    query: Value,
    local_runtime: bool,
) -> Value {
    if let Err(reason) = validate(&query) {
        return unavailable(world, &query, reason);
    }
    let authority = if local_runtime {
        SourceAuthority::local_runtime()
    } else {
        SourceAuthority::unknown()
    };
    let Some(prepared) = prepare(snapshot, world, epoch, audience, &authority) else {
        return unavailable(world, &query, "projection_limit_exceeded");
    };
    build_inner(&snapshot.config.space, world, query, &prepared, false)
}
fn query_id(
    space: &oasis7_simulator::SpaceConfig,
    prepared: &Prepared,
    q: &Query,
    scope_key: &str,
) -> String {
    blake3::hash(
        json!([
            &prepared.scope,
            scope_key,
            &prepared.epoch,
            space,
            &q.bounds,
            &q.search,
            &q.pinned_entities,
            q.page_size,
            q.detail_level.as_deref().unwrap_or("entity"),
            q.include_events
        ])
        .to_string()
        .as_bytes(),
    )
    .to_hex()
    .to_string()
}
fn query_objects(prepared: &Prepared, q: &Query, bounds: Option<Rect>) -> Vec<projection::Object> {
    let pins: Vec<_> = q
        .pinned_entities
        .iter()
        .map(|p| (p.kind.clone(), p.id.clone()))
        .collect();
    let mut objects = projection::query_objects(
        prepared,
        q.detail_level.as_deref().unwrap_or("entity"),
        bounds,
        &pins,
        q.search.as_deref(),
        q.include_events,
    );
    let order = |object: &projection::Object| {
        format!(
            "{}:{}",
            u8::from(
                !pins
                    .iter()
                    .any(|(kind, id)| object.key == format!("entity:{kind}:{id}"))
            ),
            object.key
        )
    };
    objects.sort_by_key(order);
    objects
}
fn build_inner(
    space: &oasis7_simulator::SpaceConfig,
    world: &str,
    query: Value,
    prepared: &Prepared,
    source_frozen: bool,
) -> Value {
    let (q, bounds) = match validate(&query) {
        Ok(query) => query,
        Err(reason) => return unavailable(world, &query, reason),
    };
    let objects = query_objects(prepared, &q, bounds);
    build_page(space, world, &query, prepared, source_frozen, &objects)
}
fn build_page(
    space: &oasis7_simulator::SpaceConfig,
    world: &str,
    query: &Value,
    prepared: &Prepared,
    source_frozen: bool,
    objects: &[projection::Object],
) -> Value {
    let (q, _) = match validate(query) {
        Ok(query) => query,
        Err(reason) => return unavailable(world, query, reason),
    };
    let fingerprint = blake3::hash(
        json!([
            &prepared.scope,
            &prepared.cut,
            &q.bounds,
            &q.search,
            &q.pinned_entities,
            q.page_size,
            q.detail_level.as_deref().unwrap_or("entity"),
            q.include_events
        ])
        .to_string()
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    let after = if let Some(cursor) = &q.cursor {
        if cursor.len() > 4096 {
            return unavailable(world, query, "cursor_reset_required");
        }
        match hex::decode(cursor)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<(String, String)>(&bytes).ok())
        {
            Some((binding, key)) if binding == fingerprint => Some(key),
            _ => return unavailable(world, query, "cursor_reset_required"),
        }
    } else {
        None
    };
    let order = |object: &projection::Object| {
        format!(
            "{}:{}",
            u8::from(
                !q.pinned_entities
                    .iter()
                    .any(|pin| object.key == format!("entity:{}:{}", pin.kind, pin.id))
            ),
            object.key
        )
    };
    let remaining: Vec<_> = objects
        .iter()
        .filter(|object| after.as_ref().is_none_or(|after| order(object) > *after))
        .collect();
    let partial = remaining.len() > q.page_size;
    let page: Vec<_> = remaining.into_iter().take(q.page_size).collect();
    let next = if partial {
        page.last().map(|object| {
            hex::encode(serde_json::to_vec(&(&fingerprint, order(object))).expect("string tuple"))
        })
    } else {
        None
    };
    if next.as_ref().is_some_and(|cursor| cursor.len() > 4096) {
        return unavailable(world, query, "cursor_limit_exceeded");
    }
    let mut entities = Vec::new();
    let mut relations = Vec::new();
    let mut aggregates = Vec::new();
    let mut events = Vec::new();
    for object in page {
        let mut value = object.value.clone();
        let aggregate = value.get("aggregate_id").is_some();
        let event = value.get("event_id").is_some();
        let provenance = if object.relation || event {
            &mut value
        } else {
            &mut value["provenance"]
        };
        if source_frozen && provenance["freshness"] == "current" {
            provenance["freshness"] = json!("last_known");
            provenance["reason"] = json!("frozen_pagination_cut");
        }
        if object.relation {
            relations.push(value)
        } else if aggregate {
            aggregates.push(value)
        } else if event {
            events.push(value)
        } else {
            entities.push(value)
        }
    }
    let view = json!({"api_revision":1,"world_id":world,"reorg_epoch":prepared.epoch,"scope_id":prepared.scope,"source_cut":prepared.cut,"source_binding":prepared.source_binding,"request_generation":q.request_generation,"observed_at_tick":prepared.observed_at_tick,
        "world_bounds":{"min_x_cm":"0","min_y_cm":"0","max_x_cm":space.width_cm.to_string(),"max_y_cm":space.depth_cm.to_string(),"width_cm":space.width_cm.to_string(),"depth_cm":space.depth_cm.to_string(),"height_cm":space.height_cm.to_string()},
        "coverage":{"kind":"viewer_snapshot_projection","bounds":q.bounds,"search":q.search,"detail_level":q.detail_level.as_deref().unwrap_or("entity"),"include_events":q.include_events,"event_window":{"kind":"latest_typed_candidates","candidate_limit":200,"complete_history":false},"replacement_scope":"query_only","absence_means_world_deletion":false},
        "entities":entities,"relations":relations,"aggregates":aggregates,"events":events,"next_cursor":next,"completeness":if partial {"partial"} else {"complete"},"source_frozen":source_frozen,"unavailable_reasons":[],"excluded_kinds":if q.include_events {vec!["resource"]} else {vec!["resource","event"]}});
    if view.to_string().len() > 2 * 1024 * 1024 {
        return unavailable(world, query, "response_limit_exceeded");
    }
    view
}
pub(super) struct SessionProjection {
    world: String,
    epoch: u64,
    audience: String,
    source_scope: String,
    source_cut_key: String,
    created: std::time::Instant,
    space: oasis7_simulator::SpaceConfig,
    prepared: Prepared,
    delta: Option<delta::Baseline>,
    source_confirmed: bool,
}
#[cfg(test)]
pub(super) fn build_session(
    cache: &mut Option<SessionProjection>,
    snapshot: impl FnOnce() -> WorldSnapshot,
    world: &str,
    epoch: u64,
    audience: &str,
    query: Value,
    local_runtime: bool,
) -> Value {
    let authority = if local_runtime {
        SourceAuthority::local_runtime()
    } else {
        SourceAuthority::unknown()
    };
    build_session_bound(cache, snapshot, world, epoch, audience, query, authority)
}
#[cfg(test)]
pub(super) fn build_session_bound(
    cache: &mut Option<SessionProjection>,
    snapshot: impl FnOnce() -> WorldSnapshot,
    world: &str,
    epoch: u64,
    audience: &str,
    query: Value,
    authority: SourceAuthority,
) -> Value {
    build_session_evolving(
        cache,
        snapshot,
        (world, epoch, audience),
        query,
        authority,
        false,
    )
}
pub(super) fn build_session_evolving(
    cache: &mut Option<SessionProjection>,
    snapshot: impl FnOnce() -> WorldSnapshot,
    context: (&str, u64, &str),
    query: Value,
    authority: SourceAuthority,
    delta_enabled: bool,
) -> Value {
    let (world, epoch, audience) = context;
    let (q, bounds) = match validate(&query) {
        Ok(query) => query,
        Err(reason) => return unavailable(world, &query, reason),
    };
    if q.resume_cursor.is_some() && !delta_enabled {
        return unavailable(world, &query, "capability_unavailable");
    }
    let continuation = q.cursor.is_some();
    let resumed = q.resume_cursor.is_some();
    let source_scope = authority.scope_key();
    let source_cut_key = authority.cut_key();
    let valid = cache.as_ref().is_some_and(|cached| {
        cached.world == world
            && cached.epoch == epoch
            && cached.audience == audience
            && cached.source_scope == source_scope
            && cached.created.elapsed() < std::time::Duration::from_secs(10)
    });
    if (continuation || resumed) && !valid {
        *cache = None;
        return if resumed {
            delta::reset(world, &query, "resume_cursor_invalid")
        } else {
            unavailable(world, &query, "cursor_reset_required")
        };
    }
    let freeze_continuation = continuation
        && (!authority.has_bound_cut()
            || cache
                .as_ref()
                .is_some_and(|cached| cached.source_cut_key != source_cut_key));
    let same_source = valid
        && authority.current()
        && cache
            .as_ref()
            .is_some_and(|cached| cached.source_cut_key == source_cut_key);
    if !continuation && !same_source {
        let retirement_candidates = if valid && resumed {
            cache
                .as_ref()
                .map(|cached| delta::seen_factory_ids(&cached.delta))
                .unwrap_or_default()
        } else {
            Default::default()
        };
        let previous = if valid && resumed {
            cache.as_mut().and_then(|cached| cached.delta.take())
        } else {
            None
        };
        *cache = None;
        let snapshot = snapshot();
        let source = if authority.validate_snapshot(&snapshot) {
            authority.with_retirement_candidates(retirement_candidates)
        } else {
            SourceAuthority::unknown()
        };
        let confirmed = source.has_bound_cut();
        let Some(prepared) = prepare(&snapshot, world, epoch, audience, &source) else {
            return if resumed {
                delta::reset(world, &query, "projection_limit_exceeded")
            } else {
                unavailable(world, &query, "projection_limit_exceeded")
            };
        };
        *cache = Some(SessionProjection {
            world: world.into(),
            epoch,
            audience: audience.into(),
            source_scope,
            source_cut_key: source_cut_key.clone(),
            created: std::time::Instant::now(),
            space: snapshot.config.space.clone(),
            prepared,
            delta: previous,
            source_confirmed: confirmed,
        });
    }
    let cached = cache.as_mut().expect("prepared above");
    let id = query_id(&cached.space, &cached.prepared, &q, &cached.source_scope);
    let objects = query_objects(&cached.prepared, &q, bounds);
    let mut view = build_page(
        &cached.space,
        world,
        &query,
        &cached.prepared,
        freeze_continuation,
        &objects,
    );
    if view["completeness"] == "unknown" {
        return if resumed {
            cached.delta = None;
            delta::reset(world, &query, "delta_source_unavailable")
        } else {
            view
        };
    }
    if delta_enabled {
        if let Some(resume) = &q.resume_cursor {
            if let Err(reason) = delta::update(
                &mut view,
                &mut cached.delta,
                &id,
                resume,
                &objects,
                &cached.prepared,
                cached.source_confirmed && cached.source_cut_key == source_cut_key,
            ) {
                cached.delta = None;
                return delta::reset(world, &query, reason);
            }
            cached.created = std::time::Instant::now();
        } else {
            if !continuation {
                cached.delta = None;
            }
            delta::snapshot(
                &mut view,
                &mut cached.delta,
                id,
                &objects,
                cached.source_confirmed && !freeze_continuation,
            );
        }
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::WorldKernel;
    fn snapshot() -> WorldSnapshot {
        let mut snapshot = WorldKernel::new().snapshot();
        snapshot.model.agents.insert(
            "page-a".into(),
            oasis7_simulator::Agent::new("page-a", "", crate::geometry::GeoPos::new(1, 2, 0)),
        );
        snapshot.model.agents.insert(
            "page-b".into(),
            oasis7_simulator::Agent::new("page-b", "", crate::geometry::GeoPos::new(3, 4, 0)),
        );
        snapshot
    }
    fn query() -> Value {
        json!({"request_generation":"1","page_size":1})
    }
    #[test]
    fn pages_bind_scope_cut_and_selector_without_claiming_deletion() {
        let snapshot = snapshot();
        let first = build(&snapshot, "w", 0, "alice", query());
        assert_eq!(first["coverage"]["absence_means_world_deletion"], false);
        assert_eq!(first["entities"][0]["provenance"]["freshness"], "unknown");
        assert!(first["next_cursor"].is_string());
        {
            let mut q = query();
            q["cursor"] = first["next_cursor"].clone();
            let second = build(&snapshot, "w", 0, "alice", q.clone());
            assert_eq!(first["source_cut"], second["source_cut"]);
            assert_ne!(
                first["entities"][0]["entity_id"],
                second["entities"][0]["entity_id"]
            );
            for (world, epoch, audience) in [("x", 0, "alice"), ("w", 1, "alice"), ("w", 0, "bob")]
            {
                assert_eq!(
                    build(&snapshot, world, epoch, audience, q.clone())["unavailable_reasons"][0],
                    "cursor_reset_required"
                );
            }
            let mut changed = snapshot.clone();
            changed.time += 1;
            assert_eq!(
                build(&changed, "w", 0, "alice", q)["unavailable_reasons"][0],
                "cursor_reset_required"
            );
        }
    }
    #[test]
    fn session_pages_use_bounded_frozen_cut_and_drop_other_audiences() {
        let source = snapshot();
        let mut cache = None;
        let first = build_session(
            &mut cache,
            || source.clone(),
            "w",
            0,
            "alice",
            query(),
            true,
        );
        let mut q = query();
        q["cursor"] = first["next_cursor"].clone();
        let second = build_session(
            &mut cache,
            || panic!("must not rebuild snapshot for continuation"),
            "w",
            0,
            "alice",
            q.clone(),
            true,
        );
        assert_eq!(first["source_cut"], second["source_cut"]);
        assert_eq!(
            second["entities"][0]["provenance"]["freshness"],
            "last_known"
        );
        let denied = build_session(
            &mut cache,
            || panic!("reject before access"),
            "w",
            0,
            "bob",
            q,
            true,
        );
        assert_eq!(denied["unavailable_reasons"][0], "cursor_reset_required");
        assert!(cache.is_none());
    }
    #[test]
    fn huge_private_pending_actions_do_not_enter_projection_cache() {
        let mut source = snapshot();
        let secret = "private_pending_action".repeat(1024 * 1024);
        source
            .pending_actions
            .push(oasis7_simulator::ActionEnvelope {
                id: 1,
                action: oasis7_simulator::Action::RegisterAgent {
                    agent_id: secret,
                    location_id: "private".into(),
                },
                submitter: Default::default(),
            });
        let mut cache = None;
        let view = build_session(&mut cache, || source, "w", 0, "alice", query(), true);
        let cached = cache
            .as_ref()
            .expect("cache accepts large unrelated source");
        let bytes = cached
            .prepared
            .objects
            .iter()
            .map(|e| e.value.to_string().len())
            .sum::<usize>();
        assert!(bytes < 1024 * 1024);
        assert!(!view.to_string().contains("private_pending_action"));
        // Metadata is only SpaceConfig; there is no WorldSnapshot/pending-actions field.
        assert_eq!(
            std::mem::size_of_val(&cached.space),
            std::mem::size_of::<oasis7_simulator::SpaceConfig>()
        );
    }
    #[test]
    fn invalid_session_bounds_fail_before_snapshot_work() {
        let mut q = query();
        q["bounds"] = json!({"min_x_cm":"9","max_x_cm":"2","min_y_cm":"0","max_y_cm":"1"});
        let result = build_session(
            &mut None,
            || panic!("invalid bounds must not build snapshot"),
            "w",
            0,
            "public",
            q,
            false,
        );
        assert_eq!(result["unavailable_reasons"][0], "invalid_bounds");
    }
    #[test]
    fn search_and_bounds_do_not_return_unmatched_entities() {
        let snapshot = snapshot();
        let mut q = query();
        q["search"] = json!("nonexistent-needle");
        let view = build(&snapshot, "w", 0, "public", q);
        assert_eq!(view["entities"], json!([]));
        assert_eq!(view["completeness"], "complete");
        let mut q = query();
        q["bounds"] = json!({"min_x_cm":"3","max_x_cm":"2","min_y_cm":"0","max_y_cm":"1"});
        assert_eq!(
            build(&snapshot, "w", 0, "public", q)["unavailable_reasons"][0],
            "invalid_bounds"
        );
    }
}

#[cfg(test)]
#[path = "world_view_spatial_tests.rs"]
mod spatial_tests;
