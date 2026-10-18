use oasis7_command_center_core::codex::*;
use oasis7_command_center_core::context::*;
use oasis7_command_center_core::document_directory::*;
use oasis7_command_center_core::document_inventory::*;
use oasis7_command_center_core::document_semantic_tree::*;
use oasis7_command_center_core::document_tree_reading::*;
use oasis7_command_center_core::domain::*;
use oasis7_command_center_core::github::{Check, GitHubSnapshot, PullChecksSource, PullRequest};
use oasis7_command_center_core::tools::jobs::ToolJob;
use oasis7_command_center_core::tools::*;
use oasis7_command_center_core::work_projection::*;
use ts_rs::TS;
fn main() {
    let declarations = [
        WorkRef::decl(),
        SourceFact::decl(),
        FactCell::decl(),
        WorkGoal::decl(),
        WorkRun::decl(),
        WorkItem::decl(),
        WorkSource::decl(),
        WorkProjection::decl(),
        ContextBinding::decl(),
        ContextEntry::decl(),
        ContextSummary::decl(),
        ContextQuery::decl(),
        ContextSaveRequest::decl(),
        ContextSnapshotRestoreSource::decl(),
        ContextRestoreIdentity::decl(),
        ToolObservation::decl(),
        FileSignInput::decl(),
        FileSignPrepared::decl(),
        FilePrepareRecovery::decl(),
        FilePrepareEffect::decl(),
        FilePrepareError::decl(),
        ToolJob::decl(),
        oasis7_command_center_core::tools::cli::CliInstallation::decl(),
        PrivateInputRef::decl(),
        PrivateInputPreview::decl(),
        Project::decl(),
        SourceStatus::decl(),
        DocumentLifecycle::decl(),
        DocumentSection::decl(),
        DocumentNavigation::decl(),
        DocumentTreeRouteStep::decl(),
        DocumentTreeReadingScope::decl(),
        DocumentTreeReadingKey::decl(),
        DocumentTreeReadingFilters::decl(),
        DocumentTreeReadingSelection::decl(),
        DocumentTreeReadingState::decl(),
        DocumentTreeReadingSaveRequest::decl(),
        DocumentTreeReadingLoadResult::decl(),
        DocumentTreeLastView::decl(),
        DocumentSemanticTreeQuery::decl(),
        DocumentSemanticTreeEntry::decl(),
        DocumentSemanticTreePage::decl(),
        DocumentSemanticBreadcrumb::decl(),
        DocumentPairing::decl(),
        DocumentPairings::decl(),
        DocumentIdentity::decl(),
        DocumentAllocation::decl(),
        DocumentAllocations::decl(),
        IndexedDocument::decl(),
        DocumentInventoryQuery::decl(),
        DocumentInventoryPage::decl(),
        DocumentDirectoryQuery::decl(),
        DocumentDirectoryEntry::decl(),
        DocumentDirectoryPage::decl(),
        Preferences::decl(),
        DecisionDraft::decl(),
        DecisionRecord::decl(),
        ReadingPosition::decl(),
        GitSnapshot::decl(),
        WorkAssociation::decl(),
        Snapshot::decl(),
        GitHubSnapshot::decl(),
        PullRequest::decl(),
        Check::decl(),
        PullChecksSource::decl(),
        RuntimeTarget::decl(),
        RuntimeSession::decl(),
        RuntimeOperation::decl(),
        RuntimeQuestion::decl(),
        RuntimeRequest::decl(),
        RequestAnswer::decl(),
        RuntimeSnapshot::decl(),
        QuitStatus::decl(),
        SessionConfig::decl(),
    ];
    let result = format!(
        "// Generated from Rust DTOs by cargo run -p oasis7-command-center-core --example export_types\n{}\n",
        declarations
            .iter()
            .map(|s| format!(
                "export {}",
                s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/generated/core.ts");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, result).unwrap();
}
