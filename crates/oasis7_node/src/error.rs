use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeError {
    InvalidRole {
        role: String,
    },
    InvalidConfig {
        reason: String,
    },
    Consensus {
        reason: String,
    },
    Gossip {
        reason: String,
    },
    ReplicationNetwork {
        reason: String,
        failure: Box<oasis7_proto::network_failure::NetworkFailure>,
    },
    Replication {
        reason: String,
    },
    ExecutionMismatchRollbackUnavailable {
        payload_height: u64,
        rollback_height: u64,
        reason: String,
    },
    Execution {
        reason: String,
    },
    AlreadyRunning {
        node_id: String,
    },
    NotRunning {
        node_id: String,
    },
    ThreadSpawnFailed {
        reason: String,
    },
    ThreadJoinFailed {
        node_id: String,
    },
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeError::InvalidRole { role } => {
                write!(f, "invalid node role: {}", role)
            }
            NodeError::InvalidConfig { reason } => write!(f, "invalid node config: {}", reason),
            NodeError::Consensus { reason } => write!(f, "node consensus error: {}", reason),
            NodeError::Gossip { reason } => write!(f, "node gossip error: {}", reason),
            NodeError::Replication { reason } | NodeError::ReplicationNetwork { reason, .. } => {
                write!(f, "node replication error: {}", reason)
            }
            NodeError::ExecutionMismatchRollbackUnavailable {
                payload_height,
                rollback_height,
                reason,
            } => write!(
                f,
                "node replication error: synced replication height {} execution hash validation failed: {}; rollback record for height {} is unavailable",
                payload_height, reason, rollback_height
            ),
            NodeError::Execution { reason } => write!(f, "node execution error: {}", reason),
            NodeError::AlreadyRunning { node_id } => {
                write!(f, "node runtime already running: {}", node_id)
            }
            NodeError::NotRunning { node_id } => write!(f, "node runtime not running: {}", node_id),
            NodeError::ThreadSpawnFailed { reason } => {
                write!(f, "failed to spawn node thread: {}", reason)
            }
            NodeError::ThreadJoinFailed { node_id } => {
                write!(f, "failed to join node thread: {}", node_id)
            }
        }
    }
}

impl std::error::Error for NodeError {}

impl NodeError {
    pub(crate) fn replication_reason(&self) -> Option<&str> {
        match self {
            Self::Replication { reason } | Self::ReplicationNetwork { reason, .. } => Some(reason),
            _ => None,
        }
    }

    pub(crate) fn is_fresh_observer_checkpoint_fallback_eligible(&self) -> bool {
        matches!(self, Self::ExecutionMismatchRollbackUnavailable { .. })
    }
}
