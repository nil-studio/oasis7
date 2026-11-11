//! Hold the original settled Wait before its real wake, after recovery observation starts.
use super::*;

const ENABLE: &str = "PRE2_WAIT_RECOVERY_DELAY_PROBE";
const READY: &str = "hosted-wait-recovery-observer-ready.json";
const HOLD: Duration = Duration::from_secs(13);

pub(super) fn enabled() -> bool {
    std::env::var_os(ENABLE).is_some()
}

pub(super) fn enable(command: &mut std::process::Command) {
    command.env(ENABLE, "1");
}

pub(super) fn observer_ready(root: &std::path::Path) {
    if !enabled() {
        return;
    }
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap()).unwrap();
    let marker = serde_json::json!({
        "request_digest": origin["request_digest"],
        "lease_id": origin["lease_id"]
    });
    assert!(same_origin(&marker, &origin));
    application_hosted_wait::publish_marker(
        &root.join(READY),
        &serde_json::to_vec(&marker).unwrap(),
    )
    .unwrap();
}

fn same_origin(marker: &serde_json::Value, origin: &serde_json::Value) -> bool {
    ["request_digest", "lease_id"].iter().all(|key| {
        origin[*key]
            .as_str()
            .is_some_and(|value| !value.is_empty() && marker[*key].as_str() == Some(value))
    })
}

#[derive(Default)]
pub(super) struct Delay {
    started: Option<Instant>,
    released: bool,
}

impl Delay {
    /// Called only while the same canonical Admit and settled lease are verified.
    /// False keeps the existing clock loop pending; it never holds a lock or RPC open.
    pub(super) fn ready(
        &mut self,
        root: &std::path::Path,
        origin: &serde_json::Value,
        wake_selected: bool,
    ) -> bool {
        if self.released {
            return true;
        }
        assert!(
            !wake_selected && !root.join("hosted-resumed-model.json").exists(),
            "causal delay must precede the real original wake and resumed model"
        );
        let marker = match fs::read(root.join(READY)) {
            Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
            Err(error) => panic!("read recovery observer identity: {error}"),
        };
        assert!(
            same_origin(&marker, origin),
            "recovery observer must bind the original Wait"
        );
        let started = self.started.get_or_insert_with(|| {
            println!(
                "hosted_wait_recovery_delay_started original_request_digest={} lease_id={} required_ms={}",
                origin["request_digest"], origin["lease_id"], HOLD.as_millis()
            );
            Instant::now()
        });
        let elapsed = started.elapsed();
        if elapsed < HOLD {
            return false;
        }
        self.released = true;
        println!(
            "hosted_wait_recovery_delay_released elapsed_ms={} original_request_digest={} lease_id={}",
            elapsed.as_millis(),
            origin["request_digest"],
            origin["lease_id"]
        );
        true
    }
}

#[test]
fn recovery_observer_requires_both_original_digest_and_lease() {
    let origin =
        serde_json::json!({"request_digest":"original-digest","lease_id":"original-lease"});
    assert!(same_origin(&origin, &origin));
    for marker in [
        serde_json::Value::Null,
        serde_json::json!({"request_digest":"replacement","lease_id":"original-lease"}),
        serde_json::json!({"request_digest":"original-digest","lease_id":"replacement"}),
        serde_json::json!({"request_digest":"original-digest"}),
    ] {
        assert!(!same_origin(&marker, &origin));
    }
    assert!(!same_origin(
        &serde_json::Value::Null,
        &serde_json::Value::Null
    ));
}

#[test]
fn absent_observer_cannot_start_the_recovery_delay() {
    let root = temp_dir("qa-wait-recovery-observer-absent");
    fs::create_dir_all(&root).unwrap();
    let origin =
        serde_json::json!({"request_digest":"original-digest","lease_id":"original-lease"});
    let mut delay = Delay::default();
    assert!(!delay.ready(&root, &origin, false));
    assert!(delay.started.is_none() && !delay.released);
    fs::remove_dir_all(root).unwrap();
}
