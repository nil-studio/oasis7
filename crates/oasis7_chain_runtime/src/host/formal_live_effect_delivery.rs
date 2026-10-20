//! Delivery consumes qualified closures; local retry records never grant authority.
use super::{
    execution_bridge::formal_node_hook::FormalPublicationSink,
    formal_effect_delivery_store::FormalEffectDeliveryStore,
    formal_effect_outbox::{QualifiedEffectOutboxV1, validate_qualified_effect_outbox},
    formal_effect_transport::{EffectDeliveryDriveResultV1, FormalEffectDeliveryTransport},
    formal_live_config::FormalEffectDeliveryConfigV1,
    formal_live_service::{FormalLiveHistoryResolver, FormalLiveState, PublishedReadState},
};
use ed25519_dalek::SigningKey;
use oasis7_distfs::controlled_authority::formal::VerifiedFormalReadAnchorV1;
use oasis7_runtime::World;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FormalEffectDeliveryStatusV1 {
    pub committed_position: u64,
    pub pending: usize,
    pub unknown: usize,
    pub blocked: usize,
    pub terminal: usize,
    pub last_error: Option<String>,
}

pub(crate) struct QualifiedOutboxPublicationSink {
    published: Arc<PublishedReadState>,
    history: Arc<dyn FormalLiveHistoryResolver>,
    store: Arc<FormalEffectDeliveryStore>,
    latest: Arc<Mutex<Option<QualifiedEffectOutboxV1>>>,
}

impl FormalPublicationSink for QualifiedOutboxPublicationSink {
    fn publish(&self, world: World, anchor: VerifiedFormalReadAnchorV1) -> Result<(), String> {
        // The commit is already qualified: keep that fact readable even when
        // local delivery persistence fails and the hook fences further writes.
        self.published.publish(world, anchor.clone())?;
        let material = self.history.full_sync_material(&anchor)?;
        let outbox = QualifiedEffectOutboxV1::from_qualified_material(&anchor, &material)?;
        for obligation in &outbox.obligations {
            self.store.observe(obligation)?;
        }
        *self
            .latest
            .lock()
            .map_err(|_| "qualified effect publication lock poisoned")? = Some(outbox);
        Ok(())
    }
}

pub(crate) struct FormalEffectDeliveryRuntime {
    transport: Arc<FormalEffectDeliveryTransport>,
    latest: Arc<Mutex<Option<QualifiedEffectOutboxV1>>>,
    stopping: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    done: Option<mpsc::Receiver<()>>,
}

impl FormalEffectDeliveryRuntime {
    pub(crate) fn open(
        config: &FormalEffectDeliveryConfigV1,
        writer_directory: &Path,
        sender: SigningKey,
        published: Arc<PublishedReadState>,
        history: Arc<dyn FormalLiveHistoryResolver>,
    ) -> Result<(Self, Arc<QualifiedOutboxPublicationSink>), String> {
        let store = Arc::new(FormalEffectDeliveryStore::open(
            &writer_directory.join("effect-delivery-v1"),
        )?);
        let latest = Arc::new(Mutex::new(None));
        let transport = Arc::new(FormalEffectDeliveryTransport::new(
            config.routes.clone(),
            sender,
            store.clone(),
        )?);
        let sink = Arc::new(QualifiedOutboxPublicationSink {
            published,
            history,
            store,
            latest: latest.clone(),
        });
        Ok((
            Self {
                transport,
                latest,
                stopping: Arc::new(AtomicBool::new(false)),
                worker: None,
                done: None,
            },
            sink,
        ))
    }

    pub(crate) fn start(&mut self, state: FormalLiveState) -> Result<(), String> {
        if self.worker.is_some() {
            return Err("effect delivery worker is already owned".into());
        }
        self.stopping.store(false, Ordering::Release);
        let stopping = self.stopping.clone();
        let latest = self.latest.clone();
        let transport = self.transport.clone();
        let (send, receive) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("oasis7-qualified-effects".into())
            .spawn(move || {
                while !stopping.load(Ordering::Acquire) {
                    let outbox = match latest.lock() {
                        Ok(latest) => latest.clone(),
                        Err(_) => break,
                    };
                    if let Some(outbox) = outbox {
                        if let Err(error) = validate_qualified_effect_outbox(&outbox) {
                            if let Ok(mut status) = state.effect_delivery.lock() {
                                *status = Some(FormalEffectDeliveryStatusV1 {
                                    last_error: Some(format!("qualified delivery source: {error}")),
                                    ..Default::default()
                                });
                            }
                            break;
                        }
                        let mut status = FormalEffectDeliveryStatusV1 {
                            committed_position: outbox.commit.position,
                            blocked: outbox.blocked.len(),
                            ..Default::default()
                        };
                        for obligation in &outbox.obligations {
                            if stopping.load(Ordering::Acquire) {
                                break;
                            }
                            match transport.drive_qualified_obligation(obligation) {
                                EffectDeliveryDriveResultV1::Blocked { .. } => status.blocked += 1,
                                EffectDeliveryDriveResultV1::Pending { .. } => status.pending += 1,
                                EffectDeliveryDriveResultV1::Unknown { .. } => status.unknown += 1,
                                EffectDeliveryDriveResultV1::Delivered(_)
                                | EffectDeliveryDriveResultV1::RejectedNoEffect { .. }
                                | EffectDeliveryDriveResultV1::AlreadyTerminal => {
                                    status.terminal += 1
                                }
                            }
                        }
                        if status.unknown > 0 || status.pending > 0 {
                            status.last_error =
                                Some("external effect delivery requires reconciliation".into());
                        }
                        if let Ok(mut current) = state.effect_delivery.lock() {
                            *current = Some(status);
                        }
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                let _ = send.send(());
            })
            .map_err(|error| format!("start effect delivery worker: {error}"))?;
        self.worker = Some(worker);
        self.done = Some(receive);
        Ok(())
    }

    pub(crate) fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    pub(crate) fn stop(&mut self, timeout: Duration) -> Result<(), String> {
        self.request_stop();
        if let Some(done) = &self.done {
            done.recv_timeout(timeout)
                .map_err(|_| "effect delivery stop remains pending; worker is still owned")?;
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "effect delivery worker panicked")?;
        }
        self.done = None;
        Ok(())
    }
}

impl Drop for FormalEffectDeliveryRuntime {
    fn drop(&mut self) {
        self.request_stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
