//! Stable, dependency-light contracts shared by Oasis7 clients.
//!
//! This package deliberately contains data contracts and pure helpers only. It
//! does not own a world, perform provider I/O, or depend on the server/runtime
//! crate. The first consumers can be migrated independently in later slices.

pub mod bootstrap;
pub mod config;
pub mod formal_projection;
pub mod provider;
pub mod signing;
pub mod world_service;
pub mod world_service_full_sync;

pub use bootstrap::{
    BootstrapConfig, ChainBootstrapConfig, ClientBootstrapConfig, DEFAULT_CHAIN_NETWORK_TIER,
    DEFAULT_CHAIN_REPLICATION_BOOTSTRAP_PEERS, DEFAULT_CHAIN_WORLD_ID,
    default_chain_replication_bootstrap_peers_csv, default_chain_replication_bootstrap_peers_vec,
};
pub use config::{
    AGENT_DECISION_SOURCE_BUILTIN_LLM, AGENT_DECISION_SOURCE_PROVIDER_BACKED,
    AGENT_EXECUTION_LANE_HEADLESS_AGENT, AGENT_EXECUTION_LANE_PLAYER_PARITY,
    AGENT_PROVIDER_BACKEND_LOCAL_BRIDGE, AGENT_PROVIDER_CONTRACT_WORLDSIM_V1,
    AGENT_PROVIDER_MODE_DIRECT_CONNECT_ALIAS, AGENT_PROVIDER_MODE_PROVIDER_LOOPBACK_HTTP_ALIAS,
    AGENT_PROVIDER_TRANSPORT_LOOPBACK_HTTP, AGENT_PROVIDER_TRANSPORT_REMOTE_HTTPS, ClientConfig,
    DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS, DEFAULT_AGENT_PROVIDER_PROFILE,
    DEFAULT_AGENT_PROVIDER_URL, ProviderConfig, canonical_decision_source,
    canonical_execution_lane, canonical_provider_backend, canonical_provider_contract,
    canonical_provider_transport,
};
pub use provider::{
    ProviderCompatibilityReport, ProviderCompatibilityStatus, ProviderError, ProviderHealth,
    ProviderInfo, ProviderProbe, evaluate_provider_compatibility, provider_phase1_required_actions,
    provider_phase1_required_capabilities,
};
pub use signing::{
    ClientSigningError, MAIN_TOKEN_ACTION_AUTH_PAYLOAD_VERSION,
    MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V1_PREFIX, MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V2_PREFIX,
    MainTokenActionAuthProof, MainTokenActionAuthScheme, MainTokenActionParticipantSignature,
    MainTokenTransfer, MainTokenTransferRequest, SignedMainTokenTransfer,
    SignedMainTokenTransferRequest, TRANSFER_TX_TYPE_ASSET_TRANSFER, TRANSFER_TX_VERSION_V2,
    VerifiedMainTokenActionAuth, build_main_token_transfer_signing_payload,
    build_signed_main_token_transfer_request, build_transfer_signing_payload,
    main_token_transfer_signing_payload, sign_main_token_transfer,
    sign_main_token_transfer_request, sign_transfer, verify_main_token_transfer_signature,
};

pub mod world_service_correlation;
pub mod world_service_wire;

pub mod world_service_feedback;

pub mod world_service_read;

pub mod world_read_observation;
