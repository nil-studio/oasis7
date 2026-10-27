//! Bounded diagnostics compiled only into explicit approval-harness artifacts.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use wasm_bindgen::JsValue;
#[derive(Clone, Serialize)]
struct CallbackSample {
    event: String,
    snapshot_digest: String,
    bytes: usize,
    decode_count: u64,
    event_clock_ms: f64,
}
#[derive(Default)]
struct Metrics {
    decode_count: u64,
    callback_count: u64,
    callbacks: Vec<CallbackSample>,
}
thread_local! { static METRICS:RefCell<Metrics>=RefCell::new(Metrics::default()); }
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
}
pub(super) fn callback(event: &str, raw: &str) -> Callback {
    Callback {
        event: event.into(),
        digest: oasis7_custody_core::approval::bytes_digest(raw.as_bytes()),
        bytes: raw.len(),
        start: clock(),
        before: count(),
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        let sample = CallbackSample {
            event: self.event.clone(),
            snapshot_digest: self.digest.clone(),
            bytes: self.bytes,
            decode_count: count().saturating_sub(self.before),
            event_clock_ms: (clock() - self.start).max(0.0),
        };
        METRICS.with(|m| {
            let mut m = m.borrow_mut();
            m.callback_count = m.callback_count.saturating_add(1);
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
    let mut samples = Vec::new();
    if matches!(mode, MeasureMode::SnapshotReuse) {
        return snapshot_reuse(raw);
    }
    let mut stage = serde_json::Value::Null;
    for _ in 0..repeats {
        let before = count();
        let started = clock();
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
        samples.push(serde_json::json!({"equal":equal,"decode_count":count().saturating_sub(before),"event_clock_ms":(clock()-started).max(0.0)}));
    }
    let (callback_count, callbacks) = METRICS.with(|m| {
        let m = m.borrow();
        (m.callback_count, m.callbacks.clone())
    });
    Ok(
        serde_json::json!({"mode":mode,"validation_scope":match mode{MeasureMode::SnapshotReuse=>"exact_bytes_restore_reuse",MeasureMode::Restore=>"full_persistence_restore",MeasureMode::ParseJsonOnly=>"json_syntax_only",MeasureMode::LegacyDoubleDecode|MeasureMode::TransitionClone=>"full_persistence_restore_and_state_eq"},"parse_succeeded":true,"stage":stage,"snapshot_digest":oasis7_custody_core::approval::bytes_digest(raw.as_bytes()),"bytes":raw.len(),"repeats":repeats,"samples":samples,"observed_callback_count":callback_count,"recent_callbacks":callbacks,"clock":"workerd_event_clock_may_not_advance_during_cpu; use_node_rpc_wall_ms","crypto_count":null}),
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
