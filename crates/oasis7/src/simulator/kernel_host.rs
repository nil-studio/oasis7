//! Host assembly and historical archive compatibility for the native kernel.
mod codec;
mod forwarding;
use super::archive::{WorldEvent, WorldSnapshotExtensions};
use super::persist::{PersistError, WorldJournal, WorldSnapshot};
use super::{KernelJournal, KernelSnapshot};
use crate::chain_resource_schema::ChainResourceDerivationContext;
use oasis7_simulator::ConsumeReason;
use oasis7_simulator::LongTermMemoryEntry;
use oasis7_simulator::WorldKernel as NativeKernel;
use oasis7_simulator::*;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct WorldKernel {
    kernel: NativeKernel,
    journal: Vec<WorldEvent>,
    archive_origin: Option<Box<(KernelSnapshot, WorldSnapshotExtensions)>>,
}
impl PartialEq for WorldKernel {
    fn eq(&self, other: &Self) -> bool {
        self.kernel == other.kernel && self.journal == other.journal
    }
}
// Immutable access exposes pure queries without allowing journal synchronization to be bypassed.
impl std::ops::Deref for WorldKernel {
    type Target = NativeKernel;
    fn deref(&self) -> &Self::Target {
        &self.kernel
    }
}
impl Default for WorldKernel {
    fn default() -> Self {
        Self::from_native(NativeKernel::default())
    }
}
impl WorldKernel {
    fn from_native(mut kernel: NativeKernel) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        kernel.set_module_source_compiler(Arc::new(
            crate::module_source_compiler::HostModuleSourceCompiler,
        ));
        let journal = kernel
            .journal()
            .iter()
            .cloned()
            .map(|event| WorldEvent::from_kernel_parts(event, None))
            .collect();
        Self {
            kernel,
            journal,
            archive_origin: None,
        }
    }
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_config(config: WorldConfig) -> Self {
        Self::from_native(NativeKernel::with_config(config))
    }
    pub fn with_model(config: WorldConfig, model: WorldModel) -> Self {
        Self::from_native(NativeKernel::with_model(config, model))
    }
    pub fn with_model_and_chunk_runtime(
        config: WorldConfig,
        model: WorldModel,
        chunk_runtime: ChunkRuntimeConfig,
    ) -> Self {
        Self::from_native(NativeKernel::with_model_and_chunk_runtime(
            config,
            model,
            chunk_runtime,
        ))
    }
    pub(super) fn record_effect_intent(
        &mut self,
        agent_id: AgentId,
        intent: super::LlmEffectIntentTrace,
    ) {
        self.kernel.record_effect_intent(agent_id, intent);
        self.sync_journal();
    }
    pub(super) fn record_effect_receipt(
        &mut self,
        agent_id: AgentId,
        receipt: super::LlmEffectReceiptTrace,
    ) {
        self.kernel.record_effect_receipt(agent_id, receipt);
        self.sync_journal();
    }
    fn sync_journal(&mut self) {
        let native = self.kernel.journal();
        assert!(
            native.len() >= self.journal.len(),
            "native kernel journal cannot shrink"
        );
        self.journal.extend(
            native[self.journal.len()..]
                .iter()
                .cloned()
                .map(|event| WorldEvent::from_kernel_parts(event, None)),
        );
    }
    pub(crate) fn into_model(self) -> WorldModel {
        self.kernel.into_state().model
    }
    pub fn journal(&self) -> &[WorldEvent] {
        &self.journal
    }
    pub fn kernel_snapshot(&self) -> KernelSnapshot {
        self.kernel.kernel_snapshot()
    }
    pub fn kernel_journal_snapshot(&self) -> KernelJournal {
        self.kernel.kernel_journal_snapshot()
    }
    pub fn journal_snapshot(&self) -> WorldJournal {
        WorldJournal {
            version: JOURNAL_VERSION,
            events: self.journal.clone(),
        }
    }
    pub fn snapshot(&self) -> WorldSnapshot {
        if let Some((native, extensions)) = self.archive_origin.as_deref()
            && *native == self.kernel_snapshot()
        {
            return WorldSnapshot::from_kernel_parts(native.clone(), extensions.clone());
        }
        let mut snapshot = self.snapshot_with_chain_resource_context(
            ChainResourceDerivationContext::simulator_default(self.time()),
        );
        if let Some((native, extensions)) = self.archive_origin.as_deref() {
            let context = ChainResourceDerivationContext::simulator_default(native.time);
            let prefix = &self.kernel.journal()[..native.journal_len];
            let derived_manifest = oasis7_simulator::from_simulator_state(
                context,
                &native.model,
                &native.config,
                &native.chunk_runtime,
                prefix,
            );
            let derived_delta =
                oasis7_simulator::latest_from_simulator_journal(context, &derived_manifest, prefix);
            // Host-supplied chain evidence describes its original committed cut.
            // Only the explicit context API may replace that evidence. Ordinary
            // pure-kernel archives continue refreshing their default derivation.
            if extensions.chain_resource_manifest != derived_manifest
                || extensions.latest_chain_resource_delta != derived_delta
            {
                snapshot.chain_resource_manifest = extensions.chain_resource_manifest.clone();
                snapshot.latest_chain_resource_delta =
                    extensions.latest_chain_resource_delta.clone();
            }
        }
        snapshot
    }
    pub fn snapshot_with_chain_resource_context(
        &self,
        context: ChainResourceDerivationContext<'_>,
    ) -> WorldSnapshot {
        let state = self.kernel_snapshot();
        let chain_resource_manifest = oasis7_simulator::from_simulator_state(
            context,
            &state.model,
            &state.config,
            &state.chunk_runtime,
            self.kernel.journal(),
        );
        let latest_chain_resource_delta = oasis7_simulator::latest_from_simulator_journal(
            context,
            &chain_resource_manifest,
            self.kernel.journal(),
        );
        WorldSnapshot::from_kernel_parts(
            state,
            WorldSnapshotExtensions {
                runtime_snapshot: self
                    .archive_origin
                    .as_deref()
                    .and_then(|(_, extensions)| extensions.runtime_snapshot.clone()),
                player_gameplay: self
                    .archive_origin
                    .as_deref()
                    .and_then(|(_, extensions)| extensions.player_gameplay.clone()),
                chain_resource_manifest,
                latest_chain_resource_delta,
            },
        )
    }
    pub fn from_snapshot(
        snapshot: WorldSnapshot,
        journal: WorldJournal,
    ) -> Result<Self, PersistError> {
        let (native, extensions) = snapshot.into_kernel_parts();
        let events = journal
            .events
            .iter()
            .cloned()
            .map(|event| event.into_kernel_parts().0)
            .collect();
        let kernel = NativeKernel::from_kernel_snapshot(
            native.clone(),
            KernelJournal {
                version: journal.version,
                events,
            },
        )?;
        let mut host = Self::from_native(kernel);
        host.journal = journal.events;
        host.archive_origin = Some(Box::new((native, extensions)));
        Ok(host)
    }
    pub fn replay_from_snapshot(
        snapshot: WorldSnapshot,
        journal: WorldJournal,
    ) -> Result<Self, PersistError> {
        Self::replay_with_compiler(snapshot, journal, None)
    }
    pub fn replay_from_snapshot_with_source_compiler(
        snapshot: WorldSnapshot,
        journal: WorldJournal,
        compiler: Arc<dyn oasis7_agent_api::module_source::ModuleSourceCompiler>,
    ) -> Result<Self, PersistError> {
        Self::replay_with_compiler(snapshot, journal, Some(compiler))
    }
    fn replay_with_compiler(
        snapshot: WorldSnapshot,
        journal: WorldJournal,
        compiler: Option<Arc<dyn oasis7_agent_api::module_source::ModuleSourceCompiler>>,
    ) -> Result<Self, PersistError> {
        let (native, extensions) = snapshot.into_kernel_parts();
        let events = journal
            .events
            .iter()
            .cloned()
            .map(|event| event.into_kernel_parts().0)
            .collect();
        let native_journal = KernelJournal {
            version: journal.version,
            events,
        };
        let kernel = NativeKernel::replay_from_kernel_snapshot(native.clone(), native_journal)?;
        let mut host = Self::from_native(kernel);
        if let Some(compiler) = compiler {
            host.kernel.set_module_source_compiler(compiler);
        }
        host.journal = journal.events;
        host.archive_origin = Some(Box::new((native, extensions)));
        Ok(host)
    }
    pub fn save_to_dir(&self, dir: impl AsRef<Path>) -> Result<(), PersistError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        self.snapshot().save_json(dir.join("snapshot.json"))?;
        self.journal_snapshot()
            .save_json(dir.join("journal.json"))?;
        Ok(())
    }
    pub fn save_to_dir_with_chain_resource_context(
        &self,
        dir: impl AsRef<Path>,
        context: ChainResourceDerivationContext<'_>,
    ) -> Result<(), PersistError> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        self.snapshot_with_chain_resource_context(context)
            .save_json(dir.join("snapshot.json"))?;
        self.journal_snapshot()
            .save_json(dir.join("journal.json"))?;
        Ok(())
    }
    pub fn load_from_dir(dir: impl AsRef<Path>) -> Result<Self, PersistError> {
        let dir = dir.as_ref();
        Self::from_snapshot(
            WorldSnapshot::load_json(dir.join("snapshot.json"))?,
            WorldJournal::load_json(dir.join("journal.json"))?,
        )
    }
}
pub fn initialize_kernel(
    config: WorldConfig,
    init: oasis7_simulator::WorldInitConfig,
) -> Result<(WorldKernel, oasis7_simulator::WorldInitReport), oasis7_simulator::WorldInitError> {
    oasis7_simulator::initialize_kernel(config, init)
        .map(|(kernel, report)| (WorldKernel::from_native(kernel), report))
}
