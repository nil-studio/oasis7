//! Isolated test ports. They exercise real Core transitions and actual SQL;
//! no snapshot, budget or authorization boolean is supplied by a fixture.
use super::*;
impl OwnerProvisionApprovalStore {
    pub(crate) fn drain_client_source_budget(&self, id: &str) -> Result<(u64, u64, u64)> {
        let mut reserved = 0;
        for index in 0..160 {
            let saved = self.snapshot()?;
            let session = saved.state.test_sessions().get(id).ok_or_else(rejected)?;
            let remaining = session
                .identity_transport
                .as_ref()
                .ok_or_else(rejected)?
                .remaining()
                .map_err(|_| rejected())?;
            if remaining == 0 {
                break;
            }
            let operation = format!("fixture-client-drain-{index}");
            let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
            self.local_reservation("fixture_client_source_reserved", id, |state, time| {
                state
                    .reserve_test_session_source(
                        id,
                        SessionSourceReservation {
                            operation_id: &operation,
                            nonce: &nonce,
                            funding: SessionSourceFundingV1::Control,
                        },
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            })?;
            reserved += 1;
        }
        let saved = self.snapshot()?;
        let session = saved.state.test_sessions().get(id).ok_or_else(rejected)?;
        let native = session
            .identity_transport
            .as_ref()
            .ok_or_else(rejected)?
            .remaining()
            .map_err(|_| rejected())?;
        let host = session
            .control
            .as_ref()
            .ok_or_else(rejected)?
            .transport
            .remaining()
            .map_err(|_| rejected())?;
        if native != 0 {
            return Err(rejected());
        }
        Ok((reserved, native, host))
    }
    pub(crate) async fn direct_admit_client_http(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
        purpose: SessionPurposeV1,
    ) -> Result<(
        Option<ProvisionOriginInspectionV1>,
        Option<TestSessionLiveDescriptorV1>,
    )> {
        let value = self.admit_client_http(env, rpc, operation, purpose).await?;
        Ok((value.source, value.descriptor))
    }
}
