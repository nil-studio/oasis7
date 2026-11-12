//! Workers implementation of dev-test-accounts-v1. World authority is remote.
#![forbid(unsafe_code)]
mod config;
mod platform;
mod protocol;
mod state;
mod store;

mod cache;
mod native;

mod operations;

#[cfg(test)]
mod config_tests;
#[cfg(test)]
mod protocol_tests;
