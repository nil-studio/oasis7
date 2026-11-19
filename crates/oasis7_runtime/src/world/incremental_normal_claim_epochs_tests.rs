//! Fixed decision/mutation regressions only. These selected DATA fixtures do
//! not claim Current source, counted publication or whole-tick acceptance.
use super::*;
use oasis7_world_state::{AgentClaimState,MainTokenAccountBalance,MainTokenSupplyState};
use std::collections::BTreeMap;
fn claim()->AgentClaimState{serde_json::from_value(serde_json::json!({"target_agent_id":"target","claim_owner_id":"owner","slot_index":1,"upkeep_per_epoch":5,"upkeep_paid_through_epoch":2,"release_cooldown_epochs":2,"grace_epochs":2,"idle_warning_epochs":3,"forced_idle_reclaim_epochs":5,"locked_bond_amount":100,"claim_bond_locked_restricted_amount":40,"claim_bond_locked_liquid_amount":60,"forced_reclaim_penalty_bps":1000})).unwrap()}
#[test]
fn epoch_upkeep_retains_original_funding_and_enables_only_fresh_followup(){
 let mut current=claim();let refund=evaluate::refund_sink_v1(&current,None);
 let event=evaluate::prepare_epoch_v1(4,&current,7,3,0,&refund).unwrap().unwrap();
 assert!(matches!(&event,crate::DomainEvent::AgentClaimUpkeepSettled{activity_contract_version:1,charged_epochs:2,amount:10,restricted_spent_amount:3,liquid_spent_amount:7,upkeep_paid_through_epoch:4,..}));
 let mut account=Some(MainTokenAccountBalance{account_id:"real-inner-account".into(),liquid_balance:7,restricted_starter_claim_balance:3,..Default::default()});
 let mut supply=MainTokenSupplyState{total_supply:1000,circulating_supply:1000,..Default::default()};let mut treasury=BTreeMap::new();
 oasis7_world_state::world::agent_claim_economic_publication::apply_selected_agent_claim_upkeep_v1(&event,"owner",&mut current,&mut account,&mut supply,&BTreeMap::new(),&mut treasury).unwrap();
 assert_eq!(current.upkeep_paid_through_epoch,4);assert_eq!(account.as_ref().unwrap().account_id,"real-inner-account");assert_eq!(account.as_ref().unwrap().liquid_balance,0);assert_eq!(supply.circulating_supply,990);
 assert_eq!(treasury[oasis7_world_state::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL],10);
 let next=evaluate::prepare_release_or_idle_v1(4,&current,0,&refund).unwrap().unwrap();assert!(matches!(next,crate::DomainEvent::AgentClaimIdleWarning{warning_emitted_at_epoch:4,forced_reclaim_at_epoch:5,..}));
}
#[test]
fn epoch_grace_boundary_and_release_precedence_preserve_original_refund(){
 let mut current=claim();let refund=evaluate::refund_sink_v1(&current,None);
 let grace=evaluate::prepare_epoch_v1(4,&current,0,0,0,&refund).unwrap().unwrap();assert!(matches!(grace,crate::DomainEvent::AgentClaimEnteredGrace{delinquent_since_epoch:3,grace_deadline_epoch:5,upkeep_arrears_amount:10,..}));
 let mut actual_last_active=77;oasis7_world_state::world::agent_claim_light_lifecycle_publication::apply_selected_agent_claim_light_v1(&grace,&mut current,&mut actual_last_active,99).unwrap();assert_eq!(actual_last_active,77);
 assert!(evaluate::prepare_epoch_v1(5,&current,0,0,0,&refund).unwrap().is_none());
 let reclaimed=evaluate::prepare_epoch_v1(6,&current,0,0,0,&refund).unwrap().unwrap();assert!(matches!(reclaimed,crate::DomainEvent::AgentClaimReclaimed{upkeep_arrears_amount:20,collected_upkeep_amount:20,penalty_amount:8,refunded_bond_amount:72,..}));
 current.release_ready_at_epoch=Some(6);
 let released=evaluate::prepare_release_or_idle_v1(6,&current,0,&refund).unwrap().unwrap();assert!(matches!(released,crate::DomainEvent::AgentClaimReleased{refunded_bond_amount:100,refunded_bond_restricted_amount:40,refunded_bond_liquid_amount:60,..}));
}
