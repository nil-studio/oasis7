//! Model-facing values contain only fixed handles and safe Control acknowledgements.
use super::*;
use oasis7_custody_core::provisioning::{
    SessionChallengeKindV1, SessionChallengeRequestV1, SessionControlKindV1,
    SessionControlRequestV1,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Method {
    Initialize,
    ToolsList,
    Heartbeat,
    RequestFinish,
    RequestAbort,
}
impl Method {
    fn kind(self) -> SessionControlKindV1 {
        match self {
            Self::Initialize => SessionControlKindV1::Initialize,
            Self::ToolsList => SessionControlKindV1::ToolsList,
            Self::Heartbeat => SessionControlKindV1::Heartbeat,
            Self::RequestFinish => SessionControlKindV1::Finish,
            Self::RequestAbort => SessionControlKindV1::Abort,
        }
    }
    fn route(self, sid: String) -> HttpPopRoute {
        match self {
            Self::Initialize => HttpPopRoute::Initialize(sid),
            Self::ToolsList => HttpPopRoute::ToolsList(sid),
            Self::Heartbeat => HttpPopRoute::Heartbeat(sid),
            Self::RequestFinish => HttpPopRoute::Finish(sid),
            Self::RequestAbort => HttpPopRoute::Abort(sid),
        }
    }
    fn closing(self) -> bool {
        matches!(self, Self::RequestFinish | Self::RequestAbort)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelRequest {
    pub api_version: u8,
    pub resource_id: String,
    pub request_id: String,
    pub method: Method,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Accepted,
    Historical,
    Unknown,
    Rejected,
    Unavailable,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelReply {
    pub api_version: u8,
    pub request_id: String,
    pub status: Status,
    pub close_requested: bool,
}
impl ModelReply {
    pub fn unavailable(id: String) -> Self {
        Self {
            api_version: 1,
            request_id: id,
            status: Status::Unavailable,
            close_requested: false,
        }
    }
}
impl Host {
    pub(crate) fn control(&self, q: ModelRequest) -> Result<ModelReply, String> {
        if q.api_version != 1 || !id(&q.request_id) {
            return Err(denied());
        }
        let b = self
            .vault
            .read(|s| Ok(s.session.as_ref().ok_or_else(denied)?.binding.clone()))?;
        if q.resource_id != b.resource_id {
            return Err(denied());
        }
        let kind = q.method.kind();
        let request = SessionControlRequestV1 {
            api_version: 1,
            session_id: b.session_id.clone(),
            request_id: q.request_id.clone(),
            kind,
        };
        let body = serde_json::to_vec(&request).map_err(|_| denied())?;
        let body_digest = request.digest().map_err(|_| denied())?;
        let old = self.vault.update(|s| {
            let session = s.session.as_mut().ok_or_else(denied)?;
            if let Some(old) = session.logicals.get(&q.request_id) {
                if old.digest != body_digest || old.kind != Some(kind) {
                    return Err(denied());
                }
                if old.ack.is_some() {
                    return Ok(true);
                }
            }
            if session.closing && !q.method.closing() {
                return Err(denied());
            }
            if !session.logicals.contains_key(&q.request_id) {
                let now = vault::now()?;
                if kind == SessionControlKindV1::Heartbeat {
                    let beats: Vec<_> = session
                        .logicals
                        .values()
                        .filter(|x| x.kind == Some(SessionControlKindV1::Heartbeat))
                        .collect();
                    if beats.len() >= 64
                        || beats
                            .iter()
                            .any(|x| now.checked_sub(x.created_at).is_none_or(|d| d < 30_000))
                    {
                        return Err(denied());
                    }
                }
                session.logicals.insert(
                    q.request_id.clone(),
                    Logical {
                        digest: body_digest.clone(),
                        kind: Some(kind),
                        created_at: now,
                        ack: None,
                        retained: false,
                    },
                );
            }
            if q.method.closing() {
                session.closing = true;
            }
            Ok(false)
        })?;
        self.vault.retain(
            &q.request_id,
            &self.target(self.scoped(q.method.route(b.session_id.clone()))?)?,
            &body,
        )?;
        if old {
            return Ok(ModelReply {
                api_version: 1,
                request_id: q.request_id,
                status: Status::Historical,
                close_requested: q.method.closing(),
            });
        }
        let purpose = if q.method.closing() {
            SessionPurposeV1::ReconcileRead
        } else {
            SessionPurposeV1::Execution
        };
        let (generation, deadline) = self.vault.read(|s| {
            let session = s.session.as_ref().ok_or_else(denied)?;
            Ok((
                session.credential(purpose)?.generation,
                session.binding.deadline(purpose)?,
            ))
        })?;
        let c_request = SessionChallengeRequestV1 {
            api_version: 1,
            session_id: b.session_id.clone(),
            request_id: vault::random_id()?,
            purpose,
            kind: SessionChallengeKindV1::Control(kind),
            logical_request_id: q.request_id.clone(),
            operation_request_digest: body_digest,
        };
        let c_body = serde_json::to_vec(&c_request).map_err(|_| denied())?;
        let outcome: Result<Status, String> = (|| {
            let response = self.signed(SignedSend {
                route: self.scoped(HttpPopRoute::ProofChallenges)?,
                profile: HttpPopProfileV1::CustodyChallenge,
                request_id: &c_request.request_id,
                nonce: None,
                body: &c_body,
                purpose: Some(purpose),
                expected_generation: Some(generation),
                deadline,
            })?;
            if response.status != 200 {
                return Ok(if response.status < 500 {
                    Status::Rejected
                } else {
                    Status::Unknown
                });
            }
            let c = reply::challenge(response.body(), &c_request, Some(generation), deadline)?;
            let response = self.signed(SignedSend {
                route: self.scoped(q.method.route(b.session_id.clone()))?,
                profile: HttpPopProfileV1::Custody,
                request_id: &q.request_id,
                nonce: Some(&c.nonce),
                body: &body,
                purpose: Some(purpose),
                expected_generation: Some(generation),
                deadline: deadline.min(c.expires_at.get()),
            })?;
            if response.status != 200 {
                return Ok(if response.status < 500 {
                    Status::Rejected
                } else {
                    Status::Unknown
                });
            }
            let a = reply::ack(
                response.body(),
                &q.request_id,
                kind,
                b.plan.write_deadline(),
            )?;
            self.vault.update(|s| {
                let old = s
                    .session
                    .as_mut()
                    .ok_or_else(denied)?
                    .logicals
                    .get_mut(&q.request_id)
                    .ok_or_else(denied)?;
                if old.ack.as_ref().is_some_and(|v| v != &a) {
                    return Err(denied());
                }
                old.ack = Some(a);
                Ok(())
            })?;
            Ok(Status::Accepted)
        })();
        Ok(ModelReply {
            api_version: 1,
            request_id: q.request_id,
            status: outcome.unwrap_or(Status::Unknown),
            close_requested: q.method.closing(),
        })
    }
    pub(super) fn heartbeat_if_due(&self) -> Result<(), String> {
        let due = self.vault.read(|s| {
            let now = vault::now()?;
            let session = s.session.as_ref().ok_or_else(denied)?;
            let latest = session
                .logicals
                .values()
                .filter(|x| x.kind == Some(SessionControlKindV1::Heartbeat))
                .map(|x| x.created_at)
                .max();
            Ok(!session.closing
                && now < session.binding.plan.write_deadline()
                && latest.is_none_or(|v| now.checked_sub(v).is_some_and(|d| d >= 30_000)))
        })?;
        if due {
            let resource = self.vault.read(|s| {
                Ok(s.session
                    .as_ref()
                    .ok_or_else(denied)?
                    .binding
                    .resource_id
                    .clone())
            })?;
            let _ = self.control(ModelRequest {
                api_version: 1,
                resource_id: resource,
                request_id: vault::random_id()?,
                method: Method::Heartbeat,
            })?;
        }
        Ok(())
    }
}
