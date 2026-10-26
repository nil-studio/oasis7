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
#[cfg(all(target_arch = "wasm32", any(feature = "owner", feature = "identity")))]
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

#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod native_identity;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_account;

#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod directory_config;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod directory_rpc;
#[cfg(all(target_arch = "wasm32", any(feature = "identity", feature = "owner")))]
mod entrypoints;
#[cfg(all(target_arch = "wasm32", feature = "identity"))]
mod management;
#[cfg(all(target_arch = "wasm32", feature = "owner"))]
mod owner_directory;
