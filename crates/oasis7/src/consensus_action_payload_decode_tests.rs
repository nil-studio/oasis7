use super::*;
#[derive(Debug, PartialEq)]
enum Stop { Cancelled }
struct Trace { calls: Vec<&'static str>, stop_envelope: bool }
impl ConsensusPayloadDecodeOpsV1 for Trace {
    type Error = Stop;
    type Value = serde_cbor::Value;
    type Envelope = ConsensusActionPayloadEnvelope;
    type RuntimeAction = runtime::Action;
    type Encoded = Vec<u8>;
    fn decode_value(&mut self, bytes: &[u8]) -> Result<Result<Self::Value, String>, Stop> {
        self.calls.push("value"); Ok(Legacy.decode_value(bytes).unwrap())
    }
    fn decode_envelope(&mut self, bytes: &[u8]) -> Result<Result<Self::Envelope, String>, Stop> {
        self.calls.push("envelope");
        if self.stop_envelope { return Err(Stop::Cancelled); }
        Ok(Legacy.decode_envelope(bytes).unwrap())
    }
    fn decode_runtime(&mut self, bytes: &[u8]) -> Result<Result<Self::RuntimeAction, String>, Stop> {
        self.calls.push("runtime"); Ok(Legacy.decode_runtime(bytes).unwrap())
    }
    fn encode_envelope(&mut self, envelope: &ConsensusActionPayloadEnvelope) -> Result<Result<Vec<u8>, String>, Stop> {
        self.calls.push("encode"); Ok(Legacy.encode_envelope(envelope).unwrap())
    }
}
fn trace(stop_envelope: bool) -> Trace { Trace { calls: Vec::new(), stop_envelope } }
fn action() -> runtime::Action { runtime::Action::QueryObservation { agent_id: "original-agent".into() } }
#[test]
fn fatal_typed_failure_cannot_become_a_valid_legacy_fallback() {
    let bytes = serde_cbor::to_vec(&action()).unwrap();
    let mut ops = trace(true);
    assert!(matches!(decode_consensus_action_payload_envelope_with(&bytes, &mut ops), Err(Stop::Cancelled)));
    assert_eq!(ops.calls, ["value", "envelope"]);
}
#[test]
fn ordinary_envelope_codec_failure_keeps_original_legacy_acceptance() {
    let expected = action(); let bytes = serde_cbor::to_vec(&expected).unwrap();
    let mut ops = trace(false);
    let decoded = decode_consensus_action_payload_envelope_with(&bytes, &mut ops).unwrap().unwrap();
    assert!(matches!(decoded, DecodedConsensusPayloadV1::LegacyRuntime(actual) if actual == expected));
    assert_eq!(ops.calls, ["value", "envelope", "value", "runtime"]);
}
#[test]
fn recognized_null_metadata_remains_the_first_error() {
    let mut value = match serde_cbor::value::to_value(ConsensusActionPayloadEnvelope::from_runtime_action(action())).unwrap() {
        serde_cbor::Value::Map(map) => map, _ => panic!("original envelope map"),
    };
    value.insert(serde_cbor::Value::Text("gameplay_submission_origin".into()), serde_cbor::Value::Null);
    let bytes = serde_cbor::to_vec(&serde_cbor::Value::Map(value)).unwrap();
    let mut ops = trace(true);
    let result = decode_consensus_action_payload_envelope_with(&bytes, &mut ops).unwrap();
    assert!(matches!(result, Err(error) if error == "gameplay submission origin must not be null"));
    assert_eq!(ops.calls, ["value"]);
}
#[test]
fn actual_version_gate_cannot_be_overridden_by_the_decode_adapter() {
    let mut source = ConsensusActionPayloadEnvelope::from_runtime_action(action());
    source.version = 2;
    let bytes = serde_cbor::to_vec(&source).unwrap();
    let mut ops = trace(false);
    let result = decode_consensus_action_payload_envelope_with(&bytes, &mut ops).unwrap();
    assert!(matches!(result, Err(error) if error == "unsupported consensus payload envelope version/origin combination 2"));
    assert_eq!(ops.calls, ["value", "envelope"]);
}
