use super::*;
use std::fmt::Display;

impl Drop for ClientLauncherApp {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(mut running) = self.running.take() {
                let _ = stop_child_process(&mut running.child);
            }
        }
    }
}

impl ClientLauncherApp {
    pub(super) fn ui_ok_color() -> egui::Color32 {
        egui::Color32::from_rgb(62, 152, 92)
    }

    pub(super) fn ui_error_color() -> egui::Color32 {
        egui::Color32::from_rgb(188, 60, 60)
    }

    pub(super) fn ui_idle_color() -> egui::Color32 {
        egui::Color32::from_rgb(116, 119, 124)
    }

    fn ui_accent_color() -> egui::Color32 {
        egui::Color32::from_rgb(39, 113, 198)
    }

    fn dashboard_bg() -> egui::Color32 {
        egui::Color32::from_rgb(247, 249, 251)
    }

    pub(super) fn card_fill() -> egui::Color32 {
        egui::Color32::from_rgb(255, 255, 255)
    }

    pub(super) fn card_stroke() -> egui::Stroke {
        egui::Stroke::new(1.0, egui::Color32::from_rgb(224, 229, 236))
    }

    fn launcher_status_color(&self) -> egui::Color32 {
        match self.status {
            LauncherStatus::Running => Self::ui_ok_color(),
            LauncherStatus::Idle | LauncherStatus::Stopped => Self::ui_idle_color(),
            LauncherStatus::InvalidArgs
            | LauncherStatus::StartFailed
            | LauncherStatus::StopFailed
            | LauncherStatus::QueryFailed
            | LauncherStatus::Exited(_) => Self::ui_error_color(),
        }
    }

    fn open_current_game_page(&mut self) {
        let url = self.current_game_url();
        if let Err(err) = open_browser(url.as_str()) {
            self.append_log(format!("open browser failed: {err}"));
        } else {
            self.append_log(format!("open browser: {url}"));
        }
    }

    pub(super) fn compact_middle(value: &str, max_chars: usize) -> String {
        let char_count = value.chars().count();
        if char_count <= max_chars {
            return value.to_string();
        }
        let keep = max_chars.saturating_sub(1);
        let head = keep / 2;
        let tail = keep.saturating_sub(head);
        let prefix: String = value.chars().take(head).collect();
        let suffix: String = value
            .chars()
            .rev()
            .take(tail)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{prefix}…{suffix}")
    }

