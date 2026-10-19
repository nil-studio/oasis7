//! Query-local industrial levels over the authorized source; never business objects.
use super::{Object, Prepared};
use crate::viewer::world_view::index::Rect;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::viewer::world_view) fn query_objects(
    prepared: &Prepared,
    detail_level: &str,
    bounds: Option<Rect>,
    pins: &[(String, String)],
    search: Option<&str>,
    include_events: bool,
) -> Vec<Object> {
    let search = search.unwrap_or("").to_lowercase();
    let candidates = bounds.map_or_else(
        || (0..prepared.objects.len()).collect(),
        |b| prepared.index.query(b),
    );
    let mut chosen: BTreeSet<usize> = candidates
        .into_iter()
        .filter(|i| {
            let o = &prepared.objects[*i];
            (include_events || o.value.get("event_id").is_none())
                && (search.is_empty()
                    || ["label", "entity_id", "relation_id", "event_id"]
                        .iter()
                        .any(|k| {
                            o.value[*k]
                                .as_str()
                                .is_some_and(|s| s.to_lowercase().contains(&search))
                        }))
        })
        .collect();
    for pin in pins {
        if let Some(i) = prepared.identities.get(pin) {
            chosen.insert(*i);
        }
    }
    for i in chosen.iter().copied().collect::<Vec<_>>() {
        if prepared.objects[i].relation {
            for end in ["from", "to"] {
                let e = &prepared.objects[i].value[end];
                if let (Some(kind), Some(id)) = (e["kind"].as_str(), e["id"].as_str())
                    && let Some(i) = prepared.identities.get(&(kind.into(), id.into()))
                {
                    chosen.insert(*i);
                }
            }
        }
    }
    let objects = chosen
        .into_iter()
        .map(|i| prepared.objects[i].clone())
        .collect();
    aggregate(prepared, objects, detail_level, bounds, pins)
}

fn level<'a>(prepared: &Prepared, requested: &'a str, bounds: Option<Rect>) -> &'a str {
    if requested != "auto" {
        return requested;
    }
    let width = i128::from(prepared.space.width_cm).max(1);
    let span = bounds.map_or(width, |r| i128::from(r.1) - i128::from(r.0));
    if span * 2 >= width {
        "overview"
    } else if span * 8 >= width {
        "site"
    } else {
        "entity"
    }
}
fn integer(value: i128) -> i64 {
    value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}
fn aggregate(
    prepared: &Prepared,
    objects: Vec<Object>,
    requested: &str,
    bounds: Option<Rect>,
    pins: &[(String, String)],
) -> Vec<Object> {
    let level = level(prepared, requested, bounds);
    let cells = match level {
        "overview" => 4,
        "site" => 16,
        _ => return objects,
    };
    let width = i128::from(prepared.space.width_cm).max(1);
    let depth = i128::from(prepared.space.depth_cm).max(1);
    let dx = ((width + cells - 1) / cells).max(1);
    let dy = ((depth + cells - 1) / cells).max(1);
    let mut groups: BTreeMap<(i128, i128), Vec<usize>> = BTreeMap::new();
    for (i, o) in objects.iter().enumerate() {
        if o.value["kind"] != "factory"
            || o.value["provenance"]["freshness"] != "current"
            || o.value["provenance"]["geometry_source"] != "authoritative"
        {
            continue;
        }
        let Some(id) = o.value["entity_id"].as_str() else {
            continue;
        };
        if pins.iter().any(|p| p.0 == "factory" && p.1 == id) {
            continue;
        }
        if let Some(Rect(x, _, y, _)) = o.bounds {
            if x < 0 || y < 0 || i128::from(x) > width || i128::from(y) > depth {
                continue;
            }
            groups
                .entry((
                    i128::from(x).div_euclid(dx).min(cells - 1),
                    i128::from(y).div_euclid(dy).min(cells - 1),
                ))
                .or_default()
                .push(i);
        }
    }
    let mut replaced = BTreeSet::new();
    let mut endpoints = BTreeMap::new();
    let mut summaries = Vec::new();
    for ((x, y), members) in groups {
        if members.len() < 2 {
            continue;
        }
        let id = format!(
            "industry:{}",
            blake3::hash(
                json!([prepared.scope, level, x.to_string(), y.to_string()])
                    .to_string()
                    .as_bytes()
            )
            .to_hex()
        );
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for i in &members {
            let o = &objects[*i];
            replaced.insert(*i);
            endpoints.insert(
                o.value["entity_id"].as_str().unwrap().to_owned(),
                id.clone(),
            );
            *counts
                .entry(
                    o.value["render_record"]["kind"]
                        .as_str()
                        .unwrap_or("facility")
                        .into(),
                )
                .or_default() += 1;
        }
        let (x0, x1, y0, y1) = (
            integer(x * dx),
            integer(((x + 1) * dx).min(width)),
            integer(y * dy),
            integer(((y + 1) * dy).min(depth)),
        );
        let bounds = json!({"min_x_cm":x0.to_string(),"max_x_cm":x1.to_string(),"min_y_cm":y0.to_string(),"max_y_cm":y1.to_string()});
        let value = json!({"aggregate_id":id,"label":format!("{} facilities",members.len()),"level":level,
            "member_count":members.len().to_string(),"counts_by_kind":counts.into_iter().map(|(k,v)|(k,v.to_string())).collect::<BTreeMap<_,_>>(),
            "bounds":bounds,"position":{"x_cm":integer((i128::from(x0)+i128::from(x1))/2).to_string(),"y_cm":integer((i128::from(y0)+i128::from(y1))/2).to_string(),"z_cm":"0"},
            "provenance":{"geometry_source":"display_aggregation","freshness":"current","valid_at_source_cut":prepared.cut,"observed_at_tick":prepared.observed_at_tick},
            "drilldown":{"bounds":bounds,"detail_level":if level=="overview" {"site"}else{"entity"}}});
        summaries.push(Object {
            key: format!("aggregate:{id}"),
            value,
            relation: false,
            bounds: Some(Rect(x0, x1, y0, y1)),
        });
    }
    let mut routes: BTreeMap<String, (Value, usize)> = BTreeMap::new();
    let mut output = Vec::new();
    for (i, mut object) in objects.into_iter().enumerate() {
        if replaced.contains(&i) {
            continue;
        }
        if object.relation {
            let mut changed = false;
            for end in ["from", "to"] {
                if object.value[end]["kind"] == "factory"
                    && let Some(id) = object.value[end]["id"].as_str()
                    && let Some(aggregate) = endpoints.get(id)
                {
                    object.value[end] = json!({"kind":"aggregate","id":aggregate});
                    changed = true;
                }
            }
            if changed {
                let key = json!([
                    &object.value["from"],
                    &object.value["to"],
                    &object.value["status"],
                    &object.value["freshness"]
                ])
                .to_string();
                let entry = routes.entry(key).or_insert((object.value, 0));
                entry.1 += 1;
                continue;
            }
        }
        output.push(object);
    }
    for (key, (mut value, count)) in routes {
        let id = format!("industry-route:{}", blake3::hash(key.as_bytes()).to_hex());
        value["relation_id"] = json!(&id);
        value["kind"] = json!("industry_route");
        value["source_class"] = json!("display_aggregation");
        value["route_count"] = json!(count.to_string());
        value["label"] = json!(format!("{count} logistics routes"));
        output.push(Object {
            key: format!("relation:{id}"),
            value,
            relation: true,
            bounds: None,
        });
    }
    output.extend(summaries);
    output
}
