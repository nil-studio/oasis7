//! The same signed request in independent, legitimate current-C0 and Frozen branches.
//! Governance/endpoint custody is supplied by the isolated protected-process fixture.
use super::*;
use oasis7::world_service::client::WorldServiceClientError;
use oasis7_client_api::world_service::SubmitObservation;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

type Request = SubmitIntentRequest<WorldServicePayloadV1>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RevalidationCondition {
    Permission,
    Manifest,
    Deadline,
}

#[derive(Clone)]
pub(crate) struct RevalidationCase {
    pub(crate) condition: RevalidationCondition,
    pub(crate) fixed: bool,
    pair: Arc<Mutex<Option<Original>>>,
}

impl PartialEq for RevalidationCase {
    fn eq(&self, other: &Self) -> bool {
        self.condition == other.condition
            && self.fixed == other.fixed
            && Arc::ptr_eq(&self.pair, &other.pair)
    }
}
impl Eq for RevalidationCase {}

struct Original {
    request: Request,
    manifest: String,
    frozen_observed: bool,
}

fn now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "revalidation wall-clock overflow".into())
}

fn bytes(request: &Request) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(request).map_err(|e| e.to_string())
}

pub(crate) fn prepare_world(
    case: &RevalidationCase,
    world: &mut oasis7_runtime::World,
    identity: &FormalWorldIdentityV1,
) -> Result<Request, String> {
    let identity = WorldIdentity {
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
    };
    let registry = &world.capability_revocation_state().participants;
    let policy = registry
        .policy
        .as_ref()
        .ok_or("revalidation policy missing")?;
    if world.state().time != 0
        || !world.state().agents.is_empty()
        || policy.generation != 1
        || !policy.enabled
        || registry.policy_history.is_empty()
        || !registry.devlocal_policy_history.is_empty()
    {
        return Err("revalidation requires the actual governed genesis policy".into());
    }
    let namespace = policy.identity_namespace.clone();
    let generated = governed_participant::registration_request(world, &identity)?;
    let manifest = world
        .current_manifest_hash()
        .map_err(|e| format!("{e:?}"))?;
    let mut pair = case.pair.lock().map_err(|_| "revalidation pair poisoned")?;
    if case.fixed {
        if pair.is_some() {
            return Err("revalidation fixed branch attempted to replace its original".into());
        }
        *pair = Some(Original {
            request: generated.clone(),
            manifest,
            frozen_observed: false,
        });
        return Ok(generated);
    }
    let original = pair
        .as_ref()
        .ok_or("revalidation original was not fixed first")?;
    if !original.frozen_observed || original.manifest != manifest {
        return Err("paired genesis differs from the original governed Frozen branch".into());
    }
    // The deadline is attached only immediately before the original admission.
    // All signed payload bytes and the original operation identity must already agree.
    let mut expected = original.request.clone();
    expected.deadline_unix_ms = None;
    if bytes(&generated)? != bytes(&expected)? {
        return Err("paired branch regenerated different signed request bytes".into());
    }
    let request = original.request.clone();
    drop(pair);
    match case.condition {
        RevalidationCondition::Permission | RevalidationCondition::Manifest => {
            // Both are genuine governance-finality transitions before this branch's
            // C0. The manifest case keeps registration enabled but rotates its issuer.
            governed_participant::update_fixture_policy(
                world,
                &identity,
                2,
                case.condition == RevalidationCondition::Manifest,
                namespace,
                if case.condition == RevalidationCondition::Manifest {
                    74
                } else {
                    73
                },
            )?;
            // Installing participant authority verifies an approved proposal;
            // publish that very proposal through the real finality API so this
            // branch's governing manifest also changes before its C0 is signed.
            let certificate = world
                .capability_revocation_state()
                .participants
                .policy_history
                .last()
                .ok_or("updated governance proof missing")?
                .proof
                .certificate
                .clone();
            world
                .apply_proposal_with_finality(certificate.proposal_id, &certificate)
                .map_err(|e| format!("apply governed revalidation manifest: {e:?}"))?;
            let current = world
                .current_manifest_hash()
                .map_err(|e| format!("{e:?}"))?;
            if current == manifest {
                return Err("governed pending branch did not change its manifest".into());
            }
        }
        RevalidationCondition::Deadline => {
            let now = now_ms()?;
            if !request
                .deadline_unix_ms
                .is_some_and(|deadline| deadline < now)
            {
                return Err("paired pending request has not naturally expired".into());
            }
        }
    }
    Ok(request)
}