    fn render_status_chip(
        ui: &mut egui::Ui,
        label: &str,
        value: impl Display,
        color: egui::Color32,
    ) -> egui::Response {
        let value = value.to_string();
        let text = if label.is_empty() {
            value
        } else {
            format!("{label}  {value}")
        };
        let fill = if color == Self::ui_ok_color() {
            egui::Color32::from_rgb(229, 246, 235)
        } else if color == Self::ui_error_color() {
            egui::Color32::from_rgb(252, 235, 232)
        } else if color == egui::Color32::from_rgb(201, 146, 44) {
            egui::Color32::from_rgb(253, 244, 224)
        } else {
            egui::Color32::from_rgb(238, 244, 252)
        };
        // Measure before allocation so the wrapping parent moves the whole
        // read-only status item instead of squeezing its text into a column.
        let text = format!("• {text}");
        let galley =
            ui.painter()
                .layout_no_wrap(text.clone(), egui::FontId::proportional(10.5), color);
        let (rect, response) =
            ui.allocate_exact_size(galley.size() + egui::vec2(12.0, 4.0), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().rect(
                rect,
                0.0,
                fill,
                Self::card_stroke(),
                egui::StrokeKind::Inside,
            );
            ui.painter()
                .galley(rect.min + egui::vec2(6.0, 2.0), galley, color);
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), &text)
        });
        response
    }

    fn render_top_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
            Self::render_launcher_logo(ui);
            ui.label(egui::RichText::new("oasis7 Launcher").strong().size(14.0));
            ui.label(
                egui::RichText::new("v1.3.0")
                    .size(10.5)
                    .color(Self::ui_idle_color()),
            );
            ui.add_space(6.0);
            Self::render_status_chip(
                ui,
                self.tr("▶ 游戏", "▶ Game"),
                self.status.text(self.ui_language),
                self.launcher_status_color(),
            );
            ui.add_space(2.0);
            let response = Self::render_status_chip(
                ui,
                self.tr("◇ 区块链", "◇ Chain"),
                self.chain_runtime_status.text(self.ui_language),
                self.chain_runtime_status.color(),
            );
            if let Some(detail) = self.chain_runtime_status.detail() {
                response.on_hover_text(detail);
            }
            if is_provider_http_mode(&self.config) {
                ui.add_space(2.0);
                let provider_status = match &self.provider_check_status {
                    ProviderCheckStatus::Disabled => ProviderCheckStatus::Idle,
                    other => other.clone(),
                };
                let response = Self::render_status_chip(
                    ui,
                    self.tr("○ Provider", "○ Provider"),
                    provider_status.text(self.ui_language),
                    provider_status.color(),
                );
                if let Some(detail) = provider_status.detail() {
                    response.on_hover_text(detail);
                }
            }
            ui.add_space(8.0);
            if Self::toolbar_item(ui, self.ui_language.display_name()).clicked() {
                self.ui_language = match self.ui_language {
                    UiLanguage::ZhCn => UiLanguage::EnUs,
                    UiLanguage::EnUs => UiLanguage::ZhCn,
                };
            }
            let expert_mode = self.is_expert_mode();
            let expert_label = if expert_mode {
                self.tr("专家 ON", "Expert ON")
            } else {
                self.tr("专家 OFF", "Expert OFF")
            };
            if Self::toolbar_toggle(ui, expert_label, expert_mode).clicked() {
                self.set_expert_mode(!expert_mode);
            }
            if Self::toolbar_item(ui, self.tr("设置", "Settings")).clicked() {
                self.llm_settings_panel.open();
            }
            Self::toolbar_item(ui, self.tr("关于", "About"))
                .on_hover_text(self.tr("关于启动器", "About launcher"));
        });
    }

    fn render_launcher_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(Self::card_fill())
            .stroke(Self::card_stroke())
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.render_top_status_bar(ui);
            });
    }

    fn render_launcher_logo(ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
        let painter = ui.painter();
        let center = rect.center();
        painter.circle_filled(center, 11.0, egui::Color32::from_rgb(230, 242, 255));
        painter.circle_stroke(
            center,
            11.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(82, 133, 184)),
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "O7",
            egui::FontId::proportional(10.5),
            egui::Color32::from_rgb(36, 91, 150),
        );
    }

    fn toolbar_item(ui: &mut egui::Ui, label: &str) -> egui::Response {
        ui.add(
            egui::Button::new(egui::RichText::new(label).size(10.5))
                .fill(Self::card_fill())
                .stroke(Self::card_stroke())
                .min_size(egui::vec2(44.0, 22.0)),
        )
    }

    fn toolbar_toggle(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
        let fill = if enabled {
            egui::Color32::from_rgb(229, 246, 235)
        } else {
            Self::card_fill()
        };
        ui.add(
            egui::Button::new(egui::RichText::new(label).size(10.5))
                .fill(fill)
                .stroke(Self::card_stroke())
                .min_size(egui::vec2(58.0, 22.0)),
        )
    }

    fn render_next_action_deck(
        &mut self,
        ui: &mut egui::Ui,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
        game_running: bool,
        chain_running: bool,
    ) {
        let hint = resolve_next_task_hint(
            self.config.chain_enabled,
            game_required_issues,
            chain_required_issues,
            game_running,
            chain_running,
        );
        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(self.tr("下一步", "Next Action"))
                            .strong()
                            .color(egui::Color32::from_rgb(56, 86, 101)),
                    );
                    ui.label(
                        egui::RichText::new(self.next_task_hint_text(hint))
                            .strong()
                            .size(17.0),
                    );
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    match hint {
                        NextTaskHint::FixChainConfig => {
                            if ui
                                .button(self.tr("修复区块链配置", "Fix Chain Config"))
                                .clicked()
                            {
                                self.record_guided_quick_action_click();
                                self.open_chain_config_guide();
                            }
                        }
                        NextTaskHint::StartChain => {
                            if ui
                                .button(self.tr("启动区块链", "Start Blockchain"))
                                .clicked()
                            {
                                self.record_guided_quick_action_click();
                                self.handle_start_chain_click(chain_required_issues);
                            }
                        }
                        NextTaskHint::FixGameConfig => {
                            if ui
                                .button(self.tr("修复游戏配置", "Fix Game Config"))
                                .clicked()
                            {
                                self.record_guided_quick_action_click();
                                self.open_game_config_guide();
                            }
                        }
                        NextTaskHint::StartGame => {
                            if ui.button(self.tr("启动游戏", "Start Game")).clicked() {
                                self.record_guided_quick_action_click();
                                self.handle_start_game_click(game_required_issues);
                            }
                        }
                        NextTaskHint::OpenGamePage => {
                            if ui.button(self.tr("打开游戏页", "Open Game Page")).clicked() {
                                self.record_guided_quick_action_click();
                                self.open_current_game_page();
                            }
                        }
                    }
                    let demo_running = matches!(
                        self.demo_mode_phase,
                        DemoModePhase::StartChainRequested
                            | DemoModePhase::WaitChainReady
                            | DemoModePhase::StartGameRequested
                            | DemoModePhase::WaitGameRunning
                    );
                    if ui
                        .add_enabled(
                            !demo_running,
                            egui::Button::new(
                                self.tr("演示模式一键启动", "Demo Mode One-Click Start"),
                            ),
                        )
                        .clicked()
                    {
                        self.record_guided_quick_action_click();
                        self.start_demo_mode_one_click();
                    }
                    ui.small(format!(
                        "{}={}",
                        self.tr("演示模式", "Demo"),
                        self.demo_mode_phase_text()
                    ));
                });
            });
        });
    }

    fn render_launcher_command_deck(
        &mut self,
        ui: &mut egui::Ui,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
        game_running: bool,
        chain_running: bool,
    ) {
        let can_click_start_game = !game_running;
        let can_click_start_chain = self.config.chain_enabled && !chain_running;
        let has_saved_profile = self.ux_state.last_successful_config.is_some();

        egui::Frame::group(ui.style())
            .fill(Self::card_fill())
            .stroke(Self::card_stroke())
            .inner_margin(egui::Margin::same(9))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(self.tr("快捷命令", "Command Dock"))
                        .strong()
                        .size(14.0),
                );
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    if Self::command_tile(
                        ui,
                        "🔗",
                        self.tr("启动链", "Chain"),
                        can_click_start_chain,
                    )
                    .clicked()
                    {
                        self.handle_start_chain_click(chain_required_issues);
                    }
                    if Self::command_tile(ui, "▶", self.tr("游戏", "Game"), can_click_start_game)
                        .clicked()
                    {
                        self.handle_start_game_click(game_required_issues);
                    }
                    if Self::command_tile(ui, "↗", self.tr("页面", "Page"), true).clicked() {
                        self.open_current_game_page();
                    }
                    if Self::command_tile(ui, "⚙", self.tr("配置", "Config"), true).clicked() {
                        self.config_window_open = true;
                    }
                    if !self.is_expert_mode()
                        && Self::command_tile(ui, "?", self.tr("引导", "Guide"), true).clicked()
                    {
                        self.open_onboarding_manual();
                    }
                    if Self::command_tile(ui, "◇", self.tr("洞察", "Insights"), true).clicked()
                    {
                        self.guidance_insights_open = true;
                    }
                    ui.menu_button(self.tr("更多", "More"), |ui| {
                        if ui
                            .add_enabled(
                                chain_running,
                                egui::Button::new(self.tr("停止区块链", "Stop Chain")),
                            )
                            .clicked()
                        {
                            self.stop_chain_process();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                game_running,
                                egui::Button::new(self.tr("停止游戏", "Stop Game")),
                            )
                            .clicked()
                        {
                            self.stop_process();
                            ui.close();
                        }
                        if ui.button(self.tr("重置引导", "Reset Guide")).clicked() {
                            self.reset_onboarding();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                has_saved_profile,
                                egui::Button::new(self.tr("恢复配置", "Restore Config")),
                            )
                            .clicked()
                        {
                            self.restore_last_successful_config_profile();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                has_saved_profile,
                                egui::Button::new(self.tr("清空配置", "Clear Config")),
                            )
                            .clicked()
                        {
                            self.clear_last_successful_config_profile();
                            ui.close();
                        }
                        let can_reset_demo = matches!(
                            self.demo_mode_phase,
                            DemoModePhase::Done | DemoModePhase::Failed
                        );
                        if ui
                            .add_enabled(
                                can_reset_demo,
                                egui::Button::new(self.tr("重置演示", "Reset Demo")),
                            )
                            .clicked()
                        {
                            self.reset_demo_mode();
                            ui.close();
                        }
                        if ui.button(self.tr("设置", "Settings")).clicked() {
                            self.llm_settings_panel.open();
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.is_feedback_available(),
                                egui::Button::new(self.tr("反馈", "Feedback")),
                            )
                            .clicked()
                        {
                            self.feedback_window_open = true;
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.is_feedback_available(),
                                egui::Button::new(self.tr("转账", "Transfer")),
                            )
                            .clicked()
                        {
                            self.transfer_window_open = true;
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.is_feedback_available(),
                                egui::Button::new(self.tr("浏览器", "Explorer")),
                            )
                            .clicked()
                        {
                            self.explorer_window_open = true;
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                self.config.chain_enabled,
                                egui::Button::new(self.tr("Peer 窗口", "Peers")),
                            )
                            .clicked()
                        {
                            self.peer_details_window_open = true;
                            ui.close();
                        }
                        if ui.button(self.tr("清空日志", "Clear Log")).clicked() {
                            self.logs.clear();
                            ui.close();
                        }
                    });
                });
                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "↻  {}",
                                self.tr("一键修复", "One-Click Repair")
                            ))
                            .size(12.0),
                        )
                        .fill(egui::Color32::from_rgb(255, 255, 255))
                        .stroke(Self::card_stroke())
                        .min_size(egui::vec2(ui.available_width(), 26.0)),
                    )
                    .clicked()
                {
                    self.open_game_config_guide();
                }
            });
    }

    fn command_tile(ui: &mut egui::Ui, icon: &str, label: &str, enabled: bool) -> egui::Response {
        let text = egui::RichText::new(format!("{icon}\n{label}"))
            .size(10.0)
            .color(if enabled {
                Self::ui_accent_color()
            } else {
                Self::ui_idle_color()
            });
        ui.add_enabled(
            enabled,
            egui::Button::new(text)
                .fill(Self::card_fill())
                .stroke(Self::card_stroke())
                .min_size(egui::vec2(50.0, 46.0)),
        )
    }

    fn render_launcher_footer_band(&self, ui: &mut egui::Ui) {
        let game_url = self.current_game_url();
        let profile = self.config.agent_provider_profile.trim();
        let profile = if profile.is_empty() {
            self.tr("default", "default")
        } else {
            profile
        };
        egui::Frame::new()
            .fill(Self::card_fill())
            .stroke(Self::card_stroke())
            .inner_margin(egui::Margin::symmetric(9, 5))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("● {}", self.tr("就绪", "Ready")))
                            .strong()
                            .size(10.5)
                            .color(Self::ui_ok_color()),
                    );
                    ui.add_space(34.0);
                    ui.small(format!("{}: {profile}", self.tr("配置", "Profile")));
                    ui.separator();
                    ui.small(format!(
                        "{}: {}",
                        self.tr("网络", "Network"),
                        self.config.chain_network_tier.trim()
                    ));
                    ui.separator();
                    ui.small(format!(
                        "{}: {}",
                        self.tr("游戏地址", "Game URL"),
                        Self::compact_middle(game_url.as_str(), 34)
                    ))
                    .on_hover_text(game_url);
                    ui.separator();
                    ui.small(format!(
                        "{}  {}",
                        self.tr("本地时间", "Local Time"),
                        self.tr("强 Strong", "Strong")
                    ));
                });
            });
    }

    fn render_launcher_logs_panel(&self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style())
            .fill(Self::card_fill())
            .stroke(Self::card_stroke())
            .inner_margin(egui::Margin::same(9))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(self.tr("运行日志", "Runtime Log"))
                            .strong()
                            .size(14.0),
                    );
                    Self::log_filter_chip(ui, self.tr("全部", "All"), true);
                    Self::log_filter_chip(ui, self.tr("信息", "Info"), false);
                    Self::log_filter_chip(ui, self.tr("警告", "Warn"), false);
                    Self::log_filter_chip(ui, self.tr("错误", "Error"), false);
                });
                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.set_height(16.0);
                    ui.small(egui::RichText::new("LEVEL").color(Self::ui_idle_color()));
                    ui.add_space(36.0);
                    ui.small(egui::RichText::new("SOURCE").color(Self::ui_idle_color()));
                    ui.add_space(48.0);
                    ui.small(egui::RichText::new("MESSAGE").color(Self::ui_idle_color()));
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(112.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.logs {
                            ui.horizontal(|ui| {
                                let level = if line.contains("ERROR") || line.contains("error") {
                                    ("ERR", Self::ui_error_color())
                                } else if line.contains("WARN") || line.contains("warn") {
                                    ("WARN", egui::Color32::from_rgb(201, 146, 44))
                                } else {
                                    ("INFO", Self::ui_ok_color())
                                };
                                let (source, message) = Self::log_source_and_message(line.as_str());
                                ui.set_height(22.0);
                                Self::render_log_badge(ui, level.0, level.1);
                                ui.add_space(4.0);
                                ui.allocate_ui_with_layout(
                                    egui::vec2(82.0, 18.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.small(Self::compact_middle(source, 15));
                                    },
                                );
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), 18.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.small(Self::compact_middle(message, 58))
                                            .on_hover_text(line);
                                    },
                                );
                            });
                        }
                    });
            });
    }

    fn render_log_badge(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
        let fill = if color == Self::ui_ok_color() {
            egui::Color32::from_rgb(229, 246, 235)
        } else if color == Self::ui_error_color() {
            egui::Color32::from_rgb(252, 235, 232)
        } else {
            egui::Color32::from_rgb(253, 244, 224)
        };
        egui::Frame::new()
            .fill(fill)
            .stroke(Self::card_stroke())
            .inner_margin(egui::Margin::symmetric(6, 2))
            .show(ui, |ui| {
                ui.small(egui::RichText::new(label).strong().color(color));
            });
    }

    fn log_source_and_message(line: &str) -> (&str, &str) {
        if let Some(rest) = line.strip_prefix('[')
            && let Some(end) = rest.find(']')
        {
            let source = &rest[..end];
            let message = rest[end + 1..].trim();
            return (source, message);
        }
        if line.contains("control plane") {
            ("control", line)
        } else if line.contains("launcher") {
            ("launcher", line)
        } else {
            ("system", line)
        }
    }

    fn log_filter_chip(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
        let fill = if active {
            egui::Color32::from_rgb(232, 242, 255)
        } else {
            Self::card_fill()
        };
        ui.add(
            egui::Button::new(egui::RichText::new(label).size(10.0))
                .fill(fill)
                .stroke(Self::card_stroke())
                .min_size(egui::vec2(34.0, 20.0)),
        )
    }

    fn render_normal_dashboard(
        &mut self,
        ui: &mut egui::Ui,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
        game_running: bool,
        chain_running: bool,
    ) {
        let width = ui.available_width();
        if width >= 840.0 {
            let left_width = (width * 0.30).clamp(236.0, 292.0);
            let right_width = (width * 0.30).clamp(236.0, 292.0);
            let middle_width = (width - left_width - right_width - 32.0).max(300.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(left_width);
                    self.render_startup_preflight_checklist(
                        ui,
                        game_required_issues,
                        chain_required_issues,
                    );
                });
                ui.vertical(|ui| {
                    ui.set_width(middle_width);
                    self.render_task_flow_cards(
                        ui,
                        game_required_issues,
                        chain_required_issues,
                        game_running,
                        chain_running,
                    );
                });
                ui.vertical(|ui| {
                    ui.set_width(right_width);
                    self.render_runtime_overview_panel(
                        ui,
                        game_required_issues,
                        chain_required_issues,
                    );
                });
            });
        } else if width >= 700.0 {
            self.render_task_flow_cards(
                ui,
                game_required_issues,
                chain_required_issues,
                game_running,
                chain_running,
            );
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                let column_width = (width - 8.0) / 2.0;
                ui.vertical(|ui| {
                    ui.set_width(column_width);
                    self.render_startup_preflight_checklist(
                        ui,
                        game_required_issues,
                        chain_required_issues,
                    );
                });
                ui.vertical(|ui| {
                    ui.set_width(column_width);
                    self.render_runtime_overview_panel(
                        ui,
                        game_required_issues,
                        chain_required_issues,
                    );
                });
            });
        } else {
            self.render_task_flow_cards(
                ui,
                game_required_issues,
                chain_required_issues,
                game_running,
                chain_running,
            );
            ui.add_space(6.0);
            self.render_startup_preflight_checklist(
                ui,
                game_required_issues,
                chain_required_issues,
            );
            ui.add_space(6.0);
            self.render_runtime_overview_panel(ui, game_required_issues, chain_required_issues);
        }
    }

    fn render_bottom_dashboard(
        &mut self,
        ui: &mut egui::Ui,
        game_required_issues: &[ConfigIssue],
        chain_required_issues: &[ConfigIssue],
        game_running: bool,
        chain_running: bool,
    ) {
        let width = ui.available_width();
        if width >= 820.0 {
            let dock_width = (width * 0.61).clamp(430.0, 620.0);
            let log_width = (width - dock_width - 10.0).max(300.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(dock_width);
                    self.render_launcher_command_deck(
                        ui,
                        game_required_issues,
                        chain_required_issues,
                        game_running,
                        chain_running,
                    );
                    if self.is_expert_mode() {
                        self.render_disabled_action_ctas(
                            ui,
                            game_required_issues,
                            chain_required_issues,
                            chain_running,
                        );
                    }
                });
                ui.vertical(|ui| {
                    ui.set_width(log_width);
                    self.render_launcher_logs_panel(ui);
                });
            });
        } else {
            self.render_launcher_command_deck(
                ui,
                game_required_issues,
                chain_required_issues,
                game_running,
                chain_running,
            );
            if self.is_expert_mode() {
                self.render_disabled_action_ctas(
                    ui,
                    game_required_issues,
                    chain_required_issues,
                    chain_running,
                );
            }
            ui.add_space(6.0);
            self.render_launcher_logs_panel(ui);
        }
    }
}

