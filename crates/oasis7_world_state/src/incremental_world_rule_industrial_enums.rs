//! Closed original variants. Even unit variants have a tagged child tree, so
//! an outer Option's None remains distinct from Some(unit_variant).
use super::*;

use crate::incremental_typed_record_codec::tagged_codec;

tagged_codec!(oasis7_agent_api::world_action::MaterialLedgerId,
    units { World = 0 }, tuples { Agent(id) = 1, Site(id) = 2, Factory(id) = 3 }, records {});
tagged_codec!(oasis7_agent_api::world_action::IndustrialDispositionV1,
    units { Hold = 0, Terminate = 1 }, tuples {}, records {});
tagged_codec!(oasis7_agent_api::world_action::SubstitutionSelectionV1,
units {}, tuples {}, records {
    SameMaterialSource { source_lot, source_material } = 0,
    ProfileRule { rule, source_lot, source_material } = 1,
    NewRecipe { candidate_root } = 2
});
tagged_codec!(oasis7_agent_api::world_action::SubstitutionCandidateKindV1,
    units { SameMaterial = 0, ProfileSubstitute = 1, NewRecipe = 2 }, tuples {}, records {});
tagged_codec!(oasis7_agent_api::world_action::SubstitutionPreviewStatusV1,
    units { Available = 0, NoLegalSubstitute = 1, Unknown = 2 }, tuples {}, records {});
tagged_codec!(oasis7_agent_api::world_action::SubstitutionScopeV1,
    units { DecisionOnly = 0, DecisionAndExactConversion = 1 }, tuples {}, records {});
tagged_codec!(oasis7_wasm_abi::SubstitutionRoundingV1,
    units { Exact = 0, Ceil = 1 }, tuples {}, records {});
tagged_codec!(oasis7_wasm_abi::SubstitutionResidualV1,
    units { Reject = 0, Retain = 1 }, tuples {}, records {});
tagged_codec!(oasis7_wasm_abi::SubstitutionConversionV1,
    units { None = 0 }, tuples {}, records { Required { converter } = 1 });