pub(crate) fn before_submit(case: &RevalidationCase, request: &mut Request) -> Result<(), String> {
    if !case.fixed {
        return Err("pending branch cannot use the Frozen admission hook".into());
    }
    let mut pair = case.pair.lock().map_err(|_| "revalidation pair poisoned")?;
    let original = pair.as_mut().ok_or("revalidation original missing")?;
    if bytes(request)? != bytes(&original.request)? || original.frozen_observed {
        return Err("Frozen admission changed the original request".into());
    }
    if case.condition == RevalidationCondition::Deadline {
        request.deadline_unix_ms = Some(now_ms()?.checked_add(30_000).ok_or("deadline overflow")?);
        original.request = request.clone();
    }
    Ok(())
}

pub(crate) fn after_frozen(case: &RevalidationCase, request: &Request) -> Result<(), String> {
    {
        let mut pair = case.pair.lock().map_err(|_| "revalidation pair poisoned")?;
        let original = pair.as_mut().ok_or("revalidation original missing")?;
        if !case.fixed || bytes(request)? != bytes(&original.request)? {
            return Err("actual Frozen marker no longer identifies the original request".into());
        }
        original.frozen_observed = true;
    }
    if case.condition == RevalidationCondition::Deadline {
        let deadline = request.deadline_unix_ms.ok_or("Frozen deadline missing")?;
        let until = Instant::now() + Duration::from_secs(31);
        while now_ms()? <= deadline {
            if Instant::now() >= until {
                return Err(
                    "original deadline did not expire within its fixed wall-clock window".into(),
                );
            }
            thread::sleep(Duration::from_millis(20));
        }
        eprintln!("formal_revalidation condition=deadline phase=frozen_original_expired");
    }
    Ok(())
}

pub(crate) fn assert_original_manifest(
    case: &RevalidationCase,
    actual: &str,
) -> Result<(), String> {
    let pair = case.pair.lock().map_err(|_| "revalidation pair poisoned")?;
    let original = pair.as_ref().ok_or("revalidation original missing")?;
    if !original.frozen_observed || actual != original.manifest {
        return Err("Frozen retained context changed its original governing manifest".into());
    }
    Ok(())
}

fn assert_expired(
    observation: Result<SubmitObservation<Value>, WorldServiceClientError>,
) -> Result<(), String> {
    match observation {
        Err(WorldServiceClientError::Rejected {
            status: 408,
            message,
        }) if message == "submit deadline has expired" => Ok(()),
        other => Err(format!(
            "expired original did not receive signed HTTP 408: {other:?}"
        )),
    }
}

pub(crate) fn assert_retry_observation(
    case: &RevalidationCase,
    observation: Result<SubmitObservation<Value>, WorldServiceClientError>,
    request: &Request,
) -> Result<(), String> {
    if case.condition == RevalidationCondition::Deadline {
        assert_expired(observation)
    } else {
        assert_volatile_admission(
            observation.map_err(|e| e.to_string())?,
            &request.correlation,
        )
    }
}

fn effects(world: &oasis7_runtime::World, request: &Request) -> Result<Vec<u8>, String> {
    let key = correlation::key_digest(&request.correlation.key).map_err(|e| e.to_string())?;
    if optional_retained_result_from_hot_or_cold(world, &key)?.is_some() {
        return Err("rejected pending request acquired a canonical result".into());
    }
    let snapshot = world.snapshot();
    serde_cbor::to_vec(&(
        &world.state().agents,
        &world.capability_revocation_state().participants,
        &world.capability_revocation_state().agent_identities,
        &snapshot.capability_nonce_records,
        &world.state().governance_authorization_nonces,
    ))
    .map_err(|e| e.to_string())
}

