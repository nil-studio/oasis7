//! Pure custody foundations. No clocks, randomness, storage or network access.
//! Callers must persist related mutations in one authority transaction.
#![forbid(unsafe_code)]
pub use oasis7_agent_api::game::DecimalU64;
pub mod approval;
pub mod approval_admission;
pub mod budget;
pub mod envelope;
pub mod http;
pub mod lifecycle;
pub mod operations;
pub mod provisioning;
pub mod recovery;

pub mod source_revocation;

pub mod role_key_slot;

pub mod native_binding;

pub mod native_history;

pub mod http_pop;
pub mod time_units;

pub mod client_enrollment;

/// Opt-in bounded diagnostics; never an authorization or signature-result cache.
#[cfg(feature = "signature-metrics")]
pub use oasis7_client_api::world_service::private_goal::signature_metrics;

/// Pure exact-signature fact reuse in one synchronous operation; never authority.
pub use oasis7_client_api::world_service::private_goal::with_owner_signature_verification_batch;

pub mod context_body;
pub mod context_body_v2;

pub mod management_metadata;

pub mod native_cleanup_v2;

pub mod native_model_proxy_query_v2;
pub mod native_model_proxy_v2;

pub mod native_model_graph_v2;

pub mod native_execution_prepare_v2;

pub mod execution_private_current_v2;

pub mod execution_dispatch_gate_v2;

pub mod term_guarded_signer_v2;

pub mod term_guarded_close_signer_v2;

pub mod term_owner_slot_lineage_v2;

pub mod execution_pending_current_v2;

pub mod execution_fence_dispatch_v2;

pub mod execution_pending_source_inspect_v2;
pub mod execution_pending_source_result_v2;
/// Independent read-only RF signed transport codecs; no Current grant.
pub mod read_fence_dispatch_v3;

/// Guarded dual-profile FENCES history inventory; never Current.
pub mod read_fence_admission_v3;

pub mod read_claim_source_v3;
pub mod read_directory_admission_v3;
pub mod read_directory_v3;
