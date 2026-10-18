use super::super::*;
use std::collections::{BTreeMap, BTreeSet};

#[path = "main_token/raw_monetary_core_publication_transaction_regressions.rs"]
mod raw_monetary_core_publication_transaction_regressions;
#[path = "main_token/raw_monetary_governance_publication_transaction_regressions.rs"]
mod raw_monetary_governance_publication_transaction_regressions;
#[path = "main_token/raw_policy_treasury_publication_transaction_regressions.rs"]
mod raw_policy_treasury_publication_transaction_regressions;
#[path = "main_token/raw_starter_oc_publication_transaction_regressions.rs"]
mod raw_starter_oc_publication_transaction_regressions;

fn set_main_token_controller_registry_for_tests(world: &mut World, ecosystem_controller: &str) {
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string(),
                ecosystem_controller.to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::new(),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    ecosystem_controller.to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                            "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry for tests");
}

include!("main_token_core_tests.rs");
include!("main_token_governance_tests.rs");
