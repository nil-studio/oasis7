//! Bounded diagnostics compiled only into explicit approval-harness artifacts.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, JsValue};
#[derive(Clone, Serialize)]
struct CallbackSample {
    #[serde(skip_serializing)]
    actor_id: Option<String>,
    event: String,
    snapshot_digest: String,
    bytes: usize,
    decode_count: u64,
    event_clock_ms: f64,
    strict_signature_metrics: serde_json::Value,
}
#[derive(Default)]
struct Metrics {
    decode_count: u64,
    callback_count: u64,
    callbacks: Vec<CallbackSample>,
    actor_counts: Vec<(String, u64)>,
    unattributed_callbacks: u64,
    actor_counter_overflow: u64,
}
thread_local! {
    static METRICS:RefCell<Metrics>=RefCell::new(Metrics::default());
    static SIGNATURE_DEPTH:RefCell<u32>=const{RefCell::new(0)};
}
// Only samples and outer SQL callbacks own an SDK scope. Decode guards never
// reset it; nested callbacks report inherited/null rather than a false zero.
struct SignatureScope {
    owner: bool,
    closed: bool,
}
impl SignatureScope {
    fn begin() -> Self {
        let owner = SIGNATURE_DEPTH.with(|depth| {
            let mut depth = depth.borrow_mut();
            let owner = *depth == 0;
            *depth = depth.saturating_add(1);
            owner
        });
        if owner {
            oasis7_custody_core::signature_metrics::begin_scope();
        }
        Self {
            owner,
            closed: false,
        }
    }
    fn finish(&mut self) -> serde_json::Value {
        if self.closed {
            return serde_json::json!({"scope":"already_closed","summary":null});
        }
        self.closed = true;
        let summary = if self.owner {
            serde_json::to_value(oasis7_custody_core::signature_metrics::take_summary()).ok()
        } else {
            None
        };
        SIGNATURE_DEPTH.with(|depth| {
            let mut depth = depth.borrow_mut();
            *depth = depth.saturating_sub(1);
        });
        serde_json::json!({"scope":if self.owner{"owned_sdk_private_goal_strict"}else{"inherited_not_separately_measured"},"summary_available":summary.is_some(),"summary":summary})
    }
}
impl Drop for SignatureScope {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.finish();
        }
    }
}

