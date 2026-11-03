use super::super::*;

#[derive(Debug, PartialEq)]
pub(super) enum ValidationView<'a> {
    ProductValidated {
        requester_agent_id: &'a String,
        module_id: &'a String,
        stack: &'a MaterialStack,
        stack_limit: &'a u32,
        tradable: &'a bool,
        quality_levels: &'a Vec<String>,
        notes: &'a Vec<String>,
    },
    ProductValidationRecorded {
        receipt: &'a ProductValidationReceiptV1,
    },
    ProductValidationAttemptStarted {
        attempt: &'a ProductValidationAttemptV1,
    },
    MaterialProfileGoverned {
        operator_agent_id: &'a String,
        proposal_id: &'a ProposalId,
        profile: &'a MaterialProfileV1,
    },
    ProductProfileGoverned {
        operator_agent_id: &'a String,
        proposal_id: &'a ProposalId,
        profile: &'a ProductProfileV1,
    },
    RecipeProfileGoverned {
        operator_agent_id: &'a String,
        proposal_id: &'a ProposalId,
        profile: &'a RecipeProfileV1,
    },
    FactoryProfileGoverned {
        operator_agent_id: &'a String,
        proposal_id: &'a ProposalId,
        profile: &'a FactoryProfileV1,
    },
}

impl<'a> ValidationView<'a> {
    pub(super) fn from_event(event: &'a DomainEvent) -> Self {
        match event {
            DomainEvent::ProductValidated {
                requester_agent_id,
                module_id,
                stack,
                stack_limit,
                tradable,
                quality_levels,
                notes,
            } => Self::ProductValidated {
                requester_agent_id,
                module_id,
                stack,
                stack_limit,
                tradable,
                quality_levels,
                notes,
            },
            DomainEvent::ProductValidationRecorded { receipt } => {
                Self::ProductValidationRecorded { receipt }
            }
            DomainEvent::ProductValidationAttemptStarted { attempt } => {
                Self::ProductValidationAttemptStarted { attempt }
            }
            DomainEvent::MaterialProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            } => Self::MaterialProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            },
            DomainEvent::ProductProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            } => Self::ProductProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            },
            DomainEvent::RecipeProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            } => Self::RecipeProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            },
            DomainEvent::FactoryProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            } => Self::FactoryProfileGoverned {
                operator_agent_id,
                proposal_id,
                profile,
            },
            _ => unreachable!("event was routed to the wrong validation view"),
        }
    }
}
