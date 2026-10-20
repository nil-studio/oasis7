use oasis7_node::NodeRole;

pub(super) fn node_role_requires_execution_commit(role: NodeRole) -> bool {
    matches!(role, NodeRole::Sequencer)
}

pub(super) fn node_role_materializes_execution_state(role: NodeRole) -> bool {
    matches!(
        role,
        NodeRole::Sequencer | NodeRole::Storage | NodeRole::Observer
    )
}
