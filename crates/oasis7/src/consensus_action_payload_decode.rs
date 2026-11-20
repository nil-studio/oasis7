//! Upper concrete DTO adapter over the single shared payload grammar.
use super::*;
use std::{borrow::Borrow, convert::Infallible};
pub trait ConsensusPayloadDecodeOpsV1 {
    type Error;
    type Value: Borrow<serde_cbor::Value>;
    type Envelope: Borrow<ConsensusActionPayloadEnvelope>;
    type RuntimeAction: Borrow<runtime::Action>;
    type Encoded: AsRef<[u8]>;
    fn decode_value(&mut self, bytes: &[u8]) -> Result<Result<Self::Value, String>, Self::Error>;
    fn decode_envelope(
        &mut self,
        bytes: &[u8],
    ) -> Result<Result<Self::Envelope, String>, Self::Error>;
    fn decode_runtime(
        &mut self,
        bytes: &[u8],
    ) -> Result<Result<Self::RuntimeAction, String>, Self::Error>;
    fn encode_envelope(
        &mut self,
        envelope: &ConsensusActionPayloadEnvelope,
    ) -> Result<Result<Self::Encoded, String>, Self::Error>;
}

pub use oasis7_client_api::consensus_action_wire::DecodedConsensusPayloadV1;

type ConsensusPayloadDecodeResultV1<Envelope, RuntimeAction, Error> =
    Result<Result<DecodedConsensusPayloadV1<Envelope, RuntimeAction>, String>, Error>;

pub fn decode_consensus_action_payload_envelope_with<O: ConsensusPayloadDecodeOpsV1>(
    payload_cbor: &[u8],
    ops: &mut O,
) -> ConsensusPayloadDecodeResultV1<O::Envelope, O::RuntimeAction, O::Error> {
    struct Lift<'a, T>(&'a mut T);
    impl<T: ConsensusPayloadDecodeOpsV1>
        oasis7_client_api::consensus_action_wire::ConsensusPayloadDecodeOpsV1<
            ConsensusActionPayloadBody,
        > for Lift<'_, T>
    {
        type Error = T::Error;
        type Value = T::Value;
        type Envelope = T::Envelope;
        type RuntimeAction = T::RuntimeAction;
        type Encoded = T::Encoded;
        fn decode_value(
            &mut self,
            bytes: &[u8],
        ) -> Result<Result<Self::Value, String>, Self::Error> {
            self.0.decode_value(bytes)
        }
        fn decode_envelope(
            &mut self,
            bytes: &[u8],
        ) -> Result<Result<Self::Envelope, String>, Self::Error> {
            self.0.decode_envelope(bytes)
        }
        fn decode_runtime(
            &mut self,
            bytes: &[u8],
        ) -> Result<Result<Self::RuntimeAction, String>, Self::Error> {
            self.0.decode_runtime(bytes)
        }
        fn encode_envelope(
            &mut self,
            value: &ConsensusActionPayloadEnvelope,
        ) -> Result<Result<Self::Encoded, String>, Self::Error> {
            self.0.encode_envelope(value)
        }
    }
    oasis7_client_api::consensus_action_wire::decode_consensus_action_payload_envelope_with(
        payload_cbor,
        &mut Lift(ops),
    )
}

pub(super) struct Legacy;
impl ConsensusPayloadDecodeOpsV1 for Legacy {
    type Error = Infallible;
    type Value = serde_cbor::Value;
    type Envelope = ConsensusActionPayloadEnvelope;
    type RuntimeAction = runtime::Action;
    type Encoded = Vec<u8>;
    fn decode_value(&mut self, bytes: &[u8]) -> Result<Result<Self::Value, String>, Infallible> {
        Ok(serde_cbor::from_slice(bytes).map_err(|error| error.to_string()))
    }
    fn decode_envelope(
        &mut self,
        bytes: &[u8],
    ) -> Result<Result<Self::Envelope, String>, Infallible> {
        Ok(serde_cbor::from_slice(bytes).map_err(|error| error.to_string()))
    }
    fn decode_runtime(
        &mut self,
        bytes: &[u8],
    ) -> Result<Result<Self::RuntimeAction, String>, Infallible> {
        Ok(serde_cbor::from_slice(bytes).map_err(|error| error.to_string()))
    }
    fn encode_envelope(
        &mut self,
        envelope: &ConsensusActionPayloadEnvelope,
    ) -> Result<Result<Vec<u8>, String>, Infallible> {
        Ok(encode_consensus_action_payload(envelope))
    }
}

#[cfg(test)]
#[path = "consensus_action_payload_decode_tests.rs"]
mod tests;
