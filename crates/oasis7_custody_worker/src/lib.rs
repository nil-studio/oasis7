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

#[cfg(target_arch = "wasm32")]
mod platform;