pub(crate) fn run_pending(
    case: &RevalidationCase,
    config: &Path,
    public: WorldServiceClientConfig,
    world: &WorldIdentity,
    request: &Request,
) -> Result<(), String> {
    if case.fixed {
        return Err("fixed request cannot take the current-C0 pending branch".into());
    }
    let pair = case.pair.lock().map_err(|_| "revalidation pair poisoned")?;
    let original = pair.as_ref().ok_or("revalidation original missing")?;
    if bytes(request)? != bytes(&original.request)? {
        return Err("public pending Submit did not receive the exact original request".into());
    }
    let original_manifest = original.manifest.clone();
    drop(pair);
    let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
    let before = runtime.published.current()?;
    let manifest = before
        .world
        .current_manifest_hash()
        .map_err(|e| format!("{e:?}"))?;
    let policy = before
        .world
        .capability_revocation_state()
        .participants
        .policy
        .as_ref()
        .ok_or("pending current policy missing")?;
    match case.condition {
        RevalidationCondition::Permission
            if policy.generation == 2 && !policy.enabled && manifest != original_manifest => {}
        RevalidationCondition::Manifest
            if policy.generation == 2
                && policy.enabled
                && policy.issuer_public_key
                    == hex::encode(fixtures::key(74).verifying_key().to_bytes())
                && manifest != original_manifest => {}
        RevalidationCondition::Deadline
            if policy.generation == 1 && policy.enabled && manifest == original_manifest => {}
        _ => {
            return Err(
                "pending branch was not opened from the intended legitimate current C0".into(),
            );
        }
    }
    let baseline = effects(&before.world, request)?;
    let next = before
        .anchor
        .head()
        .record()
        .position
        .checked_add(1)
        .ok_or("pending position overflow")?;
    runtime.start()?;
    let result: Result<(), String> = (|| {
        let client = RemoteWorldServiceClient::new(public).map_err(|e| e.to_string())?;
        read_until_position(&client, world, next)?;
        let observation = client.submit(request.clone());
        if case.condition == RevalidationCondition::Deadline {
            assert_expired(observation)?;
        } else {
            match observation {
                Err(WorldServiceClientError::Rejected {
                    status: 400,
                    message,
                }) if message
                    == "participant admission policy, identity, history or tick mismatch" => {}
                other => {
                    return Err(format!(
                        "current governed policy did not reject original signed request: {other:?}"
                    ));
                }
            }
        }
        let position = runtime.published.current()?.anchor.head().record().position;
        wait_for_published_position(
            &runtime,
            position.checked_add(1).ok_or("pending position overflow")?,
        )?;
        if effects(&runtime.published.current()?.world, request)? != baseline {
            return Err(
                "rejected current-C0 Submit consumed identity, nonce, or participant effects"
                    .into(),
            );
        }
        Ok(())
    })();
    let stopped = stop_owned_protected_runtime(
        &mut runtime,
        Duration::from_secs(120),
        "revalidation_pending",
    );
    result?;
    stopped?;
    drop(runtime);
    let reopened = FormalLiveRuntime::open_with_native_history(config)?;
    let restored = reopened.published.current()?;
    if effects(&restored.world, request)? != baseline
        || restored
            .world
            .current_manifest_hash()
            .map_err(|e| format!("{e:?}"))?
            != manifest
    {
        return Err(
            "cold reopen changed rejected pending request effects or current manifest".into(),
        );
    }
    eprintln!(
        "formal_revalidation condition={:?} phase=pending_rejected_and_cold_checked",
        case.condition
    );
    Ok(())
}

fn run_case(case: RevalidationCase) -> Result<(), String> {
    let label = format!("condition={:?} fixed={}", case.condition, case.fixed);
    let outcome = thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            run_protected_runtime_fixture(
                true,
                if case.fixed {
                    Some("survivor_frozen")
                } else {
                    None
                },
                false,
                None,
                None,
                None,
                FixtureCase::Revalidation(case),
            )
        })
        .map_err(|e| e.to_string())?
        .join()
        .map_err(|_| format!("revalidation child panicked: {label}"))?;
    outcome.map_err(|e| format!("{label}: {e}"))?;
    eprintln!("formal_revalidation {label} phase=passed");
    Ok(())
}

pub(super) fn run_paired_revalidation() -> Result<(), String> {
    let governed_pair = Arc::new(Mutex::new(None));
    run_case(RevalidationCase {
        condition: RevalidationCondition::Permission,
        fixed: true,
        pair: governed_pair.clone(),
    })?;
    for condition in [
        RevalidationCondition::Permission,
        RevalidationCondition::Manifest,
    ] {
        run_case(RevalidationCase {
            condition,
            fixed: false,
            pair: governed_pair.clone(),
        })?;
    }
    let deadline_pair = Arc::new(Mutex::new(None));
    for fixed in [true, false] {
        run_case(RevalidationCase {
            condition: RevalidationCondition::Deadline,
            fixed,
            pair: deadline_pair.clone(),
        })?;
    }
    Ok(())
}