fn clock() -> f64 {
    js_sys::Date::now()
}
fn count() -> u64 {
    METRICS.with(|m| m.borrow().decode_count)
}
pub(super) struct Decode;
pub(super) fn decode() -> Decode {
    METRICS.with(|m| {
        let mut m = m.borrow_mut();
        m.decode_count = m.decode_count.saturating_add(1);
    });
    Decode
}
pub(super) struct Callback {
    event: String,
    digest: String,
    bytes: usize,
    start: f64,
    before: u64,
    signatures: SignatureScope,
    actor_id: Option<String>,
}
pub(super) fn callback(event: &str, raw: &str, state: &JsValue) -> Callback {
    Callback {
        event: event.into(),
        digest: oasis7_custody_core::approval::bytes_digest(raw.as_bytes()),
        bytes: raw.len(),
        start: clock(),
        before: count(),
        signatures: SignatureScope::begin(),
        actor_id: actor_id(state),
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        let sample = CallbackSample {
            actor_id: self.actor_id.clone(),
            event: self.event.clone(),
            snapshot_digest: self.digest.clone(),
            bytes: self.bytes,
            decode_count: count().saturating_sub(self.before),
            event_clock_ms: (clock() - self.start).max(0.0),
            strict_signature_metrics: self.signatures.finish(),
        };
        METRICS.with(|m| {
            let mut m = m.borrow_mut();
            m.callback_count = m.callback_count.saturating_add(1);
            if let Some(id) = &sample.actor_id {
                if let Some((_, count)) = m.actor_counts.iter_mut().find(|(actor, _)| actor == id) {
                    *count = count.saturating_add(1);
                } else if m.actor_counts.len() < 32 {
                    m.actor_counts.push((id.clone(), 1));
                } else {
                    m.actor_counter_overflow = m.actor_counter_overflow.saturating_add(1);
                }
            } else {
                m.unattributed_callbacks = m.unattributed_callbacks.saturating_add(1);
            }
            if m.callbacks.len() == 32 {
                m.callbacks.remove(0);
            }
            m.callbacks.push(sample);
        });
    }
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MeasureMode {
    #[default]
    Restore,
    SnapshotReuse,
    ParseJsonOnly,
    LegacyDoubleDecode,
    TransitionClone,
}
/// No SQL, Current or mutation. ParseJsonOnly never establishes a valid state.
pub(crate) fn measure(
    raw: &str,
    repeats: u8,
    mode: MeasureMode,
) -> Result<serde_json::Value, JsValue> {
    if !(1..=3).contains(&repeats) || raw.len() > 2 * 1024 * 1024 {
        return Err(JsValue::from_str("invalid restore measurement"));
    }
    let stage_counts = structural_counts(raw)?;
    let mut samples = Vec::new();
    if matches!(mode, MeasureMode::SnapshotReuse) {
        return snapshot_reuse(raw);
    }
    let mut stage = serde_json::Value::Null;
    for _ in 0..repeats {
        let before = count();
        let started = clock();
        let mut signatures = SignatureScope::begin();
        let mut equal = None;
        match mode {
            MeasureMode::SnapshotReuse => unreachable!(),
            MeasureMode::Restore => {
                let saved = super::decode(raw)?;
                stage = serde_json::to_value(saved.state.provision().stage())
                    .map_err(|_| JsValue::from_str("stage unavailable"))?;
            }
            MeasureMode::LegacyDoubleDecode => {
                let saved = super::decode(raw)?;
                let previous = super::decode(raw)?;
                equal = Some(saved.state == previous.state);
                stage = serde_json::to_value(saved.state.provision().stage())
                    .map_err(|_| JsValue::from_str("stage unavailable"))?;
            }
            MeasureMode::TransitionClone => {
                let (saved, previous) = super::transition_snapshot(raw)?;
                equal = Some(saved.state == previous);
                stage = serde_json::to_value(saved.state.provision().stage())
                    .map_err(|_| JsValue::from_str("stage unavailable"))?;
            }
            MeasureMode::ParseJsonOnly => {
                let _parsed: serde_json::Value = serde_json::from_str(raw)
                    .map_err(|_| JsValue::from_str("measurement JSON rejected"))?;
            }
        }
        samples.push(serde_json::json!({"strict_signature_metrics":signatures.finish(),"equal":equal,"decode_count":count().saturating_sub(before),"event_clock_ms":(clock()-started).max(0.0)}));
    }
    Ok(
        serde_json::json!({"mode":mode,"validation_scope":match mode{MeasureMode::SnapshotReuse=>"exact_bytes_restore_reuse",MeasureMode::Restore=>"full_persistence_restore",MeasureMode::ParseJsonOnly=>"json_syntax_only",MeasureMode::LegacyDoubleDecode|MeasureMode::TransitionClone=>"full_persistence_restore_and_state_eq"},"parse_succeeded":true,"stage":stage,"stage_counts":stage_counts,"snapshot_digest":oasis7_custody_core::approval::bytes_digest(raw.as_bytes()),"bytes":raw.len(),"repeats":repeats,"samples":samples,"observed_callback_count":null,"recent_callbacks":[],"clock":"workerd_event_clock_may_not_advance_during_cpu; use_node_rpc_wall_ms","crypto_count":null,"signature_metrics_coverage":"SDK private_goal::verify_owner_signature_strict only; excludes Worker NativeRoot and WebCrypto; 1024 tuple fingerprints maximum; overflow_tuples are unknown uniqueness, never claimed distinct","callback_evidence_scope":"not_collected_by_isolated_restore; use the fixed actual-Account observed port; no zero-count claim"}),
    )
}

// Exercise the same reader used by live SQL reads and callbacks. No authority
// or transition result is cached; even an unchanged audit sequence is irrelevant.
fn snapshot_reuse(raw: &str) -> Result<serde_json::Value, JsValue> {
    let reader = super::SnapshotReader::default();
    let before = count();
    let first = reader.restore(raw)?;
    let same = reader.restore(raw)?;
    if first != same || count() - before != 1 {
        return Err(JsValue::from_str("identical snapshot was not reused"));
    }
    let mut changed = first.clone();
    changed.audit_sequence.0 = changed
        .audit_sequence
        .0
        .checked_add(1)
        .ok_or_else(|| JsValue::from_str("sequence overflow"))?;
    let changed_raw = super::encode(&changed)?;
    if reader.restore(&changed_raw)? != changed || count() - before != 2 {
        return Err(JsValue::from_str("changed snapshot was not restored"));
    }
    let mut invalid_schema = changed.clone();
    invalid_schema.schema_version = 0;
    let mut invalid_state: serde_json::Value =
        serde_json::from_str(&changed_raw).map_err(|_| JsValue::from_str("invalid fixture"))?;
    invalid_state["state"]["draft"]["entry"]["plan_digest"] = "00".repeat(32).into();
    for invalid in [super::encode(&invalid_schema)?, invalid_state.to_string()] {
        if reader.restore(&invalid).is_ok() {
            return Err(JsValue::from_str("changed invalid snapshot was reused"));
        }
    }
    let after_errors = count();
    if reader.restore(&changed_raw)? != changed || count() != after_errors {
        return Err(JsValue::from_str(
            "failed restore replaced the validated snapshot",
        ));
    }
    Ok(serde_json::json!({"identical_bytes_decoded_once":true,
        "changed_bytes_restored":true,"invalid_schema_rejected":true,
        "same_sequence_corruption_rejected":true,"failed_restore_not_cached":true}))
}

fn structural_counts(raw: &str) -> Result<serde_json::Value, JsValue> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| JsValue::from_str("measurement JSON rejected"))?;
    let setups = value
        .get("state")
        .and_then(|s| s.get("owner_setups"))
        .and_then(|s| s.as_object());
    let stages=["create_task","prepare_goal","publish_goal","delegation","bind","final_observation"].into_iter().map(|stage|{
        let records=setups.into_iter().flat_map(|s|s.values()).filter_map(|s|s.get("stages").and_then(|s|s.get(stage))).collect::<Vec<_>>();
        serde_json::json!({"stage":stage,"present":records.len(),"has_original":records.iter().filter(|v|v.get("original").is_some_and(|v|!v.is_null())).count(),"has_result":records.iter().filter(|v|v.get("result").is_some_and(|v|!v.is_null())).count()})
    }).collect::<Vec<_>>();
    Ok(
        serde_json::json!({"scope":"structural_counts_only_no_authority","owner_setups":setups.map_or(0,|s|s.len()),"stages":stages}),
    )
}

