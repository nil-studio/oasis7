use std::collections::VecDeque;
use std::env;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufRead, BufReader};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::TryRecvError;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;
#[cfg(not(target_arch = "wasm32"))]
use feedback_entry::FeedbackDraft;
#[cfg(target_arch = "wasm32")]
use gloo_net::http::Request;
use llm_settings::LlmSettingsPanel;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_client_api::AGENT_PROVIDER_MODE_DIRECT_CONNECT_ALIAS as AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS;
use oasis7_client_api::{
    AGENT_DECISION_SOURCE_BUILTIN_LLM as DEFAULT_AGENT_DECISION_SOURCE,
    AGENT_EXECUTION_LANE_PLAYER_PARITY as DEFAULT_AGENT_EXECUTION_LANE,
    AGENT_PROVIDER_BACKEND_LOCAL_BRIDGE as DEFAULT_AGENT_PROVIDER_BACKEND,
    AGENT_PROVIDER_CONTRACT_WORLDSIM_V1 as DEFAULT_AGENT_PROVIDER_CONTRACT,
    AGENT_PROVIDER_TRANSPORT_LOOPBACK_HTTP as DEFAULT_AGENT_PROVIDER_TRANSPORT,
    DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS, DEFAULT_AGENT_PROVIDER_PROFILE,
    DEFAULT_AGENT_PROVIDER_URL, DEFAULT_CHAIN_NETWORK_TIER,
    default_chain_replication_bootstrap_peers_csv,
};
use platform_ops::open_browser;
use platform_ops::resolve_static_dir_path;
#[cfg(not(target_arch = "wasm32"))]
use platform_ops::{resolve_chain_runtime_binary_path, resolve_launcher_binary_path};
use serde::{Deserialize, Serialize};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(not(target_arch = "wasm32"))]
use transfer_entry::TransferDraft;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;
#[cfg(target_arch = "wasm32")]
use web_sys::HtmlCanvasElement;
#[cfg(target_arch = "wasm32")]
use web_sys::wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{Document, HtmlElement};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(not(target_arch = "wasm32"))]
mod app_process;
#[cfg(target_arch = "wasm32")]
mod app_process_web;
mod chain_pos_defaults;
mod config_ui;
mod explorer_window;
#[cfg(not(target_arch = "wasm32"))]
mod feedback_entry;
#[cfg(not(target_arch = "wasm32"))]
mod feedback_privacy;
#[cfg(not(target_arch = "wasm32"))]
mod feedback_window;
#[cfg(target_arch = "wasm32")]
mod feedback_window_web;
mod http_helpers;
mod launcher_core;
#[cfg(target_arch = "wasm32")]
mod launcher_test_hook_web;
#[cfg(not(target_arch = "wasm32"))]
mod llm_settings;
#[cfg(target_arch = "wasm32")]
#[path = "llm_settings_web.rs"]
mod llm_settings;
mod main_app_shell;
mod main_app_shell_health;
#[cfg(not(target_arch = "wasm32"))]
mod main_app_shell_screenshot_preview;
mod main_chain_status;
#[cfg(not(target_arch = "wasm32"))]
mod main_screenshot_override;
mod main_ui_helpers;
mod peer_details_window;
mod platform_ops;
mod provider_check_status;
mod self_guided;
mod self_guided_blocked_actions;
mod self_guided_error_cards;
mod self_guided_onboarding_reminder;
mod self_guided_onboarding_steps;
mod self_guided_preflight;
mod transfer_auth;
#[cfg(not(target_arch = "wasm32"))]
mod transfer_entry;
mod transfer_window;
mod web_api_support;

use config_ui::StartupGuideState;
use launcher_core::*;
use main_chain_status::*;
pub(crate) use provider_check_status::ProviderCheckStatus;
#[cfg(any(not(target_arch = "wasm32"), test))]
pub(crate) use provider_check_status::ProviderCompatibilityStatus;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use provider_check_status::ProviderSnapshot;
use self_guided::{
    DemoModePhase, LauncherUxState, NextTaskHint, OnboardingState, resolve_next_task_hint,
};
pub(crate) use web_api_support::{
    WebApiEvent, WebApiResponse, WebChainNodeObservabilityStatus, WebChainP2pStatus,
    WebChainRecoverySnapshot, WebRequestDomain, WebRequestInflight, WebStateSnapshot,
    WebTransferSubmitRequest, WebTransferSubmitResponse, encoded_query_pair,
};
#[cfg(test)]
pub(crate) use web_api_support::{WebChainNodeObservabilityAlert, encode_query_value};
#[cfg(target_arch = "wasm32")]
pub(crate) use web_api_support::{WebFeedbackSubmitRequest, WebFeedbackSubmitResponse};

const DEFAULT_SCENARIO: &str = "";
const DEFAULT_LIVE_BIND: &str = "127.0.0.1:5023";
const DEFAULT_WEB_BIND: &str = "127.0.0.1:5011";
const DEFAULT_VIEWER_HOST: &str = "127.0.0.1";
const DEFAULT_VIEWER_PORT: &str = "4173";
const DEFAULT_CHAIN_STATUS_BIND: &str = "127.0.0.1:5121";
const DEFAULT_CHAIN_NODE_ID: &str = "viewer-live-node";
const DEFAULT_CHAIN_NODE_ROLE: &str = "sequencer";
const DEFAULT_CHAIN_P2P_USER_MODE: &str = "auto_join";
const PUBLIC_TESTNET_NETWORK_TIER_MANIFEST: &str =
    "doc/testing/templates/network-tier-public-testnet.example.json";
const MAINNET_NETWORK_TIER_MANIFEST: &str =
    "doc/testing/templates/network-tier-mainnet.example.json";
