# Viewer 全功能覆盖派生目录

本目录只导航当前源与具体设计关系，不是产品PRD、规则authority或完成百分比。固定源为PR #5277 `113868200d1f91bef1008babb689bd257f241428`。

## 口径与边界

105个源码实现条目＝29常规＋76按权限/数据/capability条件显示；另1 QA与1禁用占位。条件已实现与未来目标分开。183模块/50JSX仅是字面import闭包，动态WASM、凭据DOM、键盘路径另核。218字面交互节点归属不证明动态DTO或action穷尽。

专题169个REQ标题（含3个Trace消费别名）与根101个独立合同节是要求视角，不作功能分母；根上下文也不因无独立ID失效。专业稿只承接其明确义务，未绘制状态保留gap。

## 专业 companion

- [账户、Agent 指导与模型连接](viewer-account-agent-uiux.design.md)：当前源分组，具体frame逐行核对。
- [当前玩法明细与动态动作](viewer-gameplay-details-uiux.design.md)：当前源分组，具体frame逐行核对。
- [十类报价表单与状态](viewer-quote-forms-uiux.design.md)：当前源分组，具体frame逐行核对。
- [世界、导航、空间与诊断](viewer-world-navigation-uiux.design.md)：当前源分组，具体frame逐行核对。
- [经济与工业条件目标](viewer-economic-targets-uiux.design.md)：条件目标，不计当前实现数量。
- [治理、社会与季节条件目标](viewer-governance-targets-uiux.design.md)：条件目标，不计当前实现数量。

## 图稿阅读方式

