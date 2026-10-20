use super::{
    execution_bridge::world_service_read::{self, PinnedWorld},
    feedback_submit_api::FeedbackSubmitSigner,
};
use oasis7::world_service::*;
use oasis7_node::NodeRuntime;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    net::TcpStream,
    path::Path,
    sync::{Arc, Mutex},
};

#[path = "membership_operations.rs"]
mod membership_operations;
#[path = "world_service_operations.rs"]
mod operations;
#[path = "world_service_private_goal.rs"]
pub(super) mod private_goal;

pub(super) struct Service<'a> {
    records: &'a Path,
    storage: &'a Path,
    identity: WorldIdentity,
    signer: &'a FeedbackSubmitSigner,
    runtime: &'a Arc<Mutex<NodeRuntime>>,
    guarded: Option<(
        &'a Path,
        &'a super::controlled_live_config::GuardedReadAuthority,
    )>,
}
impl Service<'_> {
    fn pin(&self, fixed: Option<&CommitRef>) -> Result<PinnedWorld, String> {
        if let Some((config, authority)) = self.guarded {
            world_service_read::pin_guarded(self.records, config, authority, fixed, None)
        } else {
            world_service_read::pin(self.records, self.storage, &self.identity, fixed)
        }
    }
    fn pin_at_height(&self, height: u64) -> Result<PinnedWorld, String> {
        if let Some((config, authority)) = self.guarded {
            world_service_read::pin_guarded(self.records, config, authority, None, Some(height))
        } else {
            world_service_read::pin_at_height(self.records, self.storage, &self.identity, height)
        }
    }
    fn signed<Q: Serialize, T: Serialize>(
        &self,
        path: &str,
        request: &Q,
        payload: T,
    ) -> Result<Value, String> {
        serde_json::to_value(sign_service_response(
            path,
            request_digest(path, request)?,
            payload,
            &self.signer.private_key_hex,
        )?)
        .map_err(|e| e.to_string())
    }
    fn scope(
        &self,
        pinned: &PinnedWorld,
        scope: &str,
        key: &str,
    ) -> Result<Option<String>, String> {
        if scope == "public" {
            return Ok(None);
        }
        let agent = scope
            .strip_prefix("agent:")
            .ok_or("unsupported visibility scope")?;
        let generation = if agent_authority::owner_public_key(&pinned.world, agent)? == key {
            0
        } else {
            pinned
                .world
                .capability_revocation_state()
                .agent_signer_delegations
                .get(agent)
                .ok_or("scope delegation unavailable")?
                .generation
        };
        agent_authority::validate_agent_signer(&pinned.world, agent, key, generation)?;
        Ok(Some(agent.into()))
    }
    // Exact settled results remain readable under current Agent signer authority.
    // This authenticates receipt access, not a new write or executor admission.
    fn check_replay_payload(
        &self,
        pinned: &PinnedWorld,
        payload: &WorldServicePayloadV1,
    ) -> Result<(), String> {
        match payload {
            WorldServicePayloadV1::Cognition(signed) => agent_authority::validate_agent_signer(
                &pinned.world,
                &signed.request.request.agent_id,
                &signed.subject_public_key,
                signed.request.delegation_generation,
            ),
            WorldServicePayloadV1::Scheduler(signed) => agent_authority::validate_agent_signer(
                &pinned.world,
                &signed.request.agent_id,
                &signed.subject_public_key,
                signed.request.delegation_generation,
            ),
            WorldServicePayloadV1::GameExecutorControl(signed) => {
                if agent_authority::owner_public_key(&pinned.world, &signed.request.fence.agent_id)?
                    != signed.subject_public_key
                {
                    return Err("executor receipt requires current owner".into());
                }
                Ok(())
            }
            _ => self.check_payload(pinned, payload),
        }
    }

    fn check_payload(
        &self,
        pinned: &PinnedWorld,
        payload: &WorldServicePayloadV1,
    ) -> Result<(), String> {
        match payload {
            WorldServicePayloadV1::GameTaskCreate(signed) => {
                oasis7::world_service::game_task::validate_owner(&pinned.world, signed)?;
            }
            WorldServicePayloadV1::GameExecutorControl(signed) => {
                game_task::validate_executor_owner(&pinned.world, signed)?;
            }
            WorldServicePayloadV1::GameGoalPublish(signed) => {
                game_task::validate_publish_owner(&pinned.world, signed)?;
            }
            WorldServicePayloadV1::GameplayJson(bytes) => {
                gameplay::authenticated_action(&pinned.world, bytes)?;
            }
            WorldServicePayloadV1::AgentChat(request) => {
                agent_chat::validate_owner(&pinned.world, request)?
            }
            WorldServicePayloadV1::Cognition(signed) => {
                agent_authority::validate_cognition(&pinned.world, signed)?
            }
            WorldServicePayloadV1::FeedbackAck(signed) => {
                agent_authority::validate_feedback_ack(&pinned.world, signed)?
            }
            WorldServicePayloadV1::Delegation(signed) => {
                agent_authority::validate_delegation(&pinned.world, signed)?
            }
            WorldServicePayloadV1::Scheduler(signed) => {
                verify_read_request("scheduler", signed)?;
                agent_authority::validate_agent_signer(
                    &pinned.world,
                    &signed.request.agent_id,
                    &signed.subject_public_key,
                    signed.request.delegation_generation,
                )?;
            }
        }
        Ok(())
    }
}

