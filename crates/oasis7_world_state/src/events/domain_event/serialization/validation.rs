use super::*;

#[derive(serde::Serialize)]
#[serde(rename = "ProductValidated")]
struct ProductValidatedPayload<'a> {
    requester_agent_id: &'a String,
    module_id: &'a String,
    stack: &'a MaterialStack,
    stack_limit: &'a u32,
    tradable: &'a bool,
    quality_levels: &'a Vec<String>,
    notes: &'a Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename = "ProductValidationRecorded")]
struct ProductValidationRecordedPayload<'a> {
    receipt: &'a ProductValidationReceiptV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "ProductValidationAttemptStarted")]
struct ProductValidationAttemptStartedPayload<'a> {
    attempt: &'a ProductValidationAttemptV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "MaterialProfileGoverned")]
struct MaterialProfileGovernedPayload<'a> {
    operator_agent_id: &'a String,
    proposal_id: &'a ProposalId,
    profile: &'a MaterialProfileV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "ProductProfileGoverned")]
struct ProductProfileGovernedPayload<'a> {
    operator_agent_id: &'a String,
    proposal_id: &'a ProposalId,
    profile: &'a ProductProfileV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "RecipeProfileGoverned")]
struct RecipeProfileGovernedPayload<'a> {
    operator_agent_id: &'a String,
    proposal_id: &'a ProposalId,
    profile: &'a RecipeProfileV1,
}

#[derive(serde::Serialize)]
#[serde(rename = "FactoryProfileGoverned")]
struct FactoryProfileGovernedPayload<'a> {
    operator_agent_id: &'a String,
    proposal_id: &'a ProposalId,
    profile: &'a FactoryProfileV1,
}

pub(super) fn serialize<S: serde::Serializer>(
    event: &DomainEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match event {
        DomainEvent::ProductValidated {
            requester_agent_id,
            module_id,
            stack,
            stack_limit,
            tradable,
            quality_levels,
            notes,
        } => serialize_adjacent(
            serializer,
            Tag::ProductValidated,
            &ProductValidatedPayload {
                requester_agent_id,
                module_id,
                stack,
                stack_limit,
                tradable,
                quality_levels,
                notes,
            },
        ),
        DomainEvent::ProductValidationRecorded { receipt } => serialize_adjacent(
            serializer,
            Tag::ProductValidationRecorded,
            &ProductValidationRecordedPayload { receipt },
        ),
        DomainEvent::ProductValidationAttemptStarted { attempt } => serialize_adjacent(
            serializer,
            Tag::ProductValidationAttemptStarted,
            &ProductValidationAttemptStartedPayload { attempt },
        ),
        DomainEvent::MaterialProfileGoverned {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_adjacent(
            serializer,
            Tag::MaterialProfileGoverned,
            &MaterialProfileGovernedPayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        DomainEvent::ProductProfileGoverned {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_adjacent(
            serializer,
            Tag::ProductProfileGoverned,
            &ProductProfileGovernedPayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        DomainEvent::RecipeProfileGoverned {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_adjacent(
            serializer,
            Tag::RecipeProfileGoverned,
            &RecipeProfileGovernedPayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        DomainEvent::FactoryProfileGoverned {
            operator_agent_id,
            proposal_id,
            profile,
        } => serialize_adjacent(
            serializer,
            Tag::FactoryProfileGoverned,
            &FactoryProfileGovernedPayload {
                operator_agent_id,
                proposal_id,
                profile,
            },
        ),
        _ => unreachable!("DomainEvent serializer group mismatch"),
    }
}
