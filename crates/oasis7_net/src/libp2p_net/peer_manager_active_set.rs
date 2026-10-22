use super::peer_record_cache::PeerRecordCache;
use std::collections::{BTreeSet, HashMap};

use libp2p::PeerId;
use oasis7_proto::distributed_dht::SignedPeerRecord;

use super::peer_discovery_observations::DiscoveryObservations;
use super::peer_manager::{
    PeerManagerHealthStatus, PeerManagerPolicy, exceeds_share_limit, ipv4_subnet_bucket,
    meets_or_exceeds_share_limit, relay_domain,
};
use super::transport_paths::{TransportPath, TransportPathKind};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ActivePeerSetStats {
    pub active_peer_count: usize,
    pub active_discovery_sources: BTreeSet<&'static str>,
    pub ipv4_subnet_counts: HashMap<String, usize>,
    pub relay_domain_counts: HashMap<String, usize>,
    pub operator_counts: HashMap<String, usize>,
    pub asn_counts: HashMap<String, usize>,
    pub relayed_active_peers: usize,
}

impl ActivePeerSetStats {
    pub(super) fn new(
        discovered_peer_records: &PeerRecordCache,
        observations: &DiscoveryObservations,
        active_transport_paths: &HashMap<PeerId, TransportPath>,
    ) -> Self {
        let mut stats = Self::default();

        for (peer_id, active_path) in active_transport_paths {
            let record = discovered_peer_records.get(peer_id);
            if let Some(record) = record {
                stats.note_record_discovery_sources(record, observations);
            }
            stats.add_unverified_peer(active_path);
        }

        stats
    }

    pub(super) fn add_admitted_peer(
        &mut self,
        record: &SignedPeerRecord,
        observations: &DiscoveryObservations,
        active_path: &TransportPath,
    ) {
        self.note_record_discovery_sources(record, observations);
        self.add_unverified_peer(active_path);
    }

    /// No local authorization currently permits a role-based budget exemption.
    /// Missing records and self-claimed observer roles therefore use the same unknown bucket.
    pub(super) fn add_unverified_peer(&mut self, active_path: &TransportPath) {
        self.active_peer_count = self.active_peer_count.saturating_add(1);
        self.note_active_path(active_path);
        self.note_unknown_control_domain();
    }

    fn note_record_discovery_sources(
        &mut self,
        record: &SignedPeerRecord,
        observations: &DiscoveryObservations,
    ) {
        self.active_discovery_sources
            .extend(observations.trusted_source_labels(record));
    }

    fn note_unknown_control_domain(&mut self) {
        *self
            .operator_counts
            .entry("unknown".to_string())
            .or_default() += 1;
        *self.asn_counts.entry("unknown".to_string()).or_default() += 1;
    }

