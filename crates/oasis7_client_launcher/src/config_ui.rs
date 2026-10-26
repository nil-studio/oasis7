use super::*;
#[cfg(not(target_arch = "wasm32"))]
use oasis7_launcher_ui::launcher_ui_fields_for_native;
#[cfg(target_arch = "wasm32")]
use oasis7_launcher_ui::launcher_ui_fields_for_web;
use oasis7_launcher_ui::{LauncherUiField, LauncherUiFieldKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupGuideTarget {
    Game,
    Chain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartupGuideState {
    pub(super) open: bool,
    pub(super) target: StartupGuideTarget,
    pub(super) first_check_done: bool,
}

impl Default for StartupGuideState {
    fn default() -> Self {
        Self {
            open: false,
            target: StartupGuideTarget::Game,
            first_check_done: false,
        }
    }
}

pub(super) fn issue_field_ids(issue: ConfigIssue) -> &'static [&'static str] {
    match issue {
        ConfigIssue::LlmRequired => &["llm_enabled"],
        ConfigIssue::LiveBindInvalid => &["live_bind"],
        ConfigIssue::WebBindInvalid => &["web_bind"],
        ConfigIssue::ViewerHostRequired => &["viewer_host"],
        ConfigIssue::ViewerPortInvalid => &["viewer_port"],
        ConfigIssue::ViewerStaticDirRequired | ConfigIssue::ViewerStaticDirMissing => {
            &["viewer_static_dir"]
        }
        ConfigIssue::AgentProviderModeInvalid => &[
            "agent_decision_source",
            "agent_provider_backend",
            "agent_provider_contract",
            "agent_provider_transport",
        ],
        ConfigIssue::ProviderBaseUrlRequired
        | ConfigIssue::ProviderBaseUrlInvalid
        | ConfigIssue::ProviderBaseUrlLoopbackRequired => &["agent_provider_url"],
        ConfigIssue::ProviderConnectTimeoutMsInvalid => &["agent_provider_connect_timeout_ms"],
        ConfigIssue::ProviderExecutionModeInvalid => &["agent_execution_lane"],
        ConfigIssue::ProviderProfileRequired => &["agent_provider_profile"],
        ConfigIssue::LauncherBinRequired | ConfigIssue::LauncherBinMissing => &["launcher_bin"],
        ConfigIssue::ChainRuntimeBinRequired | ConfigIssue::ChainRuntimeBinMissing => {
            &["chain_runtime_bin"]
        }
        ConfigIssue::ChainStatusBindInvalid => &["chain_status_bind"],
        ConfigIssue::ChainNodeIdRequired => &["chain_node_id"],
        ConfigIssue::ChainNetworkTierInvalid => &["chain_network_tier"],
        ConfigIssue::ChainRoleInvalid => &["chain_node_role"],
        ConfigIssue::ChainP2pUserModeInvalid => &["chain_p2p_user_mode"],
        ConfigIssue::ChainPublicEntryConfirmationRequired => {
            &["chain_p2p_user_mode", "chain_p2p_accept_public_entry"]
        }
        ConfigIssue::ChainReplicationBootstrapPeersInvalid => {
            &["chain_replication_bootstrap_peers"]
        }
        ConfigIssue::ChainTickMsInvalid => &["chain_node_tick_ms"],
        ConfigIssue::ChainPosSlotDurationMsInvalid => &["chain_pos_slot_duration_ms"],
        ConfigIssue::ChainPosTicksPerSlotInvalid => &["chain_pos_ticks_per_slot"],
        ConfigIssue::ChainPosProposalTickPhaseInvalid => &["chain_pos_proposal_tick_phase"],
        ConfigIssue::ChainPosProposalTickPhaseOutOfRange => {
            &["chain_pos_ticks_per_slot", "chain_pos_proposal_tick_phase"]
        }
        ConfigIssue::ChainPosSlotClockGenesisUnixMsInvalid => {
            &["chain_pos_slot_clock_genesis_unix_ms"]
        }
        ConfigIssue::ChainPosMaxPastSlotLagInvalid => &["chain_pos_max_past_slot_lag"],
        ConfigIssue::ChainValidatorsInvalid => &["chain_node_validators"],
    }
}

impl ClientLauncherApp {
    pub(super) fn ui_field_label(&self, field: &LauncherUiField) -> &'static str {
        match self.ui_language {
            UiLanguage::ZhCn => field.label_zh,
            UiLanguage::EnUs => field.label_en,
        }
    }

    pub(super) fn render_config_field(
        &mut self,
        ui: &mut egui::Ui,
        field: &LauncherUiField,
        stack_text_fields: bool,
    ) {
        let label = self.ui_field_label(field);
        if field.id == "agent_execution_lane" {
            self.render_provider_execution_mode_field(ui, label, stack_text_fields);
            return;
        }
        if field.id == "chain_p2p_user_mode" {
            self.render_chain_p2p_user_mode_field(ui, label, stack_text_fields);
            return;
        }
        if field.id == "chain_network_tier" {
            self.render_chain_network_tier_field(ui, label, stack_text_fields);
            return;
        }
        if field.id == "chain_p2p_accept_public_entry" {
            self.render_chain_p2p_accept_public_entry_field(ui, label, stack_text_fields);
            return;
        }
        match field.kind {
            LauncherUiFieldKind::Text => {
                if let Some(value) = launcher_text_field_mut(&mut self.config, field.id) {
                    let mut changed = false;
                    if stack_text_fields {
                        ui.vertical(|ui| {
                            ui.label(label);
                            let response = ui.add_sized(
                                [ui.available_width(), 0.0],
                                if field.id == "agent_provider_auth_token" {
                                    egui::TextEdit::singleline(value).password(true)
                                } else {
                                    egui::TextEdit::singleline(value)
                                },
                            );
                            if response.changed() {
                                changed = true;
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            let response = if field.id == "agent_provider_auth_token" {
                                ui.add(egui::TextEdit::singleline(value).password(true))
                            } else {
                                ui.text_edit_singleline(value)
                            };
                            if response.changed() {
                                changed = true;
                            }
                        });
                    }
                    if changed {
                        self.config.normalize();
                        if !chain_runtime_effectively_enabled(&self.config) {
                            self.chain_runtime_status = ChainRuntimeStatus::Disabled;
                        }
                        self.config_dirty = true;
                    }
                    if field.id == "chain_replication_bootstrap_peers" {
                        ui.small(self.tr(
                            "多个 bootstrap peer 可用逗号、分号或空白分隔，例如 /dns4/bootstrap1.oasis7.tech/tcp/5612 或 /ip4/127.0.0.1/tcp/4100/p2p/<peer-id>",
                            "Separate multiple bootstrap peers with commas, semicolons, or whitespace, for example /dns4/bootstrap1.oasis7.tech/tcp/5612 or /ip4/127.0.0.1/tcp/4100/p2p/<peer-id>",
                        ));
                    }
                }
            }
            LauncherUiFieldKind::Checkbox => {
                if let Some(value) = launcher_checkbox_field_mut(&mut self.config, field.id)
                    && ui.checkbox(value, label).changed()
                {
                    self.config.normalize();
                    if !chain_runtime_effectively_enabled(&self.config) {
                        self.chain_runtime_status = ChainRuntimeStatus::Disabled;
                    }
                    self.config_dirty = true;
                }
            }
        }
    }

    fn render_provider_execution_mode_field(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        stack_text_fields: bool,
    ) {
        let current = canonical_provider_execution_mode(self.config.agent_execution_lane.as_str())
            .unwrap_or(DEFAULT_AGENT_EXECUTION_LANE);
        if self.config.agent_execution_lane != current {
            self.config.agent_execution_lane = current.to_string();
            self.config_dirty = true;
        }
        let mut selected = current.to_string();

        let render_combo = |ui: &mut egui::Ui, selected: &mut String| {
            egui::ComboBox::from_id_salt("agent_execution_lane")
                .selected_text(match selected.as_str() {
                    "headless_agent" => self.tr(
                        "headless_agent（无 GUI 回归）",
                        "headless_agent (headless regression)",
                    ),
                    _ => self.tr(
                        "player_parity（GUI 体验对照）",
                        "player_parity (GUI parity lane)",
                    ),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        selected,
                        "player_parity".to_string(),
                        self.tr(
                            "player_parity（GUI 体验对照）",
                            "player_parity (GUI parity lane)",
                        ),
                    );
                    ui.selectable_value(
                        selected,
                        "headless_agent".to_string(),
                        self.tr(
                            "headless_agent（无 GUI 回归）",
                            "headless_agent (headless regression)",
                        ),
                    );
                });
        };

        if stack_text_fields {
            ui.vertical(|ui| {
                ui.label(label);
                render_combo(ui, &mut selected);
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(label);
                render_combo(ui, &mut selected);
            });
        }

        if self.config.agent_execution_lane != selected {
            self.config.agent_execution_lane = selected;
            self.config_dirty = true;
        }
    }

    fn render_chain_p2p_user_mode_field(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        stack_text_fields: bool,
    ) {
        let current = canonical_chain_p2p_user_mode(self.config.chain_p2p_user_mode.as_str())
            .unwrap_or(DEFAULT_CHAIN_P2P_USER_MODE);
        if self.config.chain_p2p_user_mode != current {
            self.config.chain_p2p_user_mode = current.to_string();
            self.config_dirty = true;
        }
        let mut selected = current.to_string();

        let render_combo = |ui: &mut egui::Ui, selected: &mut String, app: &ClientLauncherApp| {
            egui::ComboBox::from_id_salt("chain_p2p_user_mode")
                .selected_text(app.chain_user_mode_option_label(selected.as_str()))
                .show_ui(ui, |ui| {
                    for mode in ["auto_join", "private_safe", "public_entry"] {
                        ui.selectable_value(
                            selected,
                            mode.to_string(),
                            app.chain_user_mode_option_label(mode),
                        );
                    }
                });
        };

        if stack_text_fields {
            ui.vertical(|ui| {
                ui.label(label);
                render_combo(ui, &mut selected, self);
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(label);
                render_combo(ui, &mut selected, self);
            });
        }
        ui.small(self.tr(
            "默认建议使用“自动加入”；只有在你明确要承担公网入口职责时才切到“公网入口”。",
            "Use Auto Join by default; switch to Public Entry only when you intentionally expose a public ingress surface.",
        ));

        if self.config.chain_p2p_user_mode != selected {
            if selected != "public_entry" {
                self.config.chain_p2p_accept_public_entry = false;
            }
            self.config.chain_p2p_user_mode = selected;
            self.config_dirty = true;
        }
    }

    fn render_chain_network_tier_field(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        stack_text_fields: bool,
    ) {
        let current = canonical_chain_network_tier(self.config.chain_network_tier.as_str())
            .unwrap_or(DEFAULT_CHAIN_NETWORK_TIER);
        if self.config.chain_network_tier != current {
            self.config.chain_network_tier = current.to_string();
            self.config_dirty = true;
        }
        let mut selected = current.to_string();

        let render_combo = |ui: &mut egui::Ui, selected: &mut String, app: &ClientLauncherApp| {
            egui::ComboBox::from_id_salt("chain_network_tier")
                .selected_text(app.chain_network_tier_option_label(selected.as_str()))
                .show_ui(ui, |ui| {
                    for tier in ["local_devnet", "public_testnet", "mainnet"] {
                        ui.selectable_value(
                            selected,
                            tier.to_string(),
                            app.chain_network_tier_option_label(tier),
                        );
                    }
                });
        };

        if stack_text_fields {
            ui.vertical(|ui| {
                ui.label(label);
                render_combo(ui, &mut selected, self);
            });
        } else {
            ui.horizontal(|ui| {
                ui.label(label);
                render_combo(ui, &mut selected, self);
            });
        }
        ui.small(self.tr(
            "Public Testnet / Mainnet 当前使用仓库内 skeleton manifest；切换会影响下一次链运行时启动参数。",
            "Public Testnet / Mainnet currently use repository skeleton manifests; switching affects the next chain runtime start.",
        ));

        if self.config.chain_network_tier != selected {
            self.config.chain_network_tier = selected;
            self.config.chain_network_tier_manifest.clear();
            normalize_chain_network_tier_config(&mut self.config);
            self.config_dirty = true;
        }
    }

    fn render_chain_p2p_accept_public_entry_field(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        stack_text_fields: bool,
    ) {
        let needs_emphasis = self.config.chain_p2p_user_mode == "public_entry"
            || self
                .chain_p2p_status
                .as_ref()
                .is_some_and(|status| status.requires_explicit_public_entry_confirmation);
        let render_checkbox =
            |ui: &mut egui::Ui, value: &mut bool, label: &str| ui.checkbox(value, label);

        if stack_text_fields {
            ui.vertical(|ui| {
                let response =
                    render_checkbox(ui, &mut self.config.chain_p2p_accept_public_entry, label);
                if response.changed() {
                    self.config_dirty = true;
                }
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                let response =
                    render_checkbox(ui, &mut self.config.chain_p2p_accept_public_entry, label);
                if response.changed() {
                    self.config_dirty = true;
                }
            });
        }
        if needs_emphasis {
            ui.small(
                egui::RichText::new(self.tr(
                    "启用后，节点可被自动提升为公网入口；建议先阅读推荐依据，再决定是否重启链运行时应用。",
                    "When enabled, the node may be promoted to a public entry surface; review the recommendation evidence before restarting blockchain runtime with this applied.",
                ))
                .color(egui::Color32::from_rgb(188, 60, 60)),
            );
        }
    }

    pub(super) fn render_config_section(&mut self, ui: &mut egui::Ui, section: &str) {
        let stack_text_fields = ui.available_width() <= 560.0;
        ui.vertical(|ui| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                for field in
                    launcher_ui_fields_for_native().filter(|field| field.section == section)
                {
                    self.render_config_field(ui, field, stack_text_fields);
                }
            }

            #[cfg(target_arch = "wasm32")]
            {
                for field in launcher_ui_fields_for_web().filter(|field| field.section == section) {
                    self.render_config_field(ui, field, stack_text_fields);
                }
            }
        });
    }

    pub(super) fn render_config_validation_summary(
        &mut self,
        ui: &mut egui::Ui,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
    ) {
        let chain_issue_count = if self.config.chain_enabled {
            chain_required_issues.len()
        } else {
            0
        };
        let has_issue = !game_required_issues.is_empty() || chain_issue_count > 0;

        ui.horizontal_wrapped(|ui| {
            ui.label(self.tr(
                "低频配置已收口到高级配置弹窗。",
                "Low-frequency settings are grouped in Advanced Config.",
            ));
            if ui.button(self.tr("高级配置", "Advanced Config")).clicked() {
                self.config_window_open = true;
            }
        });

        if self.config_dirty {
            ui.small(
                egui::RichText::new(self.tr(
                    "检测到本地配置改动：轮询快照不会覆盖当前编辑，直到配置与服务端一致。",
                    "Local config edits detected: polling snapshots will not overwrite current edits until they match server config.",
                ))
                .color(egui::Color32::from_rgb(201, 146, 44)),
            );
        }

        self.render_provider_summary(ui);

        if !has_issue {
            ui.colored_label(
                egui::Color32::from_rgb(36, 130, 78),
                self.tr(
                    "当前配置校验通过，可直接执行高频操作。",
                    "Configuration checks passed; quick actions are ready.",
                ),
            );
            return;
        }

        let summary = if self.config.chain_enabled {
            match self.ui_language {
                UiLanguage::ZhCn => format!(
                    "存在配置问题：游戏 {} 项，区块链 {} 项",
                    game_required_issues.len(),
                    chain_issue_count
                ),
                UiLanguage::EnUs => format!(
                    "Configuration issues detected: game {}, blockchain {}",
                    game_required_issues.len(),
                    chain_issue_count
                ),
            }
        } else {
            match self.ui_language {
                UiLanguage::ZhCn => format!("存在配置问题：游戏 {} 项", game_required_issues.len()),
                UiLanguage::EnUs => format!(
                    "Configuration issues detected: game {}",
                    game_required_issues.len()
                ),
            }
        };
        ui.colored_label(egui::Color32::from_rgb(188, 60, 60), summary);
        ui.small(self.tr(
            "请点击“高级配置”查看并修复具体字段。",
            "Open Advanced Config to review and fix specific fields.",
        ));
    }

    pub(super) fn show_config_window(
        &mut self,
        ctx: &egui::Context,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
    ) {
        if !self.config_window_open {
            return;
        }

        let mut keep_open = self.config_window_open;
        let window_size = Self::modal_window_size(ctx, 900.0, 660.0);
        egui::Window::new(self.tr("高级配置", "Advanced Config"))
            .collapsible(false)
            .resizable(true)
            .default_size(window_size)
            .open(&mut keep_open)
            .show(ctx, |ui| {
                let chain_issue_count = if self.config.chain_enabled {
                    chain_required_issues.len()
                } else {
                    0
                };
                let issue_count = game_required_issues.len() + chain_issue_count;
                Self::modal_header(
                    ui,
                    self.tr("高级配置", "Advanced Config"),
                    self.tr(
                        "集中编辑启动器、链运行时和 Provider 字段。",
                        "Edit launcher, chain runtime, and provider fields in one place.",
                    ),
                    Some((
                        if issue_count == 0 {
                            self.tr("配置通过", "Config Ready")
                        } else {
                            self.tr("存在阻断", "Blocked")
                        },
                        if issue_count == 0 {
                            egui::Color32::from_rgb(62, 152, 92)
                        } else {
                            egui::Color32::from_rgb(188, 60, 60)
                        },
                    )),
                );
                ui.add_space(8.0);

                Self::modal_card(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height((window_size.y - 220.0).max(300.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for section in NATIVE_UI_SECTIONS {
                                self.render_config_section(ui, section);
                            }
                        });
                });
                ui.add_space(8.0);

                if issue_count == 0 {
                    Self::modal_banner(
                        ui,
                        self.tr(
                            "必填配置项已通过校验，可启动游戏",
                            "Required configuration check passed; game can start",
                        ),
                        egui::Color32::from_rgb(36, 130, 78),
                    );
                } else {
                    Self::modal_card(ui, |ui| {
                        ui.label(
                            egui::RichText::new(self.tr("启动前阻断项", "Preflight Blockers"))
                                .strong()
                                .size(14.0),
                        );
                        for issue in game_required_issues {
                            ui.horizontal_wrapped(|ui| {
                                Self::modal_status_chip(
                                    ui,
                                    self.tr("游戏", "Game"),
                                    egui::Color32::from_rgb(188, 60, 60),
                                );
                                ui.small(issue.text(self.ui_language));
                            });
                        }
                        if self.config.chain_enabled {
                            for issue in chain_required_issues {
                                ui.horizontal_wrapped(|ui| {
                                    Self::modal_status_chip(
                                        ui,
                                        self.tr("区块链", "Chain"),
                                        egui::Color32::from_rgb(188, 60, 60),
                                    );
                                    ui.small(issue.text(self.ui_language));
                                });
                            }
                        }
                    });
                }
            });
        self.config_window_open = keep_open;
    }

    pub(super) fn maybe_open_startup_guide_on_first_check(
        &mut self,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
    ) {
        if self.startup_guide_state.first_check_done {
            return;
        }
        self.startup_guide_state.first_check_done = true;

        if !game_required_issues.is_empty() {
            self.open_startup_guide(StartupGuideTarget::Game);
            self.append_log(self.tr(
                "首次检查发现游戏配置缺失，已打开配置引导。",
                "Initial check found missing game configuration; configuration guide opened.",
            ));
            return;
        }

        if self.config.chain_enabled && !chain_required_issues.is_empty() {
            self.open_startup_guide(StartupGuideTarget::Chain);
            self.append_log(self.tr(
                "首次检查发现区块链配置缺失，已打开配置引导。",
                "Initial check found missing blockchain configuration; configuration guide opened.",
            ));
        }
    }

    pub(super) fn handle_start_game_click(&mut self, game_required_issues: &[ConfigIssue]) {
        if game_required_issues.is_empty() {
            self.start_process();
            return;
        }

        self.status = LauncherStatus::InvalidArgs;
        self.append_log(self.tr(
            "游戏启动前校验失败：已打开配置引导，请先补齐字段。",
            "Game preflight validation failed: configuration guide opened, fill required fields first.",
        ));
        for issue in game_required_issues {
            self.append_log(format!("- {}", issue.text(self.ui_language)));
        }
        self.open_startup_guide(StartupGuideTarget::Game);
    }

    pub(super) fn handle_start_chain_click(&mut self, chain_required_issues: &[ConfigIssue]) {
        if chain_required_issues.is_empty() {
            self.start_chain_process();
            return;
        }

        let mut details = Vec::new();
        for issue in chain_required_issues {
            let detail = issue.text(self.ui_language).to_string();
            details.push(detail.clone());
            self.append_log(format!("- {detail}"));
        }
        self.chain_runtime_status = ChainRuntimeStatus::ConfigError(details.join("; "));
        self.append_log(self.tr(
            "区块链启动前校验失败：已打开配置引导，请先补齐字段。",
            "Blockchain preflight validation failed: configuration guide opened, fill required fields first.",
        ));
        self.open_startup_guide(StartupGuideTarget::Chain);
    }

    pub(super) fn open_game_config_guide(&mut self) {
        self.open_startup_guide(StartupGuideTarget::Game);
    }

    pub(super) fn open_chain_config_guide(&mut self) {
        self.open_startup_guide(StartupGuideTarget::Chain);
    }

    pub(super) fn show_startup_guide_window(
        &mut self,
        ctx: &egui::Context,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
    ) {
        if !self.startup_guide_state.open {
            return;
        }

        let target = self.startup_guide_state.target;
        let issues = match target {
            StartupGuideTarget::Game => game_required_issues,
            StartupGuideTarget::Chain => chain_required_issues,
        };
        let field_ids = self.collect_issue_fields(issues);

        let title = match target {
            StartupGuideTarget::Game => self.tr("启动引导（游戏）", "Startup Guide (Game)"),
            StartupGuideTarget::Chain => {
                self.tr("启动引导（区块链）", "Startup Guide (Blockchain)")
            }
        };
        let intro = match target {
            StartupGuideTarget::Game => self.tr(
                "检测到游戏启动前有必填问题，请直接在此窗口补齐。",
                "Required game settings are missing. Fill them directly in this window.",
            ),
            StartupGuideTarget::Chain => self.tr(
                "检测到区块链启动前有必填问题，请直接在此窗口补齐。",
                "Required blockchain settings are missing. Fill them directly in this window.",
            ),
        };

        let mut keep_open = self.startup_guide_state.open;
        let mut request_close = false;
        let window_size = Self::modal_window_size(ctx, 760.0, 560.0);
        egui::Window::new(title)
            .collapsible(false)
            .resizable(true)
            .default_size(window_size)
            .open(&mut keep_open)
            .show(ctx, |ui| {
                Self::modal_header(
                    ui,
                    title,
                    intro,
                    Some((
                        if issues.is_empty() {
                            self.tr("已通过", "Ready")
                        } else {
                            self.tr("需修复", "Needs Fix")
                        },
                        if issues.is_empty() {
                            egui::Color32::from_rgb(62, 152, 92)
                        } else {
                            egui::Color32::from_rgb(188, 60, 60)
                        },
                    )),
                );
                ui.add_space(8.0);

                if issues.is_empty() {
                    Self::modal_banner(
                        ui,
                        self.tr(
                            "当前目标已无阻断配置，可关闭窗口后继续启动。",
                            "No blocking configuration remains for this target. Close this window and start again.",
                        ),
                        egui::Color32::from_rgb(36, 130, 78),
                    );
                    ui.add_space(8.0);
                    if Self::modal_primary_button(ui, self.tr("关闭", "Close")).clicked() {
                        request_close = true;
                    }
                    return;
                }

                Self::modal_card(ui, |ui| {
                    ui.label(
                        egui::RichText::new(self.tr("待修复问题", "Issues to Fix"))
                            .strong()
                            .size(14.0),
                    );
                    ui.add_space(4.0);
                    for issue in issues {
                        ui.horizontal_wrapped(|ui| {
                            Self::modal_status_chip(
                                ui,
                                self.tr("阻断", "Blocked"),
                                egui::Color32::from_rgb(188, 60, 60),
                            );
                            ui.small(issue.text(self.ui_language));
                        });
                    }
                });
                ui.add_space(8.0);

                Self::modal_card(ui, |ui| {
                    ui.label(
                        egui::RichText::new(
                            self.tr("直接编辑下列字段", "Edit Fields Directly"),
                        )
                        .strong()
                        .size(14.0),
                    );
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height((window_size.y - 260.0).max(180.0))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let stack_text_fields = ui.available_width() <= 560.0;
                            for field_id in &field_ids {
                                if !self.render_config_field_by_id(
                                    ui,
                                    field_id,
                                    stack_text_fields,
                                ) {
                                    Self::modal_banner(
                                        ui,
                                        self.tr(
                                            "存在未映射字段，请通过“高级配置”继续修复。",
                                            "Some fields are not mapped. Use Advanced Config to continue.",
                                        ),
                                        egui::Color32::from_rgb(201, 146, 44),
                                    );
                                }
                            }
                        });
                });

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if Self::modal_secondary_button(
                        ui,
                        self.tr("打开高级配置", "Open Advanced Config"),
                    )
                    .clicked()
                    {
                        self.config_window_open = true;
                    }
                    if Self::modal_primary_button(ui, self.tr("关闭", "Close")).clicked() {
                        request_close = true;
                    }
                });
            });

        if request_close {
            keep_open = false;
        }
        self.startup_guide_state.open = keep_open;
    }

    fn open_startup_guide(&mut self, target: StartupGuideTarget) {
        self.startup_guide_state.target = target;
        self.startup_guide_state.open = true;
    }

    pub(super) fn collect_issue_fields(&self, issues: &[ConfigIssue]) -> Vec<&'static str> {
        let mut fields = Vec::new();
        for issue in issues {
            for field_id in issue_field_ids(*issue) {
                if !fields.contains(field_id) {
                    fields.push(*field_id);
                }
            }
        }
        fields
    }

    fn render_provider_summary(&mut self, ui: &mut egui::Ui) {
        if !is_provider_http_mode(&self.config) {
            return;
        }

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(self.tr("Provider 兼容检查", "Provider Compatibility Check"));
            let status = match &self.provider_check_status {
                ProviderCheckStatus::Disabled => ProviderCheckStatus::Idle,
                other => other.clone(),
            };
            ui.colored_label(status.color(), status.text(self.ui_language));
            if let Some(detail) = status.detail() {
                ui.small(detail);
            }
        });

        match effective_provider_base_url(&self.config) {
            Ok(base_url) => ui.small(format!(
                "{}: {}",
                self.tr("当前检查地址", "Check URL"),
                base_url
            )),
            Err(err) => ui.small(format!("{}: {err}", self.tr("当前检查地址", "Check URL"))),
        };
        ui.small(format!(
            "{}: {} | {}: {}ms",
            self.tr("执行 Lane", "Execution Lane"),
            canonical_provider_execution_mode(self.config.agent_execution_lane.as_str())
                .unwrap_or(DEFAULT_AGENT_EXECUTION_LANE),
            self.tr("连接超时", "Connect Timeout"),
            self.config.agent_provider_connect_timeout_ms.trim()
        ));

        ui.horizontal_wrapped(|ui| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if ui.button(self.tr("检查 Provider", "Check Provider")).clicked() {
                    self.check_local_provider();
                }
            }
            #[cfg(target_arch = "wasm32")]
            {
                ui.add_enabled(
                    false,
                    egui::Button::new(self.tr("检查 Provider", "Check Provider")),
                );
                ui.small(self.tr(
                    "Web 启动器暂不执行 native provider 兼容检查，请使用 native launcher。",
                    "Web launcher does not run the native provider compatibility check yet; use the native launcher.",
                ));
            }
        });

        if let Some(snapshot) = match &self.provider_check_status {
            ProviderCheckStatus::Ready(snapshot)
            | ProviderCheckStatus::Degraded(snapshot)
            | ProviderCheckStatus::Incompatible(snapshot) => Some(snapshot),
            _ => None,
        } {
            ui.small(format!(
                "{}: {} / {} / {}",
                self.tr("Provider", "Provider"),
                snapshot.provider_id,
                snapshot.name,
                snapshot.version,
            ));
            ui.small(format!(
                "{}: {} | {}: {}",
                self.tr("协议", "Protocol"),
                snapshot.protocol_version,
                self.tr("队列深度", "Queue Depth"),
                snapshot
                    .queue_depth
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_string())
            ));
            ui.small(format!(
                "{}: {}",
                self.tr("兼容状态", "Compatibility"),
                snapshot.compatibility_status.as_str()
            ));
            ui.small(format!(
                "{}: {}",
                self.tr("能力", "Capabilities"),
                if snapshot.capabilities.is_empty() {
                    "none".to_string()
                } else {
                    snapshot.capabilities.join(", ")
                }
            ));
            ui.small(format!(
                "{}: {}",
                self.tr("动作集", "Supported Actions"),
                if snapshot.supported_action_sets.is_empty() {
                    "none".to_string()
                } else {
                    snapshot.supported_action_sets.join(", ")
                }
            ));
            ui.small(format!(
                "{}: info={}ms health={}ms total={}ms",
                self.tr("检查延迟", "Check Latency"),
                snapshot.info_latency_ms,
                snapshot.health_latency_ms,
                snapshot.total_latency_ms,
            ));
            if let Some(fallback_reason) = snapshot.fallback_reason.as_deref()
                && !fallback_reason.trim().is_empty()
            {
                ui.small(format!(
                    "{}: {}",
                    self.tr("降级原因", "Fallback Reason"),
                    fallback_reason
                ));
            }
            if let Some(last_error) = snapshot.last_error.as_deref()
                && !last_error.trim().is_empty()
            {
                ui.small(format!(
                    "{}: {}",
                    self.tr("最近错误", "Last Error"),
                    last_error
                ));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn check_local_provider(&mut self) {
        self.provider_check_status = ProviderCheckStatus::Checking;
        let base_url = match effective_provider_base_url(&self.config) {
            Ok(value) => value,
            Err(err) => {
                self.provider_check_status = ProviderCheckStatus::InvalidConfig(err.clone());
                self.append_log(format!("provider check invalid config: {err}"));
                return;
            }
        };
        let timeout_ms = match parse_agent_provider_connect_timeout_ms(&self.config) {
            Ok(value) => value,
            Err(err) => {
                self.provider_check_status = ProviderCheckStatus::InvalidConfig(err.clone());
                self.append_log(format!("provider check invalid timeout: {err}"));
                return;
            }
        };
        match check_provider_http_provider(
            base_url.as_str(),
            Some(self.config.agent_provider_auth_token.as_str()),
            timeout_ms,
            self.config.agent_provider_transport.as_str(),
        ) {
            Ok(mut snapshot) => {
                if self.config.agent_decision_source.trim()
                    == AGENT_DIRECT_CONNECT_PROVIDER_MODE_ALIAS
                {
                    snapshot.fallback_reason = snapshot
                        .fallback_reason
                        .take()
                        .or_else(|| Some("provider_mode_alias:agent_direct_connect".to_string()));
                }
                let provider_id = snapshot.provider_id.clone();
                let version = snapshot.version.clone();
                let compatibility_status = snapshot.compatibility_status;
                let fallback_reason = snapshot.fallback_reason.clone();
                self.provider_check_status = match compatibility_status {
                    ProviderCompatibilityStatus::Ready => ProviderCheckStatus::Ready(snapshot),
                    ProviderCompatibilityStatus::Degraded => {
                        ProviderCheckStatus::Degraded(snapshot)
                    }
                    ProviderCompatibilityStatus::Incompatible => {
                        ProviderCheckStatus::Incompatible(snapshot)
                    }
                };
                self.append_log(format!(
                    "provider check succeeded: provider_id={provider_id} version={version} execution_mode={} timeout_ms={timeout_ms} compatibility_status={} fallback_reason={}",
                    canonical_provider_execution_mode(self.config.agent_execution_lane.as_str())
                        .unwrap_or(DEFAULT_AGENT_EXECUTION_LANE),
                    compatibility_status.as_str(),
                    fallback_reason.as_deref().unwrap_or("none")
                ));
            }
            Err(ProviderCheckError::InvalidConfig(detail)) => {
                self.provider_check_status = ProviderCheckStatus::InvalidConfig(detail.clone());
                self.append_log(format!("provider check invalid config: {detail}"));
            }
            Err(ProviderCheckError::Unauthorized(detail)) => {
                self.provider_check_status = ProviderCheckStatus::Unauthorized(detail.clone());
                self.append_log(format!("provider check unauthorized: {detail}"));
            }
            Err(ProviderCheckError::Unreachable(detail)) => {
                self.provider_check_status = ProviderCheckStatus::Unreachable(detail.clone());
                self.append_log(format!("provider check failed: {detail}"));
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn check_local_provider(&mut self) {
        self.provider_check_status = ProviderCheckStatus::Unsupported(
            "web launcher does not support the native provider compatibility check".to_string(),
        );
    }

    pub(super) fn render_config_field_by_id(
        &mut self,
        ui: &mut egui::Ui,
        field_id: &str,
        stack_text_fields: bool,
    ) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        let field = launcher_ui_fields_for_native().find(|field| field.id == field_id);
        #[cfg(target_arch = "wasm32")]
        let field = launcher_ui_fields_for_web().find(|field| field.id == field_id);

        if let Some(field) = field {
            self.render_config_field(ui, field, stack_text_fields);
            true
        } else {
            false
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "config_ui_provider_probe_tests.rs"]
mod provider_probe_tests;
