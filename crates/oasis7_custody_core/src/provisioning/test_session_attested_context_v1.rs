//! Public World facts for the original V1 Move grant. No private body or send grant.
use super::*;
use oasis7_client_api::world_service::{task_context::*, task_context_projection::*};
use oasis7_client_api::world_service_wire::{
    CognitionIntentV1, TaskSchedulerIntentV1, WorldServicePayloadV1,
};
use serde::de::DeserializeOwned;

const MAX_PUBLIC_FACT_BYTES: usize = 256 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestedMoveProfileV1 {
    AttestedMoveV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestedMoveFactStageV1 {
    Prepared,
    Captured,
    Move,
}
/// Closed public proof bytes. Every use re-verifies exact typed data with the
/// configured World key; persistence never reconstructs a usable context token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestedMovePublicFactV1 {
    profile: AttestedMoveProfileV1,
    stage: AttestedMoveFactStageV1,
    canonical_cbor_hex: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicFact<Q, P> {
    request: Q,
    projection: WorldContextAttestationV2<P>,
}
#[derive(Serialize)]
struct PublicFactRef<'a, Q, P> {
    request: &'a Q,
    projection: &'a WorldContextAttestationV2<P>,
}
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    result
}
fn decode_hex(value: &str) -> Result<Vec<u8>> {
    fn digit(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }
    if value.len() % 2 != 0 {
        return Err(unavailable());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            Ok((digit(pair[0]).ok_or_else(unavailable)? << 4)
                | digit(pair[1]).ok_or_else(unavailable)?)
        })
        .collect()
}
fn unavailable() -> ProvisionApprovalError {
    ProvisionApprovalError::BindingMismatch
}
impl AttestedMovePublicFactV1 {
    fn encode<Q: Serialize, P: Serialize>(
        stage: AttestedMoveFactStageV1,
        request: &Q,
        projection: &WorldContextAttestationV2<P>,
    ) -> Result<Self> {
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&PublicFactRef {
            request,
            projection,
        })
        .map_err(|_| unavailable())?;
        if bytes.is_empty() || bytes.len() > MAX_PUBLIC_FACT_BYTES {
            return Err(unavailable());
        }
        Ok(Self {
            profile: AttestedMoveProfileV1::AttestedMoveV1,
            stage,
            canonical_cbor_hex: encode_hex(&bytes),
        })
    }
    fn decode<Q: DeserializeOwned + Serialize, P: DeserializeOwned + Serialize>(
        &self,
        stage: AttestedMoveFactStageV1,
    ) -> Result<PublicFact<Q, P>> {
        if self.profile != AttestedMoveProfileV1::AttestedMoveV1
            || self.stage != stage
            || self.canonical_cbor_hex.len() > MAX_PUBLIC_FACT_BYTES * 2
        {
            return Err(unavailable());
        }
        let bytes = decode_hex(&self.canonical_cbor_hex)?;
        if bytes.is_empty() || encode_hex(&bytes) != self.canonical_cbor_hex {
            return Err(unavailable());
        }
        let fact: PublicFact<Q, P> = serde_cbor::from_slice(&bytes).map_err(|_| unavailable())?;
        if oasis7_wasm_abi::encode_canonical_cbor(&fact).map_err(|_| unavailable())? != bytes {
            return Err(unavailable());
        }
        Ok(fact)
    }
    pub fn prepared(
        request: &TaskContextPrepareAttestationRequestV2,
        projection: &WorldContextAttestationV2<TaskContextPreparedProjectionV2>,
        configured_world_key: &str,
    ) -> Result<Self> {
        verify_prepared_projection_v2(request, projection, configured_world_key)
            .map_err(|_| unavailable())?;
        Self::encode(AttestedMoveFactStageV1::Prepared, request, projection)
    }
    pub fn captured(
        request: &TaskContextGetAttestationRequestV2,
        projection: &WorldContextAttestationV2<TaskContextCapturedProjectionV2>,
        expected_prefix: &ws::SubmitIntentRequest<WorldServicePayloadV1>,
        configured_world_key: &str,
    ) -> Result<Self> {
        verify_captured_projection_v2(request, projection, configured_world_key)
            .map_err(|_| unavailable())?;
        Self::check_prefix(projection, expected_prefix)?;
        Self::encode(AttestedMoveFactStageV1::Captured, request, projection)
    }
    fn check_prefix(
        projection: &WorldContextAttestationV2<TaskContextCapturedProjectionV2>,
        expected_prefix: &ws::SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<()> {
        expected_prefix.validate().map_err(|_| unavailable())?;
        if projection.payload.prefix_correlation != expected_prefix.correlation
            || projection.payload.prefix_original_digest
                != ws::request_digest("custody-context-prefix-original/v2", expected_prefix)
                    .map_err(|_| unavailable())?
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub fn movement(
        request: &TaskContextMoveProjectionRequestV2,
        projection: &WorldContextAttestationV2<TaskContextMoveProjectionV2>,
        configured_world_key: &str,
    ) -> Result<Self> {
        verify_move_projection_v2(request, projection, configured_world_key)
            .map_err(|_| unavailable())?;
        Self::encode(AttestedMoveFactStageV1::Move, request, projection)
    }
    /// Pure candidates only. The Account must separately verify the original
    /// V1 grant, approved coordinates, paid rows and fresh Current before signing.
    pub fn prepared_candidates(
        &self,
        expected: &TaskContextPrepareAttestationRequestV2,
        configured_world_key: &str,
    ) -> Result<(TaskSchedulerIntentV1, TaskContextGetV1)> {
        let f = self
            .decode::<TaskContextPrepareAttestationRequestV2, TaskContextPreparedProjectionV2>(
                AttestedMoveFactStageV1::Prepared,
            )?;
        if &f.request != expected {
            return Err(unavailable());
        }
        let verified =
            verify_prepared_projection_v2(&f.request, &f.projection, configured_world_key)
                .map_err(|_| unavailable())?;
        Ok((
            verified.prefix_candidate().map_err(|_| unavailable())?,
            verified.get_candidate(),
        ))
    }
    pub fn verify_captured(
        &self,
        expected: &TaskContextGetAttestationRequestV2,
        expected_prefix: &ws::SubmitIntentRequest<WorldServicePayloadV1>,
        configured_world_key: &str,
    ) -> Result<()> {
        let f = self
            .decode::<TaskContextGetAttestationRequestV2, TaskContextCapturedProjectionV2>(
                AttestedMoveFactStageV1::Captured,
            )?;
        if &f.request != expected {
            return Err(unavailable());
        }
        verify_captured_projection_v2(&f.request, &f.projection, configured_world_key)
            .map_err(|_| unavailable())?;
        Self::check_prefix(&f.projection, expected_prefix)
    }
    pub fn move_candidate(
        &self,
        expected: &TaskContextMoveProjectionRequestV2,
        approved_position_cm: [i64; 3],
        configured_world_key: &str,
    ) -> Result<CognitionIntentV1> {
        let f = self.decode::<TaskContextMoveProjectionRequestV2, TaskContextMoveProjectionV2>(
            AttestedMoveFactStageV1::Move,
        )?;
        if &f.request != expected {
            return Err(unavailable());
        }
        verify_move_projection_v2(&f.request, &f.projection, configured_world_key)
            .map_err(|_| unavailable())?
            .move_candidate(approved_position_cm)
            .map_err(|_| unavailable())
    }
}
