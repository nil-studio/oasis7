//! A bounded, session-local read baseline. It never stores an execution snapshot.
use super::projection::{Object, Prepared};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
};
static STREAM_IDS: AtomicU64 = AtomicU64::new(1);
const MAX_BASELINE_OBJECTS: usize = 1000;
const MAX_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct Baseline {
    query_id: String,
    cut: String,
    sequence: u64,
    nonce: u64,
    cursor: String,
    records: BTreeMap<String, Value>,
}
pub(super) fn seen_factory_ids(baseline: &Option<Baseline>) -> BTreeSet<String> {
    baseline
        .as_ref()
        .into_iter()
        .flat_map(|baseline| baseline.records.values())
        .filter(|value| value["kind"] == "factory" && value.get("entity_id").is_some())
        .filter_map(|value| value["entity_id"].as_str().map(str::to_owned))
        .take(MAX_BASELINE_OBJECTS)
        .collect()
}
fn token(nonce: u64, sequence: u64, query: &str, cut: &str) -> String {
    blake3::hash(
        json!([nonce.to_string(), sequence.to_string(), query, cut])
            .to_string()
            .as_bytes(),
    )
    .to_hex()
    .to_string()
}
fn records(objects: &[Object]) -> BTreeMap<String, Value> {
    objects
        .iter()
        .map(|o| (o.key.clone(), o.value.clone()))
        .collect()
}
fn normalized(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        for key in ["valid_at_source_cut", "source_cut", "observed_at_tick"] {
            object.remove(key);
        }
        if let Some(provenance) = object.get_mut("provenance").and_then(Value::as_object_mut) {
            provenance.remove("valid_at_source_cut");
            provenance.remove("observed_at_tick");
        }
    }
    value
}
fn identity(value: &Value) -> (&'static str, &str) {
    for (field, kind) in [
        ("aggregate_id", "aggregate"),
        ("event_id", "event"),
        ("relation_id", "relation"),
        ("entity_id", "entity"),
    ] {
        if let Some(id) = value[field].as_str() {
            return (kind, id);
        }
    }
    ("entity", "")
}
pub(super) fn reset(world: &str, query: &Value, reason: &str) -> Value {
    json!({"world_id":world,"request_generation":query.get("request_generation"),"delivery":"reset","query_id":"","update_sequence":"0","completeness":"unknown","entities":[],"relations":[],"aggregates":[],"events":[],"removals":[],"resume_cursor":null,"unavailable_reasons":[reason]})
}
pub(super) fn snapshot(
    view: &mut Value,
    baseline: &mut Option<Baseline>,
    query_id: String,
    objects: &[Object],
    confirmed: bool,
) {
    view["delivery"] = json!("snapshot");
    view["query_id"] = json!(&query_id);
    view["update_sequence"] = json!("1");
    view["resume_cursor"] = Value::Null;
    if !confirmed
        || view["source_frozen"] == true
        || view["completeness"] != "complete"
        || !view["next_cursor"].is_null()
        || objects.len() > MAX_BASELINE_OBJECTS
        || objects
            .iter()
            .map(|o| o.value.to_string().len())
            .sum::<usize>()
            > MAX_BYTES
    {
        return;
    }
    let record_map = records(objects);
    if record_map.len() != objects.len()
        || serde_json::to_vec(&record_map).map_or(true, |bytes| bytes.len() > MAX_BYTES)
    {
        return;
    }
    let cut = view["source_cut"].as_str().unwrap_or("").to_owned();
    let nonce = STREAM_IDS.fetch_add(1, Ordering::Relaxed);
    let cursor = token(nonce, 1, &query_id, &cut);
    *baseline = Some(Baseline {
        query_id,
        cut,
        sequence: 1,
        nonce,
        cursor: cursor.clone(),
        records: record_map,
    });
    view["resume_cursor"] = json!(cursor);
    if view.to_string().len() > MAX_BYTES {
        view["resume_cursor"] = Value::Null;
        *baseline = None;
    }
}
pub(super) fn update(
    view: &mut Value,
    baseline: &mut Option<Baseline>,
    query_id: &str,
    resume: &str,
    objects: &[Object],
    prepared: &Prepared,
    confirmed: bool,
) -> Result<(), &'static str> {
    let Some(previous) = baseline.as_ref() else {
        return Err("resume_cursor_invalid");
    };
    if !confirmed || previous.query_id != query_id || previous.cursor != resume {
        return Err("resume_cursor_invalid");
    }
    if objects.len() > MAX_BASELINE_OBJECTS {
        return Err("delta_baseline_limit_exceeded");
    }
    let next = records(objects);
    if next.len() != objects.len() {
        return Err("delta_identity_conflict");
    }
    if serde_json::to_vec(&next).map_or(true, |bytes| bytes.len() > MAX_BYTES) {
        return Err("delta_baseline_limit_exceeded");
    }
    let mut entities = Vec::new();
    let mut relations = Vec::new();
    let mut aggregates = Vec::new();
    let mut events = Vec::new();
    let mut removals = Vec::new();
    for (key, value) in &next {
        if previous
            .records
            .get(key)
            .is_some_and(|old| normalized(old) == normalized(value))
        {
            continue;
        }
        match identity(value).0 {
            "aggregate" => aggregates.push(value.clone()),
            "event" => events.push(value.clone()),
            "relation" => relations.push(value.clone()),
            _ => entities.push(value.clone()),
        }
    }
    let source_keys: BTreeSet<_> = prepared
        .objects
        .iter()
        .map(|object| object.key.as_str())
        .collect();
    for (key, old) in &previous.records {
        if next.contains_key(key) {
            continue;
        }
        let (record_kind, id) = identity(old);
        let world_deleted = record_kind == "entity"
            && old["kind"] == "factory"
            && prepared.deleted_factories.contains(id);
        let reason = if world_deleted {
            "factory_recycled"
        } else if source_keys.contains(key.as_str()) {
            "left_query_scope"
        } else if record_kind == "aggregate" {
            "aggregate_replaced"
        } else {
            "projection_removed"
        };
        let mut removal = json!({"record_kind":record_kind,"id":id,"reason":reason,"world_deleted":world_deleted});
        if let Some(kind) = old.get("kind") {
            removal["kind"] = kind.clone();
        }
        removals.push(removal);
    }
    let operations =
        entities.len() + relations.len() + aggregates.len() + events.len() + removals.len();
    if operations > 200 {
        return Err("delta_operation_limit_exceeded");
    }
    let sequence = previous
        .sequence
        .checked_add(1)
        .ok_or("delta_sequence_exhausted")?;
    let cut = view["source_cut"]
        .as_str()
        .ok_or("delta_source_unavailable")?
        .to_owned();
    let cursor = token(previous.nonce, sequence, query_id, &cut);
    view["delivery"] = json!(if operations == 0 {
        "unchanged"
    } else {
        "delta"
    });
    view["query_id"] = json!(query_id);
    view["base_cut"] = json!(&previous.cut);
    view["previous_sequence"] = json!(previous.sequence.to_string());
    view["update_sequence"] = json!(sequence.to_string());
    view["resume_cursor"] = json!(&cursor);
    view["carry_forward"] = json!(true);
    view["entities"] = json!(entities);
    view["relations"] = json!(relations);
    view["aggregates"] = json!(aggregates);
    view["events"] = json!(events);
    view["removals"] = json!(removals);
    view["completeness"] = json!("complete");
    view["next_cursor"] = Value::Null;
    if view.to_string().len() > MAX_BYTES {
        return Err("delta_response_limit_exceeded");
    }
    *baseline = Some(Baseline {
        query_id: query_id.into(),
        cut,
        sequence,
        nonce: previous.nonce,
        cursor,
        records: next,
    });
    Ok(())
}
