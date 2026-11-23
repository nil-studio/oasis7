//! Nonempty public Submit handoff through the actual separately owned Runtime.
//! The scheduling pause observes an action already selected by Node, before
//! Frozen. It grants no authorization and does not manufacture durable bytes.
use super::*;
use crate::formal_live_runtime::pending_handoff::{
    PendingImportObservation, PendingImportReport, request_identity,
};
use crate::formal_live_service::{
    PublishedReadState,
    test_control::{BoundaryControl, Point},
};
use oasis7_client_api::world_service::CommitRef;
use oasis7_distfs::controlled_authority::formal::{
    RecoveryPendingHandoffV1, RecoveryPendingRequestV1,
};
use serde::{Deserialize, Serialize};

use super::super::survivor_tests::governed_participant;

type Request = SubmitIntentRequest<WorldServicePayloadV1>;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Schedule {
    original: Request,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedImport {
    original: Request,
    root: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PauseReceipt {
    original_action_digest: String,
    node_block_hash: String,
    node_height: u64,
    action_count: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportReceipt {
    root: String,
    original_action_digest: String,
    already_canonical: bool,
}

pub(crate) struct OwnerPause {
    control: Arc<BoundaryControl>,
    stopped: Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<Result<(), String>>>,
}

impl OwnerPause {
    pub(crate) fn cancel(&self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::Release);
        self.control.cancel();
    }
}

impl Drop for OwnerPause {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Called only by the test build of the genuine owner loop, before Node starts.
/// A sidecar adjacent to this exact private fixture config scopes the pause;
/// no process-global environment variable or production setting selects it.
pub(crate) fn install_owner_pause(
    config: &Path,
    published: &PublishedReadState,
) -> Result<Option<OwnerPause>, String> {
    let Some(schedule) =
        read_optional::<Schedule>(&config.with_extension("pending-schedule.cbor"))?
    else {
        return Ok(None);
    };
    let original = oasis7_world_service::correlation::encode_consensus_intent(&schedule.original)?;
    let digest = blake3::hash(&original).to_hex().to_string();
    let control = Arc::new(published.control_boundaries()?);
    let deadline = Instant::now() + Duration::from_secs(120);
    control.arm(Point::BeforeFrozen, None, Some(original), deadline)?;
    let worker_control = Arc::clone(&control);
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_stopped = Arc::clone(&stopped);
    let marker = config.with_extension("pending-observed.cbor");
    let release = config.with_extension("pending-release");
    let worker = std::thread::Builder::new()
        .name("formal-pending-before-frozen".into())
        .spawn(move || {
            let observation = worker_control.wait(deadline)?;
            if observation.point != Point::BeforeFrozen || observation.action_count == 0 {
                return Err("pending fixture did not observe a real selected Node action".into());
            }
            write_private_cbor(
                &marker,
                &PauseReceipt {
                    original_action_digest: digest,
                    node_block_hash: observation
                        .node_block_hash
                        .ok_or("selected pending proposal has no original Node block")?,
                    node_height: observation.position,
                    action_count: observation.action_count,
                },
            )?;
            loop {
                match fs::read(&release) {
                    Ok(bytes) if bytes == b"release exact selected pending action" => {
                        return worker_control.release();
                    }
                    Ok(_) => return Err("pending fixture release identity changed".into()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
                if Instant::now() >= deadline {
                    return Err("pending fixture release deadline expired".into());
                }
                // Cancellation also releases the actual Node hook. The owner
                // must never retain a detached test worker during shutdown.
                if worker_stopped.load(std::sync::atomic::Ordering::Acquire) {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(Some(OwnerPause {
        control,
        stopped,
        worker: Some(worker),
    }))
}

/// Record the result of the real startup consumer before publishing owner Ready.
pub(crate) fn record_actual_import(
    config: &Path,
    report: Option<&PendingImportReport>,
) -> Result<(), String> {
    let Some(expected) =
        read_optional::<ExpectedImport>(&config.with_extension("pending-expected.cbor"))?
    else {
        return Ok(());
    };
    let report = report.ok_or("successor omitted the actual pending import report")?;
    if report.root != expected.root
        || report.observations.len() != 1
        || report.observations[0].0 != request_identity(&expected.original)
    {
        return Err("successor import report lost the exact original pending identity".into());
    }
    let already_canonical = match &report.observations[0].1 {
        PendingImportObservation::ReceivedVolatile => false,
        PendingImportObservation::AlreadyCanonical => true,
        PendingImportObservation::Rejected { status, code } => {
            return Err(format!(
                "valid original pending request was rejected: {status}/{code}"
            ));
        }
    };
    write_private_cbor(
        &config.with_extension("pending-imported.cbor"),
        &ImportReceipt {
            root: report.root.clone(),
            original_action_digest: blake3::hash(
                &oasis7_world_service::correlation::encode_consensus_intent(&expected.original)?,
            )
            .to_hex()
            .to_string(),
            already_canonical,
        },
    )
}

pub(super) struct PendingCase {
    original: Request,
    producer_config: PathBuf,
    client: WorldServiceClientConfig,
}

pub(super) struct PendingCommit {
    commit: CommitRef,
    effect_before_duplicate: Vec<u8>,
}

impl PendingCase {
    pub(super) fn prepare(
        config: &Path,
        state: &oasis7_runtime::World,
        world: &WorldIdentity,
        trust: &FormalReadTrustConfigV1,
        address: std::net::SocketAddr,
    ) -> Result<Self, String> {
        let original = governed_participant::registration_request(state, world)?;
        write_private_cbor(
            &config.with_extension("pending-schedule.cbor"),
            &Schedule {
                original: original.clone(),
            },
        )?;
        Ok(Self {
            original,
            producer_config: config.to_path_buf(),
            client: WorldServiceClientConfig {
                formal_read_trust: Some(trust.clone()),
                endpoint: format!("http://{address}"),
                trusted_service_public_key: fixtures::public(90),
                expected_world: world.clone(),
                scope_id: "public".into(),
                read_private_key_hex: hex::encode(fixtures::key(72).to_bytes()),
                timeout: Duration::from_secs(15),
                max_response_bytes: 16 * 1024 * 1024,
            },
        })
    }

    pub(super) fn submit_and_wait_selected(&self) -> Result<(), String> {
        let client = RemoteWorldServiceClient::new(self.client.clone())
            .map_err(|error| error.to_string())?;
        assert_volatile_admission(
            client
                .submit(self.original.clone())
                .map_err(|error| error.to_string())?,
            &self.original.correlation,
        )?;
        let observed: PauseReceipt = wait_private_cbor(
            &self.producer_config.with_extension("pending-observed.cbor"),
            Instant::now() + Duration::from_secs(60),
        )?;
        if observed.action_count == 0
            || observed.node_block_hash.is_empty()
            || observed.original_action_digest
                != blake3::hash(&oasis7_world_service::correlation::encode_consensus_intent(
                    &self.original,
                )?)
                .to_hex()
                .as_str()
        {
            return Err("pending schedule did not observe this exact signed Node action".into());
        }
        Ok(())
    }

    pub(super) fn verify_frozen_export(
        &self,
        pending: &RecoveryPendingHandoffV1,
    ) -> Result<(), String> {
        let expected = RecoveryPendingHandoffV1::from_requests(vec![RecoveryPendingRequestV1 {
            identity: request_identity(&self.original),
            signed_request: oasis7_world_service::correlation::encode_consensus_intent(
                &self.original,
            )?,
        }])
        .map_err(|error| error.to_string())?;
        if pending != &expected {
            return Err("freeze lost a request already taken from the Node outer queue".into());
        }
        write_atomic(
            &self.producer_config.with_extension("pending-release"),
            b"release exact selected pending action",
        )
    }

    pub(super) fn expect_successor(
        &self,
        config: &Path,
        pending: &RecoveryPendingHandoffV1,
    ) -> Result<(), String> {
        write_private_cbor(
            &config.with_extension("pending-expected.cbor"),
            &ExpectedImport {
                original: self.original.clone(),
                root: pending.root().map_err(|error| error.to_string())?,
            },
        )
    }

    pub(super) fn verify_successor(
        &self,
        config: &Path,
        address: std::net::SocketAddr,
        pending: &RecoveryPendingHandoffV1,
        first_business_position: u64,
    ) -> Result<PendingCommit, String> {
        let receipt: ImportReceipt = wait_private_cbor(
            &config.with_extension("pending-imported.cbor"),
            Instant::now() + Duration::from_secs(15),
        )?;
        if receipt.root != pending.root().map_err(|error| error.to_string())?
            || receipt.already_canonical
            || receipt.original_action_digest
                != blake3::hash(&oasis7_world_service::correlation::encode_consensus_intent(
                    &self.original,
                )?)
                .to_hex()
                .as_str()
        {
            return Err(
                "successor did not newly revalidate and enqueue the original pending request"
                    .into(),
            );
        }
        let mut client_config = self.client.clone();
        client_config.endpoint = format!("http://{address}");
        client_config.scope_id = governed_participant::registered_scope(&self.original)?;
        let client =
            RemoteWorldServiceClient::new(client_config).map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(90);
        let committed = loop {
            match lookup_original_commit(&client, &self.original) {
                Ok(commit) => break commit,
                Err(error) if Instant::now() >= deadline => {
                    return Err(format!(
                        "pending successor original Lookup did not commit: {error}"
                    ));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        if committed.position < first_business_position {
            return Err(
                "pending request was executed before successor Control authorization".into(),
            );
        }
        let before = read_until_position(
            &client,
            &self.original.correlation.key.world,
            committed.position,
        )?;
        let baseline = native_effect_at(config, &before.version().commit, &self.original)?;
        assert_volatile_admission(
            client
                .submit(self.original.clone())
                .map_err(|error| error.to_string())?,
            &self.original.correlation,
        )?;
        let after = read_until_position(
            &client,
            &self.original.correlation.key.world,
            before
                .version()
                .commit
                .position
                .checked_add(1)
                .ok_or("pending duplicate position overflow")?,
        )?;
        if lookup_original_commit(&client, &self.original)? != committed {
            return Err("pending duplicate changed the original qualified result".into());
        }
        if native_effect_at(config, &after.version().commit, &self.original)? != baseline {
            return Err("duplicate Submit consumed a second effect or nonce".into());
        }
        Ok(PendingCommit {
            commit: committed,
            effect_before_duplicate: baseline,
        })
    }

    pub(super) fn verify_cold(
        &self,
        config: &Path,
        address: std::net::SocketAddr,
        pending: &RecoveryPendingHandoffV1,
        committed: &PendingCommit,
    ) -> Result<(), String> {
        let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
        let before = runtime.published.current()?;
        let baseline = effect_and_nonces(&before.world, &self.original)?;
        if baseline != committed.effect_before_duplicate {
            return Err("owner shutdown changed the pre-duplicate effect or nonce".into());
        }
        runtime.start()?;
        let report = runtime
            .pending_import_report()
            .ok_or("cold startup omitted original pending collection")?;
        if report.root != pending.root().map_err(|error| error.to_string())?
            || report.observations
                != [(
                    request_identity(&self.original),
                    PendingImportObservation::AlreadyCanonical,
                )]
        {
            return Err("cold pending import re-enqueued an already canonical original".into());
        }
        let mut client = self.client.clone();
        client.endpoint = format!("http://{address}");
        client.scope_id = governed_participant::registered_scope(&self.original)?;
        let client = RemoteWorldServiceClient::new(client).map_err(|error| error.to_string())?;
        if lookup_original_commit(&client, &self.original)? != committed.commit {
            return Err("cold pending Lookup changed original commit identity".into());
        }
        let next = runtime
            .published
            .current()?
            .anchor
            .head()
            .record()
            .position
            .checked_add(1)
            .ok_or("pending cold position overflow")?;
        read_until_position(&client, &self.original.correlation.key.world, next)?;
        if effect_and_nonces(&runtime.published.current()?.world, &self.original)? != baseline {
            return Err("cold pending import consumed original effects or nonce again".into());
        }
        stop_owned_protected_runtime(
            &mut runtime,
            Duration::from_secs(60),
            "pending_handoff_cold",
        )?;
        drop(runtime);
        let cold = FormalLiveRuntime::open_with_native_history(config)?;
        if effect_and_nonces(&cold.published.current()?.world, &self.original)? != baseline {
            return Err("second cold reader changed original pending effects or nonces".into());
        }
        Ok(())
    }
}

fn native_effect_at(
    config: &Path,
    commit: &CommitRef,
    request: &Request,
) -> Result<Vec<u8>, String> {
    let validated = crate::formal_live_config::FormalLiveConfigV1::load(config)?;
    let reader = crate::formal_live_runtime::business_reader_for(&validated)?;
    let history = crate::formal_live_history::from_validated(&validated, reader)?;
    let actual = history
        .fixed_publication(commit)
        .map_err(|error| error.to_string())?;
    effect_and_nonces(actual.world(), request)
}

fn effect_and_nonces(state: &oasis7_runtime::World, request: &Request) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(&(
        governed_participant::registered_effect(state, request)?,
        state.snapshot().capability_nonce_records,
        &state.state().governance_authorization_nonces,
    ))
    .map_err(|error| error.to_string())
}

fn write_private_cbor(path: &Path, value: &impl Serialize) -> Result<(), String> {
    write_atomic(
        path,
        &serde_cbor::to_vec(value).map_err(|error| error.to_string())?,
    )
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("pending fixture filename")?;
    let temporary = path.with_file_name(format!("{name}.tmp"));
    super::super::write_private(&temporary, bytes)?;
    fs::hard_link(&temporary, path).map_err(|error| error.to_string())?;
    fs::remove_file(&temporary).map_err(|error| error.to_string())?;
    fs::File::open(path.parent().ok_or("pending fixture parent")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| error.to_string())
}

fn read_optional<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    crate::controlled_live_config::require_private_owned(path, false)?;
    let bytes = crate::controlled_history_cli::read_bounded(path, 1024 * 1024)?;
    serde_cbor::from_slice(&bytes)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn wait_private_cbor<T: serde::de::DeserializeOwned>(
    path: &Path,
    deadline: Instant,
) -> Result<T, String> {
    loop {
        if let Some(value) = read_optional(path)? {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "pending fixture receipt deadline: {}",
                path.display()
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