    fn note_active_path(&mut self, active_path: &TransportPath) {
        if let Some(bucket) = ipv4_subnet_bucket(active_path) {
            *self.ipv4_subnet_counts.entry(bucket).or_default() += 1;
        }
        if matches!(active_path.kind, TransportPathKind::RelayReserved) {
            self.relayed_active_peers += 1;
            if let Some(domain) = relay_domain(active_path) {
                *self.relay_domain_counts.entry(domain).or_default() += 1;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActivePeerCandidate {
    pub discovery_source_labels: BTreeSet<&'static str>,
    pub discovery_independence_verified: bool,
    pub ipv4_subnet_bucket: Option<String>,
    pub relay_domain: Option<String>,
    pub source_operator: Option<String>,
    pub source_asn: Option<String>,
    pub relay_reserved: bool,
    pub counts_toward_share_limits: bool,
}

impl ActivePeerCandidate {
    pub(super) fn from_record_and_path(
        record: &SignedPeerRecord,
        observations: &DiscoveryObservations,
        active_path: &TransportPath,
    ) -> Self {
        Self {
            discovery_source_labels: observations.trusted_source_labels(record),
            discovery_independence_verified: observations.is_static_bootstrap(record),
            ipv4_subnet_bucket: ipv4_subnet_bucket(active_path),
            relay_domain: relay_domain(active_path),
            source_operator: Some("unknown".to_string()),
            source_asn: Some("unknown".to_string()),
            relay_reserved: matches!(active_path.kind, TransportPathKind::RelayReserved),
            counts_toward_share_limits: true,
        }
    }

    pub(super) fn from_unverified_path(active_path: &TransportPath) -> Self {
        Self {
            discovery_source_labels: BTreeSet::new(),
            discovery_independence_verified: false,
            ipv4_subnet_bucket: ipv4_subnet_bucket(active_path),
            relay_domain: relay_domain(active_path),
            source_operator: Some("unknown".to_string()),
            source_asn: Some("unknown".to_string()),
            relay_reserved: matches!(active_path.kind, TransportPathKind::RelayReserved),
            counts_toward_share_limits: true,
        }
    }
}

#[cfg(test)]
pub(super) fn candidate_status_with_active_set(
    candidate: &ActivePeerCandidate,
    active_set_stats: &ActivePeerSetStats,
    policy: &PeerManagerPolicy,
) -> PeerManagerHealthStatus {
    candidate_status(candidate, active_set_stats, policy, true)
}

/// Already established sessions may acquire bootstrap material while discovery remains degraded.
/// This never exempts the session from topology and unknown-control-domain budgets.
pub(super) fn candidate_transport_admission_status(
    candidate: &ActivePeerCandidate,
    active_set_stats: &ActivePeerSetStats,
    policy: &PeerManagerPolicy,
) -> PeerManagerHealthStatus {
    candidate_status(candidate, active_set_stats, policy, false)
}

fn candidate_status(
    candidate: &ActivePeerCandidate,
    active_set_stats: &ActivePeerSetStats,
    policy: &PeerManagerPolicy,
    require_discovery: bool,
) -> PeerManagerHealthStatus {
    let projected_active_peer_count = if candidate.counts_toward_share_limits {
        active_set_stats.active_peer_count.saturating_add(1)
    } else {
        active_set_stats.active_peer_count
    };
    let mut has_issue = require_discovery
        && (!candidate.discovery_independence_verified
            || candidate.discovery_source_labels.len() < policy.min_peer_discovery_sources);
    let mut hard_block = false;

    let projected_active_discovery_sources = active_set_stats
        .active_discovery_sources
        .union(&candidate.discovery_source_labels)
        .count();
    if require_discovery
        && projected_active_peer_count > 0
        && projected_active_discovery_sources < policy.min_active_discovery_sources
    {
        has_issue = true;
    }

    if candidate.counts_toward_share_limits
        && let Some(bucket) = candidate.ipv4_subnet_bucket.as_deref()
    {
        let projected_bucket_count = active_set_stats
            .ipv4_subnet_counts
            .get(bucket)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        if let Some(limit) = policy.max_ipv4_subnet_active_peers {
            if projected_bucket_count > limit {
                hard_block = true;
            }
        } else if policy.applies_share_limits(projected_active_peer_count)
            && projected_bucket_count >= 2
            && meets_or_exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.block_ipv4_subnet_share_per_mille,
            )
        {
            hard_block = true;
        } else if policy.applies_share_limits(projected_active_peer_count)
            && projected_bucket_count >= 2
            && exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.max_ipv4_subnet_share_per_mille,
            )
        {
            has_issue = true;
        }
    }

    if candidate.counts_toward_share_limits && candidate.relay_reserved {
        let projected_relayed_active_peers =
            active_set_stats.relayed_active_peers.saturating_add(1);
        if exceeds_share_limit(
            projected_relayed_active_peers,
            projected_active_peer_count,
            policy.max_relayed_active_peer_share_per_mille,
        ) {
            has_issue = true;
        }
        if let Some(domain) = candidate.relay_domain.as_deref() {
            let projected_bucket_count = active_set_stats
                .relay_domain_counts
                .get(domain)
                .copied()
                .unwrap_or(0)
                .saturating_add(1);
            if policy.applies_share_limits(projected_active_peer_count)
                && projected_bucket_count >= 2
                && meets_or_exceeds_share_limit(
                    projected_bucket_count,
                    projected_active_peer_count,
                    policy.block_relay_domain_share_per_mille,
                )
            {
                hard_block = true;
            } else if policy.applies_share_limits(projected_active_peer_count)
                && projected_bucket_count >= 2
                && exceeds_share_limit(
                    projected_bucket_count,
                    projected_active_peer_count,
                    policy.max_relay_domain_share_per_mille,
                )
            {
                has_issue = true;
            }
        }
    }

    if candidate.counts_toward_share_limits
        && let Some(source_operator) = candidate.source_operator.as_deref()
    {
        let projected_bucket_count = active_set_stats
            .operator_counts
            .get(source_operator)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        if policy.applies_share_limits(projected_active_peer_count)
            && projected_bucket_count >= 2
            && meets_or_exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.block_operator_share_per_mille,
            )
        {
            hard_block = true;
        } else if policy.applies_share_limits(projected_active_peer_count)
            && projected_bucket_count >= 2
            && exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.max_operator_share_per_mille,
            )
        {
            has_issue = true;
        }
    }