/// Caller must be the fixed cfg-only port on the actual Account actor. Reading
/// another actor/isolated benchmark does not establish business callback coverage.
pub(crate) fn observed_callbacks(state: &JsValue) -> serde_json::Value {
    let id = actor_id(state);
    METRICS.with(|metrics|{
        let metrics=metrics.borrow();
        let samples=id.as_ref().map(|id|metrics.callbacks.iter().filter(|sample|sample.actor_id.as_ref()==Some(id)).cloned().collect::<Vec<_>>()).unwrap_or_default();
        let count=id.as_ref().and_then(|id|metrics.actor_counts.iter().find(|(actor,_)|actor==id).map(|(_,count)|*count));
        let count=if id.is_some()&&count.is_none()&&metrics.actor_counter_overflow==0 {Some(0)}else{count};
        serde_json::json!({"evidence_scope":"actual_Account_actor_synchronous_SQL_callbacks_only","actor_identity_available":id.is_some(),"same_instance_all_actor_callback_count":metrics.callback_count,"attribution_complete":id.is_some()&&metrics.unattributed_callbacks==0&&metrics.actor_counter_overflow==0,"observed_callback_count":count,"ring_capacity":32,"dropped_samples":count.map(|count|count.saturating_sub(samples.len() as u64)),"unattributed_callbacks":metrics.unattributed_callbacks,"actor_counter_overflow":metrics.actor_counter_overflow,"recent_callbacks":samples,"signature_metrics_coverage":"SDK private_goal::verify_owner_signature_strict only; excludes Worker NativeRoot and WebCrypto; nested callback summary is inherited/null; overflow tuples have unknown uniqueness"})
    })
}
fn actor_id(state: &JsValue) -> Option<String> {
    let id = js_sys::Reflect::get(state, &JsValue::from_str("id")).ok()?;
    let to_string: js_sys::Function = js_sys::Reflect::get(&id, &JsValue::from_str("toString"))
        .ok()?
        .dyn_into()
        .ok()?;
    let value = to_string.call0(&id).ok()?.as_string()?;
    (!value.is_empty() && value.len() <= 256).then_some(value)
}