全功能稿中的并排状态、放大的 Inspector/表单和字段表是局部交接展开，不是默认 Player 首屏布局。经济、治理、玩法稿保留的窄幅 World 是位置参照，不能据其面积比例实现常驻侧栏或压缩世界；实际 shell 遵循[页面分区与首读](viewer-page-module-design-2026-06-18.design.md#detailed-uiux-handoff)和单一 Next Move。技术字段/枚举用于对应源合同，玩家表面先呈现可读状态、原因和下一步，原字段只在获准详情中按需披露。局部图没有画出的完整 shell、收起路径和响应式状态仍须另行验证。 [共享组合稿15](assets/ui-ux/15-tablet-text-reflow.svg)实际展示平板 world+Command 详情和文字重排；仅经济替代方案待确认状态已组合，其他主题及状态不能由此推定已绘制。

## 当前源与设计表示

字段、动作、条件及精确源行见[派生JSON](assets/ui-ux/full-function/coverage-catalog.json)。下表只是入口，不能凭一个normal frame判整个功能状态覆盖。

| ID / 表面 | 当前状态 | 精确设计条款 | frame / representation |
|---|---|---|---|
| R-NAV-01 World / Targets / Command 路由与返回焦点 | current | [R-NAV-01](viewer-world-navigation-uiux.design.md#r-nav-01) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg) / normal_frame；代表状态 |
| R-NAV-02 More 次级诊断入口 | current | [R-NAV-02](viewer-world-navigation-uiux.design.md#r-nav-02) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg) / state_snippet；代表状态 |
| R-NAV-03 入口菜单、locale 与账号安全披露 | current | [R-NAV-03](viewer-world-navigation-uiux.design.md#r-nav-03) | [具体稿](assets/ui-ux/full-function/account-agent/10-entry-gate-focus.svg) / normal_frame；代表状态 |
| R-AUTH-01 Passkey 免邮箱注册 | conditional_current | [R-AUTH-01](viewer-account-agent-uiux.design.md#r-auth-01) | [具体稿](assets/ui-ux/full-function/account-agent/01-account.svg) / normal_frame；代表状态 |
| R-AUTH-02 Passkey 登录 | conditional_current | [R-AUTH-02](viewer-account-agent-uiux.design.md#r-auth-02) | [具体稿](assets/ui-ux/full-function/account-agent/01-account.svg) / normal_frame；代表状态 |
| R-AUTH-03 世界进入与显式接管 | conditional_current | [R-AUTH-03](viewer-account-agent-uiux.design.md#r-auth-03) | [具体稿](assets/ui-ux/full-function/account-agent/01-account.svg) / normal_frame；代表状态 |
| R-AUTH-04 凭证列表、移除与备用通行密钥 | conditional_current | [R-AUTH-04](viewer-account-agent-uiux.design.md#r-auth-04) | [具体稿](assets/ui-ux/full-function/account-agent/01-account.svg) / normal_frame；代表状态 |
| R-AUTH-05 会话列表、单会话撤销和退出 | conditional_current | [R-AUTH-05](viewer-account-agent-uiux.design.md#r-auth-05) | [具体稿](assets/ui-ux/full-function/account-agent/01-account.svg) / normal_frame；代表状态 |
| R-AUTH-06 未知认证原操作只读核对 | conditional_current | [R-AUTH-06](viewer-account-agent-uiux.design.md#r-auth-06) | [具体稿](assets/ui-ux/full-function/account-agent/02-recovery.svg) / normal_frame；代表状态 |
| R-AUTH-07 导入 Ed25519 账号密钥登录、绑定及确认 | conditional_current | [R-AUTH-07](viewer-account-agent-uiux.design.md#r-auth-07) | [具体稿](assets/ui-ux/full-function/account-agent/01-account.svg) / normal_frame；代表状态 |
| R-AUTH-08 邮箱登录、绑定/替换与首个 Passkey 升级 | conditional_current | [R-AUTH-08](viewer-account-agent-uiux.design.md#r-auth-08) | [具体稿](assets/ui-ux/full-function/account-agent/02-recovery.svg) / normal_frame；代表状态 |
| R-AUTH-09 固定候选恢复申请、查询、批准/取消及完成 | conditional_current | [R-AUTH-09](viewer-account-agent-uiux.design.md#r-auth-09) | [具体稿](assets/ui-ux/full-function/account-agent/11-recovery-lookup.svg) / normal_frame；代表状态 |
| R-AUTH-10 能力探测与 legacy 404 回退 | conditional_current | [R-AUTH-10](viewer-account-agent-uiux.design.md#r-auth-10) | [具体稿](assets/ui-ux/full-function/account-agent/01-account.svg) / normal_frame；代表状态 |
| R-AUTH-11 Legacy 托管邮箱登录表单 | conditional_current | [R-AUTH-11](viewer-account-agent-uiux.design.md#r-auth-11) | [具体稿](assets/ui-ux/full-function/account-agent/07-legacy-session.svg) / normal_frame；代表状态 |
| R-AUTH-12 登录强制 gate 与焦点隔离 | conditional_current | [R-AUTH-12](viewer-account-agent-uiux.design.md#r-auth-12) | [具体稿](assets/ui-ux/full-function/account-agent/10-entry-gate-focus.svg) / normal_frame；代表状态 |
| R-AUTH-13 进入会话摘要、原资格恢复及释放 | conditional_current | [R-AUTH-13](viewer-account-agent-uiux.design.md#r-auth-13) | [具体稿](assets/ui-ux/full-function/account-agent/07-legacy-session.svg) / normal_frame；代表状态 |
| R-AUTH-QA 受限免邮箱 QA 测试登录 | qa_only | [R-AUTH-QA](viewer-account-agent-uiux.design.md#r-auth-qa) | text_spec_only；视觉gap |
| R-BYOK-01 准备本机玩家、既有 Agent 与任务 | conditional_current | [R-BYOK-01](viewer-account-agent-uiux.design.md#r-byok-01) | [具体稿](assets/ui-ux/full-function/account-agent/04-byok-entry.svg) / normal_frame；代表状态 |
| R-BYOK-02 核对任务与登记浏览器执行身份 | conditional_current | [R-BYOK-02](viewer-account-agent-uiux.design.md#r-byok-02) | [具体稿](assets/ui-ux/full-function/account-agent/04-byok-entry.svg) / normal_frame；代表状态 |
| R-BYOK-03 浏览器 Key 保存与存储同意 | conditional_current | [R-BYOK-03](viewer-account-agent-uiux.design.md#r-byok-03) | [具体稿](assets/ui-ux/full-function/account-agent/05-byok-model.svg) / normal_frame；代表状态 |
| R-BYOK-04 有限付费模型检查 | conditional_current | [R-BYOK-04](viewer-account-agent-uiux.design.md#r-byok-04) | [具体稿](assets/ui-ux/full-function/account-agent/05-byok-model.svg) / normal_frame；代表状态 |
| R-BYOK-05 确认运行、停止新决策与原件查询 | conditional_current | [R-BYOK-05](viewer-account-agent-uiux.design.md#r-byok-05) | [具体稿](assets/ui-ux/full-function/account-agent/05-byok-model.svg) / normal_frame；代表状态 |
| R-BYOK-06 断开并忘记本机 Key | conditional_current | [R-BYOK-06](viewer-account-agent-uiux.design.md#r-byok-06) | [具体稿](assets/ui-ux/full-function/account-agent/05-byok-model.svg) / normal_frame；代表状态 |
| R-BYOK-07 旧托管模型连接管理 | conditional_current | [R-BYOK-07](viewer-account-agent-uiux.design.md#r-byok-07) | [具体稿](assets/ui-ux/full-function/account-agent/05-byok-model.svg) / normal_frame；代表状态 |
| R-CMD-01 当前目标、可控性与连续性摘要 | current | [R-CMD-01](viewer-account-agent-uiux.design.md#r-cmd-01) | [具体稿](assets/ui-ux/full-function/account-agent/03-command.svg) / normal_frame；代表状态 |
| R-CMD-02 多行 Agent Chat 与反馈 | conditional_current | [R-CMD-02](viewer-account-agent-uiux.design.md#r-cmd-02) | [具体稿](assets/ui-ux/full-function/account-agent/03-command.svg) / normal_frame；代表状态 |
| R-CMD-03 已认证 canonical goal 回读 | conditional_current | [R-CMD-03](viewer-account-agent-uiux.design.md#r-cmd-03) | [具体稿](assets/ui-ux/full-function/account-agent/08-command-context.svg) / normal_frame；代表状态 |
| R-CMD-04 Agent 当前活动 | current | [R-CMD-04](viewer-account-agent-uiux.design.md#r-cmd-04) | [具体稿](assets/ui-ux/full-function/account-agent/08-command-context.svg) / normal_frame；代表状态 |
| R-CMD-05 Agent 意图、理由与回执状态 | current | [R-CMD-05](viewer-account-agent-uiux.design.md#r-cmd-05) | [具体稿](assets/ui-ux/full-function/account-agent/08-command-context.svg) / normal_frame；代表状态 |
| R-CMD-06 Agent Context Lite | current | [R-CMD-06](viewer-account-agent-uiux.design.md#r-cmd-06) | [具体稿](assets/ui-ux/full-function/account-agent/08-command-context.svg) / normal_frame；代表状态 |
| R-CMD-07 安全替代方案比较、确认、执行及恢复 | conditional_current | [R-CMD-07](viewer-account-agent-uiux.design.md#r-cmd-07) | [具体稿](assets/ui-ux/full-function/account-agent/09-safe-alternative.svg) / normal_frame；代表状态 |
| R-CMD-08 强认证批准码 | conditional_current | [R-CMD-08](viewer-account-agent-uiux.design.md#r-cmd-08) | [具体稿](assets/ui-ux/full-function/account-agent/03-command.svg) / normal_frame；代表状态 |
| R-CMD-09 Prompt override 草稿与预览 | conditional_current | [R-CMD-09](viewer-account-agent-uiux.design.md#r-cmd-09) | [具体稿](assets/ui-ux/full-function/account-agent/03-command.svg) / normal_frame；代表状态 |
| R-CMD-10 Prompt apply 与反馈恢复 | conditional_current | [R-CMD-10](viewer-account-agent-uiux.design.md#r-cmd-10) | [具体稿](assets/ui-ux/full-function/account-agent/03-command.svg) / normal_frame；代表状态 |
| R-CMD-11 Prompt 版本回滚 | conditional_current | [R-CMD-11](viewer-account-agent-uiux.design.md#r-cmd-11) | [具体稿](assets/ui-ux/full-function/account-agent/03-command.svg) / normal_frame；代表状态 |
| R-CMD-12 资产 / 治理后置通道占位 | unimplemented_surface | [R-CMD-12](viewer-account-agent-uiux.design.md#r-cmd-12) | [具体稿](assets/ui-ux/full-function/account-agent/08-command-context.svg) / normal_frame；代表状态 |
| R-TARGET-01 授权 Targets 搜索与分类计数 | current | [R-TARGET-01](viewer-world-navigation-uiux.design.md#r-target-01) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg) / normal_frame；代表状态 |
| R-TARGET-02 Agent、地点和模块选择 | current | [R-TARGET-02](viewer-world-navigation-uiux.design.md#r-target-02) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg) / normal_frame；代表状态 |
| R-TARGET-03 分页窗口与完整授权搜索 | current | [R-TARGET-03](viewer-world-navigation-uiux.design.md#r-target-03) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg) / state_snippet；代表状态 |
| R-TARGET-04 空实体与同步恢复指引 | conditional_current | [R-TARGET-04](viewer-world-navigation-uiux.design.md#r-target-04) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg) / state_snippet；代表状态 |
| R-TARGET-05 原始对象、模块与快照明细 | current | [R-TARGET-05](viewer-world-navigation-uiux.design.md#r-target-05) | [具体稿](assets/ui-ux/full-function/gameplay/frames/selection-empty.svg) / normal_frame；代表状态 |
| R-WORLD-01 世界舞台身份、尺度与连接摘要 | current | [R-WORLD-01](viewer-world-navigation-uiux.design.md#r-world-01) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W01-world-targets.svg) / normal_frame；代表状态 |
| R-WORLD-02 单一 Next Move / Objective / Leverage HUD | current | [R-WORLD-02](viewer-world-navigation-uiux.design.md#r-world-02) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W16-hud-gates.svg) / state_snippet；代表状态 |
| R-WORLD-03 原行动独立回执 | current | [R-WORLD-03](viewer-world-navigation-uiux.design.md#r-world-03) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W17-original-receipt.svg) / state_snippet；代表状态 |
| R-WORLD-04 Bevy canvas、命中与输入桥接 | current | [R-WORLD-04](viewer-world-navigation-uiux.design.md#r-world-04) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg) / state_snippet；代表状态 |
| R-WORLD-05 电影视图、最大化与退出 | current | [R-WORLD-05](viewer-world-navigation-uiux.design.md#r-world-05) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg) / state_snippet；代表状态 |
| R-WORLD-06 聚焦小地图、Command 与诊断抽屉 | current | [R-WORLD-06](viewer-world-navigation-uiux.design.md#r-world-06) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg) / state_snippet；代表状态 |
| R-WORLD-07 热点焦点、键盘与 tooltip | current | [R-WORLD-07](viewer-world-navigation-uiux.design.md#r-world-07) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W13-hotspots.svg) / state_snippet；代表状态 |
| R-WORLD-08 Renderer 原生对象命中目标 | current | [R-WORLD-08](viewer-world-navigation-uiux.design.md#r-world-08) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W13-hotspots.svg) / state_snippet；代表状态 |
| R-WORLD-09 视觉图层、图例及稀疏世界说明 | current | [R-WORLD-09](viewer-world-navigation-uiux.design.md#r-world-09) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W20-legend-sparse.svg) / state_snippet；代表状态 |
| R-SPACE-01 获准空间搜索、总览和返回主目标 | conditional_current | [R-SPACE-01](viewer-world-navigation-uiux.design.md#r-space-01) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg) / state_snippet；代表状态 |
| R-SPACE-02 对象检查、定位、区域探索和跟随 | conditional_current | [R-SPACE-02](viewer-world-navigation-uiux.design.md#r-space-02) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W02-spatial-states.svg) / state_snippet；代表状态 |
| R-SPACE-03 只读对象位置与来源检查 | conditional_current | [R-SPACE-03](viewer-world-navigation-uiux.design.md#r-space-03) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg) / state_snippet；代表状态 |
| R-SPACE-04 授权路线与端点检查 | conditional_current | [R-SPACE-04](viewer-world-navigation-uiux.design.md#r-space-04) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W03-routes-events.svg) / state_snippet；代表状态 |
| R-SPACE-05 工业区域聚合与下钻 | conditional_current | [R-SPACE-05](viewer-world-navigation-uiux.design.md#r-space-05) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W03-routes-events.svg) / state_snippet；代表状态 |
| R-SPACE-06 近期空间事件与 Feed 定位 | conditional_current | [R-SPACE-06](viewer-world-navigation-uiux.design.md#r-space-06) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W03-routes-events.svg) / state_snippet；代表状态 |
| R-FEED-01 World Feed 分类、新鲜度与事件披露 | current | [R-FEED-01](viewer-world-navigation-uiux.design.md#r-feed-01) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg) / state_snippet；代表状态 |
| R-FEED-02 重载权威快照与重试 Feed | conditional_current | [R-FEED-02](viewer-world-navigation-uiux.design.md#r-feed-02) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg) / state_snippet；代表状态 |
| R-FEED-03 Feed 事件回执引用和模块定位 | conditional_current | [R-FEED-03](viewer-world-navigation-uiux.design.md#r-feed-03) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W06-feed-states.svg) / state_snippet；代表状态 |
| R-DIAG-01 资源下载/校验/准备与 renderer 恢复 | current | [R-DIAG-01](viewer-world-navigation-uiux.design.md#r-diag-01) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg) / state_snippet；代表状态 |
| R-DIAG-02 Renderer 诊断与按需 Render DTO | current | [R-DIAG-02](viewer-world-navigation-uiux.design.md#r-diag-02) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W10-renderer-states.svg) / state_snippet；代表状态 |
| R-DIAG-03 执行通道期望与 provider 实测探测 | current | [R-DIAG-03](viewer-world-navigation-uiux.design.md#r-diag-03) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg) / state_snippet；代表状态 |
| R-DIAG-04 认证恢复、准入探针和会话阶梯 | conditional_current | [R-DIAG-04](viewer-world-navigation-uiux.design.md#r-diag-04) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W07-diagnostics.svg) / state_snippet；代表状态 |
| R-DIAG-05 托管动作矩阵、反馈与最近事件 | current | [R-DIAG-05](viewer-world-navigation-uiux.design.md#r-diag-05) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W19-diagnostic-data.svg) / state_snippet；代表状态 |
| R-DIRECTOR-01 申请/退出 Director | conditional_current | [R-DIRECTOR-01](viewer-world-navigation-uiux.design.md#r-director-01) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W08-director-states.svg) / state_snippet；代表状态 |
| R-GAME-01 初始 OC 引导与领取 gate | conditional_current | [R-GAME-01](viewer-gameplay-details-uiux.design.md#r-game-01) | [具体稿](assets/ui-ux/full-function/gameplay/frames/first-oc-pending.svg) / normal_frame；代表状态 |
| R-GAME-02 首聊解锁预览 | conditional_current | [R-GAME-02](viewer-gameplay-details-uiux.design.md#r-game-02) | [具体稿](assets/ui-ux/full-function/gameplay/frames/first-chat-preview.svg) / normal_frame；代表状态 |
| R-GAME-03 经济 Agent 候选理由与承诺比较 | conditional_current | [R-GAME-03](viewer-gameplay-details-uiux.design.md#r-game-03) | [具体稿](assets/ui-ux/full-function/gameplay/frames/claim-defer.svg) / normal_frame；代表状态 |
| R-GAME-04 经济认领明细、确认与释放边界 | conditional_current | [R-GAME-04](viewer-gameplay-details-uiux.design.md#r-game-04) | [具体稿](assets/ui-ux/full-function/gameplay/frames/claim-ready.svg) / normal_frame；代表状态 |
| R-GAME-05 认领目标选择与原会话边界 | conditional_current | [R-GAME-05](viewer-gameplay-details-uiux.design.md#r-game-05) | [具体稿](assets/ui-ux/full-function/gameplay/frames/claim-ready.svg) / normal_frame；代表状态 |
| R-GAME-06 扩张分支取舍 | conditional_current | [R-GAME-06](viewer-gameplay-details-uiux.design.md#r-game-06) | [具体稿](assets/ui-ux/full-function/gameplay/frames/expansion-incomplete.svg) / normal_frame；代表状态 |
| R-GAME-07 吸引力证明与下一选择 | conditional_current | [R-GAME-07](viewer-gameplay-details-uiux.design.md#r-game-07) | [具体稿](assets/ui-ux/full-function/gameplay/frames/attraction-agency-unknown.svg) / normal_frame；代表状态 |
| R-GAME-08 间接能动性动词 | conditional_current | [R-GAME-08](viewer-gameplay-details-uiux.design.md#r-game-08) | [具体稿](assets/ui-ux/full-function/gameplay/frames/attraction-agency-unknown.svg) / normal_frame；代表状态 |
| R-GAME-09 首胜与反刷解释 | conditional_current | [R-GAME-09](viewer-gameplay-details-uiux.design.md#r-game-09) | [具体稿](assets/ui-ux/full-function/gameplay/frames/firstwin-continuation.svg) / normal_frame；代表状态 |
| R-GAME-10 成熟世界承接 | conditional_current | [R-GAME-10](viewer-gameplay-details-uiux.design.md#r-game-10) | [具体稿](assets/ui-ux/full-function/gameplay/frames/firstwin-continuation.svg) / normal_frame；代表状态 |
| R-GAME-11 已接受意图与恢复锚点 | conditional_current | [R-GAME-11](viewer-gameplay-details-uiux.design.md#r-game-11) | [具体稿](assets/ui-ux/full-function/gameplay/frames/goal-execution-blocked.svg) / normal_frame；代表状态 |
| R-GAME-12 执行状态机与原因 | conditional_current | [R-GAME-12](viewer-gameplay-details-uiux.design.md#r-game-12) | [具体稿](assets/ui-ux/full-function/gameplay/frames/goal-execution-blocked.svg) / normal_frame；代表状态 |
| R-GAME-13 当前目标、进度、阻塞与下一步 | current | [R-GAME-13](viewer-gameplay-details-uiux.design.md#r-game-13) | [具体稿](assets/ui-ux/full-function/gameplay/frames/goal-execution-blocked.svg) / normal_frame；代表状态 |
| R-GAME-14 等待解决预估 | conditional_current | [R-GAME-14](viewer-gameplay-details-uiux.design.md#r-game-14) | [具体稿](assets/ui-ux/full-function/gameplay/frames/wait-resolution-safe.svg) / normal_frame；代表状态 |
| R-GAME-15 Fallback 取舍与无安全方案交接 | conditional_current | [R-GAME-15](viewer-gameplay-details-uiux.design.md#r-game-15) | [具体稿](assets/ui-ux/full-function/gameplay/frames/fallback-choices.svg) / normal_frame；代表状态 |
| R-GAME-16 产品验证解锁预览 | conditional_current | [R-GAME-16](viewer-gameplay-details-uiux.design.md#r-game-16) | [具体稿](assets/ui-ux/full-function/gameplay/frames/validation-preview-available.svg) / normal_frame；代表状态 |
| R-GAME-17 能力经济投入、产出、新用途 | conditional_current | [R-GAME-17](viewer-gameplay-details-uiux.design.md#r-game-17) | [具体稿](assets/ui-ux/full-function/gameplay/frames/capability-economics.svg) / normal_frame；代表状态 |
| R-GAME-18 工业起步结果与阶段承接 | conditional_current | [R-GAME-18](viewer-gameplay-details-uiux.design.md#r-game-18) | [具体稿](assets/ui-ux/full-function/gameplay/frames/industrial-settled.svg) / normal_frame；代表状态 |
| R-GAME-19 微型仓库设施、库存与技术证据 | conditional_current | [R-GAME-19](viewer-gameplay-details-uiux.design.md#r-game-19) | [具体稿](assets/ui-ux/full-function/gameplay/frames/depot-empty.svg) / normal_frame；代表状态 |
| R-GAME-20 首个交付预览 | conditional_current | [R-GAME-20](viewer-gameplay-details-uiux.design.md#r-game-20) | [具体稿](assets/ui-ux/full-function/gameplay/frames/first-delivery-preview.svg) / normal_frame；代表状态 |
| R-GAME-21 配方替代方案 | conditional_current | [R-GAME-21](viewer-gameplay-details-uiux.design.md#r-game-21) | [具体稿](assets/ui-ux/full-function/gameplay/frames/recipe-known.svg) / normal_frame；代表状态 |
| R-GAME-22 生产失败处置 | conditional_current | [R-GAME-22](viewer-gameplay-details-uiux.design.md#r-game-22) | [具体稿](assets/ui-ux/full-function/gameplay/frames/failure-blocked.svg) / normal_frame；代表状态 |
| R-GAME-23 控制影响证明 | conditional_current | [R-GAME-23](viewer-gameplay-details-uiux.design.md#r-game-23) | [具体稿](assets/ui-ux/full-function/gameplay/frames/proof-awaiting.svg) / normal_frame；代表状态 |
| R-GAME-24 repair / rebuild / pivot 恢复比较 | conditional_current | [R-GAME-24](viewer-gameplay-details-uiux.design.md#r-game-24) | [具体稿](assets/ui-ux/full-function/gameplay/frames/recovery-compare.svg) / normal_frame；代表状态 |
| R-GAME-25 短期目标重排表单 | conditional_current | [R-GAME-25](viewer-gameplay-details-uiux.design.md#r-game-25) | [具体稿](assets/ui-ux/full-function/gameplay/frames/reprioritize-keyboard.svg) / normal_frame；代表状态 |
| R-ACTION-01 动态已公布 gameplay action 执行 | conditional_current | [R-ACTION-01](viewer-gameplay-details-uiux.design.md#r-action-01) | [具体稿](assets/ui-ux/full-function/gameplay/frames/build-blocked.svg) / normal_frame；代表状态 |
| R-ACTION-02 可用动作列表及未暴露动作说明 | conditional_current | [R-ACTION-02](viewer-gameplay-details-uiux.design.md#r-action-02) | [具体稿](assets/ui-ux/full-function/gameplay/frames/build-blocked.svg) / normal_frame；代表状态 |
| R-QUOTE-01 精炼报价与 preflight | conditional_current | [R-QUOTE-01](viewer-quote-forms-uiux.design.md#r-quote-01) | [具体稿](assets/ui-ux/full-function/quotes/q05-refine_quote_preflight.svg) / normal_frame；代表状态 |
| R-QUOTE-02 售电预估与生存风险 | conditional_current | [R-QUOTE-02](viewer-quote-forms-uiux.design.md#r-quote-02) | [具体稿](assets/ui-ux/full-function/quotes/q03-power_sale_quote.svg) / normal_frame；代表状态 |
| R-QUOTE-03 电量生存与恢复预估 | conditional_current | [R-QUOTE-03](viewer-quote-forms-uiux.design.md#r-quote-03) | [具体稿](assets/ui-ux/full-function/quotes/q02-power_survival_quote.svg) / normal_frame；代表状态 |
| R-QUOTE-04 工厂配方排产预估 | conditional_current | [R-QUOTE-04](viewer-quote-forms-uiux.design.md#r-quote-04) | [具体稿](assets/ui-ux/full-function/quotes/q09-schedule_recipe_quote.svg) / normal_frame；代表状态 |
| R-QUOTE-05 物料转运预估与路线取舍 | conditional_current | [R-QUOTE-05](viewer-quote-forms-uiux.design.md#r-quote-05) | [具体稿](assets/ui-ux/full-function/quotes/q10-transfer_material_quote.svg) / normal_frame；代表状态 |
| R-QUOTE-06 材料区块补充预览 | conditional_current | [R-QUOTE-06](viewer-quote-forms-uiux.design.md#r-quote-06) | [具体稿](assets/ui-ux/full-function/quotes/q04-fragment_refill_preview.svg) / normal_frame；代表状态 |
| R-QUOTE-07 战争声明预览 | conditional_current | [R-QUOTE-07](viewer-quote-forms-uiux.design.md#r-quote-07) | [具体稿](assets/ui-ux/full-function/quotes/q07-war_declaration_quote.svg) / normal_frame；代表状态 |
| R-QUOTE-08 治理投票结果预览 | conditional_current | [R-QUOTE-08](viewer-quote-forms-uiux.design.md#r-quote-08) | [具体稿](assets/ui-ux/full-function/quotes/q08-governance_vote_quote.svg) / normal_frame；代表状态 |
| R-QUOTE-09 市场材料决策预估 | conditional_current | [R-QUOTE-09](viewer-quote-forms-uiux.design.md#r-quote-09) | [具体稿](assets/ui-ux/full-function/quotes/q06-market_quote_decision.svg) / normal_frame；代表状态 |
| R-QUOTE-10 产品验证报价 | conditional_current | [R-QUOTE-10](viewer-quote-forms-uiux.design.md#r-quote-10) | [具体稿](assets/ui-ux/full-function/quotes/q01-product_validation_quote.svg) / normal_frame；代表状态 |
| R-NAV-04 帮助气泡、外点关闭与 Escape | current | [R-NAV-04](viewer-world-navigation-uiux.design.md#r-nav-04) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W18-targets-inspection.svg) / state_snippet；代表状态 |
| R-CMD-13 控制丢失反馈恢复按钮 | conditional_current | [R-CMD-13](viewer-account-agent-uiux.design.md#r-cmd-13) | [具体稿](assets/ui-ux/full-function/account-agent/03-command.svg) / normal_frame；代表状态 |
| R-WORLD-10 电影模式键盘退出与抽屉焦点返回 | current | [R-WORLD-10](viewer-world-navigation-uiux.design.md#r-world-10) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg) / state_snippet；代表状态 |
| R-WORLD-11 画布相机移动、缩放与交互命令边界 | conditional_current | [R-WORLD-11](viewer-world-navigation-uiux.design.md#r-world-11) | [具体稿](assets/ui-ux/full-function/world-diagnostics/W12-cinematic.svg) / normal_frame；代表状态 |
| R-GAME-26 Gameplay Details 按需展开 | current | [R-GAME-26](viewer-gameplay-details-uiux.design.md#r-game-26) | [具体稿](assets/ui-ux/full-function/gameplay/frames/gameplay-disclosure-closed.svg) / normal_frame；代表状态 |

## 已知差距

- 旧WorldStageHero邮箱-only文案与实际多凭证入口不同；设计不采其为authority，本轮未修代码。
- imported-key产品关系采用ENTRY-MODE并另承Flow-KEYAUTH-008，不因无独立产品页而N/A。
- main custody新增实时上下文新鲜读取义务；涉及恢复的稿不得把旧签名响应当本次fresh证明。
- 未接纳的family最终稿、动态payload、native、真实运行/世界结果链均未被目录闭合。
- 官网、launcher、节点发布/共识/历史存储算法不能一律转成Viewer管理页；约束仍有效，非UI不等于产品不适用。

## 源事实修正（不修改实现）

- R-AUTH-12：当前HostedLoginGate有Tab循环、focusin guard与cleanup焦点返回；没有login-gate专用inert绑定，AppShell inert只由starterOcGateOpen控制。背景隔离完整行为仍待运行验收。
- R-NAV-03：当前Entry为native details/summary；没有任意关闭后的显式焦点恢复实现，不将该提议当当前事实。
- R-NAV-04：InlineHelpTip支持外点与Escape关闭；无显式focus-restoration，不将通用目标合同当当前已实现。
- R-WORLD-03：当前Receipt无独立返回原目标按钮，也无next_step字段；只映射真实已显示字段与主体/阶段，不按旧摘要造控件。
- R-DIAG-05：当前metric row不保证单列主体ID；反馈与事件分区可读，但不补造源未发布身份。

## 逐要求适用性与未覆盖范围

[要求交叉索引](assets/ui-ux/full-function/requirements-crosswalk.json)逐条定位真实产品声明、现行设计关系、条件目标及合理非UI边界。它不改变产品状态、不发新Trace ID，也不以关系/图稿数量推导完成。根上下文仍随其authority适用，候选源关联不自动成为完整承接。
