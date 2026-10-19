use oasis7_command_center_core::context::*;
use oasis7_command_center_core::daily_work::*;
use oasis7_command_center_core::document_directory::*;
use oasis7_command_center_core::document_inventory::*;
use oasis7_command_center_core::document_semantic_tree::*;
use oasis7_command_center_core::document_tree_reading::*;
use oasis7_command_center_core::domain::*;
use oasis7_command_center_core::github::{GitHubSnapshot, PullMetadata, PullRequest};
use oasis7_command_center_core::tools::jobs::ToolJob;
use oasis7_command_center_core::tools::*;
use oasis7_command_center_core::work_projection::*;
use ts_rs::TS;
fn main() {
    let declarations = [
        WorkNavigation::decl(),
        WorkRelation::decl(),
        SyncEvidence::decl(),
        DecisionFollowthrough::decl(),
        WorkAttention::decl(),
        ExternalWork::decl(),
        WorktreeReference::decl(),
        DailyWorkState::decl(),
        DailyWorkAction::decl(),
        DailyWorkRequest::decl(),
        WorkRef::decl(),
        SourceFact::decl(),
        FactCell::decl(),
        WorkGoal::decl(),
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
        PullMetadata::decl(),
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
