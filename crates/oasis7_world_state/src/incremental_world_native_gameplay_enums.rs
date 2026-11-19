//! Original closed enum variants; Option never aliases a unit variant.
use super::*;
tagged_codec!(oasis7_wasm_abi::ModuleKind,
    units { Reducer=0,Pure=1 }, tuples {}, records {});
tagged_codec!(oasis7_wasm_abi::ModuleRole,
    units { Rule=0,Domain=1,Gameplay=2,Body=3,AgentInternal=4 }, tuples {}, records {});
tagged_codec!(oasis7_wasm_abi::ModuleSubscriptionStage,
    units { PreAction=0,PostAction=1,PostEvent=2,Tick=3 }, tuples {}, records {});
tagged_codec!(oasis7_wasm_abi::GameplayModuleKind,
    units { War=0,Governance=1,Crisis=2,Economic=3,Meta=4 }, tuples {}, records {});
tagged_codec!(oasis7_agent_api::world_action::ModuleInstallTarget,
    units { SelfAgent=0 }, tuples {}, records { LocationInfrastructure {location_id}=1 });
tagged_codec!(crate::state::ModuleReleaseRequestStatus,
    units { Requested=0,Shadowed=1,PartiallyApproved=2,Approved=3,Rejected=4,Applied=5 }, tuples {}, records {});
