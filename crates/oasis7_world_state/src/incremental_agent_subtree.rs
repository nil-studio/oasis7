//! Original AgentCell subtree consumers of the existing dedicated wire.
//! A parsed field is not a complete agent, history, World or permission receipt.
use crate::incremental_agent_fields::*;
#[path = "incremental_agent_subtree_read.rs"]
mod read;
pub use read::{
    AgentSubtreeScalarRequestV1, AgentSubtreeValueV1, read_agent_scalar_tree_v1,
    read_agent_subtree_scalar_raw_v1,
};

#[path = "incremental_agent_subtree_key.rs"]
mod key;
pub use key::{AgentSubtreeKeyV1, parse_agent_subtree_key_v1};
#[cfg(test)]
#[path = "incremental_agent_subtree_tests.rs"]
mod tests;

#[path = "incremental_agent_subtree_closure.rs"]
mod closure;
#[path = "incremental_agent_subtree_schema.rs"]
mod schema;
pub use closure::{
    AgentMailboxReferenceV1, AgentSubtreeScanWorkV1, AgentSubtreeStructureQualificationV1,
    qualify_agent_subtree_structure_v1,
};

#[path = "incremental_agent_subtree_point.rs"]
mod point;
pub use point::{
    AgentSubtreeCellChangeV1, AgentSubtreeQualifiedRequestV1, change_agent_subtree_scalar_v1,
    read_agent_subtree_scalar_overlay_v1,
};

#[path = "incremental_agent_subtree_receiver.rs"]
mod receiver;
pub use receiver::{AgentSubtreeTransitionWorkV1, accept_agent_subtree_transition_v1};

#[path = "incremental_agent_subtree_optional.rs"]
mod optional;
pub use optional::{
    AgentSubtreeOptionalChangeV1, AgentSubtreeOptionalRefV1, change_agent_subtree_optional_v1,
};

#[path = "incremental_agent_subtree_rows.rs"]
mod rows;
pub use rows::{
    AgentSubtreeRowChangeV1, AgentSubtreeRowExpectedV1, AgentSubtreeRowRefV1,
    change_agent_subtree_row_v1, verify_agent_subtree_row_expected_overlay_v1,
};

#[path = "incremental_agent_subtree_create.rs"]
mod create;
pub use create::{AgentSubtreeCreateV1, create_agent_subtree_v1};

#[path = "incremental_agent_subtree_delete.rs"]
mod delete;
pub use delete::{AgentSubtreeDeleteV1, delete_agent_subtree_v1};

#[path = "incremental_agent_subtree_mailbox.rs"]
mod mailbox;
pub use mailbox::{
    AgentSubtreeMailboxHeadV1, AgentSubtreeMailboxMutationV1, AgentSubtreeMailboxQueryV1,
    AgentSubtreeMailboxResultV1, change_agent_subtree_mailbox_reference_v1,
    read_agent_subtree_mailbox_head_overlay_v1,
};

#[path = "incremental_agent_subtree_pages.rs"]
mod pages;
pub use pages::{
    AgentSubtreePagesV1, AgentSubtreePagesWorkV1, mark_agent_subtree_pages_v1,
    transfer_agent_subtree_pages_v1,
};
