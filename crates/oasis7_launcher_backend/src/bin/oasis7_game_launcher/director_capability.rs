use super::hosted_access::DeploymentMode;
use super::hosted_player_session::{
    HostedPlayerSessionAdmissionSnapshot, HostedPlayerSessionIssuer,
};
use oasis7_client_api::viewer_protocol::DirectorCapabilityGrant;
use serde::Serialize;

/// Dedicated capability status endpoint. A real issuer is intentionally not wired until the
/// operator approval plane can bind the grant to the live runtime session epoch.
pub(super) const DIRECTOR_CAPABILITY_ROUTE: &str = "/api/public/director/capability";

#[derive(Debug, Clone, Serialize)]
pub(super) struct DirectorCapabilityResponse {
    pub(super) ok: bool,
    pub(super) error_code: String,
    pub(super) error: String,
    pub(super) deployment_mode: String,
    pub(super) availability: String,
    pub(super) authority: String,
    pub(super) admission: HostedPlayerSessionAdmissionSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) grant: Option<DirectorCapabilityGrant>,
}

pub(super) fn director_capability_unavailable(
    deployment_mode: DeploymentMode,
    issuer: &mut HostedPlayerSessionIssuer,
) -> DirectorCapabilityResponse {
    DirectorCapabilityResponse {
        ok: false,
        error_code: "director_capability_unavailable".to_string(),
        error: "no trusted server authority can bind a Director grant to the current session_epoch"
            .to_string(),
        deployment_mode: deployment_mode.as_str().to_string(),
        availability: "unavailable".to_string(),
        authority: "server_session_epoch_operator_approval".to_string(),
        admission: issuer.admission(deployment_mode).admission,
        grant: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn director_capability_status_fails_closed_without_grant() {
        let mut issuer = HostedPlayerSessionIssuer::default();
        let response =
            director_capability_unavailable(DeploymentMode::HostedPublicJoin, &mut issuer);
        assert!(!response.ok);
        assert_eq!(response.error_code, "director_capability_unavailable");
        assert_eq!(response.availability, "unavailable");
        assert_eq!(response.authority, "server_session_epoch_operator_approval");
        assert!(response.grant.is_none());
    }
}