const UNSUPPORTED_REQUEST_FIELD: &str = "unsupported service request field";
const PUBLIC_SERVICE_UNAVAILABLE: &str = "service request cannot be served";

fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    if body.len() > 256 * 1024 {
        return Err("service request exceeds byte bound".into());
    }
    let mut decoder = serde_json::Deserializer::from_slice(body);
    let mut ignored = false;
    let request = serde_ignored::deserialize(&mut decoder, |_| ignored = true)
        .map_err(|error| error.to_string())?;
    decoder.end().map_err(|error| error.to_string())?;
    if ignored {
        // Bounded deterministic error: never echo untrusted field names or
        // private payload material. Known null/default fields are not ignored.
        return Err(UNSUPPORTED_REQUEST_FIELD.into());
    }
    Ok(request)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Existing status dispatcher carries explicit authority and storage roots."
)]
pub(super) fn maybe_handle(
    stream: &mut TcpStream,
    bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    path: &str,
    world_id: &str,
    world_dir: &Path,
    records: &Path,
    storage: &Path,
    signer: &FeedbackSubmitSigner,
) -> Result<bool, String> {
    maybe_handle_with_guarded(
        stream, bytes, runtime, method, path, world_id, world_dir, records, storage, signer, None,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Existing dispatcher authority/storage roots plus explicit engineering policy."
)]
pub(super) fn maybe_handle_with_guarded(
    stream: &mut TcpStream,
    bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    path: &str,
    world_id: &str,
    world_dir: &Path,
    records: &Path,
    storage: &Path,
    signer: &FeedbackSubmitSigner,
    guarded: Option<(&Path, &super::controlled_live_config::GuardedReadAuthority)>,
) -> Result<bool, String> {
    if ![
        DESCRIBE_PATH,
        SUBMIT_PATH,
        LOOKUP_PATH,
        VIEW_PATH,
        oasis7_client_api::world_service::VIEW_JSON_PATH,
        CHANGES_PATH,
        MEMBERSHIP_PATH,
    ]
    .contains(&path)
    {
        return Ok(false);
    }
    let result = (|| {
        if method != "POST" {
            return Err("world service requires POST".into());
        }
        let body = super::feedback_submit_api::extract_http_json_body(bytes)?;
        let identity = if let Some((config, _)) = guarded {
            let authority = super::controlled_live_config::GuardedAuthority::load(config)?;
            if authority.policy.trust.world_id != world_id {
                return Err("guarded runtime world differs from external policy".into());
            }
            WorldIdentity {
                world_id: authority.policy.trust.world_id,
                genesis_digest: authority.policy.trust.genesis_digest,
            }
        } else {
            world_service_read::identity(world_dir, world_id)?
        };
        let service = Service {
            records,
            storage,
            identity,
            signer,
            runtime,
            guarded,
        };
        match path {
            DESCRIBE_PATH => operations::describe(&service, parse(body)?),
            SUBMIT_PATH => operations::submit(&service, parse(body)?),
            LOOKUP_PATH => operations::lookup(&service, parse(body)?),
            VIEW_PATH => operations::view(&service, parse(body)?),
            oasis7_client_api::world_service::VIEW_JSON_PATH => {
                operations::view_json(&service, parse(body)?)
            }
            CHANGES_PATH => operations::changes(&service, parse(body)?),
            MEMBERSHIP_PATH => membership_operations::read(&service, parse(body)?),
            _ => unreachable!(),
        }
    })();
    let (status, value) = match result {
        Ok(value) => (200, value),
        Err(reason) if reason == UNSUPPORTED_REQUEST_FIELD => (
            400,
            json!({"error": "unsupported_request_field", "reason": reason}),
        ),
        Err(_) => (
            503,
            json!({"error": "world_service_unavailable", "reason": PUBLIC_SERVICE_UNAVAILABLE}),
        ),
    };
    let encoded = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    super::write_json_response(stream, status, &encoded, false).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod strict_parser_tests {
    use super::*;
    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    struct KnownOptional {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        optional: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        items: Vec<String>,
        extension: Value,
    }

    #[test]
    fn strict_parser_preserves_known_null_empty_default_and_opaque_extensions() {
        let parsed: KnownOptional = parse(br#"{"optional":null,"items":[],"extension":{"future":{"required_guarantee":"application-data"}}}"#).unwrap();
        assert!(parsed.optional.is_none());
        assert!(parsed.items.is_empty());
        assert_eq!(
            parsed.extension["future"]["required_guarantee"],
            "application-data"
        );
        let parsed: KnownOptional = parse(br#"{"extension":null}"#).unwrap();
        assert!(parsed.optional.is_none());
        assert!(parsed.items.is_empty());
        let bytes = br#"{"legacy":"unchanged","required_guarantee":"opaque bytes"}"#.to_vec();
        let payload = WorldServicePayloadV1::GameplayJson(bytes.clone());
        let decoded: WorldServicePayloadV1 = parse(&serde_json::to_vec(&payload).unwrap()).unwrap();
        assert_eq!(decoded, WorldServicePayloadV1::GameplayJson(bytes));
    }

    #[test]
    fn strict_parser_rejects_ignored_guarantees_in_wrappers_and_typed_requests() {
        let request = SignedReadRequest {
            request: DescribeWorldRequest {
                contract_version: 1,
                expected_world: WorldIdentity {
                    world_id: "w1".into(),
                    genesis_digest: "genesis".into(),
                },
                trust_config_ref: "configured-key".into(),
            },
            subject_public_key: "subject".into(),
            signature_hex: "signature".into(),
        };
        let original = serde_json::to_value(&request).unwrap();
        for level in [0, 1, 2] {
            let mut changed = original.clone();
            let object = match level {
                0 => &mut changed,
                1 => &mut changed["request"],
                _ => &mut changed["request"]["expected_world"],
            };
            object["required_guarantee"] = json!("multi-validator");
            assert_eq!(
                parse::<SignedReadRequest<DescribeWorldRequest>>(
                    &serde_json::to_vec(&changed).unwrap()
                )
                .unwrap_err(),
                UNSUPPORTED_REQUEST_FIELD
            );
        }
        assert!(
            parse::<SignedReadRequest<DescribeWorldRequest>>(
                &serde_json::to_vec(&original).unwrap()
            )
            .is_ok()
        );
    }
}
