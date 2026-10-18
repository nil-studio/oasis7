//! Host assembly and historical archive compatibility for the native kernel.
mod archive_origin;
mod codec;
mod forwarding;
mod journal_view;
use super::archive::RuntimeWorldEvent;
use super::archive::{WorldEvent, WorldSnapshotExtensions};
use super::persist::{PersistError, WorldJournal, WorldSnapshot};
use super::{KernelJournal, KernelSnapshot};
use crate::chain_resource_schema::ChainResourceDerivationContext;
use archive_origin::ArchiveOrigin;
use journal_view::split_events;
pub use journal_view::{WorldEventRef, WorldJournalView};
use oasis7_simulator::ConsumeReason;
use oasis7_simulator::LongTermMemoryEntry;
use oasis7_simulator::WorldKernel as NativeKernel;
use oasis7_simulator::*;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct WorldKernel {
    kernel: NativeKernel,
    runtime_events: std::collections::BTreeMap<usize, RuntimeWorldEvent>,
    archive_origin: Option<Box<ArchiveOrigin>>,
}
impl PartialEq for WorldKernel {
    fn eq(&self, other: &Self) -> bool {
        self.kernel == other.kernel && self.runtime_events == other.runtime_events
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
        Self {
            kernel,
            runtime_events: Default::default(),
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
        self.assert_journal_sidecar();
    }
    pub(super) fn record_effect_receipt(
        &mut self,
        agent_id: AgentId,
        receipt: super::LlmEffectReceiptTrace,
    ) {
        self.kernel.record_effect_receipt(agent_id, receipt);
        self.assert_journal_sidecar();
    }
    fn assert_journal_sidecar(&mut self) {
        assert!(
            self.runtime_events
                .last_key_value()
                .is_none_or(|(index, _)| *index < self.kernel.journal().len()),
            "native kernel journal cannot shrink"
        );
    }
    pub(crate) fn into_model(self) -> WorldModel {
        self.kernel.into_state().model
    }
    pub fn journal(&self) -> WorldJournalView<'_> {
        WorldJournalView {
            events: self.kernel.journal(),
            runtime_events: &self.runtime_events,
        }
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
            events: self.journal().iter().map(WorldEventRef::to_owned).collect(),
        }
    }
    pub fn snapshot(&self) -> WorldSnapshot {
        let mut snapshot = self.snapshot_with_chain_resource_context(
            ChainResourceDerivationContext::simulator_default(self.time()),
        );
        if let Some(origin) = self.archive_origin.as_deref()
            && origin.external_chain_evidence
        {
            snapshot.chain_resource_manifest = origin.extensions.chain_resource_manifest.clone();
            snapshot.latest_chain_resource_delta =
                origin.extensions.latest_chain_resource_delta.clone();
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
                    .and_then(|origin| origin.extensions.runtime_snapshot.clone()),
                player_gameplay: self
                    .archive_origin
                    .as_deref()
                    .and_then(|origin| origin.extensions.player_gameplay.clone()),
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
        let (events, runtime_events) = split_events(journal.events);
        let native_journal = KernelJournal {
            version: journal.version,
            events,
        };
        native.validate_version()?;
        native_journal.validate_version()?;
        let origin = ArchiveOrigin::new(&native, extensions, &native_journal.events)?;
        let kernel = NativeKernel::from_kernel_snapshot(native, native_journal)?;
        let mut host = Self::from_native(kernel);
        host.runtime_events = runtime_events;
        host.archive_origin = Some(Box::new(origin));
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
        let (events, runtime_events) = split_events(journal.events);
        let native_journal = KernelJournal {
            version: journal.version,
            events,
        };
        native.validate_version()?;
        native_journal.validate_version()?;
        let origin = ArchiveOrigin::new(&native, extensions, &native_journal.events)?;
        let kernel = NativeKernel::replay_from_kernel_snapshot(native, native_journal)?;
        let mut host = Self::from_native(kernel);
        if let Some(compiler) = compiler {
            host.kernel.set_module_source_compiler(compiler);
        }
        host.runtime_events = runtime_events;
        host.archive_origin = Some(Box::new(origin));
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
