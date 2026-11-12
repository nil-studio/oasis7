//! Fixed factory scheduling Current profile selectors, DATA only.
//! This child reuses the original closed native Current selector and codec.
use super::*;
use crate::incremental_world_native_industry::{
    FactoryProfilesFieldV1, ProductProfilesFieldV1, RecipeProfilesFieldV1,
};
current_claim!(
    recover_current_schedule_factory_profile_v1,
    factory_profiles,
    FactoryProfilesFieldV1,
    oasis7_wasm_abi::FactoryProfileV1
);
current_claim!(
    recover_current_schedule_recipe_profile_v1,
    recipe_profiles,
    RecipeProfilesFieldV1,
    oasis7_wasm_abi::RecipeProfileV1
);
current_claim!(
    recover_current_schedule_product_profile_v1,
    product_profiles,
    ProductProfilesFieldV1,
    oasis7_wasm_abi::ProductProfileV1
);
