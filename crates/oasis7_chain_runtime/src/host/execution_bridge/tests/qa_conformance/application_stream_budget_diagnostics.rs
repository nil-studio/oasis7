//! Relay only the bounded numeric diagnostic schema past libtest's output capture.
use std::io::Write;
const PREFIX: &str = "stream_budget_numeric=";
const ROWS_PER_LINE: usize = 64;
const MAX_PARTS: usize = (4096usize + 16).div_ceil(ROWS_PER_LINE);
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    rows: Vec<[u64; 6]>,
    dropped: u64,
    part: usize,
    parts: usize,
}
fn validated(line: &[u8]) -> Option<Vec<u8>> {
    if line.len() > 16_000 {
        return None;
    }
    let json = line.strip_prefix(PREFIX.as_bytes())?;
    let evidence: Evidence = serde_json::from_slice(json).ok()?;
    if evidence.rows.len() > ROWS_PER_LINE
        || evidence.parts == 0
        || evidence.parts > MAX_PARTS
        || evidence.part >= evidence.parts
        || evidence.rows.iter().any(|row| {
            !matches!(row[0], 1..=7 | 10 | 20..=23 | 30..=35 | 40..=42 | 100..=101 | 200)
                || row[2] > row[3]
        })
    {
        return None;
    }
    let mut bytes = PREFIX.as_bytes().to_vec();
    serde_json::to_writer(&mut bytes, &evidence).ok()?;
    bytes.push(b'\n');
    Some(bytes)
}
fn relay_stream(stream: &[u8]) {
    for line in stream.split(|byte| *byte == b'\n') {
        if let Some(bytes) = validated(line) {
            // Direct fd write is intentional: println! is captured by the
            // outer ordinary libtest batch even when this child succeeds.
            std::io::stdout().lock().write_all(&bytes).unwrap();
        }
    }
}
pub(super) fn relay(output: &std::process::Output) {
    relay_stream(&output.stdout);
    relay_stream(&output.stderr);
}
pub(super) fn dump(samples: &[serde_json::Value]) {
    let mut evidence =
        oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServer::test_take_stream_budget_diagnostics(
        );
    // Keep every actual sample independently of ring saturation or try_lock drops.
    for (index, sample) in samples.iter().enumerate() {
        let start = sample["diagnostic_started_us"].as_u64().unwrap();
        let end = sample["diagnostic_finished_us"].as_u64().unwrap();
        let flags = u64::from(sample["response_within_150ms"] == true)
            | (u64::from(sample["original_response_complete"] == true) << 1)
            | (u64::from(sample["under_pressure"] == true) << 2);
        let rows = evidence["rows"].as_array_mut().unwrap();
        rows.push(serde_json::json!([
            100,
            0,
            start,
            end,
            index,
            sample["useful_elapsed_us"]
        ]));
        rows.push(serde_json::json!([101, 0, start, end, index, flags]));
    }
    // Both serving workers have finished; no Viewer guard or measured interval remains.
    let rows = evidence["rows"].as_array().unwrap();
    let parts = rows.len().div_ceil(ROWS_PER_LINE);
    for (part, rows) in rows.chunks(ROWS_PER_LINE).enumerate() {
        let chunk = serde_json::json!({"rows": rows, "dropped": evidence["dropped"], "part": part, "parts": parts});
        let line = format!("{PREFIX}{chunk}");
        let bytes = validated(line.as_bytes()).expect("numeric bounded diagnostic schema");
        std::io::stdout().lock().write_all(&bytes).unwrap();
    }
}
#[test]
fn numeric_relay_rejects_payloads_and_oversize_records() {
    assert!(validated(b"stream_budget_numeric={\"rows\":[[3,1,0,1,0,0]],\"dropped\":0,\"part\":0,\"parts\":1}").is_some());
    for json in [
        r#"{"rows":[],"dropped":0,"part":0,"parts":1,"payload":"private"}"#,
        r#"{"rows":[],"dropped":0,"part":0,"parts":0}"#,
        r#"{"rows":[],"dropped":0,"part":1,"parts":1}"#,
        r#"{"rows":[],"dropped":0,"part":0,"parts":66}"#,
        r#"{"rows":[[3,1,0,1,"private",0]],"dropped":0,"part":0,"parts":1}"#,
        r#"{"rows":[[999,1,0,1,0,0]],"dropped":0,"part":0,"parts":1}"#,
        r#"{"rows":[[3,1,2,1,0,0]],"dropped":0,"part":0,"parts":1}"#,
    ] {
        assert!(validated(format!("{PREFIX}{json}").as_bytes()).is_none());
    }
    let oversized = Evidence {
        rows: vec![[3, 1, 0, 1, 0, 0]; ROWS_PER_LINE + 1],
        dropped: 0,
        part: 0,
        parts: 1,
    };
    assert!(
        validated(format!("{PREFIX}{}", serde_json::to_string(&oversized).unwrap()).as_bytes())
            .is_none()
    );
}
#[test]
fn numeric_relay_survives_ordinary_libtest_capture() {
    const ENV: &str = "OASIS7_NUMERIC_RELAY_CAPTURE_PROBE";
    if std::env::var_os(ENV).is_some() {
        relay_stream(b"stream_budget_numeric={\"rows\":[[3,1,0,1,0,0]],\"dropped\":0,\"part\":0,\"parts\":1}\n");
        return;
    }
    let name = concat!(
        module_path!(),
        "::numeric_relay_survives_ordinary_libtest_capture"
    );
    let name = name.split_once("::").unwrap().1;
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name])
        .env(ENV, "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "capture probe must actually pass");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("running 1 test"),
        "exact probe must execute"
    );
    assert!(
        stdout
            .lines()
            .any(|line| validated(line.as_bytes()).is_some()),
        "numeric evidence must survive default libtest capture on PASS: {stdout}"
    );
}