    if candidate.counts_toward_share_limits
        && let Some(source_asn) = candidate.source_asn.as_deref()
    {
        let projected_bucket_count = active_set_stats
            .asn_counts
            .get(source_asn)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        if policy.applies_share_limits(projected_active_peer_count)
            && projected_bucket_count >= 2
            && meets_or_exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.block_asn_share_per_mille,
            )
        {
            hard_block = true;
        } else if policy.applies_share_limits(projected_active_peer_count)
            && projected_bucket_count >= 2
            && exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.max_asn_share_per_mille,
            )
        {
            has_issue = true;
        }
    }

    if hard_block {
        PeerManagerHealthStatus::Blocked
    } else if has_issue {
        PeerManagerHealthStatus::Suspect
    } else {
        PeerManagerHealthStatus::Active
    }
}

pub(super) fn candidate_would_degrade_admitted_peers(
    candidate: &ActivePeerCandidate,
    active_set_stats: &ActivePeerSetStats,
    policy: &PeerManagerPolicy,
) -> bool {
    if !candidate.counts_toward_share_limits {
        return false;
    }
    let projected_active_peer_count = active_set_stats.active_peer_count.saturating_add(1);

    if let Some(bucket) = candidate.ipv4_subnet_bucket.as_deref() {
        let current_bucket_count = active_set_stats
            .ipv4_subnet_counts
            .get(bucket)
            .copied()
            .unwrap_or(0);
        let projected_bucket_count = current_bucket_count.saturating_add(1);
        if let Some(limit) = policy.max_ipv4_subnet_active_peers {
            if current_bucket_count > 0 && projected_bucket_count > limit {
                return true;
            }
        } else if policy.applies_share_limits(projected_active_peer_count)
            && current_bucket_count > 0
            && projected_bucket_count >= 2
            && (meets_or_exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.block_ipv4_subnet_share_per_mille,
            ) || exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.max_ipv4_subnet_share_per_mille,
            ))
        {
            return true;
        }
    }

    if candidate.relay_reserved {
        let projected_relayed_active_peers =
            active_set_stats.relayed_active_peers.saturating_add(1);
        if active_set_stats.relayed_active_peers > 0
            && exceeds_share_limit(
                projected_relayed_active_peers,
                projected_active_peer_count,
                policy.max_relayed_active_peer_share_per_mille,
            )
        {
            return true;
        }
        if let Some(domain) = candidate.relay_domain.as_deref() {
            let current_bucket_count = active_set_stats
                .relay_domain_counts
                .get(domain)
                .copied()
                .unwrap_or(0);
            let projected_bucket_count = current_bucket_count.saturating_add(1);
            if policy.applies_share_limits(projected_active_peer_count)
                && current_bucket_count > 0
                && projected_bucket_count >= 2
                && (meets_or_exceeds_share_limit(
                    projected_bucket_count,
                    projected_active_peer_count,
                    policy.block_relay_domain_share_per_mille,
                ) || exceeds_share_limit(
                    projected_bucket_count,
                    projected_active_peer_count,
                    policy.max_relay_domain_share_per_mille,
                ))
            {
                return true;
            }
        }
    }

    if let Some(source_operator) = candidate.source_operator.as_deref() {
        let current_bucket_count = active_set_stats
            .operator_counts
            .get(source_operator)
            .copied()
            .unwrap_or(0);
        let projected_bucket_count = current_bucket_count.saturating_add(1);
        if policy.applies_share_limits(projected_active_peer_count)
            && current_bucket_count > 0
            && projected_bucket_count >= 2
            && (meets_or_exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.block_operator_share_per_mille,
            ) || exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.max_operator_share_per_mille,
            ))
        {
            return true;
        }
    }

    if let Some(source_asn) = candidate.source_asn.as_deref() {
        let current_bucket_count = active_set_stats
            .asn_counts
            .get(source_asn)
            .copied()
            .unwrap_or(0);
        let projected_bucket_count = current_bucket_count.saturating_add(1);
        if policy.applies_share_limits(projected_active_peer_count)
            && current_bucket_count > 0
            && projected_bucket_count >= 2
            && (meets_or_exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.block_asn_share_per_mille,
            ) || exceeds_share_limit(
                projected_bucket_count,
                projected_active_peer_count,
                policy.max_asn_share_per_mille,
            ))
        {
            return true;
        }
    }

    false
}
