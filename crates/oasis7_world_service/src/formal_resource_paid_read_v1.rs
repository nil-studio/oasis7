//! Fixed Resource business consumer of an already durable physical claim.
//! An archive is historical evidence; every request still uses the actual
//! opaque qualified Current and local participant/Owner/task/resource rules.
use super::*;
use crate::private_goal_store::PrivateGoalStore;
use crate::resource_world_paid_ledger_v1::{
    ConsumedWorldResourceAttemptV1, WorldResourcePaidLedgerV1,
};
use crate::{agent_chat, game_task};
use oasis7_client_api::world_service as ws;
use oasis7_client_api::world_service::{
    participant_discovery as participant, resource_basis as resource,
};
use oasis7_runtime::service_admission::{correlation, participant as participant_auth};

/// Bounded metadata derived only from an opaque immutable publication.
/// This does not establish Current: the actual HTTP caller must obtain it from
/// its second fresh published cut. It carries no World borrow across locks.
pub struct ResourceCurrentDeliveryCutV1 {
    commit: CommitRef,
}
impl LocalQualifiedPublicationV1 {
    pub fn resource_delivery_cut(&self) -> ResourceCurrentDeliveryCutV1 {
        ResourceCurrentDeliveryCutV1 {
            commit: self.commit.clone(),
        }
    }
}
enum Payload {
    Preflight(Box<participant::OwnerSetupObservationV1>),
    Final(Box<resource::OwnerSetupFinalResourceObservationV2>),
}
/// Neither a transferable grant nor wire state. The original consumed request,
/// exact Current cut and resulting bounded observation stay together until a
/// second actual cut/floor/deadline check immediately before response signing.
pub struct PreparedResourceWorldReadV1 {
    consumed: ConsumedWorldResourceAttemptV1,
    commit: CommitRef,
    payload: Payload,
}
fn reject(message: &str) -> FormalReadError {
    FormalReadError::GrantRejected(message.into())
}
fn plain<T>(value: Result<T, String>) -> FormalReadResult<T> {
    value.map_err(FormalReadError::GrantRejected)
}
fn participant_agent(
    cut: &LocalQualifiedPublicationV1,
    q: &participant::ParticipantOwnerViewRequestV1,
) -> FormalReadResult<String> {
    let world = &cut.world;
    plain(participant_auth::validate_admission(
        world,
        &q.owner,
        &q.admission,
        &cut.commit.binding.branch_id,
        cut.commit.binding.reorg_generation,
    ))?;
    q.read
        .request
        .validate()
        .map_err(|_| reject("Resource owner read shape"))?;
    plain(authority::verify_read_request(
        oasis7_client_api::world_service::VIEW_JSON_PATH,
        &q.read,
    ))?;
    if q.read.request.world != cut.commit.world
        || q.read.request.fixed_commit.is_some()
        || q.read.request.observation_nonce.is_none()
    {
        return Err(reject("Resource owner read current scope"));
    }
    if let Some(minimum) = &q.read.request.min_commit {
        if !cut
            .commit
            .satisfies_minimum(minimum)
            .map_err(|_| reject("Resource minimum history"))?
        {
            return Err(reject("Resource current below minimum"));
        }
    }
    let key = plain(participant_auth::account_key(&q.owner.request))?;
    let record = world
        .capability_revocation_state()
        .participants
        .bindings
        .get(&key)
        .ok_or_else(|| reject("Resource participant unavailable"))?;
    if record.owner_public_key != q.read.subject_public_key
        || record.owner_public_key != q.owner.subject_public_key
        || q.read.request.scope_id != format!("agent:{}", record.agent_id)
    {
        return Err(reject("Resource participant exact owner"));
    }
    Ok(record.agent_id.clone())
}
/// Only actual participant/Owner/task checks below can create this handle.
/// It grants no write; it permits the bounded archive reader to load the exact
/// current Goal after the physical World attempt has already been consumed.
pub struct AuthorizedResourceArchiveReadV1 {
    pub(super) commit: CommitRef,
    binding_digest: String,
    issued_at: u64,
    expires_at: u64,
    pub(super) key: String,
    pub(super) result: oasis7_client_api::legacy_world_service::CanonicalIntentResultV1,
}
impl AuthorizedResourceArchiveReadV1 {
    pub(super) fn ensure_live(&self) -> FormalReadResult<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| reject("Resource observation clock"))?
            .as_millis();
        if now < u128::from(self.issued_at) || now >= u128::from(self.expires_at) {
            return Err(reject("Resource original observation expired"));
        }
        Ok(())
    }
    pub fn validate_original(
        &self,
        consumed: &ConsumedWorldResourceAttemptV1,
    ) -> FormalReadResult<()> {
        self.ensure_live()?;
        if plain(consumed.original_binding_digest())? != self.binding_digest {
            return Err(reject("Resource Current handle is for another original"));
        }
        Ok(())
    }
    pub fn load_private_goal(
        &self,
        store: &PrivateGoalStore,
    ) -> FormalReadResult<ws::private_goal::PreparedGoalReceiptV1> {
        self.ensure_live()?;
        let oasis7_client_api::legacy_world_service::WorldServicePayloadV1::GameGoalPublish(
            publish,
        ) = &self.result.request.signed_payload
        else {
            return Err(reject("Resource authorized Goal kind"));
        };
        store.get_prepared(&publish.request).map_err(|_| {
            FormalReadError::HistoryUnavailable("Resource private Goal unavailable".into())
        })
    }
}
impl LocalQualifiedPublicationV1 {
    pub fn authorize_resource_archive_read(
        &self,
        consumed: &ConsumedWorldResourceAttemptV1,
    ) -> FormalReadResult<AuthorizedResourceArchiveReadV1> {
        plain(consumed.ensure_current_scope(&self.commit))?;
        let body = consumed
            .resource_final()
            .ok_or_else(|| reject("Resource archive requires final substep"))?;
        let agent = participant_agent(self, body.access())?;
        let q = body.resource();
        if agent != q.request.expected_fence.agent_id
            || q.subject_public_key != body.access().read.subject_public_key
        {
            return Err(reject("Resource archive exact participant"));
        }
        plain(game_task::validate_task_reader(
            &self.world,
            &q.request.expected_fence,
            &q.subject_public_key,
        ))?;
        let current = plain(game_task::current_control(&self.world, &agent))?
            .ok_or_else(|| reject("Resource archive Task unavailable"))?;
        let actual_fence = oasis7_agent_api::game::GameTaskFenceV1::from_control(&current);
        if actual_fence != q.request.expected_fence {
            return Err(reject("Resource archive Task fence changed"));
        }
        let actual_signer = self
            .world
            .capability_revocation_state()
            .agent_signer_delegations
            .get(&agent)
            .ok_or_else(|| reject("Resource archive signer unavailable"))?;
        let expected = &q.request.expected_signer_delegation;
        if q.request.world != resource::ResourceWorldV1::from(&self.commit.world)
            || actual_signer.world_id != expected.world_id
            || actual_signer.branch_id != expected.branch_id
            || actual_signer.agent_id != expected.agent_id
            || actual_signer.owner_binding != expected.owner_binding
            || actual_signer.agent_identity_generation != expected.agent_identity_generation.0
            || actual_signer.generation != expected.generation.0
            || actual_signer.delegate_public_key != expected.delegate_public_key
            || actual_signer.revoked != expected.revoked
            || actual_signer.nonce != expected.nonce.0
        {
            return Err(reject("Resource archive signer/current world changed"));
        }
        let owner = plain(agent_chat::owner_view(&self.world, &agent))?
            .ok_or_else(|| reject("Resource archive Owner unavailable"))?;
        if owner.public_key != q.subject_public_key {
            return Err(reject("Resource archive Owner changed"));
        }
        let actual = plain(game_task::active_publish(&self.world, &agent))?
            .ok_or_else(|| reject("Resource archive Goal unavailable"))?;
        let oasis7_client_api::legacy_world_service::WorldServicePayloadV1::GameGoalPublish(
            publish,
        ) = &actual.request.signed_payload
        else {
            return Err(reject("Resource archive Goal kind"));
        };
        if publish.request.goal_ref != q.request.goal_reference {
            return Err(reject("Resource archive exact Goal reference"));
        }
        let key = plain(correlation::key_digest(&actual.request.correlation.key))?;
        Ok(AuthorizedResourceArchiveReadV1 {
            commit: self.commit.clone(),
            binding_digest: plain(consumed.original_binding_digest())?,
            issued_at: consumed.original_window().0,
            expires_at: consumed.original_window().1,
            key,
            result: actual,
        })
    }
    /// The caller must obtain `consumed` from the World ledger before loading
    /// this cut, the archive, private Goal bodies or quotes. No I/O occurs here:
    /// the private receipt was loaded through the owned authorized handle
    /// after releasing publication/cache locks and is checked again below.
    pub fn prepare_paid_resource_read(
        &self,
        consumed: ConsumedWorldResourceAttemptV1,
        archive: Option<&VerifiedResourceGoalArchiveV2>,
        private_goal: Option<ws::private_goal::PreparedGoalReceiptV1>,
    ) -> FormalReadResult<PreparedResourceWorldReadV1> {
        plain(consumed.ensure_current_scope(&self.commit))?;
        let access = if let Some(q) = consumed.owner_preflight() {
            q
        } else {
            consumed
                .resource_final()
                .ok_or_else(|| reject("Resource paid substep"))?
                .access()
        };
        let agent = participant_agent(self, access)?;
        let owner = plain(agent_chat::owner_view(&self.world, &agent))?
            .ok_or_else(|| reject("Resource current Owner unavailable"))?;
        if owner.public_key != access.read.subject_public_key {
            return Err(reject("Resource current Owner changed"));
        }
        let control = plain(game_task::current_control(&self.world, &agent))?;
        if let Some(control) = &control {
            plain(game_task::validate_task_reader(
                &self.world,
                &oasis7_agent_api::game::GameTaskFenceV1::from_control(control),
                &access.read.subject_public_key,
            ))?;
        }
        let payload = if consumed.owner_preflight().is_some() {
            let view = participant::ParticipantOwnerControlViewV1 {
                canonical_agent_owner: (&owner).into(),
                game_task_control: control,
            };
            let observation = participant::OwnerSetupObservationV1 {
                api_version: 1,
                control: participant::ParticipantOwnerControlResponseV1 {
                    schema_revision: 1,
                    world: self.commit.world.clone(),
                    version: ProjectionVersion {
                        commit: self.commit.clone(),
                        visibility_scope: access.read.request.scope_id.clone(),
                        projection_revision: plain(authority::request_digest("projection", &view))?,
                    },
                    logical_tick: self.world.state().time,
                    view,
                },
                signer_delegation: self
                    .world
                    .capability_revocation_state()
                    .agent_signer_delegations
                    .get(&agent)
                    .cloned(),
            };
            plain(observation.validate(access))?;
            Payload::Preflight(Box::new(observation))
        } else {
            let body = consumed
                .resource_final()
                .ok_or_else(|| reject("Resource exact body"))?;
            let q = body.resource();
            let request = &q.request;
            if request.world.world_id != self.commit.world.world_id
                || request.world.genesis_digest != self.commit.world.genesis_digest
                || request.expected_fence.agent_id != agent
                || q.subject_public_key != access.read.subject_public_key
            {
                return Err(reject("Resource complete current binding"));
            }
            let control = control.ok_or_else(|| reject("Resource current Task unavailable"))?;
            let record = self
                .world
                .capability_revocation_state()
                .agent_signer_delegations
                .get(&agent);
            let owner_state = resource::ResourceOwnerStateV1 {
                schema_revision: 1,
                world: (&self.commit.world).into(),
                read_commit: (&self.commit).into(),
                logical_tick: resource::DecimalU64(self.world.state().time),
                owner: resource::ResourceOwnerFenceV1 {
                    agent_id: owner.agent_id,
                    player_id: owner.player_id,
                    public_key: owner.public_key,
                    world_id: owner.world_id,
                    reorg_epoch: resource::DecimalU64(owner.reorg_epoch),
                    branch_id: owner.canonical_authority.branch_id,
                    agent_identity_generation: resource::DecimalU64(
                        owner.canonical_authority.agent_identity_generation,
                    ),
                    current_intent_id: owner.current_intent_id,
                },
                control: control.clone(),
                signer_delegation: record.map(|r| resource::ResourceSignerRecordV1 {
                    world_id: r.world_id.clone(),
                    branch_id: r.branch_id.clone(),
                    agent_id: r.agent_id.clone(),
                    owner_binding: r.owner_binding.clone(),
                    agent_identity_generation: resource::DecimalU64(r.agent_identity_generation),
                    generation: resource::DecimalU64(r.generation),
                    delegate_public_key: r.delegate_public_key.clone(),
                    revoked: r.revoked,
                    nonce: resource::DecimalU64(r.nonce),
                }),
            };
            plain(owner_state.validate(
                &request.expected_fence,
                &q.subject_public_key,
                &request.expected_signer_delegation,
            ))?;
            let actual = plain(game_task::active_publish(&self.world, &agent))?
                .ok_or_else(|| reject("Resource current Goal unavailable"))?;
            let key = plain(correlation::key_digest(&actual.request.correlation.key))?;
            let archive = archive.ok_or_else(|| {
                FormalReadError::HistoryUnavailable(
                    "original Goal activation archive unavailable".into(),
                )
            })?;
            if archive.exact_result(&key) != Some(&actual) {
                return Err(reject("Resource archive is not exact active Goal"));
            }
            let oasis7_client_api::legacy_world_service::WorldServicePayloadV1::GameGoalPublish(
                publish,
            ) = &actual.request.signed_payload
            else {
                return Err(reject("Resource active Goal kind"));
            };
            let prepared = private_goal.ok_or_else(|| {
                FormalReadError::HistoryUnavailable("Resource private Goal unavailable".into())
            })?;
            prepared
                .validate_publish(&publish.request)
                .map_err(|_| reject("Resource private Goal exact Owner proof"))?;
            if control
                .active_goal_reference
                .as_ref()
                .is_none_or(|r| r.activation_committed_height.0 != actual.committed_height)
            {
                return Err(reject("Resource activation Runtime coordinate"));
            }
            let current_goal = resource::ResourceCurrentGoalV2 {
                activation_profile: Some(
                    resource::ResourceGoalActivationProfileV2::FormalQualifiedV2 {
                        committed_runtime_height: resource::DecimalU64(actual.committed_height),
                    },
                ),
                schema_revision: 1,
                control,
                read_commit: (&self.commit).into(),
                activation_commit: Some(archive.activation_commit().into()),
                active_goal: Some(prepared),
            };
            let resolution = plain(crate::resource_basis_runtime_adapter_v1::resolution(
                &self.world,
                &agent,
                &request.selection,
            ))?;
            Payload::Final(Box::new(resource::OwnerSetupFinalResourceObservationV2 {
                api_version: 2,
                owner: owner_state,
                current_goal,
                selection: request.selection.clone(),
                resource: resolution,
            }))
        };
        Ok(PreparedResourceWorldReadV1 {
            consumed,
            commit: self.commit.clone(),
            payload,
        })
    }
}
impl PreparedResourceWorldReadV1 {
    /// A fresh actual opaque cut and current ledger are mandatory; no callback
    /// supplies a verification boolean and no archived cut grants Current.
    pub fn finish(
        self,
        fresh: &ResourceCurrentDeliveryCutV1,
        ledger: &WorldResourcePaidLedgerV1,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> FormalReadResult<Vec<u8>> {
        if fresh.commit != self.commit {
            return Err(FormalReadError::NotReady(
                "Resource current changed before delivery".into(),
            ));
        }
        let public = hex::encode(signing_key.verifying_key().to_bytes());
        let private_key_hex = zeroize::Zeroizing::new(hex::encode(signing_key.to_bytes()));
        plain(ledger.validate_service_binding(&fresh.commit, &public))?;
        plain(ledger.validate_delivery(&self.consumed))?;
        let binding = plain(self.consumed.paid_reply_binding())?;
        let request_digest = plain(binding.request_digest())?;
        let payload = match self.payload {
            Payload::Preflight(payload) => {
                let q = self
                    .consumed
                    .owner_preflight()
                    .ok_or_else(|| reject("Resource preflight original"))?;
                let inner = plain(ws::sign_service_response(
                    participant::PARTICIPANT_OWNER_VIEW_PATH,
                    plain(authority::request_digest(
                        participant::PARTICIPANT_OWNER_VIEW_PATH,
                        q,
                    ))?,
                    *payload,
                    &private_key_hex,
                ))?;
                resource::ResourcePaidWorldReplyV1::OwnerPreflight {
                    binding,
                    observation: Box::new(inner),
                }
            }
            Payload::Final(payload) => {
                let q = self
                    .consumed
                    .resource_final()
                    .ok_or_else(|| reject("Resource final original"))?
                    .resource();
                let inner = plain(ws::sign_service_response(
                    resource::RESOURCE_GOAL_PATH_V2,
                    plain(authority::request_digest(
                        resource::RESOURCE_GOAL_PATH_V2,
                        q,
                    ))?,
                    *payload,
                    &private_key_hex,
                ))?;
                resource::ResourcePaidWorldReplyV1::ResourceFinal {
                    binding,
                    observation: Box::new(inner.into()),
                }
            }
        };
        let reply = plain(ws::sign_service_response(
            resource::RESOURCE_PAID_WORLD_REPLY_DOMAIN,
            request_digest,
            payload,
            &private_key_hex,
        ))?;
        let bytes = serde_json::to_vec(&reply)
            .map_err(|_| FormalReadError::Internal("Resource paid response codec".into()))?;
        if bytes.len() > resource::MAX_RESOURCE_ENVELOPE_BYTES {
            return Err(reject("Resource paid reply bound"));
        }
        Ok(bytes)
    }
}
