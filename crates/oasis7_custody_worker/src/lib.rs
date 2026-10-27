//! Four compile-time roles. Host builds have no deployable exports.
#![forbid(unsafe_code)]

#[cfg(all(
    target_arch = "wasm32",
    not(any(
        feature = "identity",
        feature = "owner",
        feature = "executor",
        feature = "recovery-fence"
    ))
))]
compile_error!("custody Wasm requires exactly one role feature");

#[cfg(any(
    all(
        feature = "identity",
        any(feature = "owner", feature = "executor", feature = "recovery-fence")
    ),
    all(
        feature = "owner",
        any(feature = "executor", feature = "recovery-fence")
    ),
    all(feature = "executor", feature = "recovery-fence")
))]
compile_error!("custody requires exactly one role feature; never use --all-features");

#[cfg(all(feature = "approval-harness", not(feature = "owner")))]
compile_error!("approval-harness requires owner and is not a deployment profile");

// Pure Rust adapter API only: no JS export and no production fetch/RPC caller.
#[cfg(all(
    target_arch = "wasm32",
    any(feature = "owner", feature = "identity", feature = "recovery-fence")
))]
pub mod approval_crypto;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
pub mod approval_store;
#[cfg(all(
    target_arch = "wasm32",
    any(feature = "identity", feature = "owner", feature = "executor")
))]
pub mod envelope_crypto;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
pub mod provision_approval_store;
#[cfg(all(target_arch = "wasm32", feature = "recovery-fence"))]
mod recovery_fence_control;
#[cfg(all(target_arch = "wasm32", feature = "recovery-fence"))]
mod recovery_fence_store;
#[cfg(all(
    target_arch = "wasm32",
    any(feature = "identity", feature = "owner", feature = "executor")
))]
pub mod role_key_store;
#[cfg(all(
    target_arch = "wasm32",
    any(feature = "identity", feature = "owner", feature = "executor")
))]
pub(crate) mod role_provision;

#[cfg(target_arch = "wasm32")]
mod platform;

#[cfg(all(target_arch = "wasm32", feature = "recovery-fence"))]
mod sql_store;

#[cfg(all(
    target_arch = "wasm32",
    feature = "owner",
    feature = "approval-harness"
))]
#[path = "../tests/support/approval_harness.rs"]
mod approval_harness;

#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod client_enrollment_authority;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod client_enrollment_entrypoints;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod client_enrollment_reader;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod client_enrollment_store;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod identity_native_runtime;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod native_identity;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_account;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_provision;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_test_session_alarm;

#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod native_runtime_rpc;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_participant_signing;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod owner_setup_reconcile_rpc;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_setup_reconcile_signing;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod owner_setup_rpc;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_setup_signing;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_test_session_client_entry;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_test_session_entrypoints;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod owner_test_session_rpc;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_test_session_signing;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod participant_runtime_rpc;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod test_session_client_authority;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod test_session_client_entrypoints;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod test_session_client_ingress;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod test_session_client_response;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod test_session_client_route;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod test_session_client_rpc;

#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod directory_config;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod directory_rpc;
#[cfg(all(
    target_arch = "wasm32",
    any(
        feature = "identity",
        feature = "owner",
        feature = "executor",
        feature = "recovery-fence"
    )
))]
mod entrypoints;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod management;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_directory;

#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod native_private_store;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod native_provision_challenge;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod native_provision_facts;

#[cfg(all(feature = "native-runtime-harness", not(feature = "identity")))]
compile_error!("native-runtime-harness requires identity and is not a deployment profile");
