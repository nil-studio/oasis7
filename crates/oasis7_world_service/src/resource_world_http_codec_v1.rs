//! Fixed paid World HTTP originals. Decoding never asserts payment/current.
use crate::resource_world_paid_ledger_v1::*;
use oasis7_client_api::world_service::{
    participant_discovery::{PARTICIPANT_OWNER_VIEW_PATH, ParticipantOwnerViewRequestV1},
    resource_basis::RESOURCE_GOAL_PATH_V2,
};
use serde::{Serialize, de::DeserializeOwned};
pub enum ResourceWorldHttpOriginalV1 {
    OwnerPreflight {
        proof: PaidResourceReadProofV1,
        request: Box<ParticipantOwnerViewRequestV1>,
    },
    ResourceFinal {
        proof: PaidResourceReadProofV1,
        request: Box<OwnerSetupResourceWorldBodyV2>,
    },
}
fn strict<T: DeserializeOwned + Serialize>(body: &[u8]) -> Result<T, String> {
    if body.is_empty() || body.len() > 64 * 1024 {
        return Err("Resource World body bound".into());
    }
    let value: T = serde_json::from_slice(body).map_err(|_| "Resource World body codec")?;
    if serde_json::to_vec(&value).map_err(|_| "Resource World body codec")? != body {
        return Err("Resource World exact body codec".into());
    }
    Ok(value)
}
impl ResourceWorldHttpOriginalV1 {
    pub fn decode(path: &str, body: &[u8], header: &str) -> Result<Self, String> {
        let proof = PaidResourceReadProofV1::decode_http_header(header)?;
        match path {
            PARTICIPANT_OWNER_VIEW_PATH => Ok(Self::OwnerPreflight {
                proof,
                request: Box::new(strict(body)?),
            }),
            RESOURCE_GOAL_PATH_V2 => Ok(Self::ResourceFinal {
                proof,
                request: Box::new(strict(body)?),
            }),
            _ => Err("Resource World exact paid route".into()),
        }
    }
    /// Only the ledger's real commit produces the consumed token. The HTTP
    /// adapter verifies fixed server configuration before invoking this.
    pub fn claim(
        self,
        ledger: &mut WorldResourcePaidLedgerV1,
    ) -> Result<ConsumedWorldResourceAttemptV1, String> {
        match self {
            Self::OwnerPreflight { proof, request } => {
                ledger.claim_owner_preflight(proof, *request)
            }
            Self::ResourceFinal { proof, request } => ledger.claim_resource_final(proof, *request),
        }
    }
}
