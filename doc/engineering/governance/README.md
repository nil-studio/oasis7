# engineering/governance 运行治理

本页是工程运行治理的 canonical 入口：定义仓库健康巡检方法，并将需要独立专业语义的环境治理分流到专题文档。

## 从这里开始

- 想确认项目 `local` / `test` / `production` 环境边界、云上服务清单、`public_testnet` 与 `mainnet` 的声明限制：`environment-lanes-and-inventory-2026-05-29.md`
- 想执行仓库健康巡检、判定 findings 归属或进行季度复核：继续阅读本页。
- 想判断文档治理是入口阅读面问题还是库存维护成本问题：先按 `scripts/doc-inventory-report.sh` 复算总量、模块/热点路径密度、`doc/devlog` backlog 与非归档近限文件；入口失去首读分流时回到 `../doc-governance/README.md`，其余结果按路径治理、历史压缩、近限拆分或季度复核建立 bounded follow-up。

## 边界

- 文档树结构、README 职责与 redirect 规则由 `../doc-governance/README.md` 分流；本页不复述这些共享规则。
- 新增本目录的运行治理文档时，同批更新本页；只在需要文件级三件套检索时更新 `../prd.index.md`。

## 仓库健康巡检

### 触发与边界

工程治理 owner 人工触发巡检；仓库不维护 scheduler 或 GitHub Actions 定时任务。巡检是人工分类的 health review，不新增 required gate，也不替代 [canonical workflow](../workflow/source-of-truth.md)。

巡检按需在独立分支或只读环境进行，记录问题、验证、建议、实际版本、命令结果和后续处置；通用规则见 [开发流程规范](../workflow/source-of-truth.md)。

### 基线检查

```bash
./scripts/doc-inventory-report.sh
./scripts/doc-governance-check.sh
./scripts/lint-skills.sh
./scripts/worktree-gc-report.sh --footprint

./scripts/ci-rust-governance-report.sh --out-dir "output/rust-governance/repository-health-$(date +%Y%m%d)"
```

代码健康抽样仅在 finding 需要时运行最窄层级，并记录选择理由：

```bash
./scripts/ci-tests.sh required
```

### 分类规则

| 信号 | 处置 |
| --- | --- |
| `doc-governance-check` / `lint-skills` 失败 | 作为 P0/P1 engineering-governance follow-up candidate，定位到具体文档或 workflow surface。 |
| `doc-inventory-report` 返回 `action_required` | 按 module/hotspot 分类；在聚焦 path-governance follow-up 与季度趋势证据之间做明确选择。 |
| `worktree-gc-report` 的注册、文件或体积异常 | 仅作 Git/文件事实线索，不判定删除资格。清理使用 `resource-cleanup-executor.py`，明确目标、branch 和 expected HEAD，并重新检查合入、资料及占用。 |
| 本地 Cargo target 占用 | 按 [testing-manual.md](../../../testing-manual.md) 盘点所属工作区，停止构建和运行后核对路径并用 Cargo clean 回收；显式外部目录及旧链接产物需单独核对归属、引用和占用。未知资料保留。 |
| Rust governance report finding | 阅读 duplicate counts 和 top-crate list；将 advisory upgrade、routine refresh、dependency prune 和 unsafe-boundary review 分类，不在巡检 task 内直接升级依赖。 |
| `ci-tests.sh required` 失败 | 区分 formatting、RustSec、file-size/code-health、scoped test 和 workflow-surface 失败。只有当前 task 引入或它已是 active merge/release blocker 时，才将巡检标为 blocked。 |

### 代码与依赖抽样

- 全库覆盖来自自动化报告：format/lint/test、warning 签名、近限或超限 Rust 文件、`unsafe` / `unwrap()` 搜索、dependency 报告和 changed-path planner。
- 人工只深读风险样本：changed diff、report hit、unsafe 边界及 `// SAFETY:` 注释、public API、依赖升级影响面和缺失/过期测试。不得从报告单独推导全库 style conformance。
- 当 finding 无法分类、安全或 dependency closure 影响不确定、workflow drift 呈系统性，或同一模式跨多模块重复时，再升级为 path-level deep reading。
- Rust 抽样把 `third_party/rust-skills/AGENTS.md` 作为只读输入。若缺失，先运行 `git submodule update --init -- third_party/rust-skills`；仍不可用时记录缺口，不得静默跳过。
- 依赖升级、prune 或 style drift 每个聚焦面单独形成 follow-up，包含 owner role、受影响 crate/package、兼容性检查、rollback 和需要参与的专业角色。

### 路由与季度复核

- 文档/代码对齐、语义、workflow drift 和证据债务：`repository_health_engineer`。
- 验证充分性或 release-blocking 判断：`qa_engineer`。
- runtime、Viewer/Web、WASM、agent、blockchain ops、gameplay 或视觉/交互：对应专业角色。
- 对外公告、incident 或玩家承诺：`liveops_community`。

季度复核比较近期巡检中的 doc hotspot、worktree 注册与体积异常、Rust/dependency/unsafe backlog 和重复 governance 失败。趋势基线见 `../evidence/engineering-governance-trend-baseline-2026-03-11.md`。只有策略、阈值或 active owner 改变时，才更新对应 engineering governance 专题文档。

复核记录至少包含 review ID/quarter/date、参与角色、trigger、inputs、`pass|watchlist|fix_required` 结论，以及每个 finding 的 category、scope/evidence、disposition、owner 和 priority。不得沿用 2026-03 baseline 的 audit-round counter；当前巡检结果定义新观察窗口。
