//! Challenge-bound fresh reads; the nonce correlates a response, never grants access.
use super::*;

pub const TASK_CONTEXT_LIVE_READ_PATH: &str = "/v1/world/private-task/context-read-live";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextLiveReadV1 {
    pub original: SignedReadRequest<TaskContextGetV1>,
    pub observation_nonce: String,
}

pub fn verify_live_context_read(request: &TaskContextLiveReadV1) -> Result<(), String> {
    if request.observation_nonce.len() != 64
        || !request
            .observation_nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid live context observation nonce".into());
    }
    verify_get(&request.original)
}

type LiveContextReadResponse<C, P, L, CP, ST, R, D, X> = SignedServiceResponse<
    CapturedTaskContextV1<C, SchedulerIntentV1<SchedulerOperationV1<P, L, CP, ST, R, D, X>>>,
>;

pub fn verify_live_context_reply<
    C: Serialize,
    P: Serialize + PartialEq,
    L: Serialize + PartialEq,
    CP: Serialize + PartialEq,
    ST: Serialize + PartialEq,
    R: Serialize + PartialEq,
    D: Serialize + PartialEq,
    X: Serialize + PartialEq,
>(
    request: &TaskContextLiveReadV1,
    response: &LiveContextReadResponse<C, P, L, CP, ST, R, D, X>,
    world_public_key: &str,
) -> Result<(), String> {
    verify_live_context_read(request)?;
    verify_private_reply(
        TASK_CONTEXT_LIVE_READ_PATH,
        request,
        response,
        world_public_key,
    )?;
    response.payload.validate_against(&request.original)
}
