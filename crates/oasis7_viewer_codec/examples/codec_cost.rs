//! Opt-in same-process comparison of legacy concrete serde and the codec boundary.
use std::hint::black_box;
use std::io::Write;
use std::time::Instant;

use oasis7::simulator::{Agent, WorldEvent, WorldEventKind, WorldKernel};
use oasis7_viewer_codec::{
    ViewerRequest, ViewerResponse, request_from_json, response_to_json, write_response_json,
};

#[derive(Default)]
struct ObservedWriter {
    bytes: usize,
}
impl Write for ObservedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let bytes = black_box(bytes);
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// Match the pre-extraction runtime-live erased writer, not a concrete optimized-away Sink.
#[inline(never)]
fn legacy_stream(writer: &mut dyn Write, response: &ViewerResponse) {
    serde_json::to_writer(writer, response).unwrap();
}

fn sample(label: &str, response: &ViewerResponse, iterations: usize) -> serde_json::Value {
    let expected = serde_json::to_vec(response).unwrap();
    assert_eq!(response_to_json(response).unwrap().as_bytes(), expected);
    let string_capacities = serde_json::json!({
        "legacy": serde_json::to_string(response).unwrap().capacity(),
        "codec": response_to_json(response).unwrap().capacity(),
    });
    let mut rows = Vec::new();
    // Alternate execution order to expose rather than hide order/warmup effects.
    for round in 0..6 {
        for codec in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut writer = ObservedWriter::default();
            let start = Instant::now();
            for _ in 0..iterations {
                if codec {
                    write_response_json(&mut writer, black_box(response)).unwrap();
                } else {
                    legacy_stream(&mut writer, black_box(response));
                }
            }
            let elapsed = start.elapsed().as_nanos();
            assert_eq!(black_box(writer.bytes), expected.len() * iterations);
            rows.push(serde_json::json!({"round":round, "codec":codec,
                "mode":"stream", "nanoseconds":elapsed}));
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(if codec {
                    response_to_json(black_box(response)).unwrap()
                } else {
                    serde_json::to_string(black_box(response)).unwrap()
                });
            }
            rows.push(serde_json::json!({"round":round, "codec":codec,
                "mode":"string", "nanoseconds":start.elapsed().as_nanos()}));
            let mut buffered = std::io::BufWriter::new(ObservedWriter::default());
            let start = Instant::now();
            for _ in 0..iterations {
                if codec {
                    oasis7_viewer_codec::write_buffered_response_json(
                        &mut buffered,
                        black_box(response),
                    )
                    .unwrap();
                } else {
                    serde_json::to_writer(&mut buffered, black_box(response)).unwrap();
                }
                buffered.flush().unwrap();
            }
            rows.push(serde_json::json!({"round":round, "codec":codec,
                "mode":"buffered", "nanoseconds":start.elapsed().as_nanos()}));
            assert_eq!(
                black_box(buffered.get_ref().bytes),
                expected.len() * iterations
            );
        }
    }
    serde_json::json!({"label":label, "encoded_bytes":expected.len(),
        "iterations_per_sample":iterations, "string_capacity_bytes":string_capacities, "samples":rows})
}

fn main() {
    let small = ViewerResponse::Error {
        message: "unsupported action".into(),
    };
    let mut snapshot = WorldKernel::new().snapshot();
    for i in 0..4096 {
        let id = format!("agent-{i:05}");
        snapshot.model.agents.insert(
            id.clone(),
            Agent::new(
                id,
                "location",
                oasis7::GeoPos {
                    x_cm: 0,
                    y_cm: 0,
                    z_cm: 0,
                },
            ),
        );
    }
    let large = ViewerResponse::Snapshot { snapshot };
    let event = ViewerResponse::Event {
        event: WorldEvent {
            id: 1,
            time: 1,
            kind: WorldEventKind::ActionRejected {
                reason: oasis7::simulator::RejectReason::RuleDenied {
                    notes: vec!["x".repeat(1 << 20)],
                },
            },
            runtime_event: None,
        },
    };
    let request = r#"{"type":"request_snapshot"}"#;
    let mut parse_rows = Vec::new();
    for round in 0..6 {
        for codec in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let start = Instant::now();
            for _ in 0..100_000 {
                black_box(if codec {
                    request_from_json(black_box(request)).unwrap()
                } else {
                    serde_json::from_str::<ViewerRequest>(black_box(request)).unwrap()
                });
            }
            parse_rows.push(serde_json::json!({"round":round,"codec":codec,
                "iterations":100_000,"nanoseconds":start.elapsed().as_nanos()}));
        }
    }
    println!(
        "{}",
        serde_json::json!({"schema":1,
        "scope":"same-process legacy vs codec cost; synthetic payloads; not socket latency",
        "responses":[sample("small_error", &small, 100_000),
                     sample("large_snapshot", &large, 10), sample("large_event", &event, 20)],
        "request_parse_samples":parse_rows})
    );
}
