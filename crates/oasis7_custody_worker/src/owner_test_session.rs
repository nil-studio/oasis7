//! Fixed TestSession management and role-specific private preflight ports.
use super::*;
use crate::{owner_test_session_rpc::*, provision_approval_store::native::test_session::*};
use oasis7_custody_core::{approval_admission::ApprovalAdmissionInput, provisioning::*};
#[path = "owner_test_session_approval.rs"]
mod session_approval;
#[path = "owner_test_session_authority.rs"]
mod session_authority;
#[path = "owner_test_session_client.rs"]
mod session_client;
#[path = "owner_test_session_dispatch.rs"]
mod session_dispatch;
pub(crate) use session_approval::{approve, basis, continue_session, invite, prepare};
pub(crate) use session_client::dispatch as dispatch_client;
pub(crate) use session_dispatch::dispatch;

#[path = "owner_test_session_v2_approval.rs"]
mod session_approval_v2;
pub(crate) use session_approval_v2::{approve_v2, basis_v2, continue_v2, prepare_v2};

#[path = "owner_test_session_v2_dispatch.rs"]
mod session_dispatch_v2;
pub(crate) use session_dispatch_v2::dispatch as dispatch_v2;

#[path = "owner_test_session_v2_credentials.rs"]
mod credentials_v2;
pub(crate) use credentials_v2::dispatch as dispatch_credentials_v2;

#[path = "owner_test_session_v2_controls.rs"]
mod controls_v2;
pub(crate) use controls_v2::dispatch as dispatch_controls_v2;

#[path = "owner_test_session_v2_client_router.rs"]
mod client_router_v2;
pub(crate) use client_router_v2::first_admit as first_client_v2_admit;

pub(crate) use session_authority::{ExecutionPrepareObservationV2, execution_prepare_observe_v2};

#[path = "owner_native_model_reservation_v2.rs"]
mod model_reservation_v2;
pub(crate) use model_reservation_v2::admit as dispatch_native_model_reservation_v2;
