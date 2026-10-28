# world-simulator PRD 分册：统一验收清单（场景 / Viewer / 启动器）

审计轮次: 6
## 目标
- 将场景系统、Viewer Web 闭环、启动器能力的验收口径统一为一份可执行清单，避免各链路口径不一致。
- 让 `PRD-WORLD_SIMULATOR-001/002` 在同一验收表里可追踪、可复跑、可留证据。
- 原 `TASK-WORLD_SIMULATOR-002/003/004` 仅保留为清单演进来源，不构成当前必须创建的任务或排期。

## 范围
- In Scope:
  - 场景系统基线（scenario matrix）验收命令与通过标准。
  - Viewer Web-first 闭环（agent-browser）验收命令与证据标准。
  - 启动器（统一启动 + 客户端启动器）核心路径验收命令与通过标准。
  - 统一证据模板（命令、产物路径、结论）。
- Out of Scope:
  - 不替代 `testing-manual.md` 的分层全量手册。
  - 不新增业务功能或新的测试框架。

## 当前适用范围与来源

先按[总手册](../../../../testing-manual.md)和本次 PR 的适用测试计划选 scope/tier/gate；来源到场景及未证明边界使用 [QA source-to-scene 合同](../../../testing/prd.md#qa-infrastructure-source-scene-method)。下表沿用 2026-03-03 的 Gate-ID 与示例来源，不是所有普通改动都要完整执行的独立门禁，命令/套件选择以同候选的现行手册与 runner 为准。

每次执行记录 source/base/tested tree、build/产物来源、独立 attempt、环境与时间窗口、实际场景和断言。新 PR 或 main 前进不重命名旧证据；未执行保留未执行，不适用项写明合同依据。只有所选范围的必需证据齐全且断言满足，才能给出该范围的 pass；不能升级为未覆盖范围或发布 go。

## 统一验收门禁（2026-03-03 基线）

| Gate-ID | 模块 | 通过标准 | 适用时的命令 / 手册入口（示例） | 证据要求 |
| --- | --- | --- | --- | --- |
| G1 | 场景系统 | 场景 ID 与模板稳定，核心场景可回放 | `env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required scenario_specs_match_ids -- --nocapture`<br>`env -u RUSTC_WRAPPER cargo test -p oasis7 --features test_tier_required scenarios_are_stable -- --nocapture` | 终端日志（通过/失败） |
| G2 | Viewer Web 闭环 | 满足所选 S6 层级的真实交互、环境与断言；页面/截图/console 仅为 surface observation；`viewer` 与 `software_safe` alias 状态分别记录 | 按[总手册 S6](../../../../testing-manual.md#testing-s6-qa-receiver)及其 browser manual/runner 执行本次适用场景 | 同构建/会话的截图、console、前后状态/操作结果；声明世界效果时另附权威 oracle |
| G3 | 启动器统一入口 | launcher 参数解析与 URL 组装路径通过 | `env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_game_launcher` | 终端日志（通过/失败） |
| G4 | 启动器客户端 | 配置校验、链路提交流程可执行 | `env -u RUSTC_WRAPPER cargo test -p oasis7_client_launcher` | 终端日志（通过/失败） |
| G5 | 文档追踪 | 适用合同与引用可达，正文和登记一致；交付事实回到 Git/PR/实际 CI/评审，Issue / Project 按需 | `./scripts/doc-governance-check.sh`（适用时） | 实际输出与版本；通过只证明文档检查，不证明技术/运行验收 |

## 执行顺序
1. 先固定候选/实际测试树、合同版本、适用范围及必需 gate，明确未覆盖项。
2. 对选中的场景/启动器范围，按实际依赖执行 G1、G3、G4；G5 仅在文档检查适用时执行，不能为其他 gate 代签。
3. 对选中的 Viewer 范围，按当前 S6 分层执行 G2，保存真实交互、结果与适用的 desktop/narrow 证据。
4. 汇总本次 scope/tier 的逐项 verdict；存在必需证据缺失则 blocked/unknown。发布另核对所有适用发布判据，不因本表有绿项就放行。

## PRD-ID 映射
- `PRD-WORLD_SIMULATOR-001`:
  - G1（场景稳定性）
  - G3/G4（启动器链路稳定性）
- `PRD-WORLD_SIMULATOR-002`:
  - G2（Web-first Viewer 闭环）
  - G5（文档与流程一致性）

## 证据模板入口
- Web-first 与 LLM 统一证据卡模板：`doc/world-simulator/prd/acceptance/web-llm-evidence-template.md`
- 体验质量趋势跟踪入口：`doc/world-simulator/prd/quality/experience-trend-tracking.md`

## 证据记录模板
```md
- Gate-ID / 适用合同精确路径、锚点与版本:
- source OID / base OID / 实际 tested tree / build ID 或产物 digest:
- 构建来源与 freshness / attempt ID / 环境、入口、会话、时间窗口:
- scope / tier / 实际场景与断言 / 必需 gate:
- 命令、最终退出码或实际可见操作:
- 结果: pass/fail/blocked/unknown/not_applicable（不适用依据）
- 原始证据路径、适用 oracle 与判定:
- 未执行 / 未覆盖 / 未证明范围及 blocker:
- 复跑记录（如有，另一个 attempt，不覆盖本次失败）:
```

## 风险与约束
- 当结论包含 Viewer UI 时，仅运行 G1/G3/G4 而缺失 G2，不能宣称 UI 闭环已验证；纯非 UI 范围无需据此增加无关浏览器测试。
- 若 G2 只截图不看 console，会遗漏运行时错误；两者都通过仍不能替代真实交互、权威世界效果或更高层可玩性证据。
- 若清单版本与 `testing-manual.md` 口径漂移，应以 `testing-manual.md` 为主并回写本清单。
