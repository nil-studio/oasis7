//! Closed original gameplay, claim and module-release native World DTOs.
//! These codecs prove data shape only; they confer no gameplay/admission authority.
use crate::incremental_typed_record_codec::*;
#[path = "incremental_world_native_gameplay_dto.rs"]
mod dto;
#[path = "incremental_world_native_gameplay_enums.rs"]
mod enums;
#[path = "incremental_world_native_gameplay_genesis.rs"]
mod genesis;
#[path = "incremental_world_native_gameplay_manifest.rs"]
mod manifest;
pub use genesis::*;

#[cfg(test)]
#[path = "incremental_world_native_gameplay_fixture.rs"]
pub(crate) mod fixture;
#[cfg(test)]
#[path = "incremental_world_native_gameplay_tests.rs"]
mod tests;
