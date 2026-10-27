@/Users/scc/.codex/RTK.md

# Oasis7 Agent Entry Point

通用规则见 [开发流程规范](doc/engineering/workflow/source-of-truth.md)。主执行者负责分析、实现、自测和组织适用评审，并在满足规范、必要检查及用户授权后推进合入和交付；专业协作按复杂度与风险使用。

- 在独立分支修改；不在 main 或主 worktree 编辑。保留用户和其他执行者的工作。
- 工作区先检查 `git worktree list --porcelain` 并优先复用干净、未占用的固定路径；确需新增使用 `./scripts/new-task-worktree.sh <module> <task> --base <ref>`，遵循 `oasis7.task-worktrees-root` 配置。除用户明确指定外，不用 App `create_worktree`、worktree fork 或 handoff 绕过仓库入口。
- 含子模块的 worktree 清理按开发规范 §5.1，通过 `resource-cleanup-executor.py --allow-submodules` 显式启用有条件例外；不直接用强制删除绕过检查。
- Cargo 默认使用各 worktree 的本地 `target/`，无需预先准备。日常反馈用开发 wrapper，正式验收保留 canonical 命令和参数。
- `third_party/` 只读；Rust 工作参考 `third_party/rust-skills/AGENTS.md`。
- 一个 PR 对应一个清楚目的，可原子修改多个 crate；Issue 和 Project 按需使用。
- 仓库文档及 Issue、PR、评审和交接文字遵循[语言与写作要求](doc/engineering/workflow/source-of-truth.md#language-and-writing)。
- 问题根因确认后，先按[设计检查与修复](doc/engineering/workflow/source-of-truth.md#root-cause-design-check)核对产品与系统设计：有缺陷先补受影响文档再修实现；纯实现 bug 直接修复验证，不强制改正确设计。
- 新增或实质修改的跨层要求按[要求追踪规范](doc/engineering/doc-governance/cross-layer-requirements-traceability.design.md)关联唯一声明、适用设计与当前 PR：复用同一要求的 Trace ID，不把它当任务身份或准入条件，不为简单编辑虚构要求或补齐三层。
- CI 控制、安全和兼容边界变化需要独立评审。专业关注点见 `.agents/roles/`，方法见 `.agents/skills/README.md`。

## 常用命令

- Rust canonical：`env -u RUSTC_WRAPPER cargo ...`
- 本地开发：`./scripts/cargo-dev.sh ...`
- 使用各 worktree 的本地 Cargo target，等待 Cargo 构建锁；不为绕过锁而更换 `CARGO_TARGET_DIR`。
- 测试：`testing-manual.md`；UI/Web 见 S6。
- Agent 游戏功能验证须按[规范触发范围](doc/engineering/workflow/source-of-truth.md#agent-gameplay-selftest)执行[自测操作手册](doc/testing/manual/agent-runtime-self-playtest.manual.md)：主 Agent 组织独立试玩、读取正式游戏 Skill、核对世界结果并修复复测；缺前置如实报告，普通 CI 不代签实玩。
- 快捷检查：`./scripts/pre-commit.sh`；文档：`./scripts/doc-governance-check.sh`。
- PR：`gh pr create`、`gh pr checks <pr> --required`。
- 合入：满足适用评审、检查及用户限制后，执行 `gh pr merge <pr> --squash --match-head-commit <expected-head>`。

PR 标题和 commit message 使用英文。