const DEFAULT_CHAIN_NODE_TICK_MS: &str = "200";
const DEFAULT_CHAIN_POS_SLOT_CLOCK_GENESIS_UNIX_MS: &str = "";
const DEFAULT_DEPLOYMENT_MODE: &str = "hosted_public_join";
const MAX_LOG_LINES: usize = 2000;
const OASIS7_CJK_FONT_NAME: &str = "oasis7-cjk";
const EGUI_CJK_FONT_BYTES: &[u8] = include_bytes!("../../../third_party/ms-yahei.ttf");
const OASIS7_CLIENT_LAUNCHER_FONT_ENV: &str = "OASIS7_CLIENT_LAUNCHER_FONT";
const OASIS7_CLIENT_LAUNCHER_LANG_ENV: &str = "OASIS7_CLIENT_LAUNCHER_LANG";
#[cfg(not(target_arch = "wasm32"))]
const OASIS7_CLIENT_LAUNCHER_WINDOW_SIZE_ENV: &str = "OASIS7_CLIENT_LAUNCHER_WINDOW_SIZE";
#[cfg(not(target_arch = "wasm32"))]
const OASIS7_CLIENT_LAUNCHER_SCREENSHOT_MODAL_ENV: &str = "OASIS7_CLIENT_LAUNCHER_SCREENSHOT_MODAL";
#[cfg(not(target_arch = "wasm32"))]
const GRACEFUL_STOP_TIMEOUT_MS: u64 = 4000;
#[cfg(not(target_arch = "wasm32"))]
const STOP_POLL_INTERVAL_MS: u64 = 80;
#[cfg(all(not(target_arch = "wasm32"), test))]
const CHAIN_STATUS_PROBE_TIMEOUT_MS: u64 = 300;
#[cfg(not(target_arch = "wasm32"))]
const OASIS7_CLIENT_LAUNCHER_CONTROL_URL_ENV: &str = "OASIS7_CLIENT_LAUNCHER_CONTROL_URL";
#[cfg(not(target_arch = "wasm32"))]
const OASIS7_CLIENT_LAUNCHER_CONTROL_BIND_ENV: &str = "OASIS7_CLIENT_LAUNCHER_CONTROL_BIND";
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_CLIENT_LAUNCHER_CONTROL_BIND: &str = "127.0.0.1:5410";
const NATIVE_UI_SECTIONS: &[&str] = &[
    "game_core",
    "viewer_core",
    "agent_provider",
    "chain_identity",
    "chain_runtime",
    "binaries",
    "static_assets",
];

#[cfg(target_arch = "wasm32")]
const WEB_CANVAS_ID: &str = "oasis7-launcher-canvas";
const WEB_POLL_INTERVAL_MS: u64 = 1000;