impl eframe::App for ClientLauncherApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let color = Self::dashboard_bg();
        color.to_normalized_gamma_f32()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_process();
        self.poll_chain_process();
        self.maybe_auto_start_chain();
        self.update_chain_runtime_status();
        #[cfg(target_arch = "wasm32")]
        launcher_test_hook_web::sync_launcher_test_hook(self);
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let game_required_issues = collect_required_config_issues(&self.config);
        let chain_required_issues = collect_chain_required_config_issues(&self.config);
        let game_running = matches!(self.status, LauncherStatus::Running);
        let chain_running = matches!(
            self.chain_runtime_status,
            ChainRuntimeStatus::Starting | ChainRuntimeStatus::Ready
        );
        self.maybe_save_last_successful_config_profile(game_running);
        self.maybe_open_onboarding_on_first_visit(
            &game_required_issues,
            &chain_required_issues,
            game_running,
            chain_running,
        );
        if self.onboarding_state.completed {
            self.maybe_open_startup_guide_on_first_check(
                &game_required_issues,
                &chain_required_issues,
            );
        }
        self.advance_demo_mode(
            &game_required_issues,
            &chain_required_issues,
            game_running,
            chain_running,
        );

        #[cfg(not(target_arch = "wasm32"))]
        let mut rendered_screenshot_modal_preview = false;
        #[cfg(target_arch = "wasm32")]
        let rendered_screenshot_modal_preview = false;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Self::dashboard_bg())
                    .inner_margin(egui::Margin::same(8)),
            )
            .show_inside(ui, |ui| {
                self.render_launcher_header(ui);
                ui.add_space(6.0);
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(modal) = Self::screenshot_modal_name() {
                    ui.horizontal_top(|ui| {
                        let preview_width =
                            Self::screenshot_modal_preview_width(modal.as_str(), ui.available_width());
                        let side_space = ((ui.available_width() - preview_width) / 2.0).max(0.0);
                        ui.add_space(side_space);
                        ui.vertical(|ui| {
                            ui.set_width(preview_width);
                            rendered_screenshot_modal_preview = self.render_screenshot_modal_preview(
                                ui,
                                modal.as_str(),
                                &game_required_issues,
                                &chain_required_issues,
                                game_running,
                                chain_running,
                            );
                        });
                    });
                    if rendered_screenshot_modal_preview {
                        return;
                    }
                }
                self.render_onboarding_reminder_banner(ui);
                if self.should_show_onboarding_reminder() {
                    ui.add_space(6.0);
            }

            if self.is_expert_mode() {
                self.render_next_action_deck(
                    ui,
                    &game_required_issues,
                    &chain_required_issues,
                    game_running,
                    chain_running,
                );
                ui.add_space(6.0);
                ui.small(self.tr(
                    "专家模式已开启：任务卡片收起，但下一步和状态仍保留。",
                    "Expert mode enabled: task cards are hidden, while next action and status remain visible.",
                ));
                self.render_startup_error_cards(ui, &game_required_issues, &chain_required_issues);
                ui.add_space(6.0);
                ui.collapsing(self.tr("完整运行诊断", "Full Runtime Diagnostics"), |ui| {
                    self.render_config_validation_summary(
                        ui,
                        &game_required_issues,
                        &chain_required_issues,
                    );
                    self.render_chain_p2p_summary(ui);
                    self.render_chain_observability_summary(ui);
                });
                ui.add_space(6.0);
            } else {
                self.render_normal_dashboard(
                    ui,
                    &game_required_issues,
                    &chain_required_issues,
                    game_running,
                    chain_running,
                );
            }
            ui.add_space(6.0);

            self.render_bottom_dashboard(
                ui,
                &game_required_issues,
                &chain_required_issues,
                game_running,
                chain_running,
            );
            ui.add_space(6.0);
            self.render_launcher_footer_band(ui);
            });

        if !rendered_screenshot_modal_preview {
            self.show_config_window(&ctx, &game_required_issues, &chain_required_issues);
            self.show_onboarding_window(
                &ctx,
                &game_required_issues,
                &chain_required_issues,
                game_running,
                chain_running,
            );
            self.show_guidance_insights_window(&ctx);
            self.show_startup_guide_window(&ctx, &game_required_issues, &chain_required_issues);
            self.llm_settings_panel
                .show(&ctx, self.ui_language, &mut self.config);
            self.show_feedback_window(&ctx);
            self.show_transfer_window(&ctx);
            self.show_peer_details_window(&ctx);
            self.show_explorer_window(&ctx);
        }
    }
}

#[cfg(test)]
#[path = "main_header_layout_tests.rs"]
mod header_layout_tests;