fn default_chain_node_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    #[cfg(not(target_arch = "wasm32"))]
    {
        format!("{DEFAULT_CHAIN_NODE_ID}-fresh-{}-{now}", std::process::id())
    }

    #[cfg(target_arch = "wasm32")]
    {
        format!("{DEFAULT_CHAIN_NODE_ID}-fresh-web-{now}")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(default_native_window_size()),
        ..Default::default()
    };

    eframe::run_native(
        "oasis7 Client Launcher",
        native_options,
        Box::new(|cc| {
            configure_egui_fonts(&cc.egui_ctx);
            configure_egui_visuals(&cc.egui_ctx);
            Ok(Box::<ClientLauncherApp>::default())
        }),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn default_native_window_size() -> egui::Vec2 {
    env::var(OASIS7_CLIENT_LAUNCHER_WINDOW_SIZE_ENV)
        .ok()
        .and_then(|raw| parse_window_size(raw.as_str()))
        .unwrap_or_else(|| egui::vec2(920.0, 680.0))
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_window_size(raw: &str) -> Option<egui::Vec2> {
    let normalized = raw.replace(['X', ','], "x");
    let (width, height) = normalized.split_once('x')?;
    let width = width.trim().parse::<f32>().ok()?;
    let height = height.trim().parse::<f32>().ok()?;
    if width >= 640.0 && height >= 520.0 {
        Some(egui::vec2(width, height))
    } else {
        None
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    let web_options = eframe::WebOptions {
        webgl_context_option: eframe::WebGlContextOption::WebGl1,
        ..Default::default()
    };
    let document = web_sys::window().and_then(|window| window.document());
    let canvas = document
        .as_ref()
        .and_then(|document| document.get_element_by_id(WEB_CANVAS_ID))
        .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
        .unwrap_or_else(|| panic!("missing launcher canvas: #{WEB_CANVAS_ID}"));
    spawn_local(async move {
        let runner = eframe::WebRunner::new();
        let start_result = runner
            .start(
                canvas,
                web_options,
                Box::new(|cc| {
                    configure_egui_fonts(&cc.egui_ctx);
                    configure_egui_visuals(&cc.egui_ctx);
                    Ok(Box::<ClientLauncherApp>::default())
                }),
            )
            .await;
        if let Err(err) = start_result {
            render_web_boot_failure(
                document.as_ref(),
                format!("failed to start launcher web app: {err:?}").as_str(),
            );
            eprintln!("failed to start launcher web app: {err:?}");
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn render_web_boot_failure(document: Option<&Document>, message: &str) {
    let Some(document) = document else {
        return;
    };
    let Some(body) = document.body() else {
        return;
    };

    let panel = if let Some(existing) = document.get_element_by_id("oasis7-launcher-web-error") {
        existing
    } else {
        let Ok(panel) = document.create_element("pre") else {
            return;
        };
        panel.set_id("oasis7-launcher-web-error");
        let _ = panel.set_attribute(
            "style",
            "position:fixed;left:16px;right:16px;top:16px;z-index:9999;white-space:pre-wrap;\
             background:#2b1f1f;color:#ffe9e9;border:1px solid #c96f6f;padding:12px;\
             font:12px/1.5 monospace;max-height:40vh;overflow:auto;",
        );
        let _ = body.append_child(&panel);
        panel
    };

    if let Some(panel) = panel.dyn_ref::<HtmlElement>() {
        panel.set_inner_text(message);
    }
}

fn configure_egui_fonts(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    match load_font_override_from_env() {
        Some((font_name, font_data)) => install_cjk_font(&mut fonts, font_name, font_data),
        None => install_cjk_font(
            &mut fonts,
            OASIS7_CJK_FONT_NAME.to_string(),
            egui::FontData::from_static(EGUI_CJK_FONT_BYTES),
        ),
    }
    context.set_fonts(fonts);
}

fn configure_egui_visuals(context: &egui::Context) {
    let mut style = (*context.global_style()).clone();
    style.visuals = egui::Visuals::light();
    style.spacing.item_spacing = egui::vec2(7.0, 5.0);
    style.spacing.button_padding = egui::vec2(8.0, 5.0);
    style.spacing.window_margin = egui::Margin::same(12);
    style.visuals.panel_fill = egui::Color32::from_rgb(247, 249, 251);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(241, 245, 249);
    style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(255, 255, 255);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(249, 251, 253);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(239, 246, 255);
    let soft_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(224, 229, 236));
    style.visuals.widgets.noninteractive.bg_stroke = soft_stroke;
    style.visuals.widgets.inactive.bg_stroke = soft_stroke;
    style.visuals.widgets.hovered.bg_stroke = soft_stroke;
    style.visuals.window_stroke = soft_stroke;
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(76, 139, 111);
    context.set_global_style(style);
}

fn load_font_override_from_env() -> Option<(String, egui::FontData)> {
    let (env_name, path) = read_named_env_value(&[OASIS7_CLIENT_LAUNCHER_FONT_ENV])?;

    match std::fs::read(path.as_str()) {
        Ok(bytes) => Some((
            format!("{OASIS7_CJK_FONT_NAME}-custom"),
            egui::FontData::from_owned(bytes),
        )),
        Err(err) => {
            eprintln!(
                "warning: failed to read font from {env_name}={path}: {err}; fallback to embedded CJK font"
            );
            None
        }
    }
}

fn install_cjk_font(
    fonts: &mut egui::FontDefinitions,
    font_name: String,
    font_data: egui::FontData,
) {
    fonts
        .font_data
        .insert(font_name.clone(), Arc::new(font_data));

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, font_name.clone());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push(font_name);
}

fn read_named_env_value_with<F>(
    lookup: &F,
    env_names: &[&'static str],
) -> Option<(&'static str, String)>
where
    F: Fn(&str) -> Option<String>,
{
    for env_name in env_names {
        let Some(raw) = lookup(env_name) else {
            continue;
        };
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Some((env_name, trimmed.to_string()));
        }
    }
    None
}

fn read_named_env_value(env_names: &[&'static str]) -> Option<(&'static str, String)> {
    read_named_env_value_with(&|env_name| env::var(env_name).ok(), env_names)
}

fn should_request_auto_chain_start(
    chain_auto_start_attempted: bool,
    chain_enabled: bool,
    control_action_inflight: bool,
    control_plane_snapshot_received: bool,
) -> bool {
    !chain_auto_start_attempted
        && chain_enabled
        && !control_action_inflight
        && control_plane_snapshot_received
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_control_plane_env_with<F>(lookup: &F) -> (Option<String>, String, String, bool)
where
    F: Fn(&str) -> Option<String>,
{
    let control_url_from_env =
        read_named_env_value_with(lookup, &[OASIS7_CLIENT_LAUNCHER_CONTROL_URL_ENV])
            .map(|(_, value)| value);
    let control_listen_bind =
        read_named_env_value_with(lookup, &[OASIS7_CLIENT_LAUNCHER_CONTROL_BIND_ENV])
            .map(|(_, value)| value)
            .unwrap_or_else(|| DEFAULT_CLIENT_LAUNCHER_CONTROL_BIND.to_string());
    let control_api_base = control_url_from_env.clone().unwrap_or_else(|| {
        let (host, port) = parse_host_port(
            control_listen_bind.as_str(),
            OASIS7_CLIENT_LAUNCHER_CONTROL_BIND_ENV,
        )
        .unwrap_or(("127.0.0.1".to_string(), 5410));
        let host = normalize_host_for_url(host.as_str());
        let host = host_for_url(host.as_str());
        format!("http://{host}:{port}")
    });
    let control_manage_service = control_url_from_env.is_none();
    (
        control_url_from_env,
        control_listen_bind,
        control_api_base,
        control_manage_service,
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_control_plane_env() -> (Option<String>, String, String, bool) {
    resolve_control_plane_env_with(&|env_name| env::var(env_name).ok())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiLanguage {
    ZhCn,
    EnUs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlossaryTerm {
    Nonce,
    Slot,
    Mempool,
    ActionId,
}

impl UiLanguage {
    fn from_tag(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh_hans" | "zh-hans" | "cn" => Some(Self::ZhCn),
            "en" | "en-us" | "en_us" | "english" => Some(Self::EnUs),
            _ => None,
        }
    }

    fn detect_from_values(launcher_lang: Option<&str>, process_lang: Option<&str>) -> Self {
        launcher_lang
            .and_then(Self::from_tag)
            .or_else(|| process_lang.and_then(Self::from_tag))
            .unwrap_or(Self::ZhCn)
    }

    fn detect_from_env() -> Self {
        let launcher_lang =
            read_named_env_value(&[OASIS7_CLIENT_LAUNCHER_LANG_ENV]).map(|(_, raw)| raw);
        let process_lang = env::var("LANG").ok();
        Self::detect_from_values(launcher_lang.as_deref(), process_lang.as_deref())
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::ZhCn => "中文",
            Self::EnUs => "English",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct LaunchConfig {
    deployment_mode: String,
    scenario: String,
    live_bind: String,
    web_bind: String,
    viewer_host: String,
    viewer_port: String,
    viewer_static_dir: String,
    #[serde(alias = "agent_provider_mode")]
    agent_decision_source: String,
    #[serde(default = "default_agent_provider_backend")]
    agent_provider_backend: String,
    #[serde(default = "default_agent_provider_contract")]
    agent_provider_contract: String,
    #[serde(default = "default_agent_provider_transport")]
    agent_provider_transport: String,
    agent_provider_url: String,
    agent_provider_auth_token: String,
    agent_provider_connect_timeout_ms: String,
    #[serde(default = "default_agent_execution_lane")]
    agent_execution_lane: String,
    agent_provider_profile: String,
    llm_enabled: bool,
    provider_auto_discover: bool,
    chain_enabled: bool,
    chain_status_bind: String,
    chain_node_id: String,
    chain_network_tier: String,
    chain_network_tier_manifest: String,
    chain_world_id: String,
    chain_node_role: String,
    chain_p2p_user_mode: String,
    chain_p2p_accept_public_entry: bool,
    chain_replication_bootstrap_peers: String,
    chain_node_tick_ms: String,
    chain_pos_slot_duration_ms: String,
    chain_pos_ticks_per_slot: String,
    chain_pos_proposal_tick_phase: String,
    chain_pos_adaptive_tick_scheduler_enabled: bool,
    chain_pos_slot_clock_genesis_unix_ms: String,
    chain_pos_max_past_slot_lag: String,
    chain_node_validators: String,
    auto_open_browser: bool,
    launcher_bin: String,
    chain_runtime_bin: String,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        let pos_defaults = chain_pos_defaults::defaults();
        #[cfg(not(target_arch = "wasm32"))]
        let launcher_bin = resolve_launcher_binary_path().to_string_lossy().to_string();
        #[cfg(target_arch = "wasm32")]
        let launcher_bin = String::new();
        #[cfg(not(target_arch = "wasm32"))]
        let chain_runtime_bin = resolve_chain_runtime_binary_path()
            .to_string_lossy()
            .to_string();
        #[cfg(target_arch = "wasm32")]
        let chain_runtime_bin = String::new();
        let viewer_static_dir = resolve_static_dir_path().to_string_lossy().to_string();

        Self {
            deployment_mode: DEFAULT_DEPLOYMENT_MODE.to_string(),
            scenario: DEFAULT_SCENARIO.to_string(),
            live_bind: DEFAULT_LIVE_BIND.to_string(),
            web_bind: DEFAULT_WEB_BIND.to_string(),
            viewer_host: DEFAULT_VIEWER_HOST.to_string(),
            viewer_port: DEFAULT_VIEWER_PORT.to_string(),
            viewer_static_dir,
            agent_decision_source: DEFAULT_AGENT_DECISION_SOURCE.to_string(),
            agent_provider_backend: DEFAULT_AGENT_PROVIDER_BACKEND.to_string(),
            agent_provider_contract: DEFAULT_AGENT_PROVIDER_CONTRACT.to_string(),
            agent_provider_transport: DEFAULT_AGENT_PROVIDER_TRANSPORT.to_string(),
            agent_provider_url: DEFAULT_AGENT_PROVIDER_URL.to_string(),
            agent_provider_auth_token: String::new(),
            agent_provider_connect_timeout_ms: DEFAULT_AGENT_PROVIDER_CONNECT_TIMEOUT_MS
                .to_string(),
            agent_execution_lane: DEFAULT_AGENT_EXECUTION_LANE.to_string(),
            agent_provider_profile: DEFAULT_AGENT_PROVIDER_PROFILE.to_string(),
            llm_enabled: true,
            provider_auto_discover: true,
            chain_enabled: false,
            chain_status_bind: DEFAULT_CHAIN_STATUS_BIND.to_string(),
            chain_node_id: default_chain_node_id(),
            chain_network_tier: DEFAULT_CHAIN_NETWORK_TIER.to_string(),
            chain_network_tier_manifest: String::new(),
            chain_world_id: String::new(),
            chain_node_role: DEFAULT_CHAIN_NODE_ROLE.to_string(),
            chain_p2p_user_mode: DEFAULT_CHAIN_P2P_USER_MODE.to_string(),
            chain_p2p_accept_public_entry: false,
            chain_replication_bootstrap_peers: default_chain_replication_bootstrap_peers_csv(),
            chain_node_tick_ms: DEFAULT_CHAIN_NODE_TICK_MS.to_string(),
            chain_pos_slot_duration_ms: pos_defaults.slot_duration_ms.to_string(),
            chain_pos_ticks_per_slot: pos_defaults.ticks_per_slot.to_string(),
            chain_pos_proposal_tick_phase: pos_defaults.proposal_tick_phase.to_string(),
            chain_pos_adaptive_tick_scheduler_enabled: false,
            chain_pos_slot_clock_genesis_unix_ms: DEFAULT_CHAIN_POS_SLOT_CLOCK_GENESIS_UNIX_MS
                .to_string(),
            chain_pos_max_past_slot_lag: pos_defaults.max_past_slot_lag.to_string(),
            chain_node_validators: String::new(),
            auto_open_browser: true,
            launcher_bin,
            chain_runtime_bin,
        }
    }
}

impl LaunchConfig {
    fn normalize(&mut self) {
        normalize_launch_config(self);
        normalize_chain_network_tier_config(self);
    }
}

fn default_agent_provider_backend() -> String {
    DEFAULT_AGENT_PROVIDER_BACKEND.to_string()
}

fn default_agent_provider_contract() -> String {
    DEFAULT_AGENT_PROVIDER_CONTRACT.to_string()
}

fn default_agent_provider_transport() -> String {
    DEFAULT_AGENT_PROVIDER_TRANSPORT.to_string()
}

fn default_agent_execution_lane() -> String {
    DEFAULT_AGENT_EXECUTION_LANE.to_string()
}

#[derive(Debug)]
#[cfg(not(target_arch = "wasm32"))]
struct RunningProcess {
    child: Child,
    log_rx: Receiver<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum LauncherStatus {
    Idle,
    Running,
    Stopped,
    InvalidArgs,
    StartFailed,
    StopFailed,
    QueryFailed,
    Exited(String),
}

impl LauncherStatus {
    fn text(&self, language: UiLanguage) -> String {
        match (self, language) {
            (Self::Idle, UiLanguage::ZhCn) => "未启动".to_string(),
            (Self::Idle, UiLanguage::EnUs) => "Not Started".to_string(),
            (Self::Running, UiLanguage::ZhCn) => "运行中".to_string(),
            (Self::Running, UiLanguage::EnUs) => "Running".to_string(),
            (Self::Stopped, UiLanguage::ZhCn) => "已停止".to_string(),
            (Self::Stopped, UiLanguage::EnUs) => "Stopped".to_string(),
            (Self::InvalidArgs, UiLanguage::ZhCn) => "参数非法".to_string(),
            (Self::InvalidArgs, UiLanguage::EnUs) => "Invalid Config".to_string(),
            (Self::StartFailed, UiLanguage::ZhCn) => "启动失败".to_string(),
            (Self::StartFailed, UiLanguage::EnUs) => "Start Failed".to_string(),
            (Self::StopFailed, UiLanguage::ZhCn) => "停止失败".to_string(),
            (Self::StopFailed, UiLanguage::EnUs) => "Stop Failed".to_string(),
            (Self::QueryFailed, UiLanguage::ZhCn) => "状态查询失败".to_string(),
            (Self::QueryFailed, UiLanguage::EnUs) => "Status Query Failed".to_string(),
            (Self::Exited(reason), UiLanguage::ZhCn) => format!("已退出: {reason}"),
            (Self::Exited(reason), UiLanguage::EnUs) => format!("Exited: {reason}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChainRuntimeStatus {
    Disabled,
    NotStarted,
    Starting,
    Ready,
    StaleExecutionWorld(String),
    Unreachable(String),
    ConfigError(String),
}

impl ChainRuntimeStatus {
    fn text(&self, language: UiLanguage) -> &'static str {
        match (self, language) {
            (Self::Disabled, UiLanguage::ZhCn) => "已禁用",
            (Self::Disabled, UiLanguage::EnUs) => "Disabled",
            (Self::NotStarted, UiLanguage::ZhCn) => "未启动",
            (Self::NotStarted, UiLanguage::EnUs) => "Not Started",
            (Self::Starting, UiLanguage::ZhCn) => "启动中",
            (Self::Starting, UiLanguage::EnUs) => "Starting",
            (Self::Ready, UiLanguage::ZhCn) => "已就绪",
            (Self::Ready, UiLanguage::EnUs) => "Ready",
            (Self::StaleExecutionWorld(_), UiLanguage::ZhCn) => "旧执行世界冲突",
            (Self::StaleExecutionWorld(_), UiLanguage::EnUs) => "Stale World Conflict",
            (Self::Unreachable(_), UiLanguage::ZhCn) => "不可达",
            (Self::Unreachable(_), UiLanguage::EnUs) => "Unreachable",
            (Self::ConfigError(_), UiLanguage::ZhCn) => "配置错误",
            (Self::ConfigError(_), UiLanguage::EnUs) => "Config Error",
        }
    }

    fn color(&self) -> egui::Color32 {
        match self {
            Self::Disabled | Self::NotStarted => egui::Color32::from_rgb(130, 130, 130),
            Self::Starting => egui::Color32::from_rgb(201, 146, 44),
            Self::Ready => egui::Color32::from_rgb(62, 152, 92),
            Self::StaleExecutionWorld(_) => egui::Color32::from_rgb(196, 84, 84),
            Self::Unreachable(_) | Self::ConfigError(_) => egui::Color32::from_rgb(196, 84, 84),
        }
    }

    fn detail(&self) -> Option<&str> {
        match self {
            Self::StaleExecutionWorld(detail)
            | Self::Unreachable(detail)
            | Self::ConfigError(detail) => Some(detail.as_str()),
            Self::Disabled | Self::NotStarted | Self::Starting | Self::Ready => None,
        }
    }
}

fn launcher_status_from_web(status: &str, detail: Option<&str>) -> LauncherStatus {
    match status {
        "idle" => LauncherStatus::Idle,
        "running" => LauncherStatus::Running,
        "stopped" => LauncherStatus::Stopped,
        "invalid_config" => LauncherStatus::InvalidArgs,
        "start_failed" => LauncherStatus::StartFailed,
        "stop_failed" => LauncherStatus::StopFailed,
        "exited" => LauncherStatus::Exited(detail.unwrap_or("unknown").to_string()),
        _ => LauncherStatus::QueryFailed,
    }
}

fn chain_runtime_status_from_web(status: &str, detail: Option<&str>) -> ChainRuntimeStatus {
    match status {
        "disabled" => ChainRuntimeStatus::Disabled,
        "not_started" => ChainRuntimeStatus::NotStarted,
        "starting" => ChainRuntimeStatus::Starting,
        "ready" => ChainRuntimeStatus::Ready,
        "stale_execution_world" => {
            ChainRuntimeStatus::StaleExecutionWorld(detail.unwrap_or("unknown").to_string())
        }
        "unreachable" => ChainRuntimeStatus::Unreachable(detail.unwrap_or("unknown").to_string()),
        "config_error" => ChainRuntimeStatus::ConfigError(detail.unwrap_or("unknown").to_string()),
        _ => ChainRuntimeStatus::Unreachable(format!("unknown chain status: {status}")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigIssue {
    LlmRequired,
    LiveBindInvalid,
    WebBindInvalid,
    ViewerHostRequired,
    ViewerPortInvalid,
    ViewerStaticDirRequired,
    ViewerStaticDirMissing,
    AgentProviderModeInvalid,
    ProviderBaseUrlRequired,
    ProviderBaseUrlInvalid,
    ProviderBaseUrlLoopbackRequired,
    ProviderConnectTimeoutMsInvalid,
    ProviderExecutionModeInvalid,
    ProviderProfileRequired,
    LauncherBinRequired,
    LauncherBinMissing,
    ChainRuntimeBinRequired,
    ChainRuntimeBinMissing,
    ChainStatusBindInvalid,
    ChainNodeIdRequired,
    ChainNetworkTierInvalid,
    ChainRoleInvalid,
    ChainP2pUserModeInvalid,
    ChainPublicEntryConfirmationRequired,
    ChainReplicationBootstrapPeersInvalid,
    ChainTickMsInvalid,
    ChainPosSlotDurationMsInvalid,
    ChainPosTicksPerSlotInvalid,
    ChainPosProposalTickPhaseInvalid,
    ChainPosProposalTickPhaseOutOfRange,
    ChainPosSlotClockGenesisUnixMsInvalid,
    ChainPosMaxPastSlotLagInvalid,
    ChainValidatorsInvalid,
}

impl ConfigIssue {
    fn text(self, language: UiLanguage) -> &'static str {
        match (self, language) {
            (Self::LlmRequired, UiLanguage::ZhCn) => {
                "当前产品设定要求启用 LLM；关闭后不能进入正式游玩链路"
            }
            (Self::LlmRequired, UiLanguage::EnUs) => {
                "LLM must stay enabled; no-LLM is no longer a playable entry path"
            }
            (Self::LiveBindInvalid, UiLanguage::ZhCn) => "实时服务绑定必须是 <host:port>",
            (Self::LiveBindInvalid, UiLanguage::EnUs) => "Live bind must be in <host:port> format",
            (Self::WebBindInvalid, UiLanguage::ZhCn) => "WebSocket 绑定必须是 <host:port>",
            (Self::WebBindInvalid, UiLanguage::EnUs) => "Web bind must be in <host:port> format",
            (Self::ViewerHostRequired, UiLanguage::ZhCn) => "游戏页面主机（viewer host）是必填项",
            (Self::ViewerHostRequired, UiLanguage::EnUs) => "Viewer host is required",
            (Self::ViewerPortInvalid, UiLanguage::ZhCn) => {
                "游戏页面端口（viewer port）必须在 1..=65535"
            }
            (Self::ViewerPortInvalid, UiLanguage::EnUs) => {
                "Viewer port must be an integer in 1..=65535"
            }
            (Self::ViewerStaticDirRequired, UiLanguage::ZhCn) => {
                "前端静态资源目录（viewer static dir）是必填项"
            }
            (Self::ViewerStaticDirRequired, UiLanguage::EnUs) => {
                "Viewer static directory is required"
            }
            (Self::ViewerStaticDirMissing, UiLanguage::ZhCn) => "前端静态资源目录不存在或不是目录",
            (Self::ViewerStaticDirMissing, UiLanguage::EnUs) => {
                "Viewer static directory does not exist or is not a directory"
            }
            (Self::AgentProviderModeInvalid, UiLanguage::ZhCn) => {
                "Agent 接入方式必须是 builtin_llm、agent_direct_connect 或 provider_backed"
            }
            (Self::AgentProviderModeInvalid, UiLanguage::EnUs) => {
                "Agent access mode must be builtin_llm, agent_direct_connect, or provider_backed"
            }
            (Self::ProviderBaseUrlRequired, UiLanguage::ZhCn) => {
                "启用 ProviderBacked 且关闭自动发现时，必须填写 Provider Base URL"
            }
            (Self::ProviderBaseUrlRequired, UiLanguage::EnUs) => {
                "Provider base URL is required when auto-discover is disabled"
            }
            (Self::ProviderBaseUrlInvalid, UiLanguage::ZhCn) => {
                "Provider Base URL 必须是有效的 http://<host>:<port> 或 https://<host>"
            }
            (Self::ProviderBaseUrlInvalid, UiLanguage::EnUs) => {
                "Provider base URL must be a valid http://<host>:<port> or https://<host>"
            }
            (Self::ProviderBaseUrlLoopbackRequired, UiLanguage::ZhCn) => {
                "loopback_http 的 Provider Base URL 仅允许使用 loopback 地址（127.0.0.1 / localhost / ::1）"
            }
            (Self::ProviderBaseUrlLoopbackRequired, UiLanguage::EnUs) => {
                "loopback_http provider base URL must use a loopback host (127.0.0.1 / localhost / ::1)"
            }
            (Self::ProviderConnectTimeoutMsInvalid, UiLanguage::ZhCn) => {
                "Provider 连接超时毫秒必须是正整数"
            }
            (Self::ProviderConnectTimeoutMsInvalid, UiLanguage::EnUs) => {
                "Provider connect timeout milliseconds must be a positive integer"
            }
            (Self::ProviderExecutionModeInvalid, UiLanguage::ZhCn) => {
                "Provider execution mode 必须是 player_parity 或 headless_agent"
            }
            (Self::ProviderExecutionModeInvalid, UiLanguage::EnUs) => {
                "Provider execution mode must be player_parity or headless_agent"
            }
            (Self::ProviderProfileRequired, UiLanguage::ZhCn) => {
                "启用 ProviderBacked 时，Provider Agent Profile 不能为空"
            }
            (Self::ProviderProfileRequired, UiLanguage::EnUs) => {
                "Provider agent profile is required when ProviderBacked is enabled"
            }
            (Self::LauncherBinRequired, UiLanguage::ZhCn) => {
                "启动器二进制路径（launcher bin）是必填项"
            }
            (Self::LauncherBinRequired, UiLanguage::EnUs) => "Launcher binary path is required",
            (Self::LauncherBinMissing, UiLanguage::ZhCn) => "启动器二进制文件不存在",
            (Self::LauncherBinMissing, UiLanguage::EnUs) => "Launcher binary file does not exist",
            (Self::ChainRuntimeBinRequired, UiLanguage::ZhCn) => {
                "链运行时二进制路径（chain runtime bin）是必填项"
            }
            (Self::ChainRuntimeBinRequired, UiLanguage::EnUs) => {
                "Chain runtime binary path is required"
            }
            (Self::ChainRuntimeBinMissing, UiLanguage::ZhCn) => "链运行时二进制文件不存在",
            (Self::ChainRuntimeBinMissing, UiLanguage::EnUs) => {
                "Chain runtime binary file does not exist"
            }
            (Self::ChainStatusBindInvalid, UiLanguage::ZhCn) => "链状态服务绑定必须是 <host:port>",
            (Self::ChainStatusBindInvalid, UiLanguage::EnUs) => {
                "Chain status bind must be in <host:port> format"
            }
            (Self::ChainNodeIdRequired, UiLanguage::ZhCn) => "链节点 ID（chain node id）是必填项",
            (Self::ChainNodeIdRequired, UiLanguage::EnUs) => "Chain node id is required",
            (Self::ChainNetworkTierInvalid, UiLanguage::ZhCn) => {
                "链网络层级必须是 local_devnet、public_testnet 或 mainnet"
            }
            (Self::ChainNetworkTierInvalid, UiLanguage::EnUs) => {
                "Chain network tier must be local_devnet, public_testnet, or mainnet"
            }
            (Self::ChainRoleInvalid, UiLanguage::ZhCn) => {
                "链节点角色必须是 sequencer/storage/observer"
            }
            (Self::ChainRoleInvalid, UiLanguage::EnUs) => {
                "Chain role must be one of: sequencer/storage/observer"
            }
            (Self::ChainP2pUserModeInvalid, UiLanguage::ZhCn) => {
                "P2P 加入模式必须是 auto_join/private_safe/public_entry"
            }
            (Self::ChainP2pUserModeInvalid, UiLanguage::EnUs) => {
                "P2P join mode must be one of: auto_join/private_safe/public_entry"
            }
            (Self::ChainPublicEntryConfirmationRequired, UiLanguage::ZhCn) => {
                "选择公网入口前，必须显式确认承担公网入口职责"
            }
            (Self::ChainPublicEntryConfirmationRequired, UiLanguage::EnUs) => {
                "Public entry requires explicit confirmation of public-entry responsibility"
            }
            (Self::ChainReplicationBootstrapPeersInvalid, UiLanguage::ZhCn) => {
                "链复制 bootstrap peers 必须是 /dns4/bootstrap1.oasis7.tech/tcp/5612 或 /ip4/.../tcp/.../p2p/<peer-id> 这类 multiaddr，多个地址可用逗号、分号或空白分隔"
            }
            (Self::ChainReplicationBootstrapPeersInvalid, UiLanguage::EnUs) => {
                "Chain replication bootstrap peers must use multiaddr values like /dns4/bootstrap1.oasis7.tech/tcp/5612 or /ip4/.../tcp/.../p2p/<peer-id>; separate multiple entries with commas, semicolons, or whitespace"
            }
            (Self::ChainTickMsInvalid, UiLanguage::ZhCn) => {
                "链节点轮询间隔毫秒（chain node poll interval ms）必须是正整数"
            }
            (Self::ChainTickMsInvalid, UiLanguage::EnUs) => {
                "Chain node poll interval milliseconds must be a positive integer"
            }
            (Self::ChainPosSlotDurationMsInvalid, UiLanguage::ZhCn) => {
                "链 PoS 槽时长（slot duration ms）必须是正整数"
            }
            (Self::ChainPosSlotDurationMsInvalid, UiLanguage::EnUs) => {
                "Chain PoS slot duration ms must be a positive integer"
            }
            (Self::ChainPosTicksPerSlotInvalid, UiLanguage::ZhCn) => {
                "链 PoS 每槽 tick 数（ticks per slot）必须是正整数"
            }
            (Self::ChainPosTicksPerSlotInvalid, UiLanguage::EnUs) => {
                "Chain PoS ticks per slot must be a positive integer"
            }
            (Self::ChainPosProposalTickPhaseInvalid, UiLanguage::ZhCn) => {
                "链 PoS 提案相位（proposal tick phase）必须是非负整数"
            }
            (Self::ChainPosProposalTickPhaseInvalid, UiLanguage::EnUs) => {
                "Chain PoS proposal tick phase must be a non-negative integer"
            }
            (Self::ChainPosProposalTickPhaseOutOfRange, UiLanguage::ZhCn) => {
                "链 PoS 提案相位必须小于每槽 tick 数"
            }
            (Self::ChainPosProposalTickPhaseOutOfRange, UiLanguage::EnUs) => {
                "Chain PoS proposal tick phase must be less than ticks per slot"
            }
            (Self::ChainPosSlotClockGenesisUnixMsInvalid, UiLanguage::ZhCn) => {
                "链 PoS 槽时钟起点（slot clock genesis unix ms）必须是整数或留空"
            }
            (Self::ChainPosSlotClockGenesisUnixMsInvalid, UiLanguage::EnUs) => {
                "Chain PoS slot clock genesis unix ms must be an integer or empty"
            }
            (Self::ChainPosMaxPastSlotLagInvalid, UiLanguage::ZhCn) => {
                "链 PoS 允许过旧槽滞后（max past slot lag）必须是非负整数"
            }
            (Self::ChainPosMaxPastSlotLagInvalid, UiLanguage::EnUs) => {
                "Chain PoS max past slot lag must be a non-negative integer"
            }
            (Self::ChainValidatorsInvalid, UiLanguage::ZhCn) => {
                "链验证者（chain validators）格式必须是 <validator_id:stake>"
            }
            (Self::ChainValidatorsInvalid, UiLanguage::EnUs) => {
                "Chain validators must be in <validator_id:stake> format"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(target_arch = "wasm32")]
enum FeedbackKind {
    Bug,
    Suggestion,
}

#[cfg(target_arch = "wasm32")]
impl FeedbackKind {
    fn slug(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Suggestion => "suggestion",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(target_arch = "wasm32")]
struct FeedbackDraft {
    kind: FeedbackKind,
    title: String,
    description: String,
}

#[cfg(target_arch = "wasm32")]
impl Default for FeedbackDraft {
    fn default() -> Self {
        Self {
            kind: FeedbackKind::Bug,
            title: String::new(),
            description: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FeedbackSubmitState {
    None,
    Success(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(target_arch = "wasm32")]
struct TransferDraft {
    from_account_id: String,
    to_account_id: String,
    amount: String,
    nonce: String,
}

#[cfg(target_arch = "wasm32")]
impl Default for TransferDraft {
    fn default() -> Self {
        Self {
            from_account_id: String::new(),
            to_account_id: String::new(),
            amount: "1".to_string(),
            nonce: "1".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TransferSubmitState {
    None,
    Success(String),
    Failed(String),
}

#[derive(Debug)]
struct ClientLauncherApp {
    config: LaunchConfig,
    config_dirty: bool,
    provider_check_status: ProviderCheckStatus,
    llm_settings_panel: LlmSettingsPanel,
    ui_language: UiLanguage,
    status: LauncherStatus,
    chain_runtime_status: ChainRuntimeStatus,
    chain_identity: Option<web_api_support::WebChainIdentityStatus>,
    chain_p2p_status: Option<WebChainP2pStatus>,
    chain_observability_status: Option<WebChainNodeObservabilityStatus>,
    chain_replication_status: Option<WebChainReplicationStatus>,
    chain_recovery: Option<WebChainRecoverySnapshot>,
    #[cfg(not(target_arch = "wasm32"))]
    running: Option<RunningProcess>,
    chain_auto_start_attempted: bool,
    logs: VecDeque<String>,
    feedback_draft: FeedbackDraft,
    feedback_submit_state: FeedbackSubmitState,
    feedback_window_open: bool,
    onboarding_state: OnboardingState,
    ux_state: LauncherUxState,
    demo_mode_phase: DemoModePhase,
    guidance_insights_open: bool,
    startup_guide_state: StartupGuideState,
    config_window_open: bool,
    transfer_draft: TransferDraft,
    transfer_submit_state: TransferSubmitState,
    transfer_window_open: bool,
    transfer_panel_state: transfer_window::TransferPanelState,
    peer_details_window_open: bool,
    explorer_window_open: bool,
    explorer_panel_state: explorer_window::ExplorerPanelState,
    web_api_tx: Sender<WebApiEvent>,
    web_api_rx: Receiver<WebApiEvent>,
    web_request_inflight: WebRequestInflight,
    web_state_generation: u64,
    last_web_poll_at: Option<Instant>,
    control_plane_snapshot_received: bool,
    web_game_url: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    control_api_base: String,
    #[cfg(not(target_arch = "wasm32"))]
    control_listen_bind: String,
    #[cfg(not(target_arch = "wasm32"))]
    control_manage_service: bool,
}

impl Default for ClientLauncherApp {
    fn default() -> Self {
        let config = LaunchConfig::default();
        let ux_state = self_guided::load_launcher_ux_state();
        let onboarding_state = OnboardingState::from_persisted(
            ux_state.onboarding_completed,
            ux_state.onboarding_dismissed,
        );
        let (web_api_tx, web_api_rx) = mpsc::channel::<WebApiEvent>();
        #[cfg(not(target_arch = "wasm32"))]
        let (_, control_listen_bind, control_api_base, control_manage_service) =
            resolve_control_plane_env();
        let chain_runtime_status = if chain_runtime_effectively_enabled(&config) {
            ChainRuntimeStatus::NotStarted
        } else {
            ChainRuntimeStatus::Disabled
        };
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut app = Self {
            config,
            config_dirty: false,
            provider_check_status: ProviderCheckStatus::Disabled,
            llm_settings_panel: LlmSettingsPanel::new(LlmSettingsPanel::default_path()),
            ui_language: UiLanguage::detect_from_env(),
            status: LauncherStatus::Idle,
            chain_runtime_status,
            chain_identity: None,
            chain_p2p_status: None,
            chain_observability_status: None,
            chain_replication_status: None,
            chain_recovery: None,
            #[cfg(not(target_arch = "wasm32"))]
            running: None,
            chain_auto_start_attempted: false,
            logs: VecDeque::new(),
            feedback_draft: FeedbackDraft::default(),
            feedback_submit_state: FeedbackSubmitState::None,
            feedback_window_open: false,
            onboarding_state,
            ux_state,
            demo_mode_phase: DemoModePhase::Idle,
            guidance_insights_open: false,
            startup_guide_state: StartupGuideState::default(),
            config_window_open: false,
            transfer_draft: TransferDraft::default(),
            transfer_submit_state: TransferSubmitState::None,
            transfer_window_open: false,
            transfer_panel_state: transfer_window::TransferPanelState::default(),
            peer_details_window_open: false,
            explorer_window_open: false,
            explorer_panel_state: explorer_window::ExplorerPanelState::default(),
            web_api_tx,
            web_api_rx,
            web_request_inflight: WebRequestInflight::default(),
            web_state_generation: 0,
            last_web_poll_at: None,
            control_plane_snapshot_received: false,
            web_game_url: None,
            #[cfg(not(target_arch = "wasm32"))]
            control_api_base,
            #[cfg(not(target_arch = "wasm32"))]
            control_listen_bind,
            #[cfg(not(target_arch = "wasm32"))]
            control_manage_service,
        };
        #[cfg(not(target_arch = "wasm32"))]
        app.apply_screenshot_modal_override();
        app
    }
}

#[cfg(test)]
#[path = "main_tests_modal_render.rs"]
mod modal_render_tests;
#[cfg(test)]
#[path = "main_observability_tests.rs"]
mod observability_tests;
#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
